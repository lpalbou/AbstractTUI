//! Meter tests: virtual-clock ballistics (attack/decay/peak-hold, dB
//! mapping), render pins (zones, peak tick, band bars), and THE IDLE
//! LAW — a settled meter's frame task drops and never bills another
//! frame (the driver-level twin lives in tests/wave_inputav.rs, the
//! allocation half in tests/alloc_budget.rs).

use super::*;
use crate::base::{Point, Size};
use crate::reactive::{
    create_root, flush_effects, frame_tasks_pending, run_frame_tasks, take_frame_request,
};
use crate::theme::TokenSet;
use crate::ui::{BufferCanvas, UiTree};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ---------------------------------------------------------------------------
// Ballistics (pure, virtual clock)
// ---------------------------------------------------------------------------

#[test]
fn attack_is_instant_decay_is_timed() {
    // 20 dB/s over a 60 dB span = full scale falls in 3 s.
    let mut b = Ballistics::new(1, 20.0 / 60.0, ms(0));
    assert!(b.set_targets(&[0.9]), "a rise is a visible change");
    assert_eq!(b.channels[0].display, 0.9, "attack is instant");
    assert!(b.settled(), "attacked to target = settled");

    b.set_targets(&[0.3]);
    assert_eq!(b.channels[0].display, 0.9, "no instant fall");
    assert!(!b.settled());
    let t0 = Instant::now();
    b.advance(t0); // anchor frame (dt = 0)
    assert_eq!(b.channels[0].display, 0.9);
    b.advance(t0 + ms(900)); // 0.9 s * (1/3 scale per s) = 0.3 fall
    assert!(
        (b.channels[0].display - 0.6).abs() < 1e-4,
        "timed decay: {}",
        b.channels[0].display
    );
    b.advance(t0 + ms(3600)); // way past: clamps exactly onto target
    assert_eq!(b.channels[0].display, 0.3, "decay lands EXACTLY on target");
}

#[test]
fn decay_is_frame_rate_independent() {
    let mut coarse = Ballistics::new(1, 0.5, ms(0));
    let mut fine = Ballistics::new(1, 0.5, ms(0));
    for b in [&mut coarse, &mut fine] {
        b.set_targets(&[1.0]);
        b.set_targets(&[0.0]);
    }
    let t0 = Instant::now();
    coarse.advance(t0);
    coarse.advance(t0 + ms(1000)); // one 1 s step
    fine.advance(t0);
    for i in 1..=100 {
        fine.advance(t0 + ms(10 * i)); // a hundred 10 ms steps
    }
    assert!(
        (coarse.channels[0].display - fine.channels[0].display).abs() < 1e-3,
        "one step vs a hundred must land together: {} vs {}",
        coarse.channels[0].display,
        fine.channels[0].display
    );
}

#[test]
fn peak_holds_then_falls_to_the_display_level() {
    let mut b = Ballistics::new(1, 1.0, ms(500)); // fast fall, 0.5 s hold
    b.set_targets(&[0.8]);
    b.set_targets(&[0.2]);
    let t0 = Instant::now();
    b.advance(t0);
    // 100 ms in: display fell (0.7), peak pinned at 0.8; the hold clock
    // stamps at this first DETACHED frame.
    b.advance(t0 + ms(100));
    assert!((b.channels[0].display - 0.7).abs() < 1e-4);
    assert_eq!(b.channels[0].peak, 0.8, "peak holds");
    // 550 ms in: display keeps falling (0.8 - 0.55); the hold
    // (t0+100 .. t0+600) has not expired yet.
    b.advance(t0 + ms(550));
    assert!((b.channels[0].display - 0.25).abs() < 1e-4);
    assert_eq!(b.channels[0].peak, 0.8, "still holding");
    assert!(!b.settled(), "a held peak keeps the animation alive");
    // Past the hold: the peak falls; the display has landed EXACTLY on
    // its target via the max clamp.
    b.advance(t0 + ms(700));
    assert_eq!(b.channels[0].display, 0.2, "decay clamps onto target");
    assert!(b.channels[0].peak < 0.8, "hold expired: falling");
    b.advance(t0 + ms(2000));
    assert_eq!(b.channels[0].peak, 0.2, "peak lands ON the display level");
    assert!(b.settled(), "fixpoint reached");
}

#[test]
fn reanchor_never_integrates_a_parked_gap() {
    let mut b = Ballistics::new(1, 1.0, ms(0));
    b.set_targets(&[1.0]);
    b.set_targets(&[0.0]);
    let t0 = Instant::now();
    b.advance(t0);
    b.advance(t0 + ms(100));
    let mid = b.channels[0].display;
    // The task parks (app idle 10 s), then re-arms: the first advance
    // after reanchor must be an anchor, not a 10 s decay bite.
    b.reanchor();
    b.advance(t0 + ms(10_100));
    assert_eq!(
        b.channels[0].display, mid,
        "re-anchor frame must not integrate the parked gap"
    );
}

#[test]
fn non_finite_targets_are_gaps_and_band_frames_grow() {
    let mut b = Ballistics::new(1, 1.0, ms(0));
    b.set_targets(&[0.5]);
    assert!(!b.set_targets(&[f32::NAN]), "NaN is ignored, not a change");
    assert_eq!(b.channels[0].target, 0.5, "gap keeps the previous target");
    // A wider frame grows the channel vector; a shorter one leaves the
    // extra channels decaying toward their old targets.
    let mut bands = Ballistics::new(1, 1.0, ms(0));
    bands.set_targets(&[0.1, 0.2, 0.3, 0.4]);
    assert_eq!(bands.channels.len(), 4);
    bands.set_targets(&[0.9]);
    assert_eq!(bands.channels.len(), 4);
    assert_eq!(bands.channels[0].display, 0.9);
    assert_eq!(bands.channels[3].target, 0.4);
    // Out-of-range inputs clamp into display space.
    let mut c = Ballistics::new(1, 1.0, ms(0));
    c.set_targets(&[7.5]);
    assert_eq!(c.channels[0].display, 1.0);
}

#[test]
fn db_mapping_floors_and_scales() {
    assert_eq!(map_level(1.0, Some(-60.0)), 1.0, "0 dB = full scale");
    assert_eq!(map_level(0.0, Some(-60.0)), 0.0, "silence floors");
    assert_eq!(map_level(0.001, Some(-60.0)), 0.0, "-60 dB sits at 0");
    let half = map_level(0.5, Some(-60.0)); // ~ -6 dB -> ~0.9
    assert!((half - 0.9).abs() < 0.01, "-6 dB ≈ 0.9: {half}");
    let linear = map_level(0.5, None);
    assert_eq!(linear, 0.5, "no floor = linear passthrough");
    assert!(
        map_level(f32::NAN, Some(-60.0)).is_nan(),
        "gaps pass through"
    );
}

// ---------------------------------------------------------------------------
// THE IDLE LAW (media-av/0620 acceptance, unit half)
// ---------------------------------------------------------------------------

#[test]
fn silent_meter_reaches_fixpoint_and_stops_requesting_frames() {
    let size = Size::new(20, 1);
    let mut tree = UiTree::new(size);
    let (root, level) = create_root(|cx| {
        let level = cx.signal(0.0f32);
        let view = Meter::new(level).decay(60.0).peak_hold(ms(100)).view(cx);
        tree.mount(cx, view);
        level
    });
    flush_effects();
    assert_eq!(frame_tasks_pending(), 0, "a quiet meter arms nothing");

    // A burst, then silence: the decay animates, then SETTLES.
    level.set(0.8);
    flush_effects();
    level.set(0.0);
    flush_effects();
    assert_eq!(frame_tasks_pending(), 1, "decay in flight");
    let t0 = Instant::now();
    run_frame_tasks(t0);
    let mut left = usize::MAX;
    for i in 1..=40 {
        left = run_frame_tasks(t0 + ms(100 * i));
        if left == 0 {
            break;
        }
    }
    assert_eq!(left, 0, "the fixpoint DROPS the frame task");
    let _ = take_frame_request(); // drain motion-frame requests

    // Unchanged input from here on: N passes request ZERO frames.
    for i in 0..16 {
        assert_eq!(
            run_frame_tasks(t0 + ms(4000 + 16 * i)),
            0,
            "no task may reappear without input"
        );
    }
    assert!(
        !take_frame_request(),
        "a settled meter must not request frames"
    );
    // Re-writing the SAME value is still a fixpoint (equality guard).
    level.set(0.0);
    flush_effects();
    assert_eq!(frame_tasks_pending(), 0, "equal input re-arms nothing");
    root.dispose();
}

// ---------------------------------------------------------------------------
// Render pins (deterministic cells, token inks only)
// ---------------------------------------------------------------------------

fn draw_tree(tree: &mut UiTree, size: Size) -> BufferCanvas {
    tree.layout();
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    canvas
}

#[test]
fn horizontal_fill_uses_zone_token_inks() {
    let size = Size::new(20, 1);
    let mut tree = UiTree::new(size);
    let (root, level) = create_root(|cx| {
        let level = cx.signal(0.0f32);
        let view = Meter::new(level).view(cx);
        tree.mount(cx, view);
        level
    });
    level.set(1.0); // full scale: every zone visible
    flush_effects();
    let canvas = draw_tree(&mut tree, size);
    let t = TokenSet::default();
    let cell = |x: i32| canvas.cell(Point::new(x, 0)).expect("cell");
    assert_eq!(cell(0).0, '█', "full cells at the low end");
    assert_eq!(cell(0).1, t.ok, "low positions wear the ok token");
    // Position 15/20 = 0.775 ≥ warn_at 0.70.
    assert_eq!(cell(15).1, t.warn, "mid positions wear the warn token");
    assert_eq!(cell(19).1, t.error, "top positions wear the error token");

    // Let the level fall: the display decays away from the held peak,
    // exposing the faint track dots and the peak tick.
    level.set(0.1);
    flush_effects();
    let t0 = Instant::now();
    run_frame_tasks(t0); // anchor
    run_frame_tasks(t0 + ms(300)); // default 1.5 s hold: peak stays at 1.0
    flush_effects();
    let canvas = draw_tree(&mut tree, size);
    let row: String = (0..20)
        .map(|x| canvas.cell(Point::new(x, 0)).map(|c| c.0).unwrap_or(' '))
        .collect();
    assert!(
        row.contains('·'),
        "unfilled track renders faint dots: {row}"
    );
    assert!(row.contains('│'), "held peak renders its tick: {row}");
    root.dispose();
}

#[test]
fn band_mode_renders_one_bar_per_band_with_partials() {
    let size = Size::new(12, 4);
    let mut tree = UiTree::new(size);
    let (root, frames) = create_root(|cx| {
        let frames = cx.signal(Vec::<f32>::new());
        let view = Meter::bands(frames).bar(2, 1).view(cx);
        tree.mount(cx, view);
        frames
    });
    // 4 rows = 32 eighths per bar: full / 17 eighths (2 cells + ▁) /
    // 4 eighths (a ▄ nub).
    frames.set(vec![1.0, 17.0 / 32.0, 0.125]);
    flush_effects();
    let canvas = draw_tree(&mut tree, size);
    let cell = |x: i32, y: i32| canvas.cell(Point::new(x, y)).map(|c| c.0).unwrap_or(' ');
    assert_eq!(cell(0, 0), '█', "band 0 reaches the top row");
    assert_eq!(cell(0, 3), '█');
    assert_eq!(cell(3, 3), '█', "band 1 fills its bottom rows");
    assert_eq!(cell(3, 1), '▁', "band 1 tips with an eighth remainder");
    assert_eq!(cell(3, 0), ' ', "band 1 leaves the top row empty");
    assert_eq!(cell(6, 3), '▄', "band 2 is a half-cell nub");
    assert_eq!(cell(2, 3), ' ', "gap column stays empty");
    root.dispose();
}

#[test]
fn theme_switch_restyles_the_meter_via_context() {
    // The dyn_view resolves tokens through reactive context; without an
    // app the default theme applies — pin that the resolved ink comes
    // from the TOKEN SET, not a literal.
    let size = Size::new(10, 1);
    let mut tree = UiTree::new(size);
    let (root, level) = create_root(|cx| {
        let level = cx.signal(0.3f32);
        let view = Meter::new(level).view(cx);
        tree.mount(cx, view);
        level
    });
    flush_effects();
    let canvas = draw_tree(&mut tree, size);
    let t = TokenSet::default();
    assert_eq!(
        canvas.cell(Point::new(0, 0)).expect("cell").1,
        t.ok,
        "ink is the theme's ok token"
    );
    let _ = level;
    root.dispose();
}
