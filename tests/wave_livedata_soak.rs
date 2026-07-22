//! LIVEDATA wave, the MEASURED binary: endurance soak + flood
//! throughput, with a counting global allocator (the alloc_budget.rs
//! per-thread pattern) confined to THIS binary — functional pins live
//! in tests/wave_livedata.rs without the counting tax.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as StdCell;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style;
use abstracttui::reactive::create_root;
use abstracttui::reactive::{self, bounded_source, drain_posted, interval, OverflowPolicy};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::{Feed, FeedItem, FeedState};

// ---------------------------------------------------------------------------
// Counting allocator (the alloc_budget.rs pattern, confined to this test
// binary): per-thread counters, so parallel sibling tests never pollute
// a measured region — the soak asserts allocation PLATEAUS on the UI
// thread. Forwards to System; counts are optimization-independent.
// ---------------------------------------------------------------------------

thread_local! {
    static TL_ALLOCS: StdCell<u64> = const { StdCell::new(0) };
    static TL_BYTES: StdCell<u64> = const { StdCell::new(0) };
}

struct CountingAlloc;

impl CountingAlloc {
    /// (allocs, bytes-requested) on the CALLING thread.
    fn snapshot() -> (u64, u64) {
        (
            TL_ALLOCS.try_with(StdCell::get).unwrap_or(0),
            TL_BYTES.try_with(StdCell::get).unwrap_or(0),
        )
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ = TL_BYTES.try_with(|c| c.set(c.get() + layout.size() as u64));
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = TL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ =
            TL_BYTES.try_with(|c| c.set(c.get() + new_size.saturating_sub(layout.size()) as u64));
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn fixed_caps() -> Capabilities {
    // ADR-0003: `Capabilities` is `#[non_exhaustive]`; construct via `with`.
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.sync_output_2026 = true;
    })
}

fn config() -> RunConfig {
    RunConfig {
        caps: Some(fixed_caps()),
        enter: None,
        probe: false,
    }
}

/// adv_app's settle: drive turns until idle, bounded, count renders.
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) -> usize {
    let mut renders = 0;
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.rendered {
            renders += 1;
        }
        if turn.idle {
            return renders;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

// ---------------------------------------------------------------------------
// Endurance soak (cycle-2 hardening): 60 virtual seconds of bursty
// producer → bounded lane → Feed (the example's exact shape), on the
// driver's injected clock. Asserts: allocation plateau on the UI
// thread, live-node plateau (leak detector), Feed/window bounded, one
// frame per burst, byte-free idle between bursts, exact accounting,
// interval cadence held (60 fires in 60 virtual seconds).
// ---------------------------------------------------------------------------

#[test]
fn soak_60_virtual_seconds_bursty_producer_through_feed() {
    const WINDOW: usize = 400;
    const CYCLES: usize = 60; // one virtual second each
    let mut term = CaptureTerm::new(Size::new(44, 12));
    let mut app = App::new(Size::new(44, 12));
    let start = Instant::now();
    let mut wiring = None;
    app.mount(|cx| {
        let (tx, events, stats) = bounded_source::<String>(cx, WINDOW, OverflowPolicy::DropOldest);
        // The example's slot-keyed window sync: Feed holds ≤ WINDOW items.
        let feed = FeedState::new(cx);
        let feed_sync = feed.clone();
        cx.effect_labeled("soak-window-sync", move || {
            events.with(|rows| {
                for (i, line) in rows.iter().enumerate() {
                    feed_sync.push(format!("slot-{i}"), FeedItem::text(line.clone()));
                }
            });
        });
        // The example's rate sampler: one fire per virtual second.
        let fires = cx.signal(0u32);
        interval(cx, Duration::from_secs(1), move || {
            fires.update(|f| *f += 1);
        });
        // The example's exact view shape: measured extent (no size
        // hint) + the engine's follow-tail, pinned true for the soak —
        // every cycle must end showing the newest tail.
        let follow = cx.signal(true);
        let oy = cx.signal(0i32);
        wiring = Some((tx, events, stats, feed.clone(), fires));
        Element::new()
            .style(Style::column())
            .child(
                abstracttui::widgets::Scroll::new(Feed::new(&feed).gap(0).view(cx))
                    .offset_y(oy)
                    .follow_tail(follow)
                    .view(cx),
            )
            .child(dyn_view(Style::line(1), move || {
                let s = stats.get();
                text(format!("{} delivered {} dropped", s.delivered, s.dropped))
            }))
            .build()
    })
    .expect("mount");
    let (tx, events, stats, feed, fires) = wiring.expect("wiring");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    let now = std::rc::Rc::new(std::cell::Cell::new(start));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    settle(&mut driver, &mut app, &mut term);

    let mut per_cycle_allocs = Vec::with_capacity(CYCLES);
    let mut per_cycle_bytes = Vec::with_capacity(CYCLES);
    let mut live_nodes_series = Vec::with_capacity(CYCLES);
    let mut sent_total = 0u64;
    for cycle in 0..CYCLES {
        // Alternating burst sizes: 250 fits the window, 650 overflows
        // transit (exact, deterministic drop accounting).
        let burst = if cycle % 2 == 0 { 250usize } else { 650 };
        sent_total += burst as u64;
        let sender = tx.clone();
        std::thread::spawn(move || {
            for n in 0..burst {
                sender.send(format!("c{cycle:02} e{n:03} payload"));
            }
        })
        .join()
        .expect("producer");

        // One virtual second passes; the burst + the interval fire land
        // in ONE turn's phase U → at most one rendered frame... plus a
        // possible second frame for Feed's width fixup (`after(0)`,
        // CONTENT's RT1-2 discipline) — allowed, then idle. Cycle
        // clocks sit at k·1s + 500ms: the interval's FIRST deadline is
        // real-creation-time + 1s ≈ start + ε + 1s, so exact-boundary
        // clocks would deterministically miss fire 1 by ε; the
        // half-period offset absorbs it, and fixed-delay re-arms keep
        // every later fire on the same offset grid.
        now.set(start + Duration::from_millis((cycle as u64 + 1) * 1000 + 500));
        let a0 = CountingAlloc::snapshot();
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.rendered, "cycle {cycle}: the burst must repaint");
        settle(&mut driver, &mut app, &mut term);
        // Byte-free idle between bursts (quiet producer, armed interval).
        let baseline = term.take_bytes();
        assert!(
            !baseline.is_empty(),
            "cycle {cycle}: frame reached the term"
        );
        for i in 0..4 {
            let turn = driver.turn(&mut app, &mut term).expect("idle turn");
            assert!(turn.idle, "cycle {cycle} idle turn {i} not idle");
        }
        assert_eq!(term.bytes().len(), 0, "cycle {cycle}: idle turns wrote");
        let a1 = CountingAlloc::snapshot();
        per_cycle_allocs.push(a1.0 - a0.0);
        per_cycle_bytes.push(a1.1 - a0.1);
        live_nodes_series.push(reactive::stats().live_nodes);

        // Bounded end to end, every cycle.
        assert!(events.with_untracked(|v| v.len()) <= WINDOW);
        assert!(feed.len() <= WINDOW, "Feed grew past the window");
        // Follow-tail holds under endurance: the newest event is on
        // screen every cycle — including the full-window steady state
        // where a drain REPLACES content without changing the extent.
        let newest = format!("c{cycle:02} e{:03}", burst - 1);
        assert!(
            term.screen().to_text().contains(&newest),
            "cycle {cycle}: tail not followed (expected {newest}):\n{}",
            term.screen().to_text()
        );
    }

    // Exact accounting across the whole soak: 30×250 admitted whole,
    // 30×650 admitted 400 + dropped 250 (transit ring, drained fully
    // each cycle).
    let s = stats.get_untracked();
    assert_eq!(s.delivered + s.dropped, sent_total, "no value unaccounted");
    assert_eq!(s.dropped, 30 * 250, "transit drops exact");
    assert_eq!(s.coalesced, 0);
    assert_eq!(s.fold_panics, 0);
    assert_eq!(
        fires.get_untracked(),
        CYCLES as u32,
        "interval cadence must hold across the soak"
    );

    // Plateau: once the window is full (cycle 1 for 650-bursts, cycle 2
    // for the ring to slide on both parities), steady-state cycles must
    // not trend upward — compare late-half averages against the early
    // half (superlinear growth = a leak in the drain/sync/render path).
    let steady = &per_cycle_allocs[4..];
    let half = steady.len() / 2;
    let early: u64 = steady[..half].iter().sum::<u64>() / half as u64;
    let late: u64 = steady[half..].iter().sum::<u64>() / (steady.len() - half) as u64;
    assert!(
        late <= early * 3 / 2,
        "UI-thread allocation grew across the soak: early {early}/cycle, late {late}/cycle \
         (series {per_cycle_allocs:?})"
    );
    let earlyb: u64 = per_cycle_bytes[4..][..half].iter().sum::<u64>() / half as u64;
    let lateb: u64 =
        per_cycle_bytes[4..][half..].iter().sum::<u64>() / (steady.len() - half) as u64;
    assert!(
        lateb <= earlyb * 3 / 2,
        "UI-thread bytes grew across the soak: early {earlyb}/cycle, late {lateb}/cycle"
    );
    // Live reactive nodes: an exact plateau once steady (node churn in
    // dyn rebuilds must free what it mints — the leak detector).
    let plateau = live_nodes_series[4];
    assert!(
        live_nodes_series[4..].iter().all(|&n| n == plateau),
        "live nodes drifted: {live_nodes_series:?}"
    );
    println!(
        "soak: {CYCLES} virtual seconds, {sent_total} sent, {} delivered, {} dropped; \
         steady-state {early} allocs/cycle ({earlyb} B) early vs {late} ({lateb} B) late; \
         live nodes {plateau}",
        s.delivered, s.dropped
    );
}

// ---------------------------------------------------------------------------
// Flood: 100k posts, bounded memory, exact accounting, throughput.
// ---------------------------------------------------------------------------

#[test]
fn flood_100k_posts_stays_bounded_with_exact_accounting() {
    const THREADS: usize = 4;
    const PER_THREAD: usize = 25_000;
    const CAPACITY: usize = 1024;
    let (root, ()) = create_root(|cx| {
        let (tx, events, stats) = bounded_source::<u64>(cx, CAPACITY, OverflowPolicy::DropOldest);
        let barrier = Arc::new(Barrier::new(THREADS));
        let started = Instant::now();
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let tx = tx.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for n in 0..PER_THREAD {
                        tx.send((t * PER_THREAD + n) as u64);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("flood thread");
        }
        let send_elapsed = started.elapsed();

        let drained = Instant::now();
        assert_eq!(drain_posted(), 1, "one drain job for a 100k flood");
        let drain_elapsed = drained.elapsed();

        let total = (THREADS * PER_THREAD) as u64;
        let s = stats.get_untracked();
        assert_eq!(
            events.with_untracked(|v| v.len()),
            CAPACITY,
            "memory bounded"
        );
        assert_eq!(s.delivered, CAPACITY as u64, "window admissions exact");
        assert_eq!(s.dropped, total - CAPACITY as u64, "drop count exact");
        assert_eq!(
            s.delivered + s.dropped + s.coalesced,
            total,
            "no value unaccounted"
        );

        let throughput = total as f64 / send_elapsed.as_secs_f64();
        println!(
            "flood: {total} posts from {THREADS} threads in {send_elapsed:?} \
             ({throughput:.0}/s), drain {drain_elapsed:?}, window {CAPACITY}, \
             dropped {} (counted, labeled)",
            s.dropped
        );
        // Loose sanity floor only — CI boxes vary wildly; the regression
        // this guards is catastrophic (per-send syscalls, lock storms).
        assert!(
            throughput > 50_000.0,
            "flood throughput collapsed: {throughput:.0}/s"
        );
    });
    root.dispose();
}
