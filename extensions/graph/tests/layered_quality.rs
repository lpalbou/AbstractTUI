//! Layered quality pins: rank correctness, crossing-free planar cases,
//! diamond centering, multi-rank waypoints, direction transposes.

use abstracttui_graph::{layered, Direction, GraphDesc, LayeredOpts, Layout};

fn opts() -> LayeredOpts {
    LayeredOpts::default()
}

/// A known 8-node DAG: two chains sharing a source and a sink plus a
/// mid join. Longest-path ranks are pinned exactly.
///
/// ```text
///        a
///      /   \
///     b     c
///     |     |
///     d     e
///      \   / \
///        f    g
///        |
///        h
/// ```
fn eight_node_dag() -> GraphDesc {
    GraphDesc::new()
        .node("a", 6, 3)
        .node("b", 6, 3)
        .node("c", 6, 3)
        .node("d", 6, 3)
        .node("e", 6, 3)
        .node("f", 6, 3)
        .node("g", 6, 3)
        .node("h", 6, 3)
        .edge("a", "b")
        .edge("a", "c")
        .edge("b", "d")
        .edge("c", "e")
        .edge("d", "f")
        .edge("e", "f")
        .edge("e", "g")
        .edge("f", "h")
}

fn rank_of(layout: &Layout, id: &str) -> usize {
    layout.node(id).unwrap().rank
}

#[test]
fn eight_node_dag_ranks_are_longest_path() {
    let layout = layered(&eight_node_dag(), &opts());
    assert_eq!(rank_of(&layout, "a"), 0);
    assert_eq!(rank_of(&layout, "b"), 1);
    assert_eq!(rank_of(&layout, "c"), 1);
    assert_eq!(rank_of(&layout, "d"), 2);
    assert_eq!(rank_of(&layout, "e"), 2);
    assert_eq!(rank_of(&layout, "f"), 3);
    assert_eq!(rank_of(&layout, "g"), 3);
    assert_eq!(rank_of(&layout, "h"), 4);
    assert!(layout.fallback.is_none());
}

/// Ranks must strictly increase along every non-broken edge — the
/// acyclicity proof of the broken-cycle orientation, asserted on data.
fn assert_ranks_increase(layout: &Layout) {
    for e in &layout.edges {
        if e.from == e.to {
            continue; // self-edge, no rank constraint
        }
        let (rf, rt) = (rank_of(layout, &e.from), rank_of(layout, &e.to));
        if e.broken {
            assert!(
                rt < rf,
                "broken edge {}->{} must point up-rank ({rf} -> {rt})",
                e.from,
                e.to
            );
        } else {
            assert!(
                rt > rf,
                "edge {}->{} must increase rank ({rf} -> {rt})",
                e.from,
                e.to
            );
        }
    }
}

#[test]
fn ranks_increase_along_every_kept_edge() {
    let layout = layered(&eight_node_dag(), &opts());
    assert_ranks_increase(&layout);
}

/// Count crossings between adjacent-rank straight edges from public
/// data: two segments cross iff their endpoint cross-orders invert.
fn crossings(layout: &Layout, vertical: bool) -> usize {
    let center = |id: &str| {
        let r = layout.node(id).unwrap().rect;
        if vertical {
            r.x * 2 + r.w
        } else {
            r.y * 2 + r.h
        }
    };
    let mut segs: Vec<(usize, i32, i32)> = layout
        .edges
        .iter()
        .filter(|e| e.from != e.to)
        .map(|e| {
            let (a, b) = (
                layout.node(&e.from).unwrap().rank,
                layout.node(&e.to).unwrap().rank,
            );
            (a.min(b), center(&e.from), center(&e.to))
        })
        .collect();
    segs.sort();
    let mut total = 0;
    for (i, a) in segs.iter().enumerate() {
        for b in segs.iter().skip(i + 1) {
            if a.0 != b.0 {
                continue; // different gaps cannot cross
            }
            if (a.1 < b.1 && a.2 > b.2) || (a.1 > b.1 && a.2 < b.2) {
                total += 1;
            }
        }
    }
    total
}

/// A planar two-chain graph drawn crossed on input order must come out
/// crossing-free: crossing reduction earns its keep.
#[test]
fn planar_case_lays_out_crossing_free() {
    // Input order deliberately interleaves the chains.
    let desc = GraphDesc::new()
        .node("a1", 6, 3)
        .node("b1", 6, 3)
        .node("a2", 6, 3)
        .node("b2", 6, 3)
        .node("a3", 6, 3)
        .node("b3", 6, 3)
        .edge("a1", "b2") // crossed pairing on purpose
        .edge("b1", "a2")
        .edge("b2", "a3")
        .edge("a2", "b3");
    let layout = layered(&desc, &opts());
    assert_eq!(crossings(&layout, true), 0, "planar graph, zero crossings");
}

#[test]
fn diamond_centers_the_sink_between_its_parents() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("b", 8, 3)
        .node("c", 8, 3)
        .node("d", 8, 3)
        .edge("a", "b")
        .edge("a", "c")
        .edge("b", "d")
        .edge("c", "d");
    let layout = layered(&desc, &opts());
    assert_eq!(rank_of(&layout, "d"), 2);
    let center_x = |id: &str| {
        let r = layout.node(id).unwrap().rect;
        r.x * 2 + r.w // doubled center avoids fractional cells
    };
    let mid = (center_x("b") + center_x("c")) / 2;
    assert!(
        (center_x("d") - mid).abs() <= 2,
        "d centered between b and c (d {} vs mid {})",
        center_x("d"),
        mid
    );
    assert!(
        (center_x("a") - mid).abs() <= 2,
        "a centered over b and c (a {} vs mid {})",
        center_x("a"),
        mid
    );
}

/// A chain plus a long edge spanning two ranks: the long edge routes
/// through the intermediate rank gap via a dummy waypoint that stays
/// clear of the card it passes.
#[test]
fn multi_rank_edge_routes_around_intermediate_cards() {
    let desc = GraphDesc::new()
        .node("a", 8, 3)
        .node("b", 8, 3)
        .node("c", 8, 3)
        .edge("a", "b")
        .edge("b", "c")
        .edge("a", "c");
    let layout = layered(&desc, &opts());
    let long = layout.edges.iter().find(|e| e.desc_index == 2).unwrap();
    assert_eq!(
        long.waypoints.len(),
        3,
        "one interior waypoint per crossed gap"
    );
    let mid = long.waypoints[1];
    let b = layout.node("b").unwrap().rect;
    assert!(
        !b.contains(mid),
        "dummy waypoint {mid:?} must not sit inside card b {b:?}"
    );
    // The interior waypoint sits in rank 1's flow band (between b's top
    // and bottom rows, exclusive of the gaps).
    assert!(mid.y >= b.y && mid.y < b.bottom());
    // Adjacent-rank edges are straight two-point segments.
    let short = layout.edges.iter().find(|e| e.desc_index == 0).unwrap();
    assert_eq!(short.waypoints.len(), 2);
}

/// Directions are transposes of one canonical picture. With square
/// cards the equivalence is exact: LR swaps x/y, BT mirrors the flow
/// axis, RL does both. Waypoints follow the same maps.
#[test]
fn four_directions_are_transpose_consistent() {
    let desc = {
        // Square cards so cross/flow extents coincide exactly.
        let mut d = GraphDesc::new();
        for id in ["a", "b", "c", "d", "e"] {
            d = d.node(id, 5, 5);
        }
        d.edge("a", "b")
            .edge("a", "c")
            .edge("b", "d")
            .edge("c", "d")
            .edge("a", "e")
    };
    let td = layered(&desc, &opts());
    let lr = layered(
        &desc,
        &LayeredOpts {
            direction: Direction::LeftRight,
            ..opts()
        },
    );
    let bt = layered(
        &desc,
        &LayeredOpts {
            direction: Direction::BottomTop,
            ..opts()
        },
    );
    let rl = layered(
        &desc,
        &LayeredOpts {
            direction: Direction::RightLeft,
            ..opts()
        },
    );

    assert_eq!(td.bounds.w, lr.bounds.h, "LR is the transpose of TD");
    assert_eq!(td.bounds.h, lr.bounds.w);
    assert_eq!(td.bounds, bt.bounds, "BT mirrors TD in place");
    assert_eq!(lr.bounds, rl.bounds, "RL mirrors LR in place");

    for n in &td.nodes {
        let l = lr.node(&n.id).unwrap();
        assert_eq!(
            (l.rect.x, l.rect.y),
            (n.rect.y, n.rect.x),
            "LR transposes node {}",
            n.id
        );
        let b = bt.node(&n.id).unwrap();
        assert_eq!(b.rect.x, n.rect.x, "BT keeps cross of node {}", n.id);
        assert_eq!(
            b.rect.y,
            td.bounds.h - (n.rect.y + n.rect.h),
            "BT mirrors flow of node {}",
            n.id
        );
        let r = rl.node(&n.id).unwrap();
        assert_eq!(r.rect.y, l.rect.y, "RL keeps cross of node {}", n.id);
        assert_eq!(
            r.rect.x,
            lr.bounds.w - (l.rect.x + l.rect.w),
            "RL mirrors flow of node {}",
            n.id
        );
        assert_eq!(n.rank, l.rank);
        assert_eq!(n.rank, b.rank);
        assert_eq!(n.rank, r.rank);
    }
    for (e_td, e_lr) in td.edges.iter().zip(lr.edges.iter()) {
        assert_eq!(e_td.waypoints.len(), e_lr.waypoints.len());
        for (p, q) in e_td.waypoints.iter().zip(e_lr.waypoints.iter()) {
            assert_eq!((q.x, q.y), (p.y, p.x), "LR transposes waypoints");
        }
    }
}
