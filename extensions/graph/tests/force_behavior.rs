//! Force-pass behavior: rank-bias effect, contract honesty, budget as
//! an act (the settle freeze itself is unit-pinned inside the module,
//! where the iteration count is observable).

use abstracttui_graph::{force, Direction, ForceOpts, GraphDesc, Layout};

fn chain() -> GraphDesc {
    GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .node("c", 6, 3)
        .node("d", 6, 3)
        .edge("a", "b")
        .edge("b", "c")
        .edge("c", "d")
}

/// Sum over edges of the target-minus-source delta along an axis.
fn downstream_delta(layout: &Layout, desc: &GraphDesc, vertical: bool) -> f64 {
    desc.edges
        .iter()
        .map(|e| {
            let f = layout.node(&e.from).unwrap().rect;
            let t = layout.node(&e.to).unwrap().rect;
            if vertical {
                f64::from(t.y * 2 + t.h) - f64::from(f.y * 2 + f.h)
            } else {
                f64::from(t.x * 2 + t.w) - f64::from(f.x * 2 + f.w)
            }
        })
        .sum::<f64>()
        / 2.0
}

#[test]
fn rank_bias_pulls_targets_downstream() {
    let desc = chain();
    let base = ForceOpts {
        seed: 11,
        budget: 300,
        ..Default::default()
    };
    let unbiased = force(&desc, &base);
    let biased = force(
        &desc,
        &ForceOpts {
            rank_bias: Some(Direction::TopDown),
            ..base.clone()
        },
    );
    // Every edge of the chain flows downward under TD bias.
    for e in &biased.edges {
        let f = biased.node(&e.from).unwrap().rect;
        let t = biased.node(&e.to).unwrap().rect;
        assert!(
            t.y * 2 + t.h > f.y * 2 + f.h,
            "edge {}->{} should flow downward under TD bias",
            e.from,
            e.to
        );
    }
    // And the aggregate downstream tendency strictly exceeds the
    // unbiased layout's.
    let with_bias = downstream_delta(&biased, &desc, true);
    let without = downstream_delta(&unbiased, &desc, true);
    assert!(
        with_bias > without,
        "bias must increase downstream delta ({with_bias} vs {without})"
    );
}

#[test]
fn rank_bias_respects_reversed_and_horizontal_directions() {
    let desc = chain();
    let base = ForceOpts {
        seed: 11,
        budget: 300,
        ..Default::default()
    };
    let lr = force(
        &desc,
        &ForceOpts {
            rank_bias: Some(Direction::LeftRight),
            ..base.clone()
        },
    );
    assert!(
        downstream_delta(&lr, &desc, false) > 0.0,
        "LR bias flows rightward"
    );
    let bt = force(
        &desc,
        &ForceOpts {
            rank_bias: Some(Direction::BottomTop),
            ..base
        },
    );
    assert!(
        downstream_delta(&bt, &desc, true) < 0.0,
        "BT bias flows upward"
    );
}

#[test]
fn force_reports_no_hierarchy_and_breaks_nothing() {
    // A cyclic graph: force needs no cycle breaking.
    let desc = GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .node("c", 6, 3)
        .edge("a", "b")
        .edge("b", "c")
        .edge("c", "a");
    let layout = force(&desc, &ForceOpts::default());
    assert!(layout.nodes.iter().all(|n| n.rank == 0), "rank 0: honest");
    assert!(layout.broken_edges().is_empty());
    assert!(layout.fallback.is_none(), "force ran cleanly, no label");
    assert_eq!(layout.edges.len(), 3);
    for e in &layout.edges {
        assert_eq!(e.waypoints.len(), 2, "straight border-to-border segments");
    }
}

#[test]
fn layout_is_origin_normalized_and_bounded() {
    let layout = force(&chain(), &ForceOpts::default());
    assert_eq!((layout.bounds.x, layout.bounds.y), (0, 0));
    for n in &layout.nodes {
        assert!(n.rect.x >= 0 && n.rect.y >= 0);
        assert!(n.rect.right() <= layout.bounds.right());
        assert!(n.rect.bottom() <= layout.bounds.bottom());
    }
    for e in &layout.edges {
        for p in &e.waypoints {
            assert!(p.x >= 0 && p.y >= 0);
            assert!(p.x < layout.bounds.right() + 1);
            assert!(p.y < layout.bounds.bottom() + 1);
        }
    }
}
