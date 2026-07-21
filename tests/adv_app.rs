//! REDTEAM cycle-2 attack: the REAL frame loop (`app::Driver`, landed
//! late cycle 2) against the damage contract — idle budget, epoch rule,
//! resize handling, session custody — driven headlessly through
//! `CaptureTerm` (never a real tty, never a sleep).

use std::time::Duration;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::{Dimension, Style};
use abstracttui::term::{Capabilities, Terminal};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view, text, Element};

fn fixed_caps() -> Capabilities {
    // Deterministic: host env must never leak into assertions.
    Capabilities {
        truecolor: true,
        colors_256: true,
        sync_output_2026: true,
        hyperlinks: true,
        ..Capabilities::default()
    }
}

fn config() -> RunConfig {
    RunConfig {
        caps: Some(fixed_caps()),
        enter: None,
        probe: false,
    }
}

/// Drive turns until the app settles (no events, no renders), with a
/// hard budget so a livelocked loop fails loudly instead of spinning.
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
// Idle budget: zero bytes, zero renders, zero anything.
// ---------------------------------------------------------------------------

#[test]
fn idle_app_emits_zero_bytes_across_idle_turns() {
    let mut term = CaptureTerm::new(Size::new(30, 6));
    let mut app = App::new(Size::new(30, 6));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(10))
                    .height(Dimension::Cells(1)),
            )
            .child(text("steady"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    let renders = settle(&mut driver, &mut app, &mut term);
    assert!(renders >= 1, "the mount frame must render");
    assert!(!term.bytes().is_empty(), "first frame must have painted");

    // Idle: N turns with no input, no signals — not one byte, not one
    // render, and every turn reports idle.
    let baseline = term.take_bytes().len();
    let flushes = term.flush_count();
    for i in 0..16 {
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.idle, "turn {i} must report idle");
        assert!(!turn.rendered, "turn {i} rendered with no damage");
    }
    assert_eq!(
        term.bytes().len(),
        0,
        "idle turns wrote bytes (baseline {baseline})"
    );
    assert_eq!(term.flush_count(), flushes, "idle turns flushed");
}

// ---------------------------------------------------------------------------
// Epoch rule (§2): a cross-thread post lands in the NEXT frame, once.
// ---------------------------------------------------------------------------

#[test]
fn cross_thread_post_lands_exactly_one_frame_later() {
    let mut term = CaptureTerm::new(Size::new(30, 4));
    let mut app = App::new(Size::new(30, 4));
    let mut counter_handle = None;
    app.mount(|cx| {
        let counter = cx.signal(0u32);
        counter_handle = Some(counter);
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(20))
                    .height(Dimension::Cells(1)),
            )
            .child(dyn_view(Style::default(), move || {
                text(format!("count {}", counter.get()))
            }))
            .build()
    })
    .expect("mount");
    let counter = counter_handle.expect("signal handle");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    assert!(term.screen().to_text().contains("count 0"));
    let _ = term.take_bytes();

    // Post from another thread while NO turn is running — by the phase
    // rules this is indistinguishable from landing mid-phases-L..S: it
    // may only be drained by the next turn's phase U.
    let wake = app.wake_handle();
    let t = std::thread::spawn(move || {
        wake.post(move || counter.set(1));
    });
    t.join().expect("poster thread");

    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(
        turn.rendered,
        "the posted write must repaint on the next turn"
    );
    assert!(
        term.screen().to_text().contains("count 1"),
        "damage was lost: {}",
        term.screen().to_text()
    );
    // Exactly once: the following turn is idle again (no double paint).
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(!turn.rendered, "posted damage painted twice");
    assert!(turn.idle);
}

// ---------------------------------------------------------------------------
// Resize: re-layout + full repaint at the new geometry, mid-stream.
// ---------------------------------------------------------------------------

#[test]
fn resize_between_keys_relayouts_and_repaints() {
    let mut term = CaptureTerm::new(Size::new(24, 4));
    let mut app = App::new(Size::new(24, 4));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
            )
            .child(text("resizable content"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // Key, resize, key — one scripted stream (the resize arrives between
    // dispatches, the mid-dispatch case by construction of phase U).
    term.push_input(b"a");
    term.push_resize(Size::new(40, 8));
    term.push_input(b"b");
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(turn.events, 3, "key + resize + key all dispatched");
    assert!(turn.rendered, "resize must force a repaint");
    assert!(!term.bytes().is_empty(), "the repaint reached the terminal");
    // The terminal's own size is the resize ground truth here; the App
    // getter is RT2-9 (below).
    assert_eq!(term.size().expect("size"), Size::new(40, 8));
    // Loop stays healthy afterwards.
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.idle || turn.rendered);
}

/// RT2-9 (CLOSED cycle 3): `Driver::apply_resize` used to bypass
/// `App::set_viewport`, leaving `App::viewport()` stale after a
/// driver-handled resize. REACT fixed it and lifted the ignore (edit to
/// this REDTEAM-owned file — reviewed, kept; ownership note filed in
/// reviews/cycle3). Permanent acceptance test.
#[test]
fn app_viewport_tracks_driver_resize() {
    let mut term = CaptureTerm::new(Size::new(24, 4));
    let mut app = App::new(Size::new(24, 4));
    app.mount(|_cx| Element::new().build()).expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    term.push_resize(Size::new(40, 8));
    let _ = driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(
        app.viewport(),
        Size::new(40, 8),
        "App::viewport must not lie after a driver-handled resize"
    );
}

// ---------------------------------------------------------------------------
// Session custody: enter/leave bytes through the real loop.
// ---------------------------------------------------------------------------

#[test]
fn driver_session_enter_leave_balance_via_model() {
    let mut term = CaptureTerm::new(Size::new(20, 4));
    let mut app = App::new(Size::new(20, 4));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(5))
                    .height(Dimension::Cells(1)),
            )
            .child(text("hi"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    assert!(
        term.screen().modes().alt_screen(),
        "enter must switch to alt screen"
    );
    settle(&mut driver, &mut app, &mut term);
    driver.finish(&mut term).expect("leave");
    let screen = term.screen();
    assert!(
        !screen.modes().alt_screen(),
        "leave must restore the main screen"
    );
    assert!(
        screen.modes().cursor_visible(),
        "leave must restore the cursor"
    );
    assert_eq!(
        screen.counters().kitty_push_depth,
        0,
        "kitty flags balanced"
    );
    // Leave carries a DEFENSIVE `?2026l`, so ends may exceed begins by
    // exactly the teardown reset; what must never happen is a frame
    // leaving the bracket OPEN.
    assert!(
        screen.counters().sync_ends >= screen.counters().sync_begins,
        "a 2026 bracket was left open"
    );
    assert!(
        !screen.modes().synchronized_output(),
        "2026 must be off after leave"
    );
    assert_eq!(
        screen.unknown_seq_count(),
        0,
        "the whole session was modeled traffic"
    );
}

// ---------------------------------------------------------------------------
// Quit paths: Ctrl+C default.
// ---------------------------------------------------------------------------

#[test]
fn ctrl_c_quits_by_default() {
    let mut term = CaptureTerm::new(Size::new(20, 4));
    let mut app = App::new(Size::new(20, 4));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(5))
                    .height(Dimension::Cells(1)),
            )
            .child(text("hi"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x03"); // raw-mode Ctrl+C is a byte, never a signal
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(
        turn.quit,
        "Ctrl+C must request quit (ISIG is off in raw mode)"
    );
}

// ---------------------------------------------------------------------------
// Worker-death surfacing (RT1-15b follow-through).
// ---------------------------------------------------------------------------

#[test]
fn spawned_worker_panic_surfaces_as_app_error() {
    let mut term = CaptureTerm::new(Size::new(20, 4));
    let mut app = App::new(Size::new(20, 4));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(5))
                    .height(Dimension::Cells(1)),
            )
            .child(text("hi"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let handle = abstracttui::reactive::spawn_worker("doomed", || {
        panic!("worker exploded");
    });
    handle.join().ok(); // the panic is CAUGHT worker-side; join is clean
                        // The failure report arrives as a posted job; the next turns must
                        // surface it as a labeled app error — bounded, never a sleep.
    for _ in 0..1000 {
        match driver.turn(&mut app, &mut term) {
            Ok(_) => std::thread::yield_now(),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("doomed") || msg.contains("worker") || msg.contains("exploded"),
                    "worker death must be named in the error: {msg}"
                );
                return;
            }
        }
    }
    panic!("worker panic never surfaced as an app error");
}

// ---------------------------------------------------------------------------
// Probe upgrade path: replies fold WITHOUT blocking the first paint.
// ---------------------------------------------------------------------------

#[test]
fn probe_replies_upgrade_caps_after_first_paint() {
    let mut term = CaptureTerm::new(Size::new(20, 4));
    let mut app = App::new(Size::new(20, 4));
    app.mount(|_cx| {
        Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(5))
                    .height(Dimension::Cells(1)),
            )
            .child(text("hi"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: true,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    // First paint happens BEFORE any probe reply exists (RT1-6a).
    let renders = settle(&mut driver, &mut app, &mut term);
    assert!(renders >= 1, "first paint must not wait for the probe");
    assert!(!driver.caps().sixel);
    // Late replies arrive; the loop folds them as ordinary events.
    term.push_input(b"\x1b[?62;4c"); // DA1 sentinel with sixel attribute
    let _ = driver.turn(&mut app, &mut term).expect("turn");
    assert!(
        driver.caps().sixel,
        "probe reply must upgrade caps mid-session"
    );
    // A caps upgrade may repaint (color depth change); either way the
    // loop settles again.
    settle(&mut driver, &mut app, &mut term);
}

/// Deadline sanity: Driver::turn never blocks on a CaptureTerm (the
/// idle-storm guard in the rig would panic if it polled unboundedly).
#[test]
fn turn_is_truly_non_blocking() {
    let mut term = CaptureTerm::new(Size::new(20, 4));
    let mut app = App::new(Size::new(20, 4));
    app.mount(|_cx| Element::new().build()).expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    // 1000 turns on an exhausted script: must return (idle), not spin
    // past the rig's storm limit per call.
    for _ in 0..1000 {
        let _ = driver.turn(&mut app, &mut term).expect("turn");
    }
    let _ = Duration::ZERO; // (import used by future timing additions)
}
