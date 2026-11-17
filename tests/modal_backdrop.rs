//! Modal backdrop, driven through the real driver and read back off the
//! terminal model:
//!
//! - the page outside the panel is the SAME page veiled by the theme's
//!   `overlay` scrim (text kept, every color composited);
//! - the panel is solid: what it paints sits on an opaque plate, so the
//!   page never shows through it, even where the panel paints a
//!   translucent color (an app following the old "modal ground is the
//!   overlay token" advice);
//! - stacked modals each veil what is under them;
//! - `set_scrim(false)` keeps the page bright and the panel solid;
//! - every close path (`close`, `layer().remove()`, the outside press)
//!   takes the backdrop with the panel.

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::Scope;
use abstracttui::render::Style;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{Element, StyledCanvas};
use std::cell::RefCell;
use std::rc::Rc;

const SCREEN: Size = Size::new(40, 12);
/// 30x8 centred in 40x12: the panel spans x 5..35, y 2..10.
const PANEL: Size = Size::new(30, 8);
const PAGE_FG: Rgba = Rgba::new(230, 230, 230, 255);
const PAGE_BG: Rgba = Rgba::new(20, 90, 160, 255);
/// A page cell well outside the panel, and one inside it (content area).
const OUTSIDE: Point = Point::new(1, 0);
const INSIDE: Point = Point::new(8, 6);

fn config() -> RunConfig {
    RunConfig {
        probe: false,
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.unicode_ok = true;
        })),
        ..RunConfig::default()
    }
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    cx: Scope,
    overlays: abstracttui::app::Overlays,
}

/// A page of 'P' on a saturated blue ground, nothing open yet.
fn rig() -> Rig {
    let mut app = App::new(SCREEN);
    let overlays = app.overlays();
    let scope: Rc<RefCell<Option<Scope>>> = Default::default();
    let s = scope.clone();
    app.mount(move |cx| {
        *s.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::fill())
            .draw(|canvas, rect| {
                canvas.fill(rect, 'P', PAGE_FG, PAGE_BG);
            })
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SCREEN);
    let driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let cx = scope.borrow_mut().take().expect("mounted");
    let mut r = Rig {
        app,
        term,
        driver,
        cx,
        overlays,
    };
    r.turn();
    r
}

impl Rig {
    fn turn(&mut self) {
        for _ in 0..2 {
            self.driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn");
        }
    }

    /// Open a modal whose content paints its first row with the
    /// TRANSLUCENT `overlay` token — the see-through case the plate fixes.
    fn open(&mut self) -> Modal {
        let m = Modal::open(&self.overlays, self.cx, SCREEN, PANEL, |_| {
            Element::new()
                .style(LayoutStyle::fill())
                .draw(|canvas: &mut dyn StyledCanvas, rect| {
                    let t = abstracttui::app::current_theme().tokens;
                    let row = abstracttui::base::Rect::new(rect.x, rect.y, rect.w, 1);
                    canvas.fill_styled(row, ' ', &Style::new().fg(t.text).bg(t.overlay));
                })
                .build()
        });
        self.turn();
        m
    }

    fn cell(&self, p: Point) -> (String, Option<Rgba>, Option<Rgba>) {
        let shot = self.term.screen().screenshot();
        let c = shot.cell(p.x, p.y).expect("cell");
        (c.text().to_string(), c.fg(), c.bg())
    }

    fn assert_page_untouched(&self, why: &str) {
        assert_eq!(
            self.cell(OUTSIDE),
            ("P".into(), Some(PAGE_FG), Some(PAGE_BG)),
            "{why}: the page is back at full brightness"
        );
        assert_eq!(
            self.cell(INSIDE),
            ("P".into(), Some(PAGE_FG), Some(PAGE_BG)),
            "{why}: nothing of the panel or its plate is left"
        );
    }
}

fn tokens() -> abstracttui::theme::TokenSet {
    abstracttui::app::current_theme().tokens
}

#[test]
fn the_page_is_veiled_and_keeps_its_text() {
    let mut r = rig();
    r.assert_page_untouched("before open");
    let _m = r.open();
    let veil = tokens().overlay;
    assert!(
        !veil.is_opaque() && !veil.is_transparent(),
        "the scrim token carries alpha"
    );
    assert_eq!(
        r.cell(OUTSIDE),
        (
            "P".into(),
            Some(veil.over(PAGE_FG)),
            Some(veil.over(PAGE_BG))
        ),
        "outside the panel: same text, every color composited under the scrim"
    );
}

#[test]
fn the_panel_is_solid_even_where_it_paints_a_translucent_color() {
    let mut r = rig();
    let _m = r.open();
    let t = tokens();
    // The panel ground is the popover ground, opaque.
    let (_, _, ground) = r.cell(INSIDE);
    assert_eq!(
        ground,
        Some(t.surface_raised),
        "the panel ground is surface_raised"
    );
    // Its first content row painted the translucent overlay token: it
    // composites over the PLATE, never over the page's blue.
    let (text, _, row_bg) = r.cell(Point::new(8, 3));
    assert_eq!(
        text, " ",
        "the page's glyph does not show through the panel"
    );
    assert_eq!(
        row_bg,
        Some(t.overlay.over(t.surface_raised)),
        "a translucent panel color sits on the opaque plate, not on the page"
    );
}

#[test]
fn stacked_modals_each_veil_what_is_under_them() {
    let mut r = rig();
    let _a = r.open();
    let _b = r.open();
    let veil = tokens().overlay;
    let (_, _, bg) = r.cell(OUTSIDE);
    assert_eq!(
        bg,
        Some(veil.over(veil.over(PAGE_BG))),
        "the page is veiled twice"
    );
}

#[test]
fn set_scrim_false_keeps_the_page_bright_and_the_panel_solid() {
    let mut r = rig();
    let m = r.open();
    m.set_scrim(false);
    r.turn();
    assert_eq!(
        r.cell(OUTSIDE),
        ("P".into(), Some(PAGE_FG), Some(PAGE_BG)),
        "no scrim: the page outside is untouched"
    );
    assert_eq!(
        r.cell(INSIDE).2,
        Some(tokens().surface_raised),
        "the panel stays solid"
    );
    m.set_scrim(true);
    r.turn();
    assert_eq!(
        r.cell(OUTSIDE).2,
        Some(tokens().overlay.over(PAGE_BG)),
        "scrim back on"
    );
}

#[test]
fn every_close_path_takes_the_backdrop() {
    // close()
    let mut r = rig();
    let m = r.open();
    m.close();
    r.turn();
    r.assert_page_untouched("close()");

    // removing the panel layer directly
    let mut r = rig();
    let m = r.open();
    m.layer().remove();
    r.turn();
    r.assert_page_untouched("layer().remove()");

    // the outside press (default close-on-outside)
    let mut r = rig();
    let _m = r.open();
    r.term.push_input(b"\x1b[<0;2;1M\x1b[<0;2;1m");
    r.turn();
    r.assert_page_untouched("outside press");
}
