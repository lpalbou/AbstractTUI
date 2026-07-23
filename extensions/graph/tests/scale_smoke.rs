//! Scale smoke: 500 nodes through both passes under stated wall-time
//! budgets. The asserted bounds are deliberately generous (CI machines
//! vary); the measured numbers print so the completion report carries
//! reality, not aspiration.

use std::time::Instant;

use abstracttui_graph::{force, layered, ForceOpts, GraphDesc, LayeredOpts};

/// A deterministic 500-node, ~720-edge layered-ish DAG: mostly local
/// edges, some rank-skipping long edges (dummy-chain pressure), a few
/// back edges (cycle-break pressure).
fn big_graph() -> GraphDesc {
    let n = 500usize;
    let mut desc = GraphDesc::new();
    for i in 0..n {
        desc = desc.node(format!("n{i}"), 6 + (i % 5) as i32, 3);
    }
    for i in 0..n - 1 {
        desc = desc.edge(format!("n{i}"), format!("n{}", i + 1));
    }
    for i in (0..n - 7).step_by(3) {
        desc = desc.edge(format!("n{i}"), format!("n{}", i + 7));
    }
    for i in (0..n - 20).step_by(11) {
        desc = desc.edge(format!("n{i}"), format!("n{}", i + 20));
    }
    for i in (25..n).step_by(50) {
        desc = desc.edge(format!("n{i}"), format!("n{}", i - 25));
    }
    desc
}

#[test]
fn layered_500_nodes_under_budget() {
    let desc = big_graph();
    let start = Instant::now();
    let layout = layered(&desc, &LayeredOpts::default());
    let elapsed = start.elapsed();
    println!(
        "layered: 500 nodes / {} edges in {elapsed:?} (bounds {}x{})",
        desc.edges.len(),
        layout.bounds.w,
        layout.bounds.h
    );
    assert_eq!(layout.nodes.len(), 500);
    assert!(!layout.edges.is_empty());
    // Generous bound: measured far below (see printed number).
    assert!(
        elapsed.as_secs_f64() < 10.0,
        "layered took {elapsed:?}, budget 10s"
    );
}

#[test]
fn force_500_nodes_under_budget() {
    let desc = big_graph();
    let opts = ForceOpts {
        seed: 3,
        budget: 64,
        ..Default::default()
    };
    let start = Instant::now();
    let layout = force(&desc, &opts);
    let elapsed = start.elapsed();
    println!(
        "force: 500 nodes / {} edges, budget 64, in {elapsed:?} (bounds {}x{})",
        desc.edges.len(),
        layout.bounds.w,
        layout.bounds.h
    );
    assert_eq!(layout.nodes.len(), 500);
    // Generous bound for unoptimized test profiles; the O(n^2 * budget)
    // pair loop is the documented cost model.
    assert!(
        elapsed.as_secs_f64() < 20.0,
        "force took {elapsed:?}, budget 20s"
    );
}
