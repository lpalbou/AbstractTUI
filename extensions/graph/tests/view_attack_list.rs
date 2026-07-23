//! Cycle-1 attack-list dispositions (layout lane), pinned from the
//! view half: BT/RL waypoint mirror correctness (FIXED — see
//! `map_point` in layout/geom.rs), the self-edge node-index invariant,
//! the rank_gap=1 anchor degeneracy (argued correct; the view owns the
//! arrowhead fallback), the BT/RL mirror bounds fixture, and a PAVA
//! three-pass coordinate golden (oscillation watchdog).

use abstracttui_graph::{layered, Direction, GraphDesc, LayeredOpts, NodeDesc, Point};

fn opts(direction: Direction) -> LayeredOpts {
    LayeredOpts {
        direction,
        ..Default::default()
    }
}

/// FAILING-FIRST proof for the `map_point` mirror fix (cycle-2 view
/// finding; citation: layout/geom.rs `map_point` mirrored waypoints as
/// `-f` while `map_rect` mirrors rects as `-(flow + extent)` — cells
/// are half-open intervals, so a bare `-f` shifted every BT/RL
/// waypoint one cell along the flow axis, landing the SOURCE anchor
/// inside its card, where the card paints over the stroke; the view's
/// BT arrowhead test caught it as a missing edge stroke).
///
/// Contract pinned here: waypoints mirror EXACTLY like rects — the BT
/// picture is the TD picture flipped, cell for cell — and no anchor of
/// a plain adjacent-rank edge sits inside either endpoint card.
#[test]
fn bt_rl_waypoints_mirror_like_rects_and_stay_out_of_cards() {
    let desc = GraphDesc::new()
        .node("a", 5, 3)
        .node("b", 5, 3)
        .edge("a", "b");

    let td = layered(&desc, &opts(Direction::TopDown));
    let bt = layered(&desc, &opts(Direction::BottomTop));
    assert_eq!(td.bounds, bt.bounds, "mirroring must not move bounds");
    let h = td.bounds.h;
    let e_td = &td.edges[0];
    let e_bt = &bt.edges[0];
    assert_eq!(e_td.waypoints.len(), e_bt.waypoints.len());
    for (p, q) in e_td.waypoints.iter().zip(&e_bt.waypoints) {
        assert_eq!(q.x, p.x, "BT keeps the cross axis");
        assert_eq!(
            q.y,
            h - 1 - p.y,
            "BT mirrors waypoint cells exactly like rect cells: {p:?} -> {q:?}"
        );
    }
    for layout in [&td, &bt] {
        for e in &layout.edges {
            for (name, p) in [
                ("first", e.waypoints[0]),
                ("last", *e.waypoints.last().unwrap()),
            ] {
                for n in &layout.nodes {
                    assert!(
                        !n.rect.contains(p),
                        "{name} anchor {p:?} sits inside card {} {:?}",
                        n.id,
                        n.rect
                    );
                }
            }
        }
    }

    // Same pin for RL against LR (the horizontal mirror).
    let lr = layered(&desc, &opts(Direction::LeftRight));
    let rl = layered(&desc, &opts(Direction::RightLeft));
    assert_eq!(lr.bounds, rl.bounds);
    let w = lr.bounds.w;
    for (p, q) in lr.edges[0].waypoints.iter().zip(&rl.edges[0].waypoints) {
        assert_eq!(q.y, p.y, "RL keeps the cross axis");
        assert_eq!(q.x, w - 1 - p.x, "RL mirrors waypoint cells");
    }
}

/// A5 (the named fixture): mirrored directions must never GROW bounds
/// relative to their canonical twin — including multi-rank edges
/// (interior waypoints), a cycle (broken edge routed against flow) and
/// a self-loop lobe.
#[test]
fn bt_rl_mirror_bounds_never_grow_on_the_stress_fixture() {
    let desc = GraphDesc::new()
        .node("a", 7, 3)
        .node("b", 5, 5)
        .node("c", 9, 3)
        .node("d", 5, 3)
        .edge("a", "b")
        .edge("a", "d") // multi-rank: interior waypoint
        .edge("b", "c")
        .edge("c", "d")
        .edge("d", "a") // cycle: broken, routed against flow
        .edge("b", "b"); // self-loop lobe
    let td = layered(&desc, &opts(Direction::TopDown));
    let bt = layered(&desc, &opts(Direction::BottomTop));
    let lr = layered(&desc, &opts(Direction::LeftRight));
    let rl = layered(&desc, &opts(Direction::RightLeft));
    assert_eq!(td.bounds, bt.bounds, "BT bounds equal TD");
    assert_eq!(lr.bounds, rl.bounds, "RL bounds equal LR");
    assert_eq!((td.bounds.x, td.bounds.y), (0, 0), "origin-normalized");
    assert_eq!((bt.bounds.x, bt.bounds.y), (0, 0));
}

/// A2: the self-edge lobe indexes `nodes[g]` after the component
/// packing flatten — Some-by-construction (every local node is placed
/// by exactly one component). This pins the CONSEQUENCE: with multiple
/// components ahead of it, a late node's self-loop must attach to THAT
/// node's rect (an index shift would attach it to a stranger).
#[test]
fn self_loop_attaches_to_its_own_node_across_components() {
    let desc = GraphDesc::new()
        .node("x", 4, 2)
        .node("y", 4, 2)
        .node("s", 6, 3)
        .edge("x", "y") // component 0
        .edge("s", "s"); // component 1, self-loop on the LAST node
    let layout = layered(&desc, &LayeredOpts::default());
    let s = layout.node("s").unwrap().rect;
    let lobe = layout
        .edges
        .iter()
        .find(|e| e.from == "s" && e.to == "s")
        .expect("self-loop present");
    let first = lobe.waypoints[0];
    assert_eq!(
        first.x,
        s.right(),
        "lobe anchors on s's right face: {first:?} vs {s:?}"
    );
    assert!(
        first.y >= s.y && first.y < s.bottom(),
        "lobe anchors within s's rows: {first:?} vs {s:?}"
    );
    // And input order survives the flatten (the invariant itself).
    let ids: Vec<&str> = layout.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["x", "y", "s"]);
}

/// A3: at rank_gap=1 the corridor is ONE cell, so the source and
/// target anchors of an adjacent-rank edge legitimately COINCIDE —
/// argued correct at the layout level (the corridor has exactly one
/// row; both borders touch it). The view owns the consequence: its
/// arrowhead direction falls back to node-rect geometry (pinned in
/// view_render/view goldens). Here we pin the layout-side geometry so
/// a future change is a conscious one.
#[test]
fn rank_gap_one_anchors_coincide_in_the_single_corridor_cell() {
    let desc = GraphDesc::new()
        .node("a", 5, 3)
        .node("b", 5, 3)
        .edge("a", "b");
    let layout = layered(
        &desc,
        &LayeredOpts {
            rank_gap: 1,
            ..Default::default()
        },
    );
    let e = &layout.edges[0];
    assert_eq!(e.waypoints.len(), 2);
    assert_eq!(
        e.waypoints[0], e.waypoints[1],
        "one-cell corridor: both anchors share it"
    );
    let a = layout.node("a").unwrap().rect;
    let b = layout.node("b").unwrap().rect;
    let p = e.waypoints[0];
    assert!(p.y == a.bottom() && p.y == b.y - 1, "the corridor row");
}

/// A6: PAVA cross-rank alignment runs a bounded down/up/down pass
/// schedule — deliberately non-convergent ("lite"). This golden pins
/// the three-pass outcome on a conflict-heavy W-fixture (two parents
/// sharing a middle child) so any change to the pass schedule or the
/// regression math shows up as a conscious diff, and oscillation
/// cannot silently degrade the picture.
#[test]
fn pava_three_pass_coordinates_are_pinned_on_the_w_fixture() {
    let desc = GraphDesc::new()
        .node("p1", 6, 3)
        .node("p2", 6, 3)
        .node("c1", 6, 3)
        .node("c2", 6, 3)
        .node("c3", 6, 3)
        .edge("p1", "c1")
        .edge("p1", "c2")
        .edge("p2", "c2")
        .edge("p2", "c3");
    let layout = layered(&desc, &LayeredOpts::default());
    let rect = |id: &str| layout.node(id).unwrap().rect;
    // Determinism at the coordinate level: two runs, identical.
    let again = layered(&desc, &LayeredOpts::default());
    assert_eq!(layout.nodes, again.nodes);
    assert_eq!(layout.edges, again.edges);

    // Structural sanity: the shared child sits between its siblings,
    // and each parent sits over its own children's span.
    let (c1, c2, c3) = (rect("c1"), rect("c2"), rect("c3"));
    assert!(
        c1.x < c2.x && c2.x < c3.x,
        "children ordered: {c1:?} {c2:?} {c3:?}"
    );
    let (p1, p2) = (rect("p1"), rect("p2"));
    assert!(p1.x < p2.x, "parents ordered");
    // The exact three-pass outcome (golden; minted from the shipped
    // solver and reviewed for the sanity above).
    assert_eq!(
        (p1.x, p2.x, c1.x, c2.x, c3.x),
        (5, 14, 0, 9, 18),
        "W-fixture cross coordinates moved — pass-schedule change?"
    );
}

/// A1 (layout side): unresolvable edges are DROPPED from
/// `Layout::edges` (positions shift) while `desc_index` keeps the
/// metadata join lawful — pinned here; the rendering half of the pin
/// (styles never shift onto survivors) lives in view_render.rs.
#[test]
fn unresolvable_edges_drop_but_desc_index_keeps_the_join() {
    let desc = GraphDesc::new()
        .with_node(NodeDesc::new("a", 5, 3))
        .with_node(NodeDesc::new("b", 5, 3))
        .edge("a", "ghost")
        .edge("a", "b");
    let layout = layered(&desc, &LayeredOpts::default());
    assert_eq!(layout.edges.len(), 1, "ghost edge dropped");
    assert_eq!(
        layout.edges[0].desc_index, 1,
        "survivor still names its OWN desc slot"
    );
    assert!(
        layout.fallback.as_deref().unwrap_or("").contains("skipped"),
        "and the drop is labeled: {:?}",
        layout.fallback
    );
    let _ = Point::new(0, 0); // keep the shared-type re-export honest
}
