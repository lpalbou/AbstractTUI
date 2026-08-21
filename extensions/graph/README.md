# abstracttui-graph

Graph auto-layout AND rendering for
[AbstractTUI](https://github.com/lpalbou/abstracttui): an
[ADR-0004](https://github.com/lpalbou/abstracttui/blob/main/docs/adr/0004-extension-packaging.md)
sibling crate (public core API only, std + `abstracttui` as the whole
dependency posture). Two halves, one crate: the layout engine
(`GraphDesc -> Layout`) and the read-only widget (`GraphView`) drawing
node cards and sub-cell edge strokes over the core canvas layer.

```sh
cargo add abstracttui abstracttui-graph
```

The family guide (pass selection, worked examples, the mermaid
consumer) lives in the repo:
[docs/graphs-and-diagrams.md](https://github.com/lpalbou/abstracttui/blob/main/docs/graphs-and-diagrams.md).
API reference: [docs.rs/abstracttui-graph](https://docs.rs/abstracttui-graph).

## The one contract: `GraphDesc -> Layout`

You describe the graph — nodes with cell sizes, edges by id — and a
layout pass returns positions, ranks, edge waypoint polylines, a
bounding box, and honesty markers. Every pass shares the same input and
output types; consumers select the *algorithm*, never a different data
contract:

| Pass | Shape | For |
| --- | --- | --- |
| `layered(&desc, &LayeredOpts)` | sugiyama-lite: longest-path ranks, bounded median crossing-reduction sweeps, aligned-median coordinates, waypoints through rank gaps, TD/LR/BT/RL | workflows, dependency/build graphs, state machines — DAG-shaped data |
| `force(&desc, &ForceOpts)` | seeded, alpha-cooled repulsion + springs + optional rank bias; bounded budget, freezes on settle | knowledge graphs — cyclic, dense, non-hierarchical data |
| `grid(&desc)` | near-square row-major placement, always labeled | the honest fallback |

Honesty markers: cycle-broken edges are *marked*
(`EdgeLayout::broken`, `Layout::broken_edges()`), never silently
reordered; `Layout::fallback` names every degradation (node cap
exceeded, duplicate ids dropped, unresolvable edges skipped, grid
placement). Everything is deterministic (same input, identical
`Layout` — golden-pinned; no transcendental floats, so goldens hold
across platforms) and bounded (sweep counts, node cap, iteration
budget — documented on the option types).

The force pass is an **act, not an animation**: run it on demand, cache
the `Layout`, re-render from the cache. Zero idle cost is the caller's
story and the engine's rule.

## Example

```rust
use abstracttui_graph::{layered, Direction, GraphDesc, LayeredOpts};

let desc = GraphDesc::new()
    .node("fetch", 9, 3)   // id, width and height in cells
    .node("build", 9, 3)
    .node("test", 8, 3)
    .node("ship", 8, 3)
    .edge("fetch", "build")
    .edge("build", "test")
    .edge("build", "ship")
    .edge("test", "ship");

let layout = layered(&desc, &LayeredOpts {
    direction: Direction::TopDown,
    ..Default::default()
});

for node in &layout.nodes {
    println!("{} at {:?} (rank {})", node.id, node.rect, node.rank);
}
for edge in &layout.edges {
    // Waypoints run from the source card border to the target card
    // border; draw a polyline or spline through them.
    println!("{} -> {}: {:?}", edge.from, edge.to, edge.waypoints);
}
// The bounding box is the content size a Scroll container advertises.
assert_eq!((layout.bounds.x, layout.bounds.y), (0, 0));
assert!(layout.fallback.is_none(), "clean run");

// Plain-ASCII debugging aid (GraphView is the real renderer):
println!("{}", abstracttui_graph::dump::ascii(&layout));
```

## The widget: `GraphView`

```rust
use abstracttui_graph::{GraphAlgo, GraphDesc, GraphView, LayeredOpts};

// Inside a component: GraphView::new(desc).view(cx) — cards with
// kind-tinted accents + badges, canvas-stroke edges (beziers through
// the layout waypoints, arrowheads, dotted/thick styles, cycle-broken
// edges dotted in the error ink), the fallback label as a notice
// line, pan via Scroll (bounds = content size), click/keyboard
// selection with `on_node_press`, hover tooltips.
```

One tab stop; arrows pan until a node is selected (Enter selects the
first, then arrows walk nodes spatially — aligned-first — Enter
presses, Escape returns to pan). Layout is an ACT at view build:
rebuild inside a `dyn_view` over your data to relayout (force re-runs
under its fixed seed; incremental reheat of cached positions is
outside the widget's scope). Colors are caller-resolved
(`GraphStyle::from_tokens`); a
parked view idles at zero (test-pinned). Examples live in the root
crate — from the workspace root, `cargo run --example workflow`
(layered pipeline with a retry cycle) and `--example network`
(force-placed concepts).

## Status

Shipped and stable: the layout engine (`layered()`, `force()`,
`grid()`, the ASCII dump helper) and `GraphView` with all four
directions (BT/RL included), selection, tooltips and pan. The
[`abstracttui-mermaid`](https://docs.rs/abstracttui-mermaid) renderer
consumes this crate as its layout authority — flowcharts you write as
mermaid text land on these same passes.

## License

MIT, same as AbstractTUI.
