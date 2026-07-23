//! Degenerate inputs: empty, single node, self-edges, duplicate edges,
//! duplicate node ids, unknown endpoints — every pass answers the same
//! contract, honestly labeled where the input degraded.

use abstracttui_graph::{
    dump, force, grid, layered, ForceOpts, GraphDesc, LayeredOpts, Rect, Size,
};

#[test]
fn empty_graph_yields_empty_layout_on_every_pass() {
    let desc = GraphDesc::new();
    for layout in [
        layered(&desc, &LayeredOpts::default()),
        force(&desc, &ForceOpts::default()),
        grid(&desc),
    ] {
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert_eq!(layout.bounds, Rect::ZERO);
    }
    assert_eq!(
        dump::ascii(&layered(&desc, &LayeredOpts::default())),
        "(empty layout)"
    );
}

#[test]
fn single_node_sits_at_origin() {
    let desc = GraphDesc::new().node("solo", 9, 4);
    for layout in [
        layered(&desc, &LayeredOpts::default()),
        force(&desc, &ForceOpts::default()),
    ] {
        assert_eq!(layout.nodes.len(), 1);
        assert_eq!(layout.nodes[0].rect, Rect::new(0, 0, 9, 4));
        assert_eq!(layout.nodes[0].rank, 0);
        assert_eq!(layout.bounds, Rect::new(0, 0, 9, 4));
        assert!(layout.fallback.is_none());
    }
}

#[test]
fn self_edge_is_present_as_a_loop_not_dropped() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("b", 8, 3)
        .edge("a", "a")
        .edge("a", "b");
    for layout in [
        layered(&desc, &LayeredOpts::default()),
        force(&desc, &ForceOpts::default()),
        grid(&desc),
    ] {
        assert_eq!(layout.edges.len(), 2, "self-edge kept");
        let lobe = &layout.edges[0];
        assert_eq!(lobe.from, "a");
        assert_eq!(lobe.to, "a");
        assert_eq!(lobe.waypoints.len(), 4, "loop lobe");
        assert!(!lobe.broken, "a self-edge is not a broken cycle edge");
        // The lobe pokes out of the card's right face.
        let card = layout.node("a").unwrap().rect;
        assert!(lobe.waypoints.iter().any(|p| p.x > card.right()));
    }
    // Ranking is unaffected by the self-edge.
    let layered_layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(layered_layout.node("a").unwrap().rank, 0);
    assert_eq!(layered_layout.node("b").unwrap().rank, 1);
}

#[test]
fn duplicate_edges_stay_distinguishable() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("b", 8, 3)
        .edge("a", "b")
        .edge("a", "b");
    let layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(layout.edges.len(), 2);
    assert_eq!(layout.edges[0].desc_index, 0);
    assert_eq!(layout.edges[1].desc_index, 1);
    assert_ne!(
        layout.edges[0].waypoints, layout.edges[1].waypoints,
        "layered spreads parallel edges so both are visible"
    );
    // Force keeps both, straight and identical (documented v1.5 shape).
    let f = force(&desc, &ForceOpts::default());
    assert_eq!(f.edges.len(), 2);
    assert_eq!(f.edges[0].waypoints.len(), 2);
}

#[test]
fn duplicate_node_ids_first_wins_with_a_label() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("a", 4, 2)
        .node("b", 8, 3)
        .edge("a", "b");
    let layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(layout.nodes.len(), 2);
    assert_eq!(
        layout.node("a").unwrap().rect.size(),
        Size::new(8, 3),
        "first occurrence wins"
    );
    let label = layout.fallback.as_deref().expect("drop must be labeled");
    assert!(label.contains("duplicate node id"), "honest: {label}");
}

#[test]
fn unknown_endpoints_skip_the_edge_with_a_label() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .edge("a", "ghost")
        .edge("phantom", "a");
    for layout in [
        layered(&desc, &LayeredOpts::default()),
        force(&desc, &ForceOpts::default()),
        grid(&desc),
    ] {
        assert!(layout.edges.is_empty());
        let label = layout.fallback.as_deref().expect("skip must be labeled");
        assert!(
            label.contains("2 edge(s) skipped (unknown endpoint id)"),
            "honest: {label}"
        );
    }
}

#[test]
fn ascii_dump_draws_cards_and_refuses_oversize() {
    let desc = GraphDesc::new()
        .node("top", 7, 3)
        .node("bot", 7, 3)
        .edge("top", "bot");
    let art = dump::ascii(&layered(&desc, &LayeredOpts::default()));
    assert!(art.contains('#'), "card borders drawn");
    assert!(art.contains("top"), "node id drawn");
    assert!(art.contains('*'), "edge polyline drawn");
    // Oversize refusal is a label, not megabytes of whitespace.
    let mut big = GraphDesc::new();
    for i in 0..200 {
        big = big.node(format!("n{i}"), 40, 3);
    }
    let refused = dump::ascii(&grid(&big));
    assert!(
        refused.contains("exceeds"),
        "oversize dump refused: {refused}"
    );
}
