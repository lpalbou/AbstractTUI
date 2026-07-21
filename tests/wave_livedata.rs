//! LIVEDATA wave: the async-source → Signal binding (0010), bounded
//! coalescing ingestion (0020), the live-feed shape (0030) and the
//! interval time source (0070), driven through the REAL frame loop
//! (`Driver::turn` against `CaptureTerm` — never a tty, never a sleep
//! on the assertion path).
//!
//! Pins, by spec:
//! - ordered delivery under concurrent senders;
//! - disposal safety (send after scope death = inert + counted);
//! - every overflow policy's exact accounting;
//! - burst → ONE wake (engine dedup) and ONE posted drain (helper
//!   dedup) and ONE frame (damage contract §2);
//! - zero-idle-cost with a live-but-quiet source (adv_app:55 shape);
//! - interval missed-tick coalescing + cancellation, through the
//!   driver's injected clock;
//! - a 100k-post flood: bounded memory, exact drop accounting,
//!   measured throughput (printed; loose sanity floor for CI).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style;
use abstracttui::reactive::{
    self, bounded_source, channel_source, create_root, drain_posted, interval, run_due_timers,
    set_wake_callback, IngestStats, OverflowPolicy,
};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view, text, Element};

fn fixed_caps() -> Capabilities {
    // ADR-0003: `Capabilities` is `#[non_exhaustive]`; this file compiles
    // as a downstream crate, so construction goes through `with`.
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
// 0010: ordering + disposal at the binding level.
// ---------------------------------------------------------------------------

#[test]
fn concurrent_senders_each_keep_emit_order() {
    let (root, ()) = create_root(|cx| {
        let (tx, events) = channel_source::<(usize, u32)>(cx);
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|id| {
                let tx = tx.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    for n in 0..2000u32 {
                        tx.send((id, n));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("sender thread");
        }
        drain_posted();
        events.with_untracked(|got| {
            assert_eq!(got.len(), 8000);
            let mut next = [0u32; 4];
            for &(id, n) in got.iter() {
                assert_eq!(n, next[id], "sender {id} delivered out of order");
                next[id] += 1;
            }
        });
    });
    root.dispose();
}

#[test]
fn sender_outliving_its_scope_is_inert_and_counted() {
    let mut escaped = None;
    let (root, ()) = create_root(|cx| {
        let pane = cx.child(); // a closable pane's scope
        let (tx, events, stats) = bounded_source::<u32>(pane, 16, OverflowPolicy::DropOldest);
        tx.send(1);
        drain_posted();
        assert_eq!(events.get_untracked(), vec![1]);
        pane.dispose();
        escaped = Some((tx, events, stats));
    });
    let (tx, events, stats) = escaped.expect("handles");
    for n in 0..64u32 {
        tx.send(n); // long after the pane closed
    }
    drain_posted();
    assert!(!events.is_alive() && !stats.is_alive());
    assert_eq!(
        events.try_get_untracked(),
        None,
        "gone is an answer, not UB"
    );
    assert_eq!(
        tx.dead_sends(),
        16,
        "the bounded batch that reached the drain"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// 0020: policies, exact accounting, wake dedup.
// ---------------------------------------------------------------------------

#[test]
fn each_policy_accounts_exactly() {
    let (root, ()) = create_root(|cx| {
        // DropOldest: newest tail survives, evictions counted.
        let (tx, ev, st) = bounded_source(cx, 4, OverflowPolicy::DropOldest);
        (0..10u32).for_each(|n| tx.send(n));
        drain_posted();
        assert_eq!(ev.get_untracked(), vec![6, 7, 8, 9]);
        assert_eq!(
            st.get_untracked(),
            IngestStats {
                delivered: 4,
                dropped: 6,
                coalesced: 0
            }
        );

        // DropNewest: head survives, refusals counted.
        let (tx, ev, st) = bounded_source(cx, 4, OverflowPolicy::DropNewest);
        (0..10u32).for_each(|n| tx.send(n));
        drain_posted();
        assert_eq!(ev.get_untracked(), vec![0, 1, 2, 3]);
        assert_eq!(
            st.get_untracked(),
            IngestStats {
                delivered: 4,
                dropped: 6,
                coalesced: 0
            }
        );

        // Coalesce: overflow merges into the newest survivor.
        let (tx, ev, st) = bounded_source(
            cx,
            4,
            OverflowPolicy::coalesce(|kept: &mut u32, new| *kept = new),
        );
        (0..10u32).for_each(|n| tx.send(n));
        drain_posted();
        assert_eq!(ev.get_untracked(), vec![0, 1, 2, 9], "last writer wins");
        assert_eq!(
            st.get_untracked(),
            IngestStats {
                delivered: 4,
                dropped: 0,
                coalesced: 6
            }
        );
    });
    root.dispose();
}

#[test]
fn burst_costs_one_wake_and_one_drain() {
    // Engine-level dedup: N raw posts between drains = ONE waker call.
    let wakes = Arc::new(AtomicUsize::new(0));
    let (root, ()) = create_root(|cx| {
        let w2 = wakes.clone();
        set_wake_callback(move || {
            w2.fetch_add(1, Ordering::SeqCst);
        });
        let _ = drain_posted(); // clear any leftover flag
        wakes.store(0, Ordering::SeqCst);

        let (tx, events) = channel_source::<u32>(cx);
        std::thread::spawn(move || {
            for n in 0..500 {
                tx.send(n);
            }
        })
        .join()
        .expect("producer");
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "500 posts must invoke the waker once (dedup ratio 500:1)"
        );
        assert_eq!(drain_posted(), 500, "every posted apply still ran");
        assert_eq!(events.with_untracked(|v| v.len()), 500);

        // Helper-level dedup: the bounded lane posts ONE drain closure
        // for the whole burst on top of the engine's one wake.
        wakes.store(0, Ordering::SeqCst);
        let (tx, events, _) = bounded_source(cx, 2048, OverflowPolicy::DropOldest);
        std::thread::spawn(move || {
            for n in 0..1000u32 {
                tx.send(n);
            }
        })
        .join()
        .expect("producer");
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "one wake for the burst");
        assert_eq!(drain_posted(), 1, "one posted job for 1000 sends");
        assert_eq!(events.with_untracked(|v| v.len()), 1000);
        set_wake_callback(|| {});
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Damage contract through the REAL loop: burst → one frame; quiet
// source → byte-for-byte idle; clean worker teardown.
// ---------------------------------------------------------------------------

#[test]
fn feed_burst_renders_one_frame_and_quiet_source_is_byte_free() {
    let mut term = CaptureTerm::new(Size::new(40, 6));
    let mut app = App::new(Size::new(40, 6));
    let mut wiring = None;
    app.mount(|cx| {
        let (tx, events, stats) = bounded_source::<String>(cx, 64, OverflowPolicy::DropOldest);
        wiring = Some((tx, events, stats));
        Element::new()
            .style(
                Style::default()
                    .width(abstracttui::layout::Dimension::Cells(38))
                    .height(abstracttui::layout::Dimension::Cells(2)),
            )
            .child(dyn_view(Style::default(), move || {
                let (n, dropped) = (events.with(|v| v.len()), stats.with(|s| s.dropped));
                text(format!("events {n} dropped {dropped}"))
            }))
            .build()
    })
    .expect("mount");
    let (tx, _, _) = wiring.expect("wiring");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    assert!(term.screen().to_text().contains("events 0"));
    let _ = term.take_bytes();

    // A burst lands between turns (indistinguishable from mid-frame by
    // the phase rules): the NEXT turn renders it, exactly once.
    let sender = tx.clone();
    std::thread::spawn(move || {
        for n in 0..50 {
            sender.send(format!("event {n}"));
        }
    })
    .join()
    .expect("producer");
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.rendered, "the burst must repaint on the next turn");
    assert!(term.screen().to_text().contains("events 50"));
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(!turn.rendered, "burst painted twice");
    assert!(turn.idle);

    // Live-but-quiet source (sender alive, worker silent): the idle
    // budget is untouched — zero bytes, zero renders, zero flushes.
    let _ = term.take_bytes();
    let flushes = term.flush_count();
    for i in 0..16 {
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.idle, "turn {i} must report idle with a quiet source");
        assert!(!turn.rendered);
    }
    assert_eq!(term.bytes().len(), 0, "quiet source must cost zero bytes");
    assert_eq!(term.flush_count(), flushes, "quiet source must not flush");
    drop(tx); // sender teardown is undramatic
}

#[test]
fn worker_quits_cleanly_without_surfacing_a_failure() {
    let mut term = CaptureTerm::new(Size::new(30, 4));
    let mut app = App::new(Size::new(30, 4));
    let stop = Arc::new(AtomicBool::new(false));
    let mut wiring = None;
    app.mount(|cx| {
        let (tx, events) = channel_source::<u32>(cx);
        wiring = Some(tx);
        Element::new()
            .style(
                Style::default()
                    .width(abstracttui::layout::Dimension::Cells(20))
                    .height(abstracttui::layout::Dimension::Cells(1)),
            )
            .child(dyn_view(Style::default(), move || {
                text(format!("got {}", events.with(|v| v.len())))
            }))
            .build()
    })
    .expect("mount");
    let tx = wiring.expect("sender");
    let stop2 = stop.clone();
    let worker = reactive::spawn_worker("feed", move || {
        let mut n = 0;
        while !stop2.load(Ordering::Relaxed) {
            tx.send(n);
            n += 1;
            if n >= 3 {
                break; // a short honest life
            }
        }
    });
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    worker.join().expect("worker exits cleanly");
    stop.store(true, Ordering::Relaxed);
    // Every subsequent turn must succeed: a clean worker exit is not a
    // failure (spawn_worker only reports PANICS).
    for _ in 0..8 {
        driver.turn(&mut app, &mut term).expect("clean turns");
    }
    assert!(term.screen().to_text().contains("got 3"));
}

// ---------------------------------------------------------------------------
// 0070: interval through the driver's injected clock.
// ---------------------------------------------------------------------------

#[test]
fn interval_ticks_render_and_missed_ticks_coalesce_through_the_driver() {
    let mut term = CaptureTerm::new(Size::new(30, 4));
    let mut app = App::new(Size::new(30, 4));
    let start = Instant::now();
    let mut wiring = None;
    app.mount(|cx| {
        let ticks = cx.signal(0u32);
        let handle = interval(cx, Duration::from_millis(100), move || {
            ticks.update(|t| *t += 1);
        });
        wiring = Some(handle);
        Element::new()
            .style(
                Style::default()
                    .width(abstracttui::layout::Dimension::Cells(20))
                    .height(abstracttui::layout::Dimension::Cells(1)),
            )
            .child(dyn_view(Style::default(), move || {
                text(format!("tick {}", ticks.get()))
            }))
            .build()
    })
    .expect("mount");
    let handle = wiring.expect("interval handle");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    // Injected clock: tests own time (Driver::set_clock is the seam).
    let now = std::rc::Rc::new(std::cell::Cell::new(start));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    settle(&mut driver, &mut app, &mut term);
    assert!(term.screen().to_text().contains("tick 0"));

    // Armed but not due: byte-for-byte idle (zero-idle-cost with a
    // pending interval — the timer bounds the SLEEP, not the frames).
    let _ = term.take_bytes();
    now.set(start + Duration::from_millis(50));
    for _ in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.idle, "not-due interval must not break idle");
    }
    assert_eq!(
        term.bytes().len(),
        0,
        "armed interval cost bytes while idle"
    );

    // Due: one fire, one frame.
    now.set(start + Duration::from_millis(160));
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.rendered);
    assert!(term.screen().to_text().contains("tick 1"));

    // A whole suspend's worth of missed periods: ONE coalesced fire.
    now.set(start + Duration::from_millis(2000));
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.rendered);
    assert!(
        term.screen().to_text().contains("tick 2"),
        "missed ticks must coalesce, screen: {}",
        term.screen().to_text()
    );
    settle(&mut driver, &mut app, &mut term);

    // Cancel: no further fires, no timer bound left behind.
    handle.cancel();
    now.set(start + Duration::from_millis(60_000));
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(!turn.rendered, "cancelled interval must never fire again");
    assert!(turn.idle);
    assert_eq!(
        reactive::next_timer_deadline(),
        None,
        "cancel must remove the pending deadline entirely"
    );
}

#[test]
fn interval_rearm_uses_the_fire_clock_not_wall_time() {
    // Pure reactive-level check that the re-arm rides run_due_timers'
    // clock: with synthetic time far from wall time, cadence holds.
    let (root, ()) = create_root(|cx| {
        let fires = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let f2 = fires.clone();
        let _h = interval(cx, Duration::from_millis(100), move || {
            f2.set(f2.get() + 1);
        });
        let t0 = Instant::now();
        assert_eq!(run_due_timers(t0 + Duration::from_millis(110)), 1);
        // Jump an hour ahead: one coalesced fire, then steady cadence
        // anchored at the synthetic hour, far from any wall clock.
        let hour = t0 + Duration::from_secs(3600);
        assert_eq!(run_due_timers(hour), 1);
        assert_eq!(run_due_timers(hour + Duration::from_millis(50)), 0);
        assert_eq!(run_due_timers(hour + Duration::from_millis(110)), 1);
        assert_eq!(fires.get(), 3);
    });
    root.dispose();
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
