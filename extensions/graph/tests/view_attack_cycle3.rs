//! Cycle-3 attack tests on GraphView (the open list from the cycle-2
//! report + mermaid's cross-crate ask): the arrowless `open` edge
//! vocabulary, bow amplitude at 3+ parallels / short chords,
//! aligned-first navigation properties on cluttered force layouts,
//! `ensure_visible` under padded roots, and edge-label overprint.
//! Dispositions recorded in reviews/wave9/canvas-final-attack.md.

use std::cell::Cell;
use std::rc::Rc;

use abstracttui::base::{Point, Rgba, Size};
use abstracttui::reactive::{create_root, flush_effects, Signal};
use abstracttui::ui::{BufferCanvas, Key, KeyEvent, UiEvent, UiTree};
use abstracttui_graph::{
    force, EdgeDesc, EdgeLayout, ForceOpts, GraphAlgo, GraphDesc, GraphStyle, GraphView,
    LayeredOpts, Layout, NodeLayout, Rect,
};

fn test_style() -> GraphStyle {
    GraphStyle {
        card_bg: Rgba::rgb(10, 10, 30),
        card_border: Rgba::rgb(100, 100, 100),
        card_border_selected: Rgba::rgb(255, 200, 0),
        card_title: Rgba::rgb(230, 230, 230),
        badge: Rgba::rgb(80, 160, 255),
        edge: Rgba::rgb(140, 140, 140),
        edge_broken: Rgba::rgb(255, 60, 60),
        edge_label: Rgba::rgb(90, 240, 90),
        notice: Rgba::rgb(255, 180, 0),
        kind_accents: Vec::new(),
    }
}

struct Rig {
    _root: abstracttui::reactive::RootScope,
    tree: UiTree,
    size: Size,
}

impl Rig {
    fn mount(size: Size, build: impl FnOnce(abstracttui::reactive::Scope) -> GraphView) -> Rig {
        let mut tree = UiTree::new(size);
        let (_root, ()) = create_root(|cx| {
            let view = build(cx).view(cx);
            tree.mount(cx, view);
        });
        Rig { _root, tree, size }
    }

    fn draw(&mut self) -> BufferCanvas {
        let mut canvas = BufferCanvas::new(self.size);
        self.tree.draw(&mut canvas);
        canvas
    }

    fn key(&mut self, key: Key) {
        self.tree.dispatch(&UiEvent::Key(KeyEvent::plain(key)));
        flush_effects();
    }
}

fn find_chars(canvas: &BufferCanvas, size: Size, set: &[char]) -> Vec<(char, i32, i32)> {
    let mut out = Vec::new();
    for y in 0..size.h {
        for x in 0..size.w {
            let ch = canvas.cell(Point::new(x, y)).unwrap().0;
            if set.contains(&ch) {
                out.push((ch, x, y));
            }
        }
    }
    out
}

fn is_braille(ch: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&ch)
}

fn stroke_cells(canvas: &BufferCanvas, size: Size, ink: Rgba) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..size.h {
        for x in 0..size.w {
            let (ch, fg, _) = canvas.cell(Point::new(x, y)).unwrap();
            if is_braille(ch) && fg == ink {
                out.push((x, y));
            }
        }
    }
    out
}

const ARROWS: [char; 4] = ['▲', '▼', '◀', '▶'];

// ---------------------------------------------------------------------------
// M1: the `open` (arrowless) edge vocabulary — mermaid's `---`.
// ---------------------------------------------------------------------------

#[test]
fn open_style_edges_render_without_an_arrowhead() {
    let size = Size::new(20, 12);
    let mut rig = Rig::mount(size, |_| {
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .with_edge(EdgeDesc::new("a", "b").style("open"));
        GraphView::new(desc).style(test_style())
    });
    let c = rig.draw();
    assert!(
        find_chars(&c, size, &ARROWS).is_empty(),
        "an `open` link draws no arrowhead"
    );
    assert!(
        !stroke_cells(&c, size, test_style().edge).is_empty(),
        "the open link still strokes"
    );

    // Control: the same graph with a plain edge keeps its arrow, and
    // an open edge stays SOLID (arrowless != dotted).
    let mut plain = Rig::mount(size, |_| {
        GraphView::new(
            GraphDesc::new()
                .node("a", 5, 3)
                .node("b", 5, 3)
                .edge("a", "b"),
        )
        .style(test_style())
    });
    let pc = plain.draw();
    assert_eq!(find_chars(&pc, size, &ARROWS).len(), 1);
    let solid_dots = dot_count(&pc, size, test_style().edge);
    let open_dots = dot_count(&c, size, test_style().edge);
    // The open edge has MORE lit dots than the arrowed one (the arrow
    // cell no longer replaces a stroke cell) — definitely not sparser.
    assert!(
        open_dots >= solid_dots,
        "open stays a solid stroke ({open_dots} vs {solid_dots} dots)"
    );
}

fn dot_count(c: &BufferCanvas, size: Size, ink: Rgba) -> u32 {
    let mut dots = 0u32;
    for y in 0..size.h {
        for x in 0..size.w {
            let (ch, fg, _) = c.cell(Point::new(x, y)).unwrap();
            if is_braille(ch) && fg == ink {
                dots += (ch as u32 - 0x2800).count_ones();
            }
        }
    }
    dots
}

// ---------------------------------------------------------------------------
// Bow amplitude: 3+ parallels on a short chord stay in bounds.
// ---------------------------------------------------------------------------

#[test]
fn triple_parallel_edges_on_a_short_chord_stay_visible_and_in_bounds() {
    // Tight hand layout: chord row 1 of a 5-row content box, so the
    // outer bows (ordinals -2/0/+2 => ~2-cell apexes) would leave the
    // grid without clamping. Every edge must still contribute cells,
    // and rows must spread (the bows separate).
    let size = Size::new(26, 6);
    let mut rig = Rig::mount(size, |_| {
        let nodes = vec![
            NodeLayout::new("a", Rect::new(0, 0, 5, 3), 0),
            NodeLayout::new("b", Rect::new(18, 0, 5, 3), 0),
        ];
        let wp = |y: i32| vec![Point::new(5, y), Point::new(17, y)];
        let edges = vec![
            EdgeLayout::new("a", "b", 0, wp(1)),
            EdgeLayout::new("a", "b", 1, wp(1)),
            EdgeLayout::new("a", "b", 2, wp(1)),
        ];
        let layout = Layout::new(nodes, edges);
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .edge("a", "b")
            .edge("a", "b")
            .edge("a", "b");
        GraphView::new(desc).style(test_style()).with_layout(layout)
    });
    let c = rig.draw();
    let cells = stroke_cells(&c, size, test_style().edge);
    assert!(
        cells.len() >= 12,
        "three separated strokes cover ground: {cells:?}"
    );
    let mut rows: Vec<i32> = cells.iter().map(|&(_, y)| y).collect();
    rows.sort_unstable();
    rows.dedup();
    assert!(
        rows.len() >= 2,
        "bows separate across rows even when clamped: {rows:?}"
    );
    // Nothing rendered outside the content bounds (the layout box is
    // 5 rows tall; the widget adds no notice row here).
    assert!(
        cells.iter().all(|&(_, y)| y <= 4),
        "clamped bows never escape the content box: {cells:?}"
    );
}

// ---------------------------------------------------------------------------
// Aligned-first navigation: structural properties on a cluttered
// force layout (no ping-pong within a direction, no stranding).
// ---------------------------------------------------------------------------

#[test]
fn spatial_navigation_never_strands_and_directional_walks_terminate() {
    // A cluttered seeded force layout, driven through the REAL widget
    // (Enter + arrows), pinning the properties structurally: from
    // every node, at least one direction moves; a Down-walk from the
    // first node terminates without revisiting (y strictly grows).
    let mut desc = GraphDesc::new();
    for i in 0..12 {
        desc = desc.node(format!("n{i}"), 7, 3);
    }
    for i in 0..12 {
        desc = desc.edge(format!("n{i}"), format!("n{}", (i * 5 + 3) % 12));
    }
    let layout = force(&desc, &ForceOpts::default());

    // Property harness on the SAME public geometry the widget uses.
    let centers: Vec<(String, (i32, i32))> = layout
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                (2 * n.rect.x + n.rect.w, 2 * n.rect.y + n.rect.h),
            )
        })
        .collect();
    let next = |cur: usize, dir: (i32, i32)| -> Option<usize> {
        let c0 = centers[cur].1;
        let mut best: Option<(i64, usize)> = None;
        for (i, (_, c)) in centers.iter().enumerate() {
            if i == cur {
                continue;
            }
            let (vx, vy) = (i64::from(c.0 - c0.0), i64::from(c.1 - c0.1));
            let fwd = vx * i64::from(dir.0) + vy * i64::from(dir.1);
            if fwd <= 0 {
                continue;
            }
            let perp = if dir.0 != 0 { vy.abs() } else { vx.abs() };
            let score = fwd + 2 * perp;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    };
    for start in 0..centers.len() {
        let moves = [(0, -1), (0, 1), (-1, 0), (1, 0)]
            .into_iter()
            .filter(|&d| next(start, d).is_some())
            .count();
        assert!(moves >= 1, "node {start} is stranded");
        // Directional walk terminates without revisits: forward > 0
        // strictly increases the projection, so a cycle is impossible.
        let mut seen = vec![start];
        let mut cur = start;
        while let Some(n) = next(cur, (0, 1)) {
            assert!(!seen.contains(&n), "Down-walk revisited node {n}");
            seen.push(n);
            cur = n;
            assert!(seen.len() <= centers.len(), "walk exceeded node count");
        }
    }

    // And the real widget agrees with the harness for the first hop.
    let size = Size::new(60, 30);
    let d2 = desc.clone();
    let sel_slot: Rc<Cell<Option<Signal<Option<String>>>>> = Rc::new(Cell::new(None));
    let slot = sel_slot.clone();
    let mut rig = Rig::mount(size, move |cx| {
        let sel = cx.signal(None::<String>);
        slot.set(Some(sel));
        GraphView::new(d2)
            .style(test_style())
            .algo(GraphAlgo::Force(ForceOpts::default()))
            .selected(sel)
    });
    rig.key(Key::Enter); // select the first node
    let first = sel_slot.get().unwrap().get_untracked().unwrap();
    rig.key(Key::Down);
    let second = sel_slot.get().unwrap().get_untracked().unwrap();
    let first_idx = centers.iter().position(|(id, _)| *id == first).unwrap();
    match next(first_idx, (0, 1)) {
        Some(expect) => assert_eq!(second, centers[expect].0, "widget matches the harness"),
        None => assert_eq!(second, first, "no candidate: selection stays"),
    }
}

// ---------------------------------------------------------------------------
// ensure_visible under a padded root: the selected card must land in
// the visible viewport, not off by the padding.
// ---------------------------------------------------------------------------

#[test]
fn arrow_selection_scrolls_the_target_into_view_under_a_padded_root() {
    use abstracttui::layout::Edges;
    let size = Size::new(24, 12);
    // A wide two-node chain: b sits far right of an 18-cell-ish
    // viewport; the root carries padding 2 on every side, shrinking
    // the true viewport well below the widget rect.
    let sel_slot: Rc<Cell<Option<Signal<Option<String>>>>> = Rc::new(Cell::new(None));
    let slot = sel_slot.clone();
    let mut rig = Rig::mount(size, move |cx| {
        let sel = cx.signal(None::<String>);
        slot.set(Some(sel));
        let desc = GraphDesc::new()
            .with_node(abstracttui_graph::NodeDesc::new("a", 10, 3).label("Alpha"))
            .with_node(abstracttui_graph::NodeDesc::new("b", 10, 3).label("Bravo"))
            .with_node(abstracttui_graph::NodeDesc::new("c", 11, 3).label("Charlie"))
            .edge("a", "b")
            .edge("b", "c");
        GraphView::new(desc)
            .style(test_style())
            .algo(GraphAlgo::Layered(LayeredOpts {
                direction: abstracttui_graph::Direction::LeftRight,
                ..Default::default()
            }))
            .layout(
                abstracttui::layout::Style::column()
                    .grow(1.0)
                    .padding(Edges::all(2)),
            )
            .selected(sel)
    });
    // Precondition: Charlie starts off-screen (the LR chain is far
    // wider than the padded viewport).
    let c = rig.draw();
    let all: String = (0..size.h).map(|y| c.row_text(y)).collect();
    assert!(all.contains("Alpha"), "Alpha visible initially");
    assert!(!all.contains("Charl"), "Charlie starts off-screen:\n{all}");

    rig.key(Key::Enter); // select Alpha
    rig.key(Key::Right); // -> Bravo
    rig.key(Key::Right); // -> Charlie: must scroll it into view
    assert_eq!(
        sel_slot.get().unwrap().get_untracked().as_deref(),
        Some("c")
    );
    let c = rig.draw();
    let all: String = (0..size.h).map(|y| c.row_text(y)).collect();
    assert!(
        all.contains("Charl"),
        "the selected card scrolled into the padded viewport:\n{all}"
    );
}

// ---------------------------------------------------------------------------
// Edge labels: never overprint a card.
// ---------------------------------------------------------------------------

#[test]
fn edge_labels_skip_when_they_would_overprint_a_card() {
    // Hand layout: a straight a->b edge whose midpoint label would
    // run INTO a third card sitting on the chord. The label must be
    // skipped (cards stay readable); the stroke still draws.
    let size = Size::new(34, 5);
    let label_ink = test_style().edge_label;
    let mut rig = Rig::mount(size, move |_| {
        let nodes = vec![
            NodeLayout::new("a", Rect::new(0, 0, 5, 3), 0),
            NodeLayout::new("mid", Rect::new(13, 0, 8, 3), 0),
            NodeLayout::new("b", Rect::new(28, 0, 5, 3), 0),
        ];
        let edges = vec![EdgeLayout::new(
            "a",
            "b",
            0,
            vec![Point::new(5, 1), Point::new(27, 1)],
        )];
        let layout = Layout::new(nodes, edges);
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("mid", 8, 3)
            .node("b", 5, 3)
            .with_edge(EdgeDesc::new("a", "b").label("collide"));
        GraphView::new(desc).style(test_style()).with_layout(layout)
    });
    let c = rig.draw();
    let mut label_cells = 0;
    for y in 0..size.h {
        for x in 0..size.w {
            if c.cell(Point::new(x, y)).unwrap().1 == label_ink {
                label_cells += 1;
            }
        }
    }
    assert_eq!(
        label_cells, 0,
        "a label that would overprint a card is skipped"
    );

    // Control: the same edge with the blocking card away from the
    // chord renders its label.
    let mut clear = Rig::mount(size, move |_| {
        let nodes = vec![
            NodeLayout::new("a", Rect::new(0, 0, 5, 3), 0),
            NodeLayout::new("b", Rect::new(28, 0, 5, 3), 0),
        ];
        let edges = vec![EdgeLayout::new(
            "a",
            "b",
            0,
            vec![Point::new(5, 1), Point::new(27, 1)],
        )];
        let layout = Layout::new(nodes, edges);
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .with_edge(EdgeDesc::new("a", "b").label("collide"));
        GraphView::new(desc).style(test_style()).with_layout(layout)
    });
    let c = clear.draw();
    let mut label_cells = 0;
    for y in 0..size.h {
        for x in 0..size.w {
            if c.cell(Point::new(x, y)).unwrap().1 == label_ink {
                label_cells += 1;
            }
        }
    }
    assert!(label_cells > 0, "an unobstructed label renders");
}
