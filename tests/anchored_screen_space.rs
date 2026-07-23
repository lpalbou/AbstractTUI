//! Screen-space anchor pin, composition level: a `GraphView` with
//! hover tooltips inside a right `Drawer`, driven through the real
//! `Driver` over wire bytes (the select-inside-modal P1's drawer-shaped
//! sibling — console field report 1050). The drawer's panel is a
//! positioned overlay layer, so the tooltip's hover anchor must be
//! captured in SCREEN space: before the fix the tip opened at the
//! card's LAYER-LOCAL rect — over the root content on the far left —
//! instead of beside the hovered card.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::{App, Drawer, DrawerEdge, DrawerHandle, DrawerSize, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::LayoutStyle;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element};
use abstracttui_graph::{GraphDesc, GraphView, NodeDesc};

fn pipeline() -> GraphDesc {
    GraphDesc::new()
        .with_node(NodeDesc::new("fetch", 11, 3).label("Fetch").kind("ok"))
        .with_node(NodeDesc::new("parse", 11, 3).label("Parse").kind("ok"))
        .edge("fetch", "parse")
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    drawer: DrawerHandle,
}

fn rig() -> Rig {
    let size = Size::new(90, 26);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let handle: Rc<RefCell<Option<DrawerHandle>>> = Default::default();
    let h = handle.clone();
    app.mount(move |cx| {
        let drawer = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(40))
            .title("Graph")
            .motion(Duration::ZERO)
            .install(cx, move |dcx| {
                GraphView::new(pipeline())
                    .tooltips(Duration::ZERO)
                    .view(dcx)
            });
        *h.borrow_mut() = Some(drawer);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("root content on the left"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(abstracttui::term::Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let drawer = handle.borrow().clone().expect("drawer handle");
    Rig {
        app,
        term,
        driver,
        drawer,
    }
}

impl Rig {
    fn settle(&mut self) {
        for _ in 0..64 {
            if self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle
            {
                break;
            }
        }
    }

    fn input(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
    }

    /// SGR any-motion move report at cell (x, y).
    fn hover(&mut self, x: i32, y: i32) {
        let (c, r) = (x + 1, y + 1);
        self.input(format!("\x1b[<35;{c};{r}M").as_bytes());
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    fn find(&self, needle: &str) -> (i32, i32) {
        for (row, line) in self.screen().lines().enumerate() {
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as i32, row as i32);
            }
        }
        panic!("{needle:?} not on screen:\n{}", self.screen());
    }
}

#[test]
fn graph_view_tooltip_inside_right_drawer_opens_beside_the_hovered_card() {
    let mut r = rig();
    r.settle();
    r.drawer.open();
    r.settle();
    let (card_col, card_row) = r.find("Fetch");
    assert!(
        card_col >= 50,
        "the card renders inside the right drawer (x >= 50): {card_col}"
    );
    r.hover(card_col + 1, card_row);
    // The Duration::ZERO one-shot fires on the next turn.
    r.settle();
    // The tip text is unique ("label [kind] (id)"); the card shows
    // only the label.
    let (tip_col, tip_row) = r.find("Fetch [ok] (fetch)");
    assert!(
        tip_col >= 50,
        "tip beside the hovered card INSIDE the drawer's screen x-range \
         (it used to open at the card's layer-local rect, over the root \
         content on the far left): col {tip_col}\n{}",
        r.screen()
    );
    assert!(
        (tip_row - card_row).abs() <= 4,
        "tip adjacent to the hovered card's screen rows: tip row \
         {tip_row}, card row {card_row}\n{}",
        r.screen()
    );
}
