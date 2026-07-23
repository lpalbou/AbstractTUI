//! Determinism goldens: same input, identical `Layout` — the chart
//! discipline ("same data, same cells") applied to layout.

use abstracttui_graph::{force, grid, layered, ForceOpts, GraphDesc, LayeredOpts, Point, Rect};

/// A gnarly fixture: cycle, multi-rank edge, duplicate edges, a
/// self-edge, non-square cards and a second component.
fn gnarly() -> GraphDesc {
    GraphDesc::new()
        .node("a", 10, 3)
        .node("b", 6, 4)
        .node("c", 8, 3)
        .node("d", 5, 3)
        .node("island", 7, 3)
        .node("isle2", 7, 3)
        .edge("a", "b")
        .edge("b", "c")
        .edge("c", "a") // cycle
        .edge("a", "c") // multi-rank
        .edge("a", "b") // duplicate
        .edge("d", "d") // self-edge
        .edge("a", "d")
        .edge("island", "isle2")
}

#[test]
fn layered_is_deterministic_run_to_run() {
    let desc = gnarly();
    let one = layered(&desc, &LayeredOpts::default());
    let two = layered(&desc, &LayeredOpts::default());
    assert_eq!(one, two, "same graph, same Layout");
}

#[test]
fn force_is_deterministic_under_fixed_seed_and_budget() {
    let desc = gnarly();
    let opts = ForceOpts {
        seed: 99,
        budget: 128,
        ..Default::default()
    };
    assert_eq!(force(&desc, &opts), force(&desc, &opts));
}

#[test]
fn grid_is_deterministic_run_to_run() {
    let desc = gnarly();
    assert_eq!(grid(&desc), grid(&desc));
}

#[test]
fn force_seeds_actually_scatter_differently() {
    let desc = gnarly();
    let a = force(
        &desc,
        &ForceOpts {
            seed: 1,
            ..Default::default()
        },
    );
    let b = force(
        &desc,
        &ForceOpts {
            seed: 2,
            ..Default::default()
        },
    );
    assert_ne!(
        a.nodes, b.nodes,
        "different seeds must not collapse to one placement"
    );
}

/// Hard-pinned golden: the diamond under default options. Catches any
/// cross-run or cross-platform drift the double-run equality cannot.
#[test]
fn layered_golden_diamond_exact_cells() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("b", 8, 3)
        .node("c", 8, 3)
        .node("d", 8, 3)
        .edge("a", "b")
        .edge("a", "c")
        .edge("b", "d")
        .edge("c", "d");
    let layout = layered(&desc, &LayeredOpts::default());
    let rects: Vec<(String, Rect, usize)> = layout
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.rect, n.rank))
        .collect();
    assert_eq!(
        rects,
        vec![
            ("a".to_string(), Rect::new(6, 0, 8, 3), 0),
            ("b".to_string(), Rect::new(0, 5, 8, 3), 1),
            ("c".to_string(), Rect::new(11, 5, 8, 3), 1),
            ("d".to_string(), Rect::new(6, 10, 8, 3), 2),
        ]
    );
    let wp: Vec<Vec<Point>> = layout.edges.iter().map(|e| e.waypoints.clone()).collect();
    assert_eq!(wp, golden_diamond_waypoints());
}

fn golden_diamond_waypoints() -> Vec<Vec<Point>> {
    vec![
        vec![Point::new(10, 3), Point::new(4, 4)],
        vec![Point::new(10, 3), Point::new(15, 4)],
        vec![Point::new(4, 8), Point::new(10, 9)],
        vec![Point::new(15, 8), Point::new(10, 9)],
    ]
}

/// Hard-pinned golden for the force pass: five nodes, fixed seed and
/// budget. IEEE-exact arithmetic only, so these cells hold on every
/// platform.
#[test]
fn force_golden_five_nodes_exact_cells() {
    let desc = GraphDesc::new()
        .node("n0", 6, 3)
        .node("n1", 6, 3)
        .node("n2", 6, 3)
        .node("n3", 6, 3)
        .node("n4", 6, 3)
        .edge("n0", "n1")
        .edge("n1", "n2")
        .edge("n2", "n3")
        .edge("n3", "n4")
        .edge("n4", "n0");
    let opts = ForceOpts {
        seed: 7,
        budget: 96,
        ..Default::default()
    };
    let layout = force(&desc, &opts);
    let rects: Vec<Rect> = layout.nodes.iter().map(|n| n.rect).collect();
    assert_eq!(rects, golden_force_rects());
}

fn golden_force_rects() -> Vec<Rect> {
    vec![
        Rect::new(28, 0, 6, 3),
        Rect::new(69, 15, 6, 3),
        Rect::new(65, 58, 6, 3),
        Rect::new(23, 70, 6, 3),
        Rect::new(0, 33, 6, 3),
    ]
}
