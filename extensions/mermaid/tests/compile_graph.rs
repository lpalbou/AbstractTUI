//! Compiler conformance: the IR -> GraphDesc mapping, and the BT/RL
//! geometry fixture over the FIXED cell-interval mirror (cycle-2 view
//! finding: `map_point` mirrors `-(f+1)`; this pins the consequence
//! from the mermaid consumer side, at odd band extents where dummy
//! waypoints sit on fractional band centers).

use abstracttui_graph::{layered, Direction, GraphDesc, LayeredOpts, Layout};
use abstracttui_mermaid::{parse, to_graph, Diagram};

fn compiled(src: &str) -> (GraphDesc, LayeredOpts) {
    match parse(src) {
        Ok(Diagram::Flowchart(fc)) => to_graph(&fc),
        other => panic!("expected flowchart, got {other:?}"),
    }
}

#[test]
fn to_graph_maps_shapes_edges_and_direction() {
    let (desc, opts) =
        compiled("graph LR\nA[Start] -->|go| B{Choice}\nB -.-> C(End)\nB ==> D([S])\nA --- D");
    assert_eq!(opts.direction, Direction::LeftRight);

    let node = |id: &str| desc.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(node("A").label.as_deref(), Some("Start"));
    assert_eq!(node("A").kind, None, "rect/plain carry no kind");
    assert_eq!(node("B").kind.as_deref(), Some("decision"));
    assert_eq!(node("C").kind.as_deref(), Some("rounded"));
    assert_eq!(node("D").kind.as_deref(), Some("stadium"));
    // Sizes derive from the label: width + card chrome, height 3.
    assert_eq!(node("A").size.h, 3);
    assert_eq!(node("A").size.w, 9, "width('Start') + 4");

    assert_eq!(desc.edges[0].label.as_deref(), Some("go"));
    assert_eq!(desc.edges[0].style, None);
    assert_eq!(desc.edges[1].style.as_deref(), Some("dotted"));
    assert_eq!(desc.edges[2].style.as_deref(), Some("thick"));
    assert_eq!(desc.edges[3].style.as_deref(), Some("open"));
}

#[test]
fn node_widths_clamp_to_readable_bounds() {
    let (desc, _) = compiled(&format!(
        "graph TD\nA[x]\nB[{}]\nA --> B",
        "a very long label ".repeat(4)
    ));
    let node = |id: &str| desc.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(node("A").size.w, 7, "floor");
    assert_eq!(node("B").size.w, 34, "cap (labels truncate at draw)");
}

/// No waypoint may land INSIDE a card. Border-adjacent is fine; the
/// card interior is the defect the cell-interval mirror fix closed.
fn assert_waypoints_clear_of_cards(layout: &Layout, tag: &str) {
    for e in &layout.edges {
        for p in &e.waypoints {
            for n in &layout.nodes {
                assert!(
                    !n.rect.contains(*p),
                    "{tag}: waypoint {p:?} of {}->{} inside card {} {:?}",
                    e.from,
                    e.to,
                    n.id,
                    n.rect
                );
            }
        }
    }
}

/// The mermaid-side BT fixture: card height 3 makes every band extent
/// ODD (fractional dummy centers at `flow_start + 1.5`), and the
/// multi-rank edge routes through such a dummy. Under the fixed
/// mirror, BT is the exact cell mirror of TD — rects by
/// `H - (y + h)`, waypoints by `H - 1 - y` — and nothing lands inside
/// a card.
#[test]
fn bt_mirrors_td_exactly_at_odd_band_extents() {
    let src_td = "flowchart TD\nA --> B\nB --> C\nA --> C";
    let src_bt = "flowchart BT\nA --> B\nB --> C\nA --> C";
    let (desc_td, opts_td) = compiled(src_td);
    let (desc_bt, opts_bt) = compiled(src_bt);
    let td = layered(&desc_td, &opts_td);
    let bt = layered(&desc_bt, &opts_bt);
    assert_eq!(td.bounds, bt.bounds, "mirror is an isometry");
    let h = td.bounds.h;

    for n in &td.nodes {
        let m = bt.node(&n.id).unwrap();
        assert_eq!(m.rect.x, n.rect.x, "cross axis untouched ({})", n.id);
        assert_eq!(
            m.rect.y,
            h - (n.rect.y + n.rect.h),
            "flow axis mirrored ({})",
            n.id
        );
    }
    for (e_td, e_bt) in td.edges.iter().zip(bt.edges.iter()) {
        assert_eq!(e_td.desc_index, e_bt.desc_index);
        assert_eq!(e_td.waypoints.len(), e_bt.waypoints.len());
        for (p, q) in e_td.waypoints.iter().zip(e_bt.waypoints.iter()) {
            assert_eq!(q.x, p.x);
            assert_eq!(q.y, h - 1 - p.y, "cell-interval mirror of {p:?}");
        }
    }
    assert_waypoints_clear_of_cards(&td, "TD");
    assert_waypoints_clear_of_cards(&bt, "BT");
}

/// Same fixture rotated: RL vs LR (odd band extents along the
/// horizontal flow need non-uniform card widths, which mermaid labels
/// produce naturally).
#[test]
fn rl_mirrors_lr_and_stays_out_of_cards() {
    let src_lr = "flowchart LR\nA[go] --> B[work item]\nB --> C[x]\nA --> C";
    let src_rl = "flowchart RL\nA[go] --> B[work item]\nB --> C[x]\nA --> C";
    let (desc_lr, opts_lr) = compiled(src_lr);
    let (desc_rl, opts_rl) = compiled(src_rl);
    let lr = layered(&desc_lr, &opts_lr);
    let rl = layered(&desc_rl, &opts_rl);
    assert_eq!(lr.bounds, rl.bounds);
    let w = lr.bounds.w;

    for n in &lr.nodes {
        let m = rl.node(&n.id).unwrap();
        assert_eq!(m.rect.y, n.rect.y);
        assert_eq!(m.rect.x, w - (n.rect.x + n.rect.w), "{}", n.id);
    }
    for (e_lr, e_rl) in lr.edges.iter().zip(rl.edges.iter()) {
        for (p, q) in e_lr.waypoints.iter().zip(e_rl.waypoints.iter()) {
            assert_eq!(q.y, p.y);
            assert_eq!(q.x, w - 1 - p.x);
        }
    }
    assert_waypoints_clear_of_cards(&lr, "LR");
    assert_waypoints_clear_of_cards(&rl, "RL");
}
