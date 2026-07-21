//! feed — live background data, done the sanctioned way.
//!
//! A worker thread produces synthetic log events on a BURSTY cadence
//! (bursts, then quiet gaps — so you can see both coalescing and true
//! idle) and hands them to the UI through the bounded ingestion lane:
//!
//! ```text
//! worker thread ──send()──> bounded_source ──posted drain──> Signal
//!   (never touches signals)  (capacity + policy + counters)  (UI thread)
//! ```
//!
//! What to watch for:
//! - a whole burst arrives as ONE repaint (one wake, one drain, one
//!   frame — the damage contract at work);
//! - during the quiet gaps the app is byte-for-byte idle (no timers
//!   spinning, no polling — the loop is parked in a blocking read);
//! - the status line counts DROPPED events honestly when the window
//!   overflows (labeled degradation, never silent loss);
//! - events/sec is sampled by `reactive::interval` — the cancellable
//!   recurring timer (fixed-delay, no catch-up storms).
//!
//! The scrolling view uses today's Scroll widget with the hand-rolled
//! follow-tail idiom (stick to bottom unless the user scrolled up),
//! noted honestly as such: the packaged Feed widget lands this wave
//! from CONTENT, and cycle 2 switches this example onto it.
//!
//! Keys: space pauses/resumes the producer · f re-follows the tail ·
//! wheel/arrows scroll · q or Ctrl+C quits (worker torn down cleanly).
//!
//! Try: `cargo run --example feed`
//!
//! OWNER: LIVEDATA.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use abstracttui::prelude::*;
use abstracttui::reactive::{
    bounded_source, interval, spawn_worker, BoundedSender, OverflowPolicy,
};

/// The retained window: at most this many events live in the signal.
/// Overflow follows DropOldest (ring) and is counted on the status line.
const WINDOW: usize = 400;

/// Rows of chrome around the scroll area (border ×2, status, help).
const CHROME_ROWS: i32 = 4;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("feed: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let stop = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));

    let mut app = App::new(Size::new(80, 24));
    let quitter = app.quitter();
    let mut sender: Option<BoundedSender<String>> = None;
    let (stop_ui, paused_ui) = (stop.clone(), paused.clone());
    // No `move`: `sender` is captured by mutable borrow (the escape
    // hatch for handles), everything used inside inner closures moves.
    app.mount(|cx| {
        // The bounded lane: capacity, explicit overflow policy, honest
        // counters. The signals die with the app scope; the sender the
        // worker holds then turns inert (never UB, drops counted).
        let (tx, events, stats) = bounded_source::<String>(cx, WINDOW, OverflowPolicy::DropOldest);
        sender = Some(tx);

        // Reactive mirrors of the worker flags (atomics are not
        // reactive; the status line re-renders through these).
        let paused_sig = cx.signal(false);

        // events/sec, sampled once per second by the recurring time
        // source. Cancelled by scope disposal — no flag bookkeeping.
        let rate = cx.signal(0u64);
        let mut last_delivered = 0u64;
        interval(cx, Duration::from_secs(1), move || {
            let delivered = stats.with_untracked(|s| s.delivered);
            rate.set(delivered - last_delivered);
            last_delivered = delivered;
        });

        // Follow-tail idiom (hand-rolled; Feed packages this): external
        // scroll offsets so rebuilds never reset the position...
        let oy = cx.signal(0i32);
        let ox = cx.signal(0i32);
        let follow = cx.signal(true);
        let viewport = use_viewport(cx);
        let view_h = move || (viewport.get().h - CHROME_ROWS).max(1);
        // ...one effect pins the offset to the bottom while following
        // (new events AND resizes re-pin, both tracked)...
        cx.effect_labeled("feed-follow", move || {
            let len = events.with(|v| v.len()) as i32;
            let max_off = (len - view_h()).max(0);
            if follow.get() {
                oy.set(max_off);
            }
        });
        // ...and one derives the follow state back from where the user
        // actually is (wheel-up releases the tail, bottom re-arms it).
        cx.effect_labeled("feed-follow-watch", move || {
            let off = oy.get();
            let len = events.with_untracked(|v| v.len()) as i32;
            let max_off = (len - (viewport.get_untracked().h - CHROME_ROWS).max(1)).max(0);
            follow.set_if_changed(off >= max_off);
        });

        let theme = use_theme(cx);
        let (stop_k, paused_k) = (stop_ui.clone(), paused_ui.clone());
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| {
                stop_k.store(true, Ordering::Relaxed);
                quitter.quit();
            })
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                let now = !paused_k.load(Ordering::Relaxed);
                paused_k.store(now, Ordering::Relaxed);
                paused_sig.set(now);
            })
            .shortcut(KeyChord::plain(Key::Char('f')), move |_| {
                follow.set(true); // the follow effect snaps to the tail
            })
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let vp = viewport.get();
                let rows = events.get(); // rebuild per drain: one frame per burst
                let content_w = (vp.w - 4).max(8);
                let content_h = rows.len() as i32;
                let mut column = Element::new().style(LayoutStyle::column());
                for line in &rows {
                    column = column.child(text(line.clone()));
                }
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("live feed (bounded, drop-oldest)")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(
                        Scroll::new(column.build())
                            .content_size(content_w, content_h)
                            .offset_y(oy)
                            .offset_x(ox)
                            .element(cx, &t)
                            .build(),
                    )
                    .element(&t)
                    .build()
            }))
            .child(dyn_view(LayoutStyle::line(1), move || {
                let s = stats.get();
                let state = if paused_sig.get() {
                    "paused"
                } else if follow.get() {
                    "following"
                } else {
                    "scrolled (f to re-follow)"
                };
                // Honesty rule: dropped is rendered the moment it is
                // nonzero — the window overflowed, the user should know.
                let dropped = if s.dropped > 0 {
                    format!(" · {} dropped", s.dropped)
                } else {
                    String::new()
                };
                text(format!(
                    " {} shown · {}/s{} · {}",
                    events.with(|v| v.len()),
                    rate.get(),
                    dropped,
                    state
                ))
            }))
            .child(text(
                " space pause producer · f follow tail · wheel scroll · q quit",
            ))
            .build()
    })?;

    // The producer lives OUTSIDE the reactive world: it only owns a
    // sender. spawn_worker surfaces a panic as a labeled app error
    // instead of a silent dead feed.
    let tx = sender.take().expect("mount ran");
    let (stop_w, paused_w) = (stop.clone(), paused.clone());
    let worker = spawn_worker("feed-producer", move || {
        produce(&tx, &stop_w, &paused_w);
    });

    let result = app.run();

    // Teardown: tell the worker to stop and WAIT for it — no detached
    // thread outliving the terminal session. The producer sleeps in
    // short slices, so the join is prompt.
    stop.store(true, Ordering::Relaxed);
    worker.join().ok();
    result
}

/// Synthetic bursty producer: a burst of events, then a quiet gap.
/// Deterministic xorshift keeps it dependency-free (std only).
fn produce(tx: &BoundedSender<String>, stop: &AtomicBool, paused: &AtomicBool) {
    const SAMPLES: [&str; 8] = [
        "GET /api/health 200 3ms",
        "worker-7 picked job #4812 (encode)",
        "cache shard 3: 96.2% hit rate",
        "peer 10.0.0.42 connected (tls1.3)",
        "GET /api/feed 200 12ms",
        "retry queue drained (0 left)",
        "job #4812 done in 412ms",
        "gc pause 1.8ms (minor)",
    ];
    let started = std::time::Instant::now();
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut seq = 0u64;
    while !stop.load(Ordering::Relaxed) {
        if paused.load(Ordering::Relaxed) {
            // Paused = truly quiet: no sends, so the UI is truly idle.
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // A burst (3..=34 events, back-to-back): the UI folds it into
        // one drain and one frame.
        let burst = 3 + next() % 32;
        for _ in 0..burst {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            seq += 1;
            let sample = SAMPLES[(next() % SAMPLES.len() as u64) as usize];
            tx.send(format!(
                "{:>8.2}s  #{seq:05}  {sample}",
                started.elapsed().as_secs_f32()
            ));
        }
        // A quiet gap (150..=950ms), slept in short slices so quit
        // teardown joins promptly. While this sleeps, the app costs
        // zero bytes and zero wakeups.
        let gap = 150 + next() % 800;
        let mut slept = 0;
        while slept < gap && !stop.load(Ordering::Relaxed) {
            let slice = 50.min(gap - slept);
            std::thread::sleep(Duration::from_millis(slice));
            slept += slice;
        }
    }
}
