//! Wave 13 MODAL-BG: the gateway-console field report — "that modal
//! crashes the background" (operator screenshot 2026-07-25: sandbox
//! modal open on the Review & Test wizard screen; page content around
//! the modal blanked; the sandbox result block + footer rendered below
//! where they belong).
//!
//! Method (repro-first): the console's EXACT screen shape rebuilt from
//! engine widgets (see wave_modal_bg_parts/fixture.rs) and driven
//! through the real `Driver`/`CaptureTerm` along the operator's
//! gesture — open the sandbox modal, work the provider Select popup,
//! type, let the store update the page's dyn regions UNDER the open
//! modal, raise a toast over it, resize, close.
//!
//! Two oracles at every step (the class pin):
//! 1. background integrity — every terminal cell OUTSIDE the modal
//!    panel (∪ the bands legitimately changed by the step) must be
//!    byte-identical to the previous step's screen;
//! 2. fresh-paint equality — `request_full_redraw()` must not change a
//!    single cell (any diff = the incremental path lied).

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver};
use abstracttui::base::{Rect, Size};
use abstracttui::testing::{CaptureTerm, VtScreen};

#[path = "wave_modal_bg_parts/harness.rs"]
mod harness;
use harness::*;

#[path = "wave_modal_bg_parts/fixture.rs"]
mod fixture;
use fixture::*;

// ---------------------------------------------------------------------
// scenario 1: the bare open
// ---------------------------------------------------------------------

#[test]
fn modal_open_never_disturbs_the_page_outside_its_panel() {
    // 100x40: everything fits. 100x14: the page over-demands (clean-
    // absence crush class) AND the panel clamps to the viewport
    // height — the modal request (18) exceeds the terminal (14).
    for size in [Size::new(100, 40), Size::new(100, 14)] {
        open_integrity_at(size);
    }
}

fn open_integrity_at(size: Size) {
    let (mut app, fx) = mount_console(size, 12, Sandbox::NotAsked);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    // The operator gesture: focus the Run button, Enter.
    let before = open_modal_via_keyboard(&mut driver, &mut app, &mut term, &mut vt, &fx);
    assert!(
        vt.to_text().contains("Sandbox test — real generation"),
        "modal content visible at {}x{}\n{}",
        size.w,
        size.h,
        vt.to_text()
    );

    // Oracle 1: outside the panel, not one cell moved. (sandbox was
    // already NotAsked, so the page's result band is unchanged too.)
    let panel = modal_rect(size, MODAL_SIZE);
    let diff = screen_diff_outside(&before, &vt.screenshot(), &[panel]);
    assert!(
        diff.is_empty(),
        "modal open disturbed the page outside its panel:\n  {}\n--- before ---\n{}\n--- after ---\n{}",
        diff.join("\n  "),
        before.to_text(),
        vt.to_text()
    );

    // Oracle 2: what stands is exactly what a fresh paint produces.
    assert_fresh_paint_equal("after-open", &mut driver, &mut app, &mut term, &mut vt);
}

// ---------------------------------------------------------------------
// scenario 2: typing in the modal
// ---------------------------------------------------------------------

#[test]
fn typing_in_the_modal_leaves_the_page_untouched() {
    let size = Size::new(100, 40);
    let (mut app, fx) = mount_console(size, 12, Sandbox::NotAsked);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let _pre = open_modal_via_keyboard(&mut driver, &mut app, &mut term, &mut vt, &fx);
    let opened = vt.screenshot();
    let panel = modal_rect(size, MODAL_SIZE);

    // ONE Tab: Select -> prompt input, then type — each keystroke is
    // a turn (Space must land in the TextInput, never on a Button).
    term.push_input(b"\t");
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    for ch in ["h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d"] {
        term.push_input(ch.as_bytes());
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
        let diff = screen_diff_outside(&opened, &vt.screenshot(), &[panel]);
        assert!(
            diff.is_empty(),
            "typing '{ch}' disturbed the page outside the panel:\n  {}\n--- screen ---\n{}",
            diff.join("\n  "),
            vt.to_text()
        );
    }
    assert_fresh_paint_equal("after-typing", &mut driver, &mut app, &mut term, &mut vt);
}

// ---------------------------------------------------------------------
// scenario 3: page dyn updates under the open modal
// ---------------------------------------------------------------------

#[test]
fn page_dyn_update_under_open_modal_stays_honest() {
    let size = Size::new(100, 40);
    let (mut app, fx) = mount_console(size, 12, Sandbox::NotAsked);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let _pre = open_modal_via_keyboard(&mut driver, &mut app, &mut term, &mut vt, &fx);
    let opened = vt.screenshot();
    let panel = modal_rect(size, MODAL_SIZE);

    // The Generate flow: the store flips Loading then Ready — BOTH the
    // page's h(4) result dyn and the modal's copy re-render under the
    // open modal. The page band around the live-test block may
    // legitimately change; everything else must hold.
    fx.sandbox.set(Sandbox::Loading);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert_fresh_paint_equal(
        "under-modal-loading",
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    fx.sandbox
        .set(Sandbox::Ready("I am a test model reply.".to_string()));
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert_fresh_paint_equal(
        "under-modal-ready",
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    // The page result band lives inside the live-test block. Everything
    // OUTSIDE panel ∪ that band must be byte-identical to the
    // post-open screen.
    let after = vt.screenshot();
    let base = live_block_band(&opened, size).expect("band visible at 100x40");
    let result_band = match live_block_band(&after, size) {
        Some(b) => base.union(b),
        None => base,
    };
    let diff = screen_diff_outside(&opened, &after, &[panel, result_band]);
    assert!(
        diff.is_empty(),
        "page dyn update under the modal disturbed unrelated regions:\n  {}\n--- after open ---\n{}\n--- after update ---\n{}",
        diff.join("\n  "),
        opened.to_text(),
        vt.to_text()
    );
}

// ---------------------------------------------------------------------
// scenario 4: resize while open, then close
// ---------------------------------------------------------------------

#[test]
fn resize_while_modal_open_then_close_matches_fresh_paint() {
    let size = Size::new(100, 40);
    let (mut app, fx) = mount_console(size, 12, Sandbox::NotAsked);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let before_open = open_modal_via_keyboard(&mut driver, &mut app, &mut term, &mut vt, &fx);

    for s in [Size::new(90, 30), Size::new(120, 44), Size::new(100, 40)] {
        term.push_resize(s);
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
        assert_fresh_paint_equal(
            &format!("after-resize-{}x{}", s.w, s.h),
            &mut driver,
            &mut app,
            &mut term,
            &mut vt,
        );
    }

    // Close from INSIDE the modal tree (the console's Esc path; 'x'
    // needs no reader timeout). The vacated region must repaint from
    // the page — final screen == the pre-open one.
    term.push_input(b"x");
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert!(fx.modal.borrow().is_none(), "modal closed");
    let diff = screen_diff_outside(&before_open, &vt.screenshot(), &[]);
    assert!(
        diff.is_empty(),
        "screen after open+resizes+close differs from the pre-open screen:\n  {}\n--- before ---\n{}\n--- after ---\n{}",
        diff.join("\n  "),
        before_open.to_text(),
        vt.to_text()
    );
    assert_fresh_paint_equal("after-close", &mut driver, &mut app, &mut term, &mut vt);
}

// ---------------------------------------------------------------------
// scenario 5: the full operator session, size sweep
// ---------------------------------------------------------------------

/// Advance the injected clock and run one turn.
fn tick_turn(
    clock: &Rc<Cell<Duration>>,
    step: Duration,
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
) {
    clock.set(clock.get() + step);
    let _ = driver.turn(app, term).expect("turn");
    let bytes = term.take_bytes();
    if vt.size() != app.viewport() {
        *vt = garbage_screen(app.viewport());
    }
    vt.feed(&bytes);
}

/// Advance time until the app goes idle (animations parked, timers
/// fired) — bounded.
fn settle(
    clock: &Rc<Cell<Duration>>,
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
) {
    for _ in 0..80 {
        clock.set(clock.get() + Duration::from_millis(100));
        let turn = driver.turn(app, term).expect("turn");
        let bytes = term.take_bytes();
        if vt.size() != app.viewport() {
            *vt = garbage_screen(app.viewport());
        }
        vt.feed(&bytes);
        if turn.idle {
            return;
        }
    }
    panic!("app never settled (animation or timer leak)");
}

/// The footer's status row (busy/notice line) — legitimately changes
/// whenever busy/notice flip.
fn footer_status_row(viewport: Size) -> Rect {
    Rect::new(0, (viewport.h - 2).max(0), viewport.w, 1)
}

#[test]
fn operator_session_full_walk_across_sizes() {
    for size in [
        Size::new(110, 32), // the console's boot size
        Size::new(100, 28),
        Size::new(80, 24), // macOS default
        Size::new(46, 28), // narrow split: modal clamps to the viewport
    ] {
        operator_session(size);
    }
}

fn operator_session(size: Size) {
    let label = format!("{}x{}", size.w, size.h);
    // Empty journal (fresh wizard) + a PREVIOUS test result: the open
    // resets Ready -> NotAsked in the very batch that raises the modal
    // (the console's exact gesture after an earlier sandbox run).
    let (mut app, fx) = mount_console(size, 0, Sandbox::Ready("earlier run".into()));
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let t0 = Instant::now();
    let clock: Rc<Cell<Duration>> = Rc::new(Cell::new(Duration::ZERO));
    {
        let clock = clock.clone();
        driver.set_clock(move || t0 + clock.get());
    }
    let mut vt = VtScreen::new(size);
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);

    // -- open (store reset rides the same turn) -----------------------
    let pre_open = open_modal_via_keyboard(&mut driver, &mut app, &mut term, &mut vt, &fx);
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    let panel = modal_rect(size, MODAL_SIZE);
    let opened = vt.screenshot();
    {
        // Outside panel ∪ live-block band (Ready -> NotAsked shrank the
        // page's result rows), nothing may move at open. The panel may
        // have veiled the anchors in `opened`; the block cannot move,
        // so the pre-open band stands in.
        let pre_band = live_block_band(&pre_open, size).expect("band visible pre-open");
        let band = match live_block_band(&opened, size) {
            Some(b) => pre_band.union(b),
            None => pre_band,
        };
        let diff = screen_diff_outside(&pre_open, &opened, &[panel, band]);
        assert!(
            diff.is_empty(),
            "[{label}] open disturbed the page outside panel+band:\n  {}\n--- before ---\n{}\n--- after ---\n{}",
            diff.join("\n  "),
            pre_open.to_text(),
            opened.to_text()
        );
    }
    assert_fresh_paint_equal(
        &format!("[{label}] after-open"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    // -- provider Select popup over the modal -------------------------
    term.push_input(b"\r"); // open the popup (Select has focus)
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    assert_fresh_paint_equal(
        &format!("[{label}] popup-open"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );
    term.push_input(b"\x1b[B"); // Down: highlight "lmstudio"
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    term.push_input(b"\r"); // commit; popup layer removed; model row regenerates
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    {
        let now = vt.screenshot();
        let diff = screen_diff_outside(&opened, &now, &[panel]);
        assert!(
            diff.is_empty(),
            "[{label}] select popup commit disturbed the page:\n  {}\n--- after open ---\n{}\n--- after commit ---\n{}",
            diff.join("\n  "),
            opened.to_text(),
            now.to_text()
        );
    }
    assert_fresh_paint_equal(
        &format!("[{label}] popup-commit"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    // -- a toast rises over the open modal and expires -----------------
    let before_toast = vt.screenshot();
    fx.notice.set(Some("⟳ refreshing Providers…".into()));
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt); // slide in + park
    assert_fresh_paint_equal(
        &format!("[{label}] toast-parked"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );
    // Past the 4 s duration + slide-out: the toast layer removes and
    // the vacated cells repaint from below.
    for _ in 0..50 {
        tick_turn(
            &clock,
            Duration::from_millis(120),
            &mut driver,
            &mut app,
            &mut term,
            &mut vt,
        );
    }
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    {
        let now = vt.screenshot();
        // Only the footer status row (the notice mirror) may differ
        // from the pre-toast screen.
        let diff = screen_diff_outside(&before_toast, &now, &[footer_status_row(size)]);
        assert!(
            diff.is_empty(),
            "[{label}] toast expiry left residue:\n  {}\n--- before toast ---\n{}\n--- after expiry ---\n{}",
            diff.join("\n  "),
            before_toast.to_text(),
            now.to_text()
        );
    }
    assert_fresh_paint_equal(
        &format!("[{label}] toast-gone"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    // -- Generate: busy ticks the footer while the page band flips -----
    for _ in 0..6 {
        // Tab-walk to the Generate button (Select -> model input ->
        // prompt -> Generate); Enter on non-buttons is inert.
        term.push_input(b"\t");
        settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
        term.push_input(b"\r");
        settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
        if fx.sandbox.get_untracked() == Sandbox::Loading {
            break;
        }
    }
    assert!(
        fx.sandbox.get_untracked() == Sandbox::Loading,
        "[{label}] Generate reached"
    );
    for _ in 0..3 {
        fx.tick.update(|t| *t += 1); // the 500 ms busy interval
        settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    }
    assert_fresh_paint_equal(
        &format!("[{label}] busy-ticking"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    fx.sandbox
        .set(Sandbox::Ready("I am a live test reply.".into()));
    fx.busy.set(None);
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    assert_fresh_paint_equal(
        &format!("[{label}] result-ready"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );

    // -- resize down and back while open ------------------------------
    for s in [Size::new(46, 24), size] {
        term.push_resize(s);
        settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
        assert_fresh_paint_equal(
            &format!("[{label}] resized-{}x{}", s.w, s.h),
            &mut driver,
            &mut app,
            &mut term,
            &mut vt,
        );
    }

    // -- close ---------------------------------------------------------
    term.push_input(b"x");
    settle(&clock, &mut driver, &mut app, &mut term, &mut vt);
    assert!(fx.modal.borrow().is_none(), "[{label}] modal closed");
    {
        // vs pre-open: the result band (NotAsked -> Ready) and the
        // footer status row legitimately changed; nothing else may.
        // The PageHost tab bar (rows 2-3) is excluded too: its sticky
        // window anchor legitimately re-windows across the 46-col
        // resize excursion (stateful by design, fresh-paint-verified).
        let now = vt.screenshot();
        let pre_band = live_block_band(&pre_open, size).expect("band visible pre-open");
        let band = match live_block_band(&now, size) {
            Some(b) => pre_band.union(b),
            None => pre_band,
        };
        let tab_bar = Rect::new(0, 2, size.w, 2);
        let diff = screen_diff_outside(&pre_open, &now, &[band, footer_status_row(size), tab_bar]);
        assert!(
            diff.is_empty(),
            "[{label}] close left the page different from pre-open:\n  {}\n--- pre-open ---\n{}\n--- after close ---\n{}",
            diff.join("\n  "),
            pre_open.to_text(),
            now.to_text()
        );
    }
    assert_fresh_paint_equal(
        &format!("[{label}] after-close"),
        &mut driver,
        &mut app,
        &mut term,
        &mut vt,
    );
}
