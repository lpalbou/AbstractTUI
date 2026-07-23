//! Wave-3 INPUTAV acceptance: key press/release state (games/0700),
//! push-to-talk (media-av/0610), and the meter idle law (media-av/0620)
//! driven through the REAL driver against scripted terminals — kitty
//! wire bytes in, state/frames/bytes out.
//!
//! OWNER: INPUTAV.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::{
    key_state, use_key_state, App, CaptureState, Driver, KeyFidelity, PushToTalk, RunConfig,
    StopReason,
};
use abstracttui::prelude::*;
use abstracttui::reactive::interval;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;

/// Kitty-class capabilities: releases reach the wire (fidelity Full).
fn kitty_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_keyboard = true;
    })
}

/// A driver over a scripted terminal with an injected, steppable clock.
fn rig(app: &mut App, caps: Capabilities) -> (Driver, CaptureTerm, Rc<Cell<Instant>>) {
    let size = app.viewport();
    let mut term = CaptureTerm::new(size);
    let cfg = RunConfig {
        caps: Some(caps),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(app, &mut term, cfg).expect("driver");
    let now = Rc::new(Cell::new(Instant::now()));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    (driver, term, now)
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// games/0700 — the key-state service through the driver
// ---------------------------------------------------------------------------

#[test]
fn kitty_wire_tracks_chords_and_releases_through_the_driver() {
    let mut app = App::new(Size::new(30, 4));
    app.mount(|cx| {
        let _ = use_key_state(cx); // arm the service
        Element::new().child(text("keys")).build()
    })
    .expect("mount");
    let (mut driver, mut term, _) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);
    let ks = key_state();
    assert_eq!(ks.fidelity_untracked(), KeyFidelity::Full);

    // Up+Right chord: both presses land in one burst.
    term.push_input(b"\x1b[119u\x1b[100u"); // w down, d down
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(
        ks.is_down(Key::Char('w')) && ks.is_down(Key::Char('d')),
        "chord"
    );
    assert!(ks.pressed(Key::Char('w')), "press edges visible this turn");

    // Next turn: edges seal, holds persist.
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(!ks.pressed(Key::Char('w')), "edges sealed");
    assert!(ks.is_down(Key::Char('w')), "hold persists");

    // Release one key of the chord.
    term.push_input(b"\x1b[100;1:3u"); // d up
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(ks.is_down(Key::Char('w')) && !ks.is_down(Key::Char('d')));
    assert!(ks.released(Key::Char('d')));

    // FocusLost clears the rest, labeled.
    term.push_input(b"\x1b[O");
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(!ks.any_down(), "focus loss empties the down-set");
    assert!(ks.released(Key::Char('w')) && ks.focus_cleared());
}

#[test]
fn legacy_wire_reports_degraded_and_never_fakes_holds() {
    let mut app = App::new(Size::new(30, 4));
    app.mount(|cx| {
        let _ = use_key_state(cx);
        Element::new().child(text("keys")).build()
    })
    .expect("mount");
    let (mut driver, mut term, _) = rig(&mut app, Capabilities::default());
    settle(&mut driver, &mut app, &mut term);
    let ks = key_state();
    assert_eq!(ks.fidelity_untracked(), KeyFidelity::Degraded);

    term.push_input(b"w");
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(ks.pressed(Key::Char('w')), "press edges stay honest");
    assert!(!ks.is_down(Key::Char('w')), "Degraded never claims held");
}

// ---------------------------------------------------------------------------
// media-av/0610 — push-to-talk end to end
// ---------------------------------------------------------------------------

/// Mount a PTT app; returns (state signal reader via key_state) through
/// the shared log the callbacks write.
type Log = Rc<std::cell::RefCell<Vec<String>>>;

fn mount_ptt(app: &mut App) -> (Log, Rc<Cell<CaptureState>>) {
    let log: Log = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = Rc::new(Cell::new(CaptureState::Idle));
    let (l1, l2) = (log.clone(), log.clone());
    let seen_in = seen.clone();
    app.mount(move |cx| {
        let ptt = PushToTalk::bind(cx, KeyChord::plain(Key::Char(' ')))
            .on_start(move || l1.borrow_mut().push("start".into()))
            .on_stop(move |r| l2.borrow_mut().push(format!("stop:{r:?}")));
        let state = ptt.state();
        cx.effect(move || seen_in.set(state.get()));
        Element::new().child(text("ptt")).build()
    })
    .expect("mount");
    (log, seen)
}

#[test]
fn ptt_hold_mode_over_kitty_bytes() {
    let mut app = App::new(Size::new(30, 4));
    let (log, seen) = mount_ptt(&mut app);
    let (mut driver, mut term, _) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[32u"); // Space down
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Held, "down -> capturing");

    // Auto-repeats while held change nothing.
    term.push_input(b"\x1b[32;1:2u");
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Held);

    term.push_input(b"\x1b[32;1:3u"); // Space up
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Idle, "up -> stopped");
    assert_eq!(log.borrow().as_slice(), ["start", "stop:Released"]);
}

#[test]
fn ptt_focus_loss_stops_capture_mid_hold() {
    let mut app = App::new(Size::new(30, 4));
    let (log, seen) = mount_ptt(&mut app);
    let (mut driver, mut term, _) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[32u");
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Held);
    term.push_input(b"\x1b[O"); // focus out, key still physically down
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(
        seen.get(),
        CaptureState::Idle,
        "mic privacy: stop on focus loss"
    );
    assert_eq!(
        log.borrow().last().unwrap(),
        &format!("stop:{:?}", StopReason::FocusLost)
    );
}

/// Cycle-2 review I-2: `Driver::suspend` owns the key-state hygiene
/// the raw `Terminal::suspend` cannot — a key held INTO the stop is
/// drained toward not-held before the process stops (releases during
/// a stop are unobservable; Ctrl+Z keeps focus, so no FocusLost ever
/// covers it), the PTT capture stops with the truthful reason, and
/// the resume path re-presents the unknown screen.
#[test]
fn driver_suspend_drains_holds_stops_ptt_and_represents() {
    let mut app = App::new(Size::new(30, 4));
    let (log, seen) = mount_ptt(&mut app);
    let (mut driver, mut term, _) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);
    let ks = key_state();

    term.push_input(b"\x1b[32u"); // Space down, HELD
    driver.turn(&mut app, &mut term).expect("press turn");
    assert_eq!(seen.get(), CaptureState::Held, "precondition: capturing");
    assert!(ks.is_down(Key::Char(' ')), "precondition: held");
    let _ = term.take_bytes();

    // The app's Ctrl+Z: the whole orchestration in one call.
    driver
        .suspend(&mut app, &mut term)
        .expect("suspend round trip");
    assert_eq!(term.suspend_count(), 1, "the terminal round-tripped");
    assert!(
        !ks.is_down(Key::Char(' ')),
        "resumed state reads not-held (releases during a stop are \
         unobservable)"
    );
    assert_eq!(
        seen.get(),
        CaptureState::Idle,
        "capture stopped BEFORE the stop signal"
    );
    assert_eq!(
        log.borrow().last().unwrap(),
        &format!("stop:{:?}", StopReason::Suspended),
        "the truthful reason: suspended, not released/focus-lost"
    );

    // Resume re-presents: the next turn renders and emits the whole
    // frame (the alt screen came back blank).
    let turn = driver.turn(&mut app, &mut term).expect("resume turn");
    assert!(turn.rendered && turn.emitted, "full re-present: {turn:?}");
    assert!(
        term.screen().to_text().contains("ptt"),
        "content restored on the resumed screen"
    );

    // The key is STILL physically held: its next auto-repeat re-proves
    // the hold WITHOUT restarting capture (the focus-return rule).
    term.push_input(b"\x1b[32;1:2u");
    driver.turn(&mut app, &mut term).expect("repeat turn");
    assert!(ks.is_down(Key::Char(' ')), "repeat re-proves the hold");
    assert_eq!(
        seen.get(),
        CaptureState::Idle,
        "capture never auto-restarts"
    );
    assert_eq!(log.borrow().len(), 2, "no third callback");
}

#[test]
fn ptt_latch_mode_over_a_legacy_wire() {
    let mut app = App::new(Size::new(30, 4));
    let (log, seen) = mount_ptt(&mut app);
    let (mut driver, mut term, _) = rig(&mut app, Capabilities::default());
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b" ");
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Latched, "press latches on");
    driver.turn(&mut app, &mut term).expect("turn"); // quiet turn between
    term.push_input(b" ");
    driver.turn(&mut app, &mut term).expect("turn");
    assert_eq!(seen.get(), CaptureState::Idle, "press again latches off");
    assert_eq!(log.borrow().as_slice(), ["start", "stop:Released"]);
}

// ---------------------------------------------------------------------------
// media-av/0620 — THE IDLE LAW through the real frame loop
// ---------------------------------------------------------------------------

/// The acceptance the item marks REQUIRED: a meter whose input stops
/// changing decays to its fixpoint, the frame task DROPS, and N further
/// turns are byte-for-byte, frame-for-frame idle. (The allocation half
/// of the same law is pinned in tests/alloc_budget.rs, which owns the
/// counting allocator.)
#[test]
fn meter_reaches_fixpoint_and_turns_go_idle() {
    let mut app = App::new(Size::new(40, 6));
    let level_out: Rc<Cell<Option<Signal<f32>>>> = Rc::new(Cell::new(None));
    let level_slot = level_out.clone();
    app.mount(move |cx| {
        let level = cx.signal(0.0f32);
        level_slot.set(Some(level));
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Meter::new(level)
                    .decay(120.0)
                    .peak_hold(Duration::from_millis(200))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let (mut driver, mut term, now) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);
    let level = level_out.get().expect("level signal");

    // A burst, then silence.
    level.set(0.9);
    driver.turn(&mut app, &mut term).expect("turn");
    level.set(0.0);
    driver.turn(&mut app, &mut term).expect("turn");

    // Decay animates: advance the clock until the fixpoint parks the
    // loop (120 dB/s over 60 dB span = 2 full scales per second; with
    // the 200 ms peak hold everything settles well under a second).
    let mut idle_at = None;
    for i in 0..120 {
        now.set(now.get() + Duration::from_millis(16));
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        if turn.idle {
            idle_at = Some(i);
            break;
        }
    }
    assert!(
        idle_at.is_some(),
        "the meter must settle, not animate forever"
    );

    // Unchanged input from here on: every turn idle, zero frames, zero
    // bytes — the law.
    let _ = term.take_bytes();
    for _ in 0..16 {
        now.set(now.get() + Duration::from_millis(16));
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.idle, "unchanged input must not wake the meter");
        assert!(!turn.rendered, "no frames at the fixpoint");
    }
    assert!(term.bytes().is_empty(), "idle turns wrote bytes");

    // New input re-arms honestly: one rise paints again.
    level.set(0.7);
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.rendered, "fresh input re-arms the meter");
}

// ---------------------------------------------------------------------------
// games lane proof — WASD pan while held (the smallest honest game loop)
// ---------------------------------------------------------------------------

/// Held-key movement on Full fidelity: a 16 ms interval reads the
/// down-set and pans; chords compose (w+d = diagonal), releases stop
/// exactly their axis, focus loss stops everything. This is the
/// games-band consumption shape 0700 exists for.
#[test]
fn wasd_pan_moves_while_held_and_stops_on_release() {
    let mut app = App::new(Size::new(30, 4));
    let pos_out: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));
    let pos_slot = pos_out.clone();
    app.mount(move |cx| {
        let keys = use_key_state(cx);
        let pos = pos_slot.clone();
        interval(cx, Duration::from_millis(16), move || {
            let mut p = pos.get();
            if keys.is_down(Key::Char('w')) {
                p.1 -= 1;
            }
            if keys.is_down(Key::Char('s')) {
                p.1 += 1;
            }
            if keys.is_down(Key::Char('a')) {
                p.0 -= 1;
            }
            if keys.is_down(Key::Char('d')) {
                p.0 += 1;
            }
            pos.set(p);
        });
        Element::new().child(text("pan")).build()
    })
    .expect("mount");
    let (mut driver, mut term, now) = rig(&mut app, kitty_caps());
    settle(&mut driver, &mut app, &mut term);

    // Turn order fact: phase U runs due timers BEFORE dispatching input,
    // so key events are fed on a FROZEN-clock turn (timer not due) and
    // the clock advances afterwards — each tick then sees settled state.
    let dispatch = |driver: &mut Driver, app: &mut App, term: &mut CaptureTerm| {
        driver.turn(app, term).expect("dispatch turn");
    };
    let step = |driver: &mut Driver, app: &mut App, term: &mut CaptureTerm, n: usize| {
        for _ in 0..n {
            now.set(now.get() + Duration::from_millis(16));
            driver.turn(app, term).expect("turn");
        }
    };

    // Hold w+d: diagonal pan, one step per tick.
    term.push_input(b"\x1b[119u\x1b[100u");
    dispatch(&mut driver, &mut app, &mut term);
    step(&mut driver, &mut app, &mut term, 5);
    let (x1, y1) = pos_out.get();
    assert_eq!((x1, y1), (5, -5), "diagonal while both held");

    // Release d: the x axis stops, w keeps panning.
    term.push_input(b"\x1b[100;1:3u");
    dispatch(&mut driver, &mut app, &mut term);
    step(&mut driver, &mut app, &mut term, 3);
    let (x2, y2) = pos_out.get();
    assert_eq!(x2, x1, "released axis stopped");
    assert_eq!(y2, y1 - 3, "held axis continues");

    // Focus loss: everything stops, even though no release ever came.
    term.push_input(b"\x1b[O");
    dispatch(&mut driver, &mut app, &mut term);
    step(&mut driver, &mut app, &mut term, 4);
    assert_eq!(pos_out.get(), (x2, y2), "focus loss stops the pan");
}
