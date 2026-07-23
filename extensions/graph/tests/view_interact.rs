//! GraphView interaction through the REAL engine loop: App + Driver +
//! CaptureTerm, events as wire bytes (SGR mouse, CSI keys) — the same
//! harness the engine's own wave tests ride, reachable here because
//! it is all public API (ADR-0004: extensions build on public API
//! only, and that includes their tests).
//!
//! Covers: click-select + press, the keyboard vocabulary end to end,
//! wheel/keyboard pan, hover tooltips (appear + dismiss), the
//! zero-idle pin, and damage containment on selection change.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rgba, Size};
use abstracttui::prelude::{Dimension, LayoutStyle, Signal};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element};
use abstracttui_graph::{GraphDesc, GraphStyle, GraphView, NodeDesc};

fn test_style() -> GraphStyle {
    GraphStyle {
        card_bg: Rgba::rgb(10, 10, 30),
        card_border: Rgba::rgb(100, 100, 100),
        card_border_selected: Rgba::rgb(255, 200, 0),
        card_title: Rgba::rgb(230, 230, 230),
        badge: Rgba::rgb(80, 160, 255),
        edge: Rgba::rgb(140, 140, 140),
        edge_broken: Rgba::rgb(255, 60, 60),
        edge_label: Rgba::rgb(90, 90, 90),
        notice: Rgba::rgb(255, 180, 0),
        kind_accents: Vec::new(),
    }
}

/// Three-rank diamond used by every scenario; 10-wide cards so the
/// labels render untruncated, wide enough to overflow an 18-cell
/// viewport for the pan tests.
fn diamond() -> GraphDesc {
    GraphDesc::new()
        .with_node(NodeDesc::new("a", 10, 3).label("Alpha").kind("svc"))
        .with_node(NodeDesc::new("b", 10, 3).label("Beta"))
        .with_node(NodeDesc::new("c", 10, 3).label("Gamma"))
        .with_node(NodeDesc::new("d", 10, 3).label("Delta"))
        .edge("a", "b")
        .edge("a", "c")
        .edge("b", "d")
        .edge("c", "d")
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    pressed: Rc<RefCell<Vec<String>>>,
    ox: Signal<i32>,
    oy: Signal<i32>,
}

fn rig(size: Size, tooltips: bool) -> Rig {
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let pressed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = pressed.clone();
    let ox_slot: Rc<std::cell::Cell<Option<Signal<i32>>>> = Rc::new(std::cell::Cell::new(None));
    let oy_slot: Rc<std::cell::Cell<Option<Signal<i32>>>> = Rc::new(std::cell::Cell::new(None));
    let (oxs, oys) = (ox_slot.clone(), oy_slot.clone());
    app.mount(move |cx| {
        let ox = cx.signal(0i32);
        let oy = cx.signal(0i32);
        oxs.set(Some(ox));
        oys.set(Some(oy));
        let mut gv = GraphView::new(diamond())
            .style(test_style())
            .offset_x(ox)
            .offset_y(oy)
            .on_node_press(move |id| sink.borrow_mut().push(id.to_string()));
        if tooltips {
            gv = gv.tooltips(Duration::ZERO);
        }
        Element::new()
            .style(LayoutStyle::column())
            // A marker row OUTSIDE the widget: damage containment is
            // "this row never repaints on graph-internal changes".
            .child(
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .height(Dimension::Cells(1))
                            .shrink(0.0),
                    )
                    .child(text("HEADER"))
                    .build(),
            )
            .child(gv.view(cx))
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
        pressed,
        ox: ox_slot.get().expect("ox"),
        oy: oy_slot.get().expect("oy"),
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

    /// SGR left click (press + release) at 0-based screen cell.
    fn click(&mut self, x: i32, y: i32) {
        let (c, r) = (x + 1, y + 1);
        self.input(format!("\x1b[<0;{c};{r}M\x1b[<0;{c};{r}m").as_bytes());
    }

    fn screen_text(&self) -> String {
        self.term.screen().to_text()
    }
}

/// Screen (col, row) of the first occurrence of `needle` (chars, not
/// bytes — the borders are multibyte).
fn find_on_screen(screen: &str, needle: &str) -> (i32, i32) {
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte) = line.find(needle) {
            return (line[..byte].chars().count() as i32, row as i32);
        }
    }
    panic!("{needle:?} not on screen:\n{screen}");
}

#[test]
fn click_selects_and_presses_through_wire_bytes() {
    let mut r = rig(Size::new(40, 20), false);
    r.settle();
    // Node "a" sits at the top rank (row 0 is the header). Click its
    // title glyph.
    let (col, row) = find_on_screen(&r.screen_text(), "Alpha");
    r.click(col, row);
    assert_eq!(r.pressed.borrow().as_slice(), ["a"], "click pressed a");
    // Click Beta: selection moves and presses again.
    let (col, row) = find_on_screen(&r.screen_text(), "Beta");
    r.click(col, row);
    assert_eq!(r.pressed.borrow().as_slice(), ["a", "b"]);
}

#[test]
fn keyboard_vocabulary_selects_moves_presses_and_pans() {
    let mut r = rig(Size::new(18, 12), false);
    r.settle();
    // Focus the widget's one tab stop (the scroll viewport).
    r.input(b"\t");

    // No selection: arrows PAN (the graph is wider than 18 cells).
    assert_eq!(r.ox.get_untracked(), 0);
    r.input(b"\x1b[C"); // Right
    assert!(r.ox.get_untracked() > 0, "arrow panned horizontally");
    r.input(b"\x1b[D"); // back left
    assert_eq!(r.ox.get_untracked(), 0);

    // Enter selects the first node (no press), Enter again presses.
    r.input(b"\r");
    assert!(r.pressed.borrow().is_empty(), "first Enter only selects");
    r.input(b"\r");
    assert_eq!(r.pressed.borrow().as_slice(), ["a"]);

    // Arrows now move the SELECTION, not the pan. The vocabulary is
    // aligned-first spatial: Down from the diamond's apex lands on
    // the ALIGNED sink d (perpendicular offset is doubly penalized);
    // the side nodes are one more arrow away (Left from d -> b).
    let ox_before = r.ox.get_untracked();
    r.input(b"\x1b[B"); // Down: a -> d (aligned wins)
    r.input(b"\r");
    assert_eq!(r.pressed.borrow().as_slice(), ["a", "d"]);
    assert_eq!(
        r.ox.get_untracked(),
        ox_before,
        "selection movement does not pan horizontally"
    );
    r.input(b"\x1b[D"); // Left: d -> b (the left flank)
    r.input(b"\r");
    assert_eq!(r.pressed.borrow().as_slice(), ["a", "d", "b"]);

    // Escape deselects; arrows pan again.
    r.input(b"\x1b[27u"); // kitty Escape (unambiguous on the wire)
    r.input(b"\x1b[C");
    assert!(r.ox.get_untracked() > 0, "post-Escape arrows pan");
}

#[test]
fn wheel_pans_vertically() {
    let mut r = rig(Size::new(18, 8), false);
    r.settle();
    assert_eq!(r.oy.get_untracked(), 0);
    // SGR wheel down over the graph area.
    r.input(b"\x1b[<65;6;5M");
    assert!(r.oy.get_untracked() > 0, "wheel panned down");
}

#[test]
fn parked_graph_view_idles_at_zero() {
    let mut r = rig(Size::new(40, 20), true);
    r.settle();
    let _ = r.term.take_bytes();
    for _ in 0..16 {
        let turn = r.driver.turn(&mut r.app, &mut r.term).expect("idle turn");
        assert!(turn.idle, "turn must report idle");
        assert!(!turn.rendered, "idle turn rendered");
    }
    assert!(
        r.term.bytes().is_empty(),
        "a parked GraphView emitted bytes"
    );
}

#[test]
fn selection_change_damage_stays_inside_the_graph_region() {
    let mut r = rig(Size::new(40, 20), false);
    r.settle();
    let before = r.screen_text();
    let header_before = before.lines().next().unwrap().to_string();
    assert!(header_before.contains("HEADER"), "precondition");
    let _ = r.term.take_bytes();

    // Select a card (click on Alpha's title).
    let (col, row) = find_on_screen(&before, "Alpha");
    r.click(col, row);

    let bytes = r.term.take_bytes();
    assert!(!bytes.is_empty(), "the selection restyle repainted");
    // Byte-level containment: no cursor MOVE targets screen row 1
    // (the header row). CSI moves are `\x1b[1;<col>H` — terminator
    // 'H'; a bare `\x1b[1;...m` is SGR (bold) and must not trip this.
    let s = String::from_utf8_lossy(&bytes);
    let mut rest = s.as_ref();
    while let Some(i) = rest.find("\x1b[1;") {
        let tail = &rest[i + 4..];
        let terminator = tail.chars().find(|c| c.is_ascii_alphabetic());
        assert_ne!(
            terminator,
            Some('H'),
            "selection change moved the cursor into the header row: {s:?}"
        );
        rest = tail;
    }
    // And the visible header text is untouched.
    let after = r.screen_text();
    assert_eq!(after.lines().next().unwrap(), header_before);
    // The restyle is a COLOR change (glyphs stay): the repaint bytes
    // must carry the selection ink (255,200,0 from test_style).
    assert!(
        s.contains("255;200;0"),
        "the card restyled in the selection ink: {s:?}"
    );
}

/// Cycle-3 attack item: tooltip lifetime across a SELECTION REBUILD.
/// Clicking the hovered card re-renders it (dyn per-generation scope
/// disposal closes the tip — a stale tip over a restyled card would
/// lie); pointer motion over the rebuilt card re-arms and re-shows.
/// Pinned so the behavior is a decision, not an accident.
#[test]
fn tooltip_survives_selection_via_rehover_not_staleness() {
    let mut r = rig(Size::new(40, 20), true);
    r.settle();
    let (col, row) = find_on_screen(&r.screen_text(), "Alpha");
    r.input(format!("\x1b[<35;{};{}M", col + 1, row + 1).as_bytes());
    r.settle();
    r.input(b"");
    assert!(r.screen_text().contains("Alpha [svc]"), "tip up pre-click");

    // Click the hovered card: selection restyles it (rebuild).
    r.click(col, row);
    r.settle();
    r.input(b""); // give a re-armed tip its after(0) turn, if any
    let post_click = r.screen_text().contains("Alpha [svc]");

    // Move one cell within the card: motion must (re)show the tip on
    // the REBUILT card — the affordance never dies with the rebuild.
    r.input(format!("\x1b[<35;{};{}M", col + 2, row + 1).as_bytes());
    r.settle();
    r.input(b"");
    assert!(
        r.screen_text().contains("Alpha [svc]"),
        "motion over the rebuilt card shows its tip (post-click state was {post_click})"
    );
}

#[test]
fn tooltip_appears_on_hover_and_dismisses_on_leave() {
    let mut r = rig(Size::new(40, 20), true);
    r.settle();
    let screen = r.screen_text();
    assert!(
        !screen.contains("[svc]"),
        "no tooltip before hover: {screen}"
    );
    // Hover Alpha's card (SGR motion event, any-motion encoding).
    let (col, row) = find_on_screen(&screen, "Alpha");
    r.input(format!("\x1b[<35;{};{}M", col + 1, row + 1).as_bytes());
    // The zero-delay tip arms an `after(0)`: give it turns to fire.
    r.settle();
    r.input(b""); // one more settle round for the layer paint
    let hovered = r.screen_text();
    assert!(
        hovered.contains("Alpha [svc]"),
        "tooltip shows label + kind: {hovered}"
    );
    // Leave: move far away — the tip hides.
    r.input(b"\x1b[<35;39;19M");
    let left = r.screen_text();
    assert!(!left.contains("[svc]"), "tooltip dismissed: {left}");
}
