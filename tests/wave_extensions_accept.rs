//! Extensions acceptance battery (wave 9): the "pipeline monitor" a
//! fresh app author would build from the crates' READMEs alone —
//! `abstracttui-graph` + `abstracttui-mermaid` composed with the app
//! shell (PageHost pages, a right Drawer) and driven end to end
//! through the REAL Driver over wire bytes. This is the composition
//! proof: extensions x PageHost x Drawer, public API only (ADR-0004).
//!
//! Scene: page "Monitor" hosts a layered GraphView workflow (kind
//! tints, one retry CYCLE showing the broken-edge marker, a live
//! badge fed by a signal); page "Diagram" renders a mermaid flowchart
//! from source; `on_node_press` opens a right drawer with the node's
//! detail. Driven: keyboard + click selection, drawer open/Escape,
//! live badge update (bar repaints, the graph does NOT remount —
//! selection survives), page switches both ways, and a final park
//! with the zero-idle pin.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::{App, Drawer, DrawerEdge, DrawerSize, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::{Dimension, LayoutStyle, Signal};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::PageHost;
use abstracttui_graph::{EdgeDesc, GraphDesc, GraphStyle, GraphView, NodeDesc};
use abstracttui_mermaid::MermaidView;

fn pipeline() -> GraphDesc {
    GraphDesc::new()
        .with_node(NodeDesc::new("fetch", 11, 3).label("Fetch").kind("ok"))
        .with_node(NodeDesc::new("parse", 11, 3).label("Parse").kind("ok"))
        .with_node(NodeDesc::new("render", 12, 3).label("Render").kind("warn"))
        .with_node(NodeDesc::new("publish", 13, 3).label("Publish").kind("ok"))
        .edge("fetch", "parse")
        .edge("parse", "render")
        .edge("render", "publish")
        // The retry loop: cycle-broken, rendered dotted in the
        // honesty ink.
        .with_edge(EdgeDesc::new("publish", "fetch").label("retry"))
}

const MERMAID_SRC: &str =
    "graph TD\nA[Ingest] --> B{Valid?}\nB -->|yes| C(Store)\nB -.->|no| D[Reject]";

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    queue: Signal<u32>,
    pressed: Rc<RefCell<Vec<String>>>,
}

fn rig() -> Rig {
    let size = Size::new(90, 26);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let pressed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = pressed.clone();
    let queue_slot: Rc<std::cell::Cell<Option<Signal<u32>>>> = Rc::new(std::cell::Cell::new(None));
    let qs = queue_slot.clone();
    app.mount(move |cx| {
        // Durable app state OUTSIDE the page builders (the PageHost
        // no-keep-alive rule from its docs).
        let queue = cx.signal(7u32);
        qs.set(Some(queue));
        let detail = cx.signal(String::new());

        // The node-detail drawer: instant motion, modal (Esc closes).
        let drawer = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(26))
            .title("Node detail")
            .motion(Duration::ZERO)
            .install(cx, move |_dcx| {
                Element::new()
                    .style(LayoutStyle::column().grow(1.0))
                    .child(dyn_view(
                        LayoutStyle::default().height(Dimension::Cells(1)),
                        move || text(format!("node: {}", detail.get())),
                    ))
                    .build()
            });

        let sink = sink.clone();
        let host = PageHost::new()
            .page("monitor", "Monitor", move |gcx| {
                let t = abstracttui::app::use_theme(gcx).get().tokens;
                let style = GraphStyle::from_tokens(&t)
                    .kind_accent("ok", t.ok)
                    .kind_accent("warn", t.warn);
                let sink = sink.clone();
                let drawer = drawer.clone();
                GraphView::new(pipeline())
                    .style(style)
                    .badges(move |id| (id == "fetch").then(|| queue.get().to_string()))
                    .on_node_press(move |id| {
                        sink.borrow_mut().push(id.to_string());
                        detail.set(id.to_string());
                        drawer.open();
                    })
                    .view(gcx)
            })
            .page("diagram", "Diagram", move |gcx| {
                MermaidView::new(MERMAID_SRC).view(gcx)
            });

        Element::new()
            .style(LayoutStyle::column())
            .child(host.view(cx))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(abstracttui::term::Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.kitty_keyboard = true;
        })),
        enter: None,
        probe: false,
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    Rig {
        app,
        term,
        driver,
        queue: queue_slot.get().expect("queue"),
        pressed,
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

    fn click(&mut self, x: i32, y: i32) {
        let (c, r) = (x + 1, y + 1);
        self.input(format!("\x1b[<0;{c};{r}M\x1b[<0;{c};{r}m").as_bytes());
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

    fn visible(&self, needle: &str) -> bool {
        self.screen().contains(needle)
    }
}

#[test]
fn pipeline_monitor_scene_end_to_end() {
    let mut r = rig();
    r.settle();

    // ---- page 1: the workflow renders with badges + honesty -------
    assert!(r.visible("Monitor") && r.visible("Diagram"), "tab bar up");
    assert!(r.visible("Fetch") && r.visible("Publish"), "cards render");
    assert!(r.visible("7"), "the live badge shows the initial queue");
    assert!(r.visible("retry"), "the cycle edge carries its label");

    // ---- keyboard: select + press opens the drawer -----------------
    // Fresh-author finding (kept in the battery deliberately): inside
    // a PageHost the FIRST Tab lands on the page-tab bar (its
    // documented single tab stop); the graph is the SECOND stop.
    r.input(b"\t\t");
    r.input(b"\r"); // Enter: select the first node (fetch)
    assert!(r.pressed.borrow().is_empty(), "first Enter selects only");
    r.input(b"\r"); // Enter: press -> drawer opens
    assert_eq!(r.pressed.borrow().as_slice(), ["fetch"]);
    assert!(r.visible("Node detail"), "drawer opened:\n{}", r.screen());
    assert!(r.visible("node: fetch"), "drawer shows the pressed node");

    // Escape closes the modal drawer.
    r.input(b"\x1b[27u");
    assert!(!r.visible("Node detail"), "Escape closed the drawer");

    // ---- click: select another node, drawer follows ----------------
    let (col, row) = r.find("Parse");
    r.click(col, row);
    assert_eq!(r.pressed.borrow().as_slice(), ["fetch", "parse"]);
    assert!(r.visible("node: parse"));
    r.input(b"\x1b[27u");

    // ---- live badge update: bar repaints, graph does NOT remount ---
    // The parse card is still SELECTED (its restyle survives), which
    // is only possible if the badge change re-rendered the badge's
    // card and nothing remounted the graph (internal selection state
    // lives in the mounted view).
    let _ = r.term.take_bytes();
    r.queue.set(42);
    r.settle();
    assert!(r.visible("42"), "the badge updated live");
    assert!(!r.visible("7"), "the old count is gone");
    let bytes = r.term.take_bytes();
    assert!(!bytes.is_empty(), "the update repainted");
    r.input(b"\r"); // Enter presses the STILL-SELECTED parse node
    assert_eq!(
        r.pressed.borrow().as_slice(),
        ["fetch", "parse", "parse"],
        "selection survived the badge update — the graph did not remount"
    );
    r.input(b"\x1b[27u");
    assert!(
        !r.visible("Node detail"),
        "third Escape closed the drawer:\n{}",
        r.screen()
    );

    // ---- page switch: mermaid renders; page 1 disposes --------------
    // (This click found a REAL defect in cycle 3: GraphView cards used
    // to fire on mouse DOWN, and the drawer opened by on_node_press
    // swallowed the release — the tree's pointer capture stuck on the
    // card and this tab click pressed "parse" again instead of
    // switching pages. Cards now fire on release-inside, the engine's
    // Button convention.)
    let (col, row) = r.find("Diagram");
    r.click(col, row);
    assert!(
        r.visible("Ingest") && r.visible("Valid?"),
        "mermaid page:\n{}",
        r.screen()
    );
    assert!(r.visible("yes") && r.visible("no"), "edge labels render");
    assert!(!r.visible("Fetch"), "page 1 unmounted (no keep-alive)");

    // ---- and back: the workflow rebuilds fresh ----------------------
    let (col, row) = r.find("Monitor");
    r.click(col, row);
    assert!(r.visible("Fetch"), "workflow page rebuilt");
    assert!(r.visible("42"), "badge reads the durable signal");
    assert!(!r.visible("Ingest"), "mermaid page unmounted");

    // ---- park: the whole composed scene idles at zero ---------------
    let _ = r.term.take_bytes();
    for _ in 0..16 {
        let turn = r.driver.turn(&mut r.app, &mut r.term).expect("idle turn");
        assert!(turn.idle && !turn.rendered, "parked scene stays idle");
    }
    assert!(
        r.term.bytes().is_empty(),
        "a parked pipeline monitor emits zero bytes"
    );
}
