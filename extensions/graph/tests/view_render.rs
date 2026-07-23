//! GraphView rendering goldens: card recipe, selection restyle, edge
//! strokes + arrowheads in all four directions, dotted/thick/broken
//! styles, the fallback notice line, and the desc_index metadata join
//! (the cycle-1 attack item: unresolvable-edge drops must never shift
//! styles positionally).
//!
//! Rendered through the real UiTree into a BufferCanvas (the engine's
//! headless tree path — public API, no Driver needed here).

use std::cell::Cell;
use std::rc::Rc;

use abstracttui::base::{Point, Rgba, Size};
use abstracttui::reactive::{create_root, flush_effects, Signal};
use abstracttui::render::Attrs;
use abstracttui::ui::{BufferCanvas, Key, KeyEvent, UiEvent, UiTree};
use abstracttui_graph::{
    EdgeDesc, EdgeLayout, ForceOpts, GraphAlgo, GraphDesc, GraphStyle, GraphView, LayeredOpts,
    Layout, NodeDesc, NodeLayout, Rect,
};

// Deterministic test inks (never theme-derived: goldens must not move
// with the default theme).
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

fn find_char(canvas: &BufferCanvas, size: Size, ch: char) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..size.h {
        for x in 0..size.w {
            if canvas.cell(Point::new(x, y)).unwrap().0 == ch {
                out.push((x, y));
            }
        }
    }
    out
}

fn is_braille(ch: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&ch)
}

/// Cells carrying a braille stroke in the given ink.
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

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

#[test]
fn card_renders_border_title_badge_and_kind_accent() {
    let accent = Rgba::rgb(0, 200, 120);
    let size = Size::new(16, 5);
    let mut rig = Rig::mount(size, move |_| {
        let desc =
            GraphDesc::new().with_node(NodeDesc::new("alpha", 12, 3).label("Alpha").kind("svc"));
        GraphView::new(desc)
            .style(test_style().kind_accent("svc", accent))
            .badges(|_| Some("3".to_string()))
    });
    let c = rig.draw();
    assert_eq!(c.row_text(0).trim_end(), "╭ Alpha ───╮");
    assert_eq!(c.row_text(1).trim_end(), "│         3│");
    assert_eq!(c.row_text(2).trim_end(), "╰──────────╯");
    // Kind accent tints the LEFT border column; the rest stays border.
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, accent);
    assert_eq!(c.cell(Point::new(0, 1)).unwrap().1, accent);
    assert_eq!(
        c.cell(Point::new(11, 0)).unwrap().1,
        test_style().card_border
    );
    // Badge ink.
    assert_eq!(c.cell(Point::new(10, 1)).unwrap().1, test_style().badge);
    // Card ground.
    assert_eq!(c.cell(Point::new(5, 1)).unwrap().2, test_style().card_bg);
}

#[test]
fn selection_restyles_the_card_border_and_title() {
    let size = Size::new(16, 5);
    let sel_slot: Rc<Cell<Option<Signal<Option<String>>>>> = Rc::new(Cell::new(None));
    let slot = sel_slot.clone();
    let mut rig = Rig::mount(size, move |cx| {
        let sel = cx.signal(None::<String>);
        slot.set(Some(sel));
        let desc = GraphDesc::new().with_node(NodeDesc::new("a", 12, 3).label("Alpha"));
        GraphView::new(desc).style(test_style()).selected(sel)
    });
    let style = test_style();
    let c = rig.draw();
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, style.card_border);

    sel_slot.get().unwrap().set(Some("a".to_string()));
    flush_effects();
    let c = rig.draw();
    // Border restyled to the selection ink; title goes bold.
    assert_eq!(
        c.cell(Point::new(0, 0)).unwrap().1,
        style.card_border_selected
    );
    assert_eq!(
        c.cell(Point::new(11, 0)).unwrap().1,
        style.card_border_selected
    );
    assert!(
        c.attrs_at(Point::new(2, 0)).contains(Attrs::BOLD),
        "selected title is bold"
    );

    // Deselect restores the plain border (the dyn card re-renders).
    sel_slot.get().unwrap().set(None);
    flush_effects();
    let c = rig.draw();
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, style.card_border);
}

// ---------------------------------------------------------------------------
// Edges: strokes, arrowheads, styles, honesty
// ---------------------------------------------------------------------------

fn two_node_desc() -> GraphDesc {
    GraphDesc::new()
        .node("a", 5, 3)
        .node("b", 5, 3)
        .edge("a", "b")
}

#[test]
fn arrowheads_orient_in_all_four_directions() {
    use abstracttui_graph::Direction;
    for (dir, glyph) in [
        (Direction::TopDown, '▼'),
        (Direction::BottomTop, '▲'),
        (Direction::LeftRight, '▶'),
        (Direction::RightLeft, '◀'),
    ] {
        let size = Size::new(24, 12);
        let mut rig = Rig::mount(size, move |_| {
            GraphView::new(two_node_desc())
                .style(test_style())
                .algo(GraphAlgo::Layered(LayeredOpts {
                    direction: dir,
                    ..Default::default()
                }))
        });
        let c = rig.draw();
        let hits = find_char(&c, size, glyph);
        assert_eq!(
            hits.len(),
            1,
            "{dir:?}: one {glyph} arrowhead, got {hits:?}"
        );
        // And the stroke exists in the edge ink.
        assert!(
            !stroke_cells(&c, size, test_style().edge).is_empty(),
            "{dir:?}: edge stroke drawn"
        );
    }
}

#[test]
fn dotted_style_draws_sparser_than_solid_and_broken_edges_use_their_ink() {
    let size = Size::new(20, 14);
    // Solid control: taller layout so the edge has real length.
    let tall = |style: Option<&'static str>| {
        let mut e = EdgeDesc::new("a", "b");
        if let Some(s) = style {
            e = e.style(s);
        }
        GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .with_edge(e)
    };
    let opts = LayeredOpts {
        rank_gap: 6,
        ..Default::default()
    };
    // Dotted changes DOT density, not necessarily cell coverage —
    // compare lit dots, the honest sparsity metric.
    let solid_opts = opts.clone();
    let solid = braille_dot_count(
        &mut Rig::mount(size, move |_| {
            GraphView::new(tall(None))
                .style(test_style())
                .algo(GraphAlgo::Layered(solid_opts))
        }),
        size,
    );
    let dotted_opts = opts.clone();
    let dotted = braille_dot_count(
        &mut Rig::mount(size, move |_| {
            GraphView::new(tall(Some("dotted")))
                .style(test_style())
                .algo(GraphAlgo::Layered(dotted_opts))
        }),
        size,
    );
    assert!(dotted >= 1, "dotted edge still visible");
    assert!(
        dotted * 2 < solid,
        "dotted ({dotted} dots) draws far sparser than solid ({solid} dots)"
    );

    // A 2-cycle: the broken edge strokes in its own ink and both
    // arrow directions render (forward ▼, reversed-back ▲ in TD).
    let mut rig = Rig::mount(size, move |_| {
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .edge("a", "b")
            .edge("b", "a");
        GraphView::new(desc).style(test_style())
    });
    let c = rig.draw();
    assert!(
        !stroke_cells(&c, size, test_style().edge_broken).is_empty(),
        "broken edge strokes in the honesty ink"
    );
    assert_eq!(find_char(&c, size, '▼').len(), 1, "forward arrow");
    assert_eq!(find_char(&c, size, '▲').len(), 1, "reversed-back arrow");
}

#[test]
fn thick_style_draws_denser_than_solid() {
    let size = Size::new(20, 14);
    let opts = LayeredOpts {
        rank_gap: 6,
        ..Default::default()
    };
    let mk = |style: Option<&'static str>, opts: LayeredOpts| {
        move |_: abstracttui::reactive::Scope| {
            let mut e = EdgeDesc::new("a", "b");
            if let Some(s) = style {
                e = e.style(s);
            }
            GraphView::new(
                GraphDesc::new()
                    .node("a", 5, 3)
                    .node("b", 5, 3)
                    .with_edge(e),
            )
            .style(test_style())
            .algo(GraphAlgo::Layered(opts))
        }
    };
    let solid_dots = braille_dot_count(&mut Rig::mount(size, mk(None, opts.clone())), size);
    let thick_dots = braille_dot_count(&mut Rig::mount(size, mk(Some("thick"), opts)), size);
    assert!(
        thick_dots > solid_dots,
        "thick ({thick_dots} dots) denser than solid ({solid_dots})"
    );
}

/// Total LIT DOTS (bits of every braille cell) in the edge ink.
fn braille_dot_count(rig: &mut Rig, size: Size) -> u32 {
    let c = rig.draw();
    let mut dots = 0u32;
    for y in 0..size.h {
        for x in 0..size.w {
            let (ch, fg, _) = c.cell(Point::new(x, y)).unwrap();
            if is_braille(ch) && fg == test_style().edge {
                dots += (ch as u32 - 0x2800).count_ones();
            }
        }
    }
    dots
}

#[test]
fn self_loop_renders_a_lobe_with_an_arrow_back_into_the_card() {
    let size = Size::new(16, 5);
    let mut rig = Rig::mount(size, |_| {
        let desc = GraphDesc::new().node("a", 8, 3).edge("a", "a");
        GraphView::new(desc).style(test_style())
    });
    let c = rig.draw();
    // Lobe strokes sit right of the card (card right edge at x=7).
    let cells = stroke_cells(&c, size, test_style().edge);
    assert!(
        cells.iter().any(|&(x, _)| x >= 8),
        "self-loop lobe right of the card: {cells:?}"
    );
    assert_eq!(find_char(&c, size, '◀').len(), 1, "loop arrow points back");
}

// ---------------------------------------------------------------------------
// Honesty: fallback notice + the desc_index join
// ---------------------------------------------------------------------------

#[test]
fn fallback_label_renders_as_a_notice_line_that_never_scrolls() {
    let size = Size::new(40, 8);
    let mut rig = Rig::mount(size, |_| {
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .edge("a", "ghost")
            .edge("a", "b");
        GraphView::new(desc).style(test_style())
    });
    let c = rig.draw();
    let top = c.row_text(0);
    assert!(top.contains('⚠'), "notice marker on row 0: {top:?}");
    assert!(
        top.contains("skipped"),
        "the layout's own words render: {top:?}"
    );
    assert_eq!(
        c.cell(Point::new(0, 0)).unwrap().1,
        test_style().notice,
        "notice ink"
    );
}

/// The cycle-1 attack item (A1): metadata joins by `desc_index`, never
/// positionally. With edge 0 unresolvable (dropped) and edge 1 styled
/// dotted, the surviving edge must STILL render dotted — a positional
/// zip would have given it edge 0's plain style.
#[test]
fn unresolvable_edge_drop_does_not_shift_styles_onto_survivors() {
    let size = Size::new(20, 14);
    let opts = LayeredOpts {
        rank_gap: 6,
        ..Default::default()
    };
    let solid_opts = opts.clone();
    let solid = braille_dot_count(
        &mut Rig::mount(size, move |_| {
            // Control: same graph, no ghost, plain style.
            GraphView::new(
                GraphDesc::new()
                    .node("a", 5, 3)
                    .node("b", 5, 3)
                    .edge("a", "b"),
            )
            .style(test_style())
            .algo(GraphAlgo::Layered(solid_opts))
        }),
        size,
    );

    let mut rig = Rig::mount(size, move |_| {
        let desc = GraphDesc::new()
            .node("a", 5, 3)
            .node("b", 5, 3)
            .edge("a", "ghost") // desc_index 0: dropped by resolve
            .with_edge(EdgeDesc::new("a", "b").style("dotted")); // index 1
        GraphView::new(desc)
            .style(test_style())
            .algo(GraphAlgo::Layered(opts))
    });
    let c = rig.draw();
    let survivor = braille_dot_count(&mut rig, size);
    assert!(survivor >= 1);
    assert!(
        survivor * 2 < solid,
        "survivor renders DOTTED ({survivor} vs solid {solid} dots) — desc_index join held"
    );
    assert!(c.row_text(0).contains("skipped"), "and the drop is noticed");
}

// ---------------------------------------------------------------------------
// Parallel + opposite edges (A4): canonical-frame bowing
// ---------------------------------------------------------------------------

#[test]
fn opposite_direction_edges_bow_to_opposite_sides() {
    // Hand-built (origin-normalized, per the Layout contract) layout
    // pins the geometry exactly: two 5-tall cards, chord on row 2, an
    // a->b and a b->a — the reverse one marked broken so the two
    // strokes are separable by ink.
    let size = Size::new(30, 8);
    let mut rig = Rig::mount(size, |_| {
        let nodes = vec![
            NodeLayout::new("a", Rect::new(0, 0, 5, 5), 0),
            NodeLayout::new("b", Rect::new(22, 0, 5, 5), 0),
        ];
        let edges = vec![
            EdgeLayout::new("a", "b", 0, vec![Point::new(5, 2), Point::new(21, 2)]),
            EdgeLayout::new("b", "a", 1, vec![Point::new(21, 2), Point::new(5, 2)]).broken(),
        ];
        let layout = Layout::new(nodes, edges);
        let desc = GraphDesc::new()
            .node("a", 5, 5)
            .node("b", 5, 5)
            .edge("a", "b")
            .edge("b", "a");
        GraphView::new(desc).style(test_style()).with_layout(layout)
    });
    let c = rig.draw();
    let fwd = stroke_cells(&c, size, test_style().edge);
    let rev = stroke_cells(&c, size, test_style().edge_broken);
    assert!(
        !fwd.is_empty() && !rev.is_empty(),
        "both strokes visible: fwd {fwd:?}, rev {rev:?}"
    );
    // The bows separate vertically around the chord row (y=2): the
    // two strokes must occupy some rows the other never touches.
    let rows = |cells: &[(i32, i32)]| {
        let mut r: Vec<i32> = cells.iter().map(|&(_, y)| y).collect();
        r.sort_unstable();
        r.dedup();
        r
    };
    let (fr, rr) = (rows(&fwd), rows(&rev));
    assert!(
        fr.iter().any(|y| !rr.contains(y)) && rr.iter().any(|y| !fr.contains(y)),
        "opposite edges bow apart: forward rows {fr:?}, reverse rows {rr:?}"
    );
}

#[test]
fn parallel_duplicate_edges_separate_in_force_layouts() {
    // Two a->b edges through the force pass: the planner bows them to
    // distinct ordinals, so the union of stroke cells must exceed a
    // single edge's stroke on the SAME frozen layout.
    let size = Size::new(40, 20);
    let desc2 = GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .edge("a", "b")
        .edge("a", "b");
    let frozen = abstracttui_graph::force(&desc2, &ForceOpts::default());
    let single = {
        let mut layout = frozen.clone();
        layout.edges.truncate(1);
        layout
    };
    let d2 = desc2.clone();
    let f2 = frozen.clone();
    let mut rig = Rig::mount(size, move |_| {
        GraphView::new(d2).style(test_style()).with_layout(f2)
    });
    let both = stroke_cells(&rig.draw(), size, test_style().edge).len();
    let mut rig = Rig::mount(size, move |_| {
        GraphView::new(desc2)
            .style(test_style())
            .with_layout(single)
    });
    let one = stroke_cells(&rig.draw(), size, test_style().edge).len();
    assert!(
        both > one,
        "two parallel edges cover more cells ({both}) than one ({one}) — they no longer coincide"
    );
}

// ---------------------------------------------------------------------------
// Keyboard selection at the tree level
// ---------------------------------------------------------------------------

#[test]
fn enter_selects_first_arrows_move_spatially_escape_deselects() {
    let size = Size::new(40, 16);
    let pressed: Rc<std::cell::RefCell<Vec<String>>> = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = pressed.clone();
    let mut rig = Rig::mount(size, move |_| {
        // Diamond: a -> b, a -> c, b -> d, c -> d.
        let desc = GraphDesc::new()
            .node("a", 6, 3)
            .node("b", 6, 3)
            .node("c", 6, 3)
            .node("d", 6, 3)
            .edge("a", "b")
            .edge("a", "c")
            .edge("b", "d")
            .edge("c", "d");
        GraphView::new(desc)
            .style(test_style())
            .on_node_press(move |id| sink.borrow_mut().push(id.to_string()))
    });
    let style = test_style();
    let selected_corner = |rig: &mut Rig| -> Vec<(i32, i32)> {
        let c = rig.draw();
        find_char(&c, size, '╭')
            .into_iter()
            .filter(|&(x, y)| c.cell(Point::new(x, y)).unwrap().1 == style.card_border_selected)
            .collect()
    };

    assert!(selected_corner(&mut rig).is_empty(), "nothing selected yet");
    rig.key(Key::Enter); // select first (a) — no press
    assert_eq!(selected_corner(&mut rig).len(), 1);
    assert!(
        pressed.borrow().is_empty(),
        "first Enter selects, never presses"
    );

    rig.key(Key::Down); // a -> b or c (spatial; earliest wins ties)
    rig.key(Key::Down); // -> d
    rig.key(Key::Enter); // press d
    assert_eq!(pressed.borrow().as_slice(), ["d"]);

    rig.key(Key::Escape);
    assert!(selected_corner(&mut rig).is_empty(), "Escape deselects");
}
