# 0440 — Read-only auto-layout graph view (layered/sugiyama-lite)

## Metadata
- Created: 2026-07-22
- Status: Completed (layout half cycle 1 by GRAPH; view half cycle 2
  by CANVAS — both in workspace member `abstracttui-graph`)
- Track: extensions
- Completed: 2026-07-24

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
- [x] 0400 ruled (crate home shared with 0430) — ADR-0004 accepted
- [x] GraphDesc/Layout types + layered() (rank, order, coords)
- [x] Cycle handling + grid-snap fallback (labeled)
- [x] GraphView rendering over 0420 (cards, beziers, arrowheads)
- [x] Scroll/pan + activation callback
- [x] Fixtures, goldens, perf budget, example (view goldens +
      interaction wave + two examples landed cycle 2)

## Progress notes

### 2026-07-24 — layout half SHIPPED (`extensions/graph`, cycle 1)

The layout engine landed as workspace member `abstracttui-graph`
0.1.0 (`extensions/graph/`; std + abstracttui only per ADR-0004 §4;
`cargo package` verifies). The view half (GraphView over 0420) remains
— this item stays open for cycle 2.

- **Contract as frozen**: `GraphDesc` (nodes id+Size+kind/label, edges
  from/to+label/style) -> `Layout` (per-node `Rect`+rank, per-edge
  waypoint polylines, origin-normalized bounds, honesty markers:
  per-edge `broken` + derived `broken_edges()`, `fallback` label).
  Passes: `layout::layered(&GraphDesc, &LayeredOpts)`,
  `layout::force(&GraphDesc, &ForceOpts)` (`ForceOpts { seed, budget:
  IterationBudget, rank_bias: Option<Direction>, spacing }`, plain FRU
  struct per ADR-0003 §2), `layout::grid(&GraphDesc)` (always
  labeled). Directions TD/LR/BT/RL computed in (cross, flow) space and
  mapped (LR = transpose, BT/RL = flow mirror); cards never rotate.
- **Coordinate note**: naive median passes with a left-to-right clamp
  left the diamond's parent left-aligned over its first child; the
  shipped `coords.rs` reduces rank packing to isotonic regression
  (pool-adjacent-violators after the cumulative-width change of
  variables), so colliding siblings center as a block — diamond
  symmetric, chains straight, no rightward drift, still O(rank) and
  deterministic.
- **Bounds**: sweeps default 4, node cap default 512 (past it: grid
  fallback labeled "node cap exceeded (N > cap)"), force budget
  default 256 with settle freeze (unit-pinned via the internal
  iteration count). Determinism: no map iteration, stable sorts, f64
  restricted to + - * / sqrt (no transcendentals) — goldens are
  cross-platform.
- **Measured (dev machine, unoptimized test profile)**: 500 nodes /
  718 edges — `layered` 13.9 ms, `force` (budget 64) 29.8 ms; asserted
  bounds 10 s / 20 s stay generous for CI.
- **Tests (47)**: determinism —
  `layered_is_deterministic_run_to_run`,
  `force_is_deterministic_under_fixed_seed_and_budget`,
  `grid_is_deterministic_run_to_run`,
  `force_seeds_actually_scatter_differently`,
  `layered_golden_diamond_exact_cells` (hard-pinned cells),
  `force_golden_five_nodes_exact_cells`; layered quality —
  `eight_node_dag_ranks_are_longest_path`,
  `ranks_increase_along_every_kept_edge` (DAG-after-break proof),
  `planar_case_lays_out_crossing_free`,
  `diamond_centers_the_sink_between_its_parents`,
  `multi_rank_edge_routes_around_intermediate_cards`,
  `four_directions_are_transpose_consistent`; cycles/fallback —
  `two_cycle_breaks_exactly_one_edge_and_marks_it`,
  `three_cycle_ranks_stay_a_dag_and_the_break_is_deterministic`,
  `figure_eight_breaks_one_edge_per_cycle`,
  `node_cap_degrades_to_grid_with_the_cap_named`,
  `explicit_grid_is_always_labeled_and_row_ranked`,
  `components_lay_out_side_by_side`; force behavior —
  `rank_bias_pulls_targets_downstream`,
  `rank_bias_respects_reversed_and_horizontal_directions`,
  `force_reports_no_hierarchy_and_breaks_nothing`,
  `layout_is_origin_normalized_and_bounded`; edge cases (empty /
  single / self-edge / duplicate edge / duplicate id / unknown
  endpoint / ascii-dump oversize refusal); scale —
  `layered_500_nodes_under_budget`, `force_500_nodes_under_budget`
  (measured numbers printed); plus 12 unit tests (resolve, ordering,
  coords/PAVA, geom, force settle, PRNG) and 4 doctests.
- **Deliberate v1 shapes (for the view half to revisit)**: self-edges
  render as a constant right-face lobe in all directions; force edges
  are straight border-clipped segments (parallel force edges
  coincide); `dump::ascii` is a public debugging aid (integration
  tests compile as foreign crates, so a test-only helper could not be
  shared); grid ignores direction (row-major).

### 2026-07-24 — view half SHIPPED (cycle 2, CANVAS seat) — item COMPLETE

`GraphView` landed in the same crate over the core canvas layer
(0420): `src/view.rs` (+ `view_cards.rs`, `view_edges.rs` siblings,
all <600 lines), exports `GraphView`/`GraphStyle`/`GraphAlgo`.

- **Rendering**: node cards (title on the top border, kind-tinted
  left-column accent from `GraphStyle::kind_accents`, reactive badge
  slot via `badges(fn)`, honest size degradation down to a 1-row
  chip); edges as `abstracttui::canvas` strokes — midpoint-smoothed
  quadratic beziers through the layout waypoints (straight chains
  collapse naturally), arrowheads (`▲▼◀▶` by dominant axis of the
  last non-degenerate segment, node-rect fallback for the rank_gap=1
  coincident-anchor case), `EdgeDesc::style` vocabulary
  ("dotted"/"dashed" = every-third-dot sampling, "thick"/"bold" =
  three offset passes), cycle-broken edges FORCED dotted in
  `edge_broken` ink and blitted after normal strokes (the honesty
  marker wins shared cells); `Layout::fallback` renders as a
  non-scrolling "⚠ …" notice row above the viewport.
- **Parallel/opposite legibility (peer item 4)**: 2-point edges
  sharing an unordered endpoint pair bow as quadratics with
  canonical-frame ordinals — a->b and b->a bow to OPPOSITE sides,
  duplicates fan out; composes with the layered pass's anchor
  spreading. Self-loops render the layout's right-face lobe with a
  `◀` return arrow (direction-blind lobe documented as the layout's
  v1 shape — not worth view-side surgery).
- **Interaction**: pan via `Scroll` composition (`content_size` =
  `Layout::bounds`, both axes, auto-hidden scrollbar, offsets
  bindable for overflow-honesty chrome); ONE tab stop; Enter selects
  the first node / presses the selected one, arrows walk nodes
  spatially (aligned-first: perpendicular offset doubly penalized,
  input-order tiebreak, ensure-visible adjusts the pan) or PAN when
  nothing is selected, Escape deselects; plain keys only (modified
  combinations pass through — the PageHost container-chord lesson);
  click selects + presses; `on_node_press` is disposal-safe
  (take-call-restore); hover tooltips ride the core anchored
  `Tooltip` (overlays from context or `overlays()`, absent = skipped,
  per-generation scope closes stale tips).
- **Reactivity rule (documented)**: layout is an ACT at `view(cx)`;
  data changes rebuild via `dyn_view` (the chart recipe); force
  re-runs under its fixed seed — no warm-start surface in v1
  (cached-position reheat is 0430's lane). Style derives from the
  active theme (tracked) unless an explicit `GraphStyle` is given.
- **Layout-lane fix (verified failing-first)**: `map_point` mirrored
  BT/RL waypoints as `-f`; cells are half-open intervals so the
  mirror is `-(f+1)` (the `map_rect` model; round-half-away commutes:
  `round(-(f+1)) == -(round(f)+1)`). Symptom: BT source anchors
  landed INSIDE the source card and were painted over — caught by the
  view's BT arrowhead golden, pinned by
  `view_attack_list.rs::bt_rl_waypoints_mirror_like_rects_and_stay_out_of_cards`
  (red before the fix, green after; the cycle-1 suite untouched and
  green — its transpose test pinned node mirrors but not waypoints).
- **Attack-list dispositions** (cycle-1 items): unresolvable-edge
  drops — view joins metadata ONLY via `desc_index`
  (`unresolvable_edge_drop_does_not_shift_styles_onto_survivors` +
  layout-side pin); `nodes[g]` flatten — Some-by-construction
  verified, consequence pinned
  (`self_loop_attaches_to_its_own_node_across_components`);
  rank_gap=1 anchors — argued CORRECT at layout level (one corridor
  cell, both anchors legitimately share it; pinned), the view owns
  the arrowhead fallback; force parallel/self-edge — view-side
  bowing (above); BT/RL mirror bounds — fixture added
  (`bt_rl_mirror_bounds_never_grow_on_the_stress_fixture`, bounds
  equal on cycles + multi-rank + self-loops), plus the waypoint fix;
  PAVA 3-pass oscillation — argued bounded-by-design, W-fixture
  coordinate golden pins the pass-schedule outcome
  (`pava_three_pass_coordinates_are_pinned_on_the_w_fixture`).
- **Tests (23 new; crate total 70 passed / 0 failed)**: view_render
  (11): card golden, selection restyle, arrowheads x4 directions,
  dotted-sparser-than-solid (lit-dot metric), thick-denser, broken
  ink + both cycle arrows, self-loop lobe, fallback notice,
  desc_index join, opposite bows apart, force parallels separate;
  view_interact (6, REAL App/Driver/CaptureTerm wire bytes):
  click-select+press, full keyboard vocabulary incl. pan/selection
  mode switch, wheel pan, zero-idle pin (16 idle turns, zero bytes),
  damage containment (selection repaint = exactly the card's rows;
  no cursor move into the header row), tooltip appear/dismiss via
  SGR motion; view_attack_list (6, above). The full-Driver harness
  worked from the extension crate without walls — everything needed
  is public API.
- **Perf**: layout numbers stand from cycle 1 (500 nodes: layered
  13.9 ms, force 29.8 ms); the view adds build-time planning O(edges)
  and per-repaint stroke work only (idle = zero, pinned).
- **Examples**: `workflow` (layered pipeline, status kind tints, a
  dotted async edge, a publish→fetch retry cycle showing the broken
  marker, badges, press status line) and `network` (force-placed
  concept graph, seeded, tooltips, a mermaid⇄graphs opposite pair) —
  both headless-guard exit 0 and interactive.
- **Deliberate v1 shapes (view)**: edge labels render at the middle
  waypoint (+1 col), truncated to 16 cells; selection resets on
  rebuild unless bound via `selected(sig)`; arrowhead glyph uses the
  chord direction for bowed pairs (dominant axis rarely differs);
  spatial navigation is aligned-first (documented in the module docs
  and pinned) rather than rank-stepping — one rule for every pass,
  including rank-less force layouts.
