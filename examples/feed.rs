//! feed — live background data, done the sanctioned way.
//!
//! A worker thread produces synthetic log events on a BURSTY cadence
//! (bursts, then quiet gaps — so you can see both coalescing and true
//! idle) and hands them to the UI through the bounded ingestion lane,
//! rendered by the `Feed` widget (keyed rich items, windowed paint):
//!
//! ```text
//! worker thread ──send()──> bounded_source ──posted drain──> Signal
//!   (never touches signals)  (capacity + policy + counters)  (UI thread)
//!        └────────────────────────────────────────> FeedState (slot-keyed)
//! ```
//!
//! What to watch for:
//! - a whole burst arrives as ONE repaint (one wake, one drain, one
//!   frame — the damage contract at work);
//! - during the quiet gaps the app is byte-for-byte idle (no timers
//!   spinning, no polling — the loop is parked in a blocking read);
//! - the status line counts DROPPED events honestly when the window
//!   overflows (labeled degradation, never silent loss);
//! - alert events render as real markdown items (Feed's rich blocks);
//! - events/sec is sampled by `reactive::interval` — the cancellable
//!   recurring timer (fixed-delay, no catch-up storms).
//!
//! Follow-tail is the ENGINE's (`Scroll::follow_tail`): while
//! following, the offset pins to the bottom across appends and
//! resizes; scrolling up releases it; reaching the bottom (or pressing
//! f) re-arms it. The content extent is MEASURED — no size hint, no
//! rebuild-per-append: Feed's content-sized mode answers the solver
//! reactively. (`FeedState::clear()` + re-push is the simpler window
//! sync; the slot-keyed replace below re-typesets only the slots whose
//! content actually changed.)
//!
//! Selection is the ENGINE's (screen-text selection, backlog 0270):
//! this demo enables always-on drag select — drag paints the highlight
//! (clamped to the pane under the anchor, so the border never leaks
//! into a copy), releasing (or c/Enter) copies the text via OSC 52,
//! Esc or a click clears. Wheel scrolling is untouched by all of it.
//!
//! Keys: space pauses/resumes the producer · f jumps to the tail ·
//! wheel/arrows scroll · drag selects, c copies · q or Ctrl+C quits
//! (worker torn down cleanly).
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
use abstracttui::widgets::{Feed, FeedItem, FeedState};

/// The retained window: at most this many events live in the signal
/// AND in the Feed (slot-keyed sync below). Overflow follows
/// DropOldest (ring) and is counted on the status line.
const WINDOW: usize = 400;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("feed: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }
    // Engine screen-text selection (0270 tier 3), always-on: left-drag
    // has no other meaning in this app, so select mode simply stays
    // enabled. Drag → highlight, release (or c/Enter mid-drag) → OSC 52
    // copy AND clear (every copy ends the gesture — 0290: keys route to
    // the app again immediately); Esc cancels without copying; wheel
    // scroll routes normally throughout.
    selection().set_enabled(true);

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

        // The Feed: keyed rich items, windowed paint. The bounded
        // window syncs into slot keys ("slot-0".."slot-399"), so the
        // Feed holds at most WINDOW items — bounded end to end.
        // (FeedState::clear() + re-push is the simpler sync; slot-keyed
        // replace re-typesets only the slots whose content changed.)
        let feed = FeedState::new(cx);
        let feed_sync = feed.clone();
        cx.effect_labeled("feed-window-sync", move || {
            events.with(|rows| {
                for (i, line) in rows.iter().enumerate() {
                    // Alerts render as real markdown items; plain rows
                    // as text. (Rows may wrap at narrow widths; Feed's
                    // total_rows stays truthful either way.)
                    let item = match line.split_once("[alert] ") {
                        Some((head, rest)) => FeedItem::markdown(format!("{head}**ALERT** {rest}")),
                        None => FeedItem::text(line.clone()),
                    };
                    feed_sync.push(format!("slot-{i}"), item);
                }
            });
        });

        // Reactive mirror of the pause flag (atomics are not reactive).
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

        // The engine's follow-tail (Scroll::follow_tail): app-visible
        // both ways — we render it and force it true on 'f'. External
        // offset signal so a theme-switch rebuild keeps the position.
        let oy = cx.signal(0i32);
        let follow = cx.signal(true);

        let theme = use_theme(cx);
        let (stop_k, paused_k) = (stop_ui.clone(), paused_ui.clone());
        let feed_view = feed.clone();
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
                follow.set(true); // jump to latest, stick again
            })
            // Rebuilds on THEME switch only — appends never remount:
            // the content extent is MEASURED (Feed's content-sized
            // mode answers the solver reactively; no size hint).
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("live feed (bounded, drop-oldest)")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(
                        Scroll::new(Feed::new(&feed_view).gap(0).view(cx))
                            .offset_y(oy)
                            .follow_tail(follow)
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
                // Honesty rule: dropped (and fold panics, if an app's
                // coalesce fold ever bugs out) render the moment they
                // are nonzero — the user should know the window lost
                // events.
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
                " space pause · f follow tail · wheel scroll · drag to select, c to copy · q quit",
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
            let line = if next() % 23 == 0 {
                // The occasional alert exercises Feed's markdown items.
                format!("[alert] latency spike on shard {} (p99 41ms)", next() % 8)
            } else {
                SAMPLES[(next() % SAMPLES.len() as u64) as usize].to_string()
            };
            tx.send(format!(
                "{:>8.2}s  #{seq:05}  {line}",
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
