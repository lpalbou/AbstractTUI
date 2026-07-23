//! Axes (c) + (d) — Modal panels LARGER than the viewport and Drawer
//! extents at small widths.
//!
//! The console fleet opens 78x27 and 90x22 modals; small shells run
//! 70x20 and 40x12. The engine must clamp the panel inside the
//! viewport at OPEN, keep the fixed chrome rows (the 0240 floor), and
//! leave the oversized middle scrollable. A RESIZE while open must
//! re-clamp — before this wave the at-open bounds were kept forever,
//! and a shrink could park a focus-trapped modal ENTIRELY off-screen
//! (invisible panel owning every key = the app reads as locked).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerHandle, DrawerSize};
use abstracttui::app::{App, Driver, Modal};
use abstracttui::base::{Rect, Size};
use abstracttui::layout::{Direction, Style as LayoutStyle};
use abstracttui::reactive::Scope;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{text, Element, View};
use abstracttui::widgets::Scroll;

use crate::harness::{config, drive_to_idle};

/// The console modal shape: title(1) + scrollable body (grow) +
/// buttons(3). `Modal::open`'s 0240 floor pins the fixed rows; the
/// Scroll (basis 0) absorbs the loss.
fn console_modal_content(mcx: Scope) -> View {
    let body: String = (0..60)
        .map(|i| format!("mrow-{i:03} payload"))
        .collect::<Vec<_>>()
        .join("\n");
    Element::new()
        .style(LayoutStyle {
            direction: Direction::Column,
            ..LayoutStyle::fill()
        })
        .child(
            Element::new()
                .style(LayoutStyle::line(1))
                .child(text("MODAL-TITLE"))
                .build(),
        )
        .child(Scroll::new(text(body)).view(mcx))
        .child(
            Element::new()
                .style(LayoutStyle::line(3))
                .child(text("[ OK ]  [ Cancel ]"))
                .build(),
        )
        .build()
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    vt: VtScreen,
    scope: Scope,
}

/// Root app: distinct background rows, plus the captured mount scope
/// for opening overlays from the test.
fn rig(size: Size) -> Rig {
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let ss = scope_slot.clone();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..40 {
            col = col.child(text(format!("bg-row-{i:02} ..............")));
        }
        col.build()
    })
    .expect("mount");
    let scope = scope_slot.borrow().expect("scope");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    Rig {
        app,
        term,
        driver,
        vt,
        scope,
    }
}

impl Rig {
    fn settle(&mut self) {
        drive_to_idle(
            &mut self.driver,
            &mut self.app,
            &mut self.term,
            &mut self.vt,
        );
    }

    fn screen(&self) -> String {
        self.vt.to_text()
    }
}

/// (c) A modal REQUESTED larger than the viewport clamps inside it at
/// open — never panics — with the title and button rows visible (0240
/// floor) and the body scrollable in the remaining rows.
#[test]
fn oversized_modal_clamps_at_open_and_body_scrolls() {
    for &(vp, panel) in &[
        (Size::new(70, 20), Size::new(78, 27)),
        (Size::new(40, 12), Size::new(90, 22)),
    ] {
        let mut r = rig(vp);
        let modal = Modal::open(&r.app.overlays(), r.scope, vp, panel, console_modal_content);
        r.settle();

        let bounds = modal.layer().bounds().expect("layer alive");
        assert_eq!(
            bounds,
            Rect::new(0, 0, panel.w.min(vp.w), panel.h.min(vp.h)),
            "{vp:?}: an oversized panel must clamp to the viewport"
        );
        let screen = r.screen();
        assert!(
            screen.contains("MODAL-TITLE"),
            "{vp:?}: title row survives the clamp:\n{screen}"
        );
        assert!(
            screen.contains("[ OK ]"),
            "{vp:?}: button row survives the clamp (0240 floor):\n{screen}"
        );
        assert!(
            screen.contains("mrow-000"),
            "{vp:?}: body top visible before scrolling:\n{screen}"
        );

        // The body is honestly SCROLLABLE, not silently lost: PageDown
        // moves the window (focus_init landed on the Scroll).
        r.term.push_input(b"\x1b[6~");
        r.settle();
        let screen = r.screen();
        assert!(
            !screen.contains("mrow-000") && screen.contains("MODAL-TITLE"),
            "{vp:?}: PageDown must scroll the clamped body:\n{screen}"
        );
        modal.close();
        r.settle();
    }
}

/// (c) RESIZE while open re-clamps: a modal opened centered on a wide
/// terminal must follow a shrink back inside the new viewport (a
/// focus-trapped panel parked off-screen reads as a locked app), and
/// re-center/grow back toward its requested size on expansion.
#[test]
fn oversized_modal_reclamps_on_resize() {
    let start = Size::new(200, 20);
    let panel = Size::new(78, 27);
    let mut r = rig(start);
    let modal = Modal::open(
        &r.app.overlays(),
        r.scope,
        start,
        panel,
        console_modal_content,
    );
    r.settle();
    assert_eq!(
        modal.layer().bounds().expect("alive"),
        Rect::new(61, 0, 78, 20),
        "centered at open on the wide terminal"
    );

    // Shrink to the brutal floor: the panel must clamp INSIDE 40x12.
    r.term.push_resize(Size::new(40, 12));
    r.settle();
    let bounds = modal.layer().bounds().expect("alive");
    assert_eq!(
        bounds,
        Rect::new(0, 0, 40, 12),
        "shrink must re-clamp the open modal inside the viewport \
         (an off-screen focus trap locks the app)"
    );
    let screen = r.screen();
    assert!(
        screen.contains("MODAL-TITLE") && screen.contains("[ OK ]"),
        "modal chrome visible after the shrink:\n{screen}"
    );

    // Grow back: the panel re-centers and recovers its REQUESTED size
    // where it fits (height still clamped by the 24-row terminal).
    r.term.push_resize(Size::new(100, 24));
    r.settle();
    assert_eq!(
        modal.layer().bounds().expect("alive"),
        Rect::new(11, 0, 78, 24),
        "growth re-centers and un-clamps toward the requested size"
    );
    let screen = r.screen();
    assert!(
        screen.contains("MODAL-TITLE") && screen.contains("[ OK ]"),
        "modal chrome visible after the growth:\n{screen}"
    );
    modal.close();
    r.settle();
}

/// (d) Drawer extents at small widths: Percent rounds against the
/// axis, Cells clamps to the viewport, and a resize while open
/// re-solves — all pinned through the real pipeline.
#[test]
fn drawer_extents_clamp_at_small_widths() {
    let open_drawer = |r: &mut Rig, size: DrawerSize| -> DrawerHandle {
        let handle = Drawer::new(DrawerEdge::Right)
            .size(size)
            .motion(Duration::ZERO)
            .title("Details")
            .overlays(&r.app.overlays())
            .install(r.scope, |_| text("DRAWER-BODY"));
        handle.open();
        r.settle();
        handle
    };

    // Percent(0.42) of 60 = 25.2 -> 25 columns, full height.
    let mut r = rig(Size::new(60, 16));
    let h = open_drawer(&mut r, DrawerSize::Percent(0.42));
    assert_eq!(
        h.layer().and_then(|l| l.bounds()).expect("open"),
        Rect::new(35, 0, 25, 16),
        "Percent(0.42) of 60 rounds to 25"
    );
    assert!(r.screen().contains("DRAWER-BODY"), "{}", r.screen());
    h.close();
    r.settle();

    // Percent(0.42) of 40 = 16.8 -> 17 columns.
    let mut r = rig(Size::new(40, 12));
    let h = open_drawer(&mut r, DrawerSize::Percent(0.42));
    assert_eq!(
        h.layer().and_then(|l| l.bounds()).expect("open"),
        Rect::new(23, 0, 17, 12),
        "Percent(0.42) of 40 rounds to 17"
    );
    h.close();
    r.settle();

    // Cells(30) on a 25-column terminal clamps to the axis.
    let mut r = rig(Size::new(25, 12));
    let h = open_drawer(&mut r, DrawerSize::Cells(30));
    assert_eq!(
        h.layer().and_then(|l| l.bounds()).expect("open"),
        Rect::new(0, 0, 25, 12),
        "Cells(30) on 25 columns clamps to 25"
    );

    // Resize while open re-solves (the documented drawer contract).
    r.term.push_resize(Size::new(80, 20));
    r.settle();
    assert_eq!(
        h.layer().and_then(|l| l.bounds()).expect("open"),
        Rect::new(50, 0, 30, 20),
        "growth un-clamps Cells(30) to its requested extent"
    );
    h.close();
    r.settle();
}
