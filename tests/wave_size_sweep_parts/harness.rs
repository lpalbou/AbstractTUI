//! Shared harness for the size/ratio sweep: driver config, settle
//! loops, garbage-prefilled referees, the fresh-paint oracle, and the
//! heavy fixtures (the field lesson: LIGHT fixtures never see the
//! crush class — the console incident had a data-volume threshold).

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::{Capabilities, EnterOptions, KittyFlags, MouseMode};
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{text, Element, View};

/// The size matrix the sweep drives every scene through: the two
/// common shells, the console's small shell, wide-short, tall-narrow,
/// and the brutal floor.
pub const SIZE_MATRIX: [Size; 6] = [
    Size { w: 80, h: 24 },
    Size { w: 100, h: 24 },
    Size { w: 60, h: 16 },
    Size { w: 200, h: 20 }, // wide-short
    Size { w: 60, h: 50 },  // tall-narrow
    Size { w: 40, h: 12 },  // brutal
];

/// Fixed capabilities so host env never leaks into assertions.
pub fn caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.unicode_ok = true;
    })
}

pub fn config() -> RunConfig {
    RunConfig {
        caps: Some(caps()),
        enter: Some(EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: KittyFlags(0),
        }),
        probe: false,
    }
}

/// Drive turns until idle (bounded), feeding every emitted byte into
/// `vt`. On a viewport change the referee is REBUILT at the new size
/// and pre-filled with garbage — the post-resize screen is unknowable,
/// and a fresh blank grid would hide exactly the stale-band class.
pub fn drive_to_idle(
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    vt: &mut VtScreen,
) {
    for _ in 0..16 {
        let turn = driver.turn(app, term).expect("turn");
        let bytes = term.take_bytes();
        if vt.size() != app.viewport() {
            *vt = garbage_screen(app.viewport());
        }
        vt.feed(&bytes);
        assert_eq!(
            vt.unknown_seq_count(),
            0,
            "presenter emitted an unmodeled sequence: {:?}",
            vt.unknown_samples()
        );
        if turn.idle {
            break;
        }
    }
}

/// A VtScreen modeling the honest post-resize worst case: every cell a
/// visible stale glyph, cursor parked bottom-left (the 0298 shape) —
/// any cell the engine fails to re-emit surfaces as garbage instead of
/// hiding as a plausible blank.
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

/// Cell-for-cell screen comparison (glyph + paint), bounded sample.
pub fn screen_diff(a: &VtScreen, b: &VtScreen) -> Vec<String> {
    assert_eq!(a.size(), b.size(), "referee size mismatch (test bug)");
    let mut out = Vec::new();
    for y in 0..a.size().h {
        for x in 0..a.size().w {
            let ca = a.cell(x, y).expect("cell");
            let cb = b.cell(x, y).expect("cell");
            if (ca.ch() != cb.ch() || ca.paint != cb.paint) && out.len() < 12 {
                out.push(format!(
                    "({x},{y}): got {:?}/{:?} want {:?}/{:?}",
                    ca.ch(),
                    ca.paint,
                    cb.ch(),
                    cb.paint
                ));
            }
        }
    }
    out
}

/// Byte-truth damage oracle: the incumbent's final screen (incremental
/// damage-driven repaints over a garbage-prefilled referee) must equal
/// a FRESH driver's first full paint of the same scene at the same
/// size, cell for cell. Panics with the first mismatches.
///
/// The oracle world builds ON ITS OWN THREAD (harness lesson from this
/// wave): the engine's per-thread singletons — the drawer one-per-edge
/// registry, the viewport/theme/notices signals — make two live app
/// worlds on one thread interfere (the oracle's drawer open REPLACED
/// the incumbent's via the shared edge registry, silently removing its
/// scrim). A fresh thread gets fresh thread-locals; the returned
/// screen is plain data.
pub fn assert_matches_fresh_paint(
    name: &str,
    incumbent: &VtScreen,
    size: Size,
    mount: impl FnOnce(&mut App) + Send + 'static,
) {
    let vt = oracle_screen(size, mount);
    let diff = screen_diff(incumbent, &vt);
    assert!(
        diff.is_empty(),
        "{name}: {} stale/missing cells vs fresh-paint oracle at {:?}\n\
         first mismatches:\n  {}\n--- incumbent ---\n{}\n--- oracle ---\n{}",
        diff.len(),
        size,
        diff.join("\n  "),
        incumbent.to_text(),
        vt.to_text()
    );
}

/// Fresh-paint oracle screen for `mount` at `size`, built on a fresh
/// thread (fresh engine thread-locals — see `assert_matches_fresh_paint`).
pub fn oracle_screen(size: Size, mount: impl FnOnce(&mut App) + Send + 'static) -> VtScreen {
    std::thread::spawn(move || {
        let mut app = App::new(size);
        mount(&mut app);
        let mut term = CaptureTerm::new(size);
        let mut driver = Driver::new(&mut app, &mut term, config()).expect("oracle driver");
        let mut vt = VtScreen::new(size);
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
        vt
    })
    .join()
    .expect("oracle thread")
}

/// The wide-glyph frame invariant, walked over the FINAL screen: every
/// wide leader is immediately followed by its continuation (never at
/// the last column), and no continuation appears without its leader —
/// a torn glyph anywhere in the pipeline (truncation, clip edge,
/// presenter runs) surfaces here.
pub fn assert_wide_pairs_sound(vt: &VtScreen, ctx: &str) {
    for y in 0..vt.size().h {
        let mut x = 0;
        while x < vt.size().w {
            let cell = vt.cell(x, y).expect("cell in bounds");
            if cell.is_wide_leader() {
                assert!(
                    x + 1 < vt.size().w,
                    "{ctx}: wide leader at the last column ({x},{y}) — torn glyph"
                );
                let next = vt.cell(x + 1, y).expect("cell in bounds");
                assert!(
                    next.is_continuation(),
                    "{ctx}: leader at ({x},{y}) without its continuation \
                     (found {:?})",
                    next.ch()
                );
                x += 2;
                continue;
            }
            assert!(
                !cell.is_continuation(),
                "{ctx}: orphan continuation at ({x},{y})"
            );
            x += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Heavy fixtures
// ---------------------------------------------------------------------------

/// The chrome marker glyph: a full-row fill no page row ever contains,
/// so one `contains` catches any fusion of chrome into content rows.
pub const CHROME_MARK: char = '=';

/// A hand-rolled chrome bar the way the console seat wrote theirs:
/// clips HORIZONTALLY only (pads/truncates to rect.w), then paints
/// "its" row — the exact closure shape that fused into a sibling row
/// when flex crushed the bar to zero height.
pub fn hand_rolled_bar(label: &'static str) -> Element {
    Element::new()
        .style(LayoutStyle::line(1))
        .draw(move |canvas, rect| {
            let cols = rect.w.max(0) as usize;
            let mut row = String::with_capacity(cols);
            row.push_str(label);
            while row.chars().count() < cols {
                row.push(CHROME_MARK);
            }
            let row: String = row.chars().take(cols).collect();
            canvas.print(
                Point::new(rect.x, rect.y),
                &row,
                Rgba::rgb(220, 220, 40),
                Rgba::TRANSPARENT,
            );
        })
}

/// Heavy page content: `rows` distinct text lines (intrinsic height =
/// `rows`, the content pressure that triggers the crush).
pub fn heavy_page(rows: usize) -> View {
    let body: String = (0..rows)
        .map(|i| format!("data-row-{i:03} lorem ipsum dolor sit amet"))
        .collect::<Vec<_>>()
        .join("\n");
    text(body)
}
