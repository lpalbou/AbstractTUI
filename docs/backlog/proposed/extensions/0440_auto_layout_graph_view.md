# 0440 — Read-only auto-layout graph view (layered/sugiyama-lite)

## Metadata
- Created: 2026-07-22
- Status: Proposed (needs-design; sibling-crate candidate — same crate
  as 0430; sequenced BEFORE the interactive editor as its risk
  retirement)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: 0400's ADR (sibling crate, public API only).
  ADR impact: none in core.

## Context
The simpler half of the graph story, and the one more app classes
need: render a graph you did NOT hand-position — dependency/build
graphs, pipeline topologies, state machines, module maps: DAG-shaped
data. The consumer holds nodes+edges; the extension computes positions
and draws. It is also the layout engine 0450 (mermaid flowcharts)
consumes — which is why it precedes both 0430 (initial positions,
shared rendering) and 0450 (layout authority) in the track sequencing.
**Honest class boundary (cycle-2 peer finding P1-2, accepted):
knowledge graphs are NOT served by v1.** KGs are routinely cyclic,
dense, and non-hierarchical — exactly the layering defeat case — so
advertising them against a layered v1 would violate the honest-claims
principle. They are served by the v1.5 force stage below, which is
designed (not research) precisely because that class is a named
motivator of the whole graph lane.

Honest layout-algorithm scoping is the core of this item. The study's
position, argued against the real constraint set:
- Terminal canvases are SMALL (a 200x60 screen is ~400x240 braille
  dots) and CELL-QUANTIZED — node cards are 10-30 cells wide. Layout
  quality is dominated by rank assignment and crossing reduction, not
  by sub-cell precision. A full Sugiyama implementation
  (network-simplex ranking, exhaustive ordering) is thousands of lines
  for quality invisible at this resolution.
- **v1 = sugiyama-lite**: longest-path layer assignment (linear),
  median/barycenter crossing reduction with a bounded sweep count
  (deterministic — same graph, same picture), simple
  horizontal-coordinate assignment (aligned medians), edges as 0420
  beziers with layer-gap waypoints. Cycles broken by a documented
  DFS heuristic (marked, never silently reordered).
- **Grid-snap fallback**: graphs that defeat layering (dense,
  near-clique) degrade to a labeled grid placement — the engine's
  honest-degradation discipline (roadmap principle 4) applied to
  layout: a bad layout with a label beats a hung solver.
- **Force layout is a DESIGNED v1.5 stage, not research** (upgraded
  from "optional/research" on the peer review's argument): the
  zero-idle-compatible shape is proven practice — an alpha-cooled
  placement pass that runs a bounded iteration budget ON DEMAND
  (initial layout or explicit re-layout), freezes on settle, reheats
  briefly on graph mutation, and never runs as an idle animation
  (roadmap principle 5 holds because the sim is an *act*, not a
  state). v1.5 scope: repulsion + edge springs + optional rank bias,
  deterministic under a fixed seed + iteration budget so goldens
  remain possible; positions cached so re-render never re-simulates.
  This is the knowledge-graph path; v1 ships layered-only and says so.
  **Signature contract (cycle-3 closure of the open question)**: the
  force pass shares the layout module's pure data-in/out shape —
  `layout::force(&GraphDesc, &ForceOpts) -> Layout`, the SAME
  `GraphDesc` in and `Layout` out as `layered()` (§1 below), with
  `ForceOpts { seed: u64, budget: IterationBudget, rank_bias:
  Option<Direction>, .. }` (`#[non_exhaustive]`-or-FRU per ADR-0003's
  classification). Every layout pass in this crate is `GraphDesc ->
  Layout`; consumers select the ALGORITHM, never a different data
  contract — which is what lets 0450 route a future non-hierarchical
  diagram kind to `force()` without touching its renderer, and lets
  0430 use either pass for auto-arrange.

## Current code reality
- No graph or layout code exists anywhere in the crate (grepped: the
  only Bresenham is chart.rs's private grid; the flexbox solver
  src/layout/solve.rs is a box-tree solver, not a graph embedder —
  reusing it for graph layout would be a category error).
- Rendering substrate: 0420 (strokes) + the same card/edge mechanics
  as 0430 (Position::Absolute placement, src/layout/style.rs:95-101;
  Element draw closures, src/ui/view.rs:155-158).
- Scroll composition: a laid-out graph larger than the viewport pans
  via `Scroll` with an explicit content size
  (`Scroll::content_size` override wins over measurement,
  src/widgets/scroll.rs:11-24) — the layout's bounding box is that
  size; no new scroll machinery.
- Determinism precedent to match: charts pin "same data, same cells"
  (src/widgets/chart.rs:22-23, chart_tests.rs) — layout must pin "same
  graph, same positions" or golden tests are impossible.
- Theme discipline: node/edge colors from `TokenSet` ramps
  (src/theme/tokens.rs:341) resolved by the caller, per the widget
  token rule (src/widgets/mod.rs:8-15).

## Problem
There is no way to SHOW a graph on AbstractTUI without hand-placing
every node; every consuming app would import or invent a layout
algorithm, then re-derive edge drawing — the two exact costs an
extension exists to pay once.

## What we want (proposed shape — needs-design)
In `abstracttui-graph` (shared with 0430):
1. **`layout` module, render-independent**: `fn layered(&GraphDesc,
   opts) -> Layout` — pure data in/out (node sizes in cells in; ranks,
   positions, edge waypoints out). Public so 0450 consumes it without
   pulling the interactive machinery; deterministic; bounded (sweep
   counts, node caps documented — past the cap, grid-snap with label).
   **`GraphDesc -> Layout` is the module's ONE contract**: every pass
   (`layered()` v1, `force()` v1.5 — signature above) takes the same
   input type and yields the same output type, so renderers and
   consumers (0430 auto-arrange, 0450 routing) bind to the data
   contract once and choose algorithms freely.
2. **`GraphView` widget**: read-only rendering of a `Layout` — node
   cards (compact recipe by default: title + badge), 0420 bezier edges
   with arrowhead glyphs, direction TD/LR (transpose), pan via Scroll,
   node activation callback (click → app; the 0165 link-id path when
   it lands, same synergy as 0430).
3. **Overflow honesty**: layouts wider than the canvas report their
   bounding box; the widget never silently crops — Scroll owns the
   viewport, and a "N nodes off-screen" affordance is the app's to
   render from exposed counts.
4. Fixture corpus: DAGs (typical), cyclic graphs (broken-edge
   marking), degenerate (single node, disconnected components —
   components lay out side by side), dense (grid-snap fallback
   trigger).

## Scope / Non-goals
Scope: layered layout + grid fallback, GraphView, pan composition,
activation callback, fixtures + goldens, one example (a module
dependency map). v1.5 (designed above, built on the first KG-class
consumer): the bounded on-demand force pass. Non-goals: continuous/
idle force animation (never — the sim is an act, not a state); edge
label placement optimization (v1 = midpoint label, truncated per
text::truncate);
interactive editing (0430); incremental relayout on mutation (v1
relays out whole — cheap at terminal scale, measure before optimizing);
subgraphs/clusters (mermaid's `subgraph` maps to v2 here — 0450's
subset table declares it unsupported until then).

## Expected outcomes
`GraphView::new(nodes, edges).view(cx)` shows a readable dependency
graph in one call; 0450 gets its layout authority; 0430 gets initial
positions ("auto-arrange") for free.

## Validation
- Layout unit tests: rank correctness on fixture DAGs, crossing count
  non-increasing across sweeps, determinism (two runs, identical
  output), cycle-break marking, component packing, cap→grid-snap
  labeled fallback.
- CaptureTerm goldens: small DAG renders pinned cells (the chart
  determinism discipline); TD↔LR transpose; pan.
- Perf budget: 200-node/400-edge layout under a stated wall-time
  budget on the dev machine (bench, not aspiration — numbers into the
  completion report).
- Zero-idle pin: mounted view idles at zero cost.

## Progress checklist
- [ ] 0400 ruled (crate home shared with 0430)
- [ ] GraphDesc/Layout types + layered() (rank, order, coords)
- [ ] Cycle handling + grid-snap fallback (labeled)
- [ ] GraphView rendering over 0420 (cards, beziers, arrowheads)
- [ ] Scroll/pan + activation callback
- [ ] Fixtures, goldens, perf budget, example
