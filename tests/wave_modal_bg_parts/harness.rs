//! Wave 13 MODAL-BG drive/oracle harness: turn pump, garbage-prefilled
//! resize referee, capture diffing, and the forced-full-redraw oracle.
//! `#[path]` sibling of tests/wave_modal_bg.rs (file-size rule).

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Rect, Size};
use abstracttui::render::Screenshot;
use abstracttui::term::{Capabilities, EnterOptions, MouseMode};
use abstracttui::testing::{CaptureTerm, VtScreen};

pub fn caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.unicode_ok = true;
    })
}

pub fn cfg() -> RunConfig {
    RunConfig {
        caps: Some(caps()),
        enter: Some(EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: abstracttui::term::KittyFlags(0),
        }),
        probe: false,
        ..RunConfig::default()
    }
}

/// Run turns until idle (bounded), feeding emitted bytes into `vt`.
/// On viewport change the referee rebuilds pre-filled with garbage
/// (the adv_resize_modal rule: post-resize content is unknowable).
pub fn drive_to_idle(
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
) {
    for _ in 0..20 {
        let turn = driver.turn(app, term).expect("turn");
        let bytes = term.take_bytes();
        if vt.size() != app.viewport() {
            *vt = garbage_screen(app.viewport());
        }
        vt.feed(&bytes);
        if turn.idle {
            break;
        }
    }
}

pub fn garbage_screen(size: Size) -> VtScreen {
    let mut vt = VtScreen::new(size);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x1b[45;31m");
    for y in 0..size.h {
        bytes.extend_from_slice(format!("\x1b[{};1H", y + 1).as_bytes());
        bytes.extend(std::iter::repeat_n(b'X', size.w.max(0) as usize));
    }
    bytes.extend_from_slice(format!("\x1b[0m\x1b[{};1H", size.h).as_bytes());
    vt.feed(&bytes);
    vt
}

/// Cell mismatches between two captures, `skip` rects excluded.
pub fn screen_diff_outside(a: &Screenshot, b: &Screenshot, skip: &[Rect]) -> Vec<String> {
    assert_eq!(a.size(), b.size(), "referee size mismatch (test bug)");
    let mut out = Vec::new();
    let mut total = 0usize;
    for y in 0..a.size().h {
        for x in 0..a.size().w {
            if skip.iter().any(|r| r.contains(Point::new(x, y))) {
                continue;
            }
            let ca = a.cell(x, y).expect("cell");
            let cb = b.cell(x, y).expect("cell");
            if ca != cb {
                total += 1;
                if out.len() < 16 {
                    out.push(format!("({x},{y}): got {cb:?} want {ca:?}"));
                }
            }
        }
    }
    if total > out.len() {
        out.push(format!("... {total} mismatching cells total"));
    }
    out
}

/// Oracle 2: a forced full redraw must be a byte-level no-op on the
/// SCREEN (the emission may be large; the resulting cells must match).
pub fn assert_fresh_paint_equal(
    label: &str,
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
) {
    let before = vt.screenshot();
    abstracttui::app::request_full_redraw();
    drive_to_idle(driver, app, term, vt);
    let after = vt.screenshot();
    let diff = screen_diff_outside(&before, &after, &[]);
    assert!(
        diff.is_empty(),
        "{label}: full redraw changed the screen — the incremental frame was stale\n  {}\n--- incremental ---\n{}\n--- fresh ---\n{}",
        diff.join("\n  "),
        before.to_text(),
        after.to_text()
    );
}
