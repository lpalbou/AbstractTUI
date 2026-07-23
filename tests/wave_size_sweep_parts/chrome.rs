//! Axis (a) — chrome survival under content over-demand, across the
//! size matrix. Three honest arms:
//!
//! 1. `shrink(0.0)` pins (the documented recipe): chrome rows hold at
//!    EVERY size while the page absorbs the loss.
//! 2. No pins: chrome collapses to CLEAN ABSENCE (the fusion fix) and
//!    the engine NAMES the crush (zero-collapse notice, debug builds).
//! 3. The Scroll recipe: a page inside `Scroll` (its default layout is
//!    `grow(1) basis(0)`) exerts NO content pressure — chrome survives
//!    with no pins at all. This is the shape the notice text itself
//!    recommends ("absorb the overflow in a Scroll").

use abstracttui::app::{App, Driver};
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{Element, View};
use abstracttui::widgets::Scroll;

use crate::harness::{config, drive_to_idle, heavy_page, CHROME_MARK, SIZE_MATRIX};

/// A labeled 1-row chrome bar (marker-filled, hand-rolled draw).
fn bar(label: &'static str, pinned: bool) -> Element {
    let style = if pinned {
        LayoutStyle::line(1).shrink(0.0)
    } else {
        LayoutStyle::line(1)
    };
    Element::new().style(style).draw(move |canvas, rect| {
        if rect.is_empty() {
            return; // defensive app-side guard — the engine now skips anyway
        }
        let cols = rect.w.max(0) as usize;
        let mut row = String::from(label);
        while row.chars().count() < cols {
            row.push(CHROME_MARK);
        }
        let row: String = row.chars().take(cols).collect();
        canvas.print(
            Point::new(rect.x, rect.y),
            &row,
            Rgba::rgb(200, 200, 80),
            Rgba::TRANSPARENT,
        );
    })
}

/// The console shell shape: header + page + separator + footer.
fn shell(pinned: bool, page: View) -> View {
    Element::new()
        .style(LayoutStyle::column())
        .child(bar("HEADER", pinned).build())
        .child(
            Element::new()
                .style(LayoutStyle::default().grow(1.0))
                .child(page)
                .build(),
        )
        .child(bar("SEP", pinned).build())
        .child(bar("FOOTER", pinned).build())
        .build()
}

fn screen_at(size: Size, mount: impl FnOnce(&mut App)) -> (VtScreen, App) {
    let mut app = App::new(size);
    mount(&mut app);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    (vt, app)
}

fn lines(vt: &VtScreen) -> Vec<String> {
    vt.to_text().lines().map(str::to_string).collect()
}

/// Arm 1: shrink(0.0) pins hold header/separator/footer at every
/// matrix size; the page truncates between them.
#[test]
fn pinned_chrome_survives_every_matrix_size() {
    for &size in &SIZE_MATRIX {
        let (vt, _app) = screen_at(size, |app| {
            app.mount(|_cx| shell(true, heavy_page(400)))
                .expect("mount");
        });
        let rows = lines(&vt);
        let h = size.h as usize;
        assert!(
            rows[0].starts_with("HEADER"),
            "{size:?}: header row must survive: {:?}",
            rows[0]
        );
        assert!(
            rows[h - 2].starts_with("SEP"),
            "{size:?}: separator row must survive: {:?}",
            rows[h - 2]
        );
        assert!(
            rows[h - 1].starts_with("FOOTER"),
            "{size:?}: footer row must survive: {:?}",
            rows[h - 1]
        );
        assert!(
            rows[1].starts_with("data-row-000"),
            "{size:?}: page content renders under the header: {:?}",
            rows[1]
        );
        // No fusion anywhere: chrome glyphs only on chrome rows.
        for (y, row) in rows.iter().enumerate() {
            let is_chrome = y == 0 || y == h - 2 || y == h - 1;
            assert!(
                is_chrome || !row.contains(CHROME_MARK),
                "{size:?}: chrome glyphs fused into page row {y}: {row:?}"
            );
        }
    }
}

/// Arm 2: without pins, the crush is CLEAN ABSENCE at every size —
/// page rows only, zero chrome glyphs — and the engine names it.
#[test]
fn unpinned_chrome_collapses_cleanly_and_is_named() {
    for &size in &SIZE_MATRIX {
        let (vt, app) = screen_at(size, |app| {
            app.mount(|_cx| shell(false, heavy_page(400)))
                .expect("mount");
        });
        let text = vt.to_text();
        assert!(
            text.contains("data-row-000"),
            "{size:?}: page must render:\n{text}"
        );
        assert!(
            !text.contains(CHROME_MARK) && !text.contains("HEADER"),
            "{size:?}: crushed chrome must be cleanly absent (no fusion):\n{text}"
        );
        if cfg!(debug_assertions) {
            let notices = app.startup_notices().join("\n");
            assert!(
                notices.contains("collapsed to 0"),
                "{size:?}: the zero-collapse notice must name the crush: {notices:?}"
            );
        }
    }
}

/// Arm 3: the Scroll recipe — content inside a `Scroll` exerts no
/// pressure (default `basis(0)`), so unpinned chrome survives at every
/// size and the page scrolls instead.
#[test]
fn scroll_absorbed_page_keeps_unpinned_chrome_at_every_size() {
    for &size in &SIZE_MATRIX {
        let (vt, _app) = screen_at(size, |app| {
            app.mount(|cx| shell(false, Scroll::new(heavy_page(400)).view(cx)))
                .expect("mount");
        });
        let rows = lines(&vt);
        let h = size.h as usize;
        assert!(
            rows[0].starts_with("HEADER"),
            "{size:?}: header survives with a scrolled page: {:?}",
            rows[0]
        );
        assert!(
            rows[h - 1].starts_with("FOOTER"),
            "{size:?}: footer survives with a scrolled page: {:?}",
            rows[h - 1]
        );
        assert!(
            rows[1].starts_with("data-row-000"),
            "{size:?}: scrolled page renders: {:?}",
            rows[1]
        );
    }
}

/// Degenerate floor: pinned chrome TALLER than the viewport (3 pinned
/// rows into 2) must not panic and must clip honestly — the first
/// rows in document order win, nothing fuses.
#[test]
fn pinned_chrome_taller_than_viewport_clips_honestly() {
    for &size in &[Size::new(40, 3), Size::new(40, 2), Size::new(40, 1)] {
        let (vt, _app) = screen_at(size, |app| {
            app.mount(|_cx| shell(true, heavy_page(50))).expect("mount");
        });
        let rows = lines(&vt);
        assert!(
            rows[0].starts_with("HEADER"),
            "{size:?}: first pinned row wins: {:?}",
            rows[0]
        );
        // Every visible row is either chrome or page — never a mix.
        for (y, row) in rows.iter().enumerate() {
            let chrome_row =
                row.starts_with("HEADER") || row.starts_with("SEP") || row.starts_with("FOOTER");
            let page_row = row.starts_with("data-row-") || row.trim_end().is_empty();
            assert!(
                chrome_row || page_row,
                "{size:?}: row {y} is neither pure chrome nor pure page: {row:?}"
            );
        }
    }
}
