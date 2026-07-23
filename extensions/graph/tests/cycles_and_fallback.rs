//! Cycle-break marking and the labeled honesty fallbacks.

use abstracttui_graph::{grid, layered, GraphDesc, LayeredOpts};

#[test]
fn two_cycle_breaks_exactly_one_edge_and_marks_it() {
    let desc = GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .edge("a", "b")
        .edge("b", "a");
    let layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(layout.broken_edges(), vec![1], "the back edge, marked");
    let broken = &layout.edges[1];
    assert!(broken.broken);
    assert_eq!(broken.from, "b", "endpoints keep their original order");
    assert_eq!(broken.to, "a");
    assert!(
        layout.fallback.is_none(),
        "cycle breaking is not a fallback: the algorithm ran as designed"
    );
    // Both parallel gap segments exist and are distinguishable.
    assert_ne!(layout.edges[0].waypoints, layout.edges[1].waypoints);
}

#[test]
fn three_cycle_ranks_stay_a_dag_and_the_break_is_deterministic() {
    let desc = GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .node("c", 6, 3)
        .edge("a", "b")
        .edge("b", "c")
        .edge("c", "a");
    let layout = layered(&desc, &LayeredOpts::default());
    // Input-order DFS a -> b -> c sees c->a as the one back edge.
    assert_eq!(layout.broken_edges(), vec![2]);
    let rank = |id: &str| layout.node(id).unwrap().rank;
    assert_eq!((rank("a"), rank("b"), rank("c")), (0, 1, 2));
    // Non-broken edges increase rank; the broken one points back up.
    for e in &layout.edges {
        let (rf, rt) = (rank(&e.from), rank(&e.to));
        if e.broken {
            assert!(rt < rf);
        } else {
            assert!(rt > rf);
        }
    }
}

/// Two cycles sharing a node (figure eight): still a DAG afterwards,
/// every broken edge marked, nothing silently reordered.
#[test]
fn figure_eight_breaks_one_edge_per_cycle() {
    let desc = GraphDesc::new()
        .node("hub", 6, 3)
        .node("l1", 6, 3)
        .node("l2", 6, 3)
        .node("r1", 6, 3)
        .node("r2", 6, 3)
        .edge("hub", "l1")
        .edge("l1", "l2")
        .edge("l2", "hub")
        .edge("hub", "r1")
        .edge("r1", "r2")
        .edge("r2", "hub");
    let layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(
        layout.broken_edges(),
        vec![2, 5],
        "one back edge per lobe, in input order"
    );
    // Layout still carries all six edges, in input order.
    let indices: Vec<usize> = layout.edges.iter().map(|e| e.desc_index).collect();
    assert_eq!(indices, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn node_cap_degrades_to_grid_with_the_cap_named() {
    let mut desc = GraphDesc::new();
    for i in 0..10 {
        desc = desc.node(format!("n{i}"), 4, 2);
    }
    desc = desc.edge("n0", "n1").edge("n1", "n2");
    let opts = LayeredOpts {
        node_cap: 8,
        ..Default::default()
    };
    let layout = layered(&desc, &opts);
    let label = layout
        .fallback
        .as_deref()
        .expect("fallback must be labeled");
    assert!(
        label.contains("node cap exceeded (10 > 8)"),
        "label names the cap: {label}"
    );
    assert!(label.contains("grid placement"), "label names the shape");
    assert_eq!(layout.nodes.len(), 10, "grid still places every node");
    // Under the cap, the same graph lays out layered and unlabeled.
    let clean = layered(&desc, &LayeredOpts::default());
    assert!(clean.fallback.is_none());
}

#[test]
fn explicit_grid_is_always_labeled_and_row_ranked() {
    let mut desc = GraphDesc::new();
    for i in 0..5 {
        desc = desc.node(format!("n{i}"), 6, 3);
    }
    desc = desc.edge("n0", "n4");
    let layout = grid(&desc);
    let label = layout.fallback.as_deref().expect("grid always labels");
    assert!(label.contains("no hierarchy computed"), "honest: {label}");
    // Near-square: 5 nodes -> 3 columns, ranks report grid rows.
    assert_eq!(layout.node("n0").unwrap().rank, 0);
    assert_eq!(layout.node("n3").unwrap().rank, 1);
    assert_eq!(layout.node("n4").unwrap().rank, 1);
    assert_eq!(layout.edges.len(), 1);
    assert_eq!(layout.edges[0].waypoints.len(), 2);
}

/// Disconnected components pack side by side instead of interleaving.
#[test]
fn components_lay_out_side_by_side() {
    let desc = GraphDesc::new()
        .node("a1", 6, 3)
        .node("a2", 6, 3)
        .node("b1", 6, 3)
        .node("b2", 6, 3)
        .edge("a1", "a2")
        .edge("b1", "b2");
    let layout = layered(&desc, &LayeredOpts::default());
    let (a1, a2) = (
        layout.node("a1").unwrap().rect,
        layout.node("a2").unwrap().rect,
    );
    let (b1, b2) = (
        layout.node("b1").unwrap().rect,
        layout.node("b2").unwrap().rect,
    );
    let a_right = a1.right().max(a2.right());
    let b_left = b1.x.min(b2.x);
    assert!(
        b_left >= a_right + 3,
        "component b starts clear of component a ({b_left} vs {a_right})"
    );
    assert_eq!(layout.node("a1").unwrap().rank, 0);
    assert_eq!(
        layout.node("b1").unwrap().rank,
        0,
        "each component ranks from 0"
    );
}
