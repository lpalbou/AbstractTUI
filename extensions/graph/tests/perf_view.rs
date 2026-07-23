//! Wave-9 performance proofs (numbers PRINT; bounds stay generous —
//! CI machines vary; reviews/wave9/perf-proofs.md carries the
//! measured values): layout scale at 100/500 nodes, GraphView render
//! cost (full frame vs one-badge damage frame), and the edge-pass
//! allocation pin (edge-count-independent — no per-edge/per-dot heap
//! traffic; the counting-allocator pattern from the root
//! tests/alloc_budget.rs, confined to this test binary).

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::cell::Cell;
use std::time::Instant;

use abstracttui::app::{request_full_redraw, App, Driver, RunConfig};
use abstracttui::base::{Rgba, Size};
use abstracttui::prelude::LayoutStyle;
use abstracttui::reactive::{create_root, Signal};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{BufferCanvas, Element, UiTree};
use abstracttui_graph::{
    force, layered, EdgeDesc, ForceOpts, GraphDesc, GraphStyle, GraphView, LayeredOpts,
};

// ---------------------------------------------------------------------------
// The counting allocator (per-thread, forwarding — the alloc_budget
// pattern; test-binary only).
// ---------------------------------------------------------------------------

thread_local! {
    static TL_ALLOCS: Cell<u64> = const { Cell::new(0) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: AllocLayout) -> *mut u8 {
        let _ = TL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: AllocLayout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn alloc_delta(f: impl FnOnce()) -> u64 {
    let before = TL_ALLOCS.try_with(Cell::get).unwrap_or(0);
    f();
    TL_ALLOCS.try_with(Cell::get).unwrap_or(0) - before
}

// ---------------------------------------------------------------------------
// (a) Layout scale: layered + force at 100 and 500 nodes.
// ---------------------------------------------------------------------------

/// Deterministic n-node DAG in the scale_smoke shape (local edges +
/// rank skippers + a few back edges).
fn sized_graph(n: usize) -> GraphDesc {
    let mut desc = GraphDesc::new();
    for i in 0..n {
        desc = desc.node(format!("n{i}"), 6 + (i % 5) as i32, 3);
    }
    for i in 0..n - 1 {
        desc = desc.edge(format!("n{i}"), format!("n{}", i + 1));
    }
    for i in (0..n.saturating_sub(7)).step_by(3) {
        desc = desc.edge(format!("n{i}"), format!("n{}", i + 7));
    }
    for i in (25..n).step_by(50) {
        desc = desc.edge(format!("n{i}"), format!("n{}", i - 25));
    }
    desc
}

#[test]
fn layout_scale_100_and_500_nodes_measured() {
    for n in [100usize, 500] {
        let desc = sized_graph(n);
        let start = Instant::now();
        let l = layered(&desc, &LayeredOpts::default());
        let layered_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let f = force(
            &desc,
            &ForceOpts {
                seed: 3,
                budget: 64,
                ..Default::default()
            },
        );
        let force_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "PERF layout n={n} edges={}: layered {layered_ms:.1} ms (bounds {}x{}), \
             force(64) {force_ms:.1} ms (bounds {}x{})",
            desc.edges.len(),
            l.bounds.w,
            l.bounds.h,
            f.bounds.w,
            f.bounds.h
        );
        assert_eq!(l.nodes.len(), n);
        assert_eq!(f.nodes.len(), n);
        // Generous CI bounds; the printed numbers are the proof.
        assert!(layered_ms < 10_000.0, "layered {n}: {layered_ms:.1} ms");
        assert!(force_ms < 20_000.0, "force {n}: {force_ms:.1} ms");
    }
}

// ---------------------------------------------------------------------------
// (b) Render cost: full-frame bytes vs one-badge damage bytes at 80x24.
// ---------------------------------------------------------------------------

/// A 30-node workflow-ish DAG (two parallel chains with rungs).
fn monitor_graph() -> GraphDesc {
    let mut desc = GraphDesc::new();
    for i in 0..30 {
        desc = desc.node(format!("s{i}"), 8, 3);
    }
    for i in 0..14 {
        desc = desc.edge(format!("s{i}"), format!("s{}", i + 1));
        desc = desc.edge(format!("s{}", i + 15), format!("s{}", i + 16));
    }
    for i in (0..14).step_by(4) {
        desc = desc.edge(format!("s{i}"), format!("s{}", i + 15));
    }
    desc
}

#[test]
fn render_full_frame_vs_badge_damage_bytes() {
    let size = Size::new(80, 24);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let badge_slot: std::rc::Rc<Cell<Option<Signal<u32>>>> = std::rc::Rc::new(Cell::new(None));
    let slot = badge_slot.clone();
    app.mount(move |cx| {
        let count = cx.signal(1u32);
        slot.set(Some(count));
        Element::new()
            .style(LayoutStyle::column())
            .child(
                GraphView::new(monitor_graph())
                    .badges(move |id| (id == "s0").then(|| count.get().to_string()))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(abstracttui::term::Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let settle = |driver: &mut Driver, app: &mut App, term: &mut CaptureTerm| {
        for _ in 0..64 {
            if driver.turn(app, term).expect("turn").idle {
                break;
            }
        }
    };
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes(); // setup + first paint

    // Full repaint (the Ctrl+L verb): every viewport cell re-emits.
    request_full_redraw();
    settle(&mut driver, &mut app, &mut term);
    let full = term.take_bytes().len();

    // One badge change: the graph's dyn card repaints, nothing else.
    badge_slot.get().expect("badge signal").set(2);
    settle(&mut driver, &mut app, &mut term);
    let damage = term.take_bytes().len();

    println!(
        "PERF render 80x24, 30 nodes: full frame {full} B, one-badge damage {damage} B \
         ({:.1}% of full)",
        damage as f64 * 100.0 / full as f64
    );
    assert!(full > 0 && damage > 0, "both frames emitted");
    assert!(
        damage * 4 < full,
        "the damage frame ({damage} B) must be a small fraction of a full frame ({full} B)"
    );
}

// ---------------------------------------------------------------------------
// (c) Edge-pass allocation: edge-count-INDEPENDENT (no per-edge/per-dot
// heap traffic in the draw path; the plan is a build-time act).
// ---------------------------------------------------------------------------

#[test]
fn edge_pass_allocation_is_edge_count_independent() {
    let style = GraphStyle {
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
    };
    // Same six nodes; 5 edges vs 20 edges (duplicates fan out via the
    // bow planner, exercising the bowed-bezier path too).
    let nodes = |mut d: GraphDesc| {
        for i in 0..6 {
            d = d.node(format!("n{i}"), 7, 3);
        }
        d
    };
    let sparse = {
        let mut d = nodes(GraphDesc::new());
        for i in 0..5 {
            d = d.edge(format!("n{i}"), format!("n{}", i + 1));
        }
        d
    };
    let dense = {
        let mut d = nodes(GraphDesc::new());
        for i in 0..5 {
            for _ in 0..4 {
                d = d.with_edge(EdgeDesc::new(format!("n{i}"), format!("n{}", i + 1)));
            }
        }
        d
    };

    let measure = |desc: GraphDesc, style: GraphStyle| -> u64 {
        let size = Size::new(60, 30);
        let mut tree = UiTree::new(size);
        let (_root, ()) = create_root(|cx| {
            let view = GraphView::new(desc).style(style).view(cx);
            tree.mount(cx, view);
        });
        // Warm: first draw pays one-time layout/mount costs.
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        alloc_delta(|| {
            let mut canvas = BufferCanvas::new(size);
            tree.draw(&mut canvas);
            std::hint::black_box(&canvas);
        })
    };
    let sparse_allocs = measure(sparse, style.clone());
    let dense_allocs = measure(dense, style);
    println!(
        "PERF edge-pass allocs per repaint: 5 edges = {sparse_allocs}, 20 edges = {dense_allocs}"
    );
    // The draw path allocates a bounded, edge-count-INDEPENDENT amount
    // (two dot grids + per-card text shaping); a per-edge or per-dot
    // allocation would scale with the edge count. Small slack for
    // allocator noise.
    assert!(
        dense_allocs <= sparse_allocs + 4,
        "edge pass allocation scaled with edges: {sparse_allocs} -> {dense_allocs}"
    );
}
