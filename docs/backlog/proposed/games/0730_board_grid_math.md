# 0730 — Board-grid math: square + hex coordinates, range, line, projection

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: games (band 0700–0790)
- Completed: N/A
- Depends on: nothing (pure math module; zero render/reactive
  coupling). Composes with 0720 (blitting tiles at projected cells)
  and extensions 0420 (stroking hex outlines) without depending on
  either.
- Cross-band consumers: map viewers (dashboards plotting grid-placed
  assets), seat/floor-plan pickers, pathfinding visualizations, the
  BattleTech-class tactics game that motivated it.
- Promotion trigger: the first grid-mapped surface in any dogfood app
  or game example.

## ADR status
- Governing ADRs: ADR-0001 (additive wherever it lands; no existing
  API touched). ADR-0003 applies if any config struct appears (none
  planned — free functions + tiny coord structs).
- **Placement is a 0400 decision, not this item's to assert**
  (convergence cycle 2 correction — the first draft presumed a core
  home). Extensions 0400's classification rule governs: "does a
  minimal app pay for it if it is in-tree, and does it have its own
  release cadence? Both yes = sibling crate; minimal-app-cost yes but
  no independent cadence = feature; neither = core." Both precedents
  are live and pull opposite ways:
  - **Core precedent (0420)**: the dot canvas went core because "a
    minimal app drawing a sparkline already contains most of it" —
    that argument does NOT transfer here (no minimal app contains hex
    math today; this item's own Current-code-reality says the crate
    has zero grid code).
  - **Sibling precedent (0440)**: the graph extension keeps its pure
    layout math (`layout::layered/force`, `GraphDesc -> Layout`) in
    the sibling crate `abstracttui-graph` — pure data-in/out math
    riding its domain's crate, not core.
  - Arguments FOR core anyway: a few hundred lines of dependency-free
    integer math, cross-band consumers in dashboard-class apps (map
    viewers, seat plans), no plausible independent release cadence,
    and no games sibling crate exists to ride.
  - Resolution path: this item promotes only WITH a recorded 0400
    classification (core module vs a future `abstracttui-game`-class
    sibling). If 0400's ADR lands first, apply its table; if this
    promotes first, the placement paragraph in the completion report
    must run the table explicitly. ADR impact: none beyond that
    classification.

## Context
The maintainer's named target is "battletech dos" — hex-map tactics.
Hex grids are the canonical wheel every game re-derives wrong once
(coordinate system choice, neighbor parity bugs, rounding on lines),
and the terminal adds one trap of its own: cells are ~1:2, so a
hex-to-cell projection that ignores aspect renders vertically squashed
boards. Square grids (roguelikes, dungeon RPGs) need the same family:
neighbors, distance, line-of-sight traversal, range disks.

## Current code reality
- No grid math exists anywhere in src/ (grep hex/axial/grid-coord:
  the only "grid" hits are the layout `Grid` widget,
  src/widgets/grid.rs — a LAYOUT container, unrelated — and braille
  dot grids in chart.rs).
- The engine's aspect-correction precedent is consistent and load-
  bearing: particles halve vertical velocity and gravity ("cells are
  ~2x tall", src/anim/particles.rs:120-122, 138-140), bursts correct
  direction so circles look round (particles.rs:105-108), mosaic
  contain-fit uses `cols / (2*rows)` (the consumer's image_block
  re-derives the same, abstractcode-tui src/ui/transcript_view.rs:
  222-230). A grid projection module must own this correction once.
- Deterministic integer line traversal exists PRIVATELY for dots:
  `BrailleGrid::line` is a Bresenham in chart.rs (src/widgets/
  chart.rs:82-105) — dot-space, not cell/hex-space; 0420 publishes the
  dot canvas but not board-space traversal. Different layer, no reuse
  conflict.
- `Surface::blit`/draw + `LayerHandle::set_offset` give rendering and
  smooth scroll (src/render/surface.rs:421-460; src/app/overlays.rs:
  602-604); what's missing is only "which cell is hex (q,r)" and its
  inverse for mouse picking.

## Problem
A hex tactics game today starts with two hundred lines of coordinate
math copied from the literature, then debugs parity/rounding at the
edges, then discovers the 1:2 aspect squash and re-derives the
projection. Square-grid games re-derive LOS rays and range disks. All
of it is pure, deterministic, golden-testable math the engine can own
once — the same "engine already half-owns it" argument that justified
0420 for strokes.

## What we want
One small pure module (`grid`), two coordinate families:

1. **Square grids**: `Cell(x, y)` ops — 4/8-neighbors, Chebyshev/
   Manhattan distance, integer line traversal (Bresenham cell walk —
   supercover variant available for LOS that must not corner-skip),
   range disks, rect iteration. Deterministic: same endpoints, same
   walk, every platform (integer-only — the shaders' no-libm
   discipline, src/anim/shaders.rs:8-13, applied to traversal).
2. **Hex grids**: axial `Hex(q, r)` as the canonical type (offset
   conversions provided for map storage), the six neighbors, hex
   distance, `line(a, b)` (cube-lerp + round, the standard exact
   algorithm, integer-stable), `range(center, n)` iteration, and ring
   iteration (AoE donuts).
3. **Cell projection, aspect-corrected**: `to_cells(hex) -> Point` and
   `pick(point) -> Hex` for a documented cell-space hex metric —
   flat-top hexes at a fixed small footprint (e.g. 4-col × 2-row
   staggered, the classic terminal hex look) with the ~1:2 aspect
   folded in so boards read visually regular; parameterized enough for
   2×1 minimaps and bigger tiles, not a general float transform.
   Mouse picking rides the inverse (games get hover/click on hexes
   through the normal event path — pos is already cell-space,
   src/ui/event.rs).
4. **Docs**: one page with the coordinate-system choice argued (axial
   over offset for math, offset for storage), the aspect note, and a
   worked mini-board example.

## Scope / Non-goals
Scope: coordinates, neighbors, distance, lines, ranges/rings,
projection + picking, docs, goldens.
Non-goals: pathfinding (A*/Dijkstra are app or later-item territory —
the module provides the neighbor/cost hooks they need); fog-of-war
policy (LOS PRIMITIVE is in scope as line traversal; visibility
semantics are the game's); rendering (0720 blits, 0420 strokes);
animation (0710).

## Expected outcomes
A hex board is `range(center, radius)` → project → blit per hex; a
click is `pick(mouse)`; a shot is `line(a, b)` walked for blockers.
The two-hundred-line coordinate preamble of every board game becomes
an import, and the aspect squash trap is dead by construction.

## Validation
- Golden neighbor/distance tables for both families (including hex
  parity edges and the axial↔offset round-trip).
- Line exactness: hex `line` matches the cube-round reference on a
  sampled board; square supercover never corner-skips (the LOS
  property, pinned).
- Projection: `pick(to_cells(h)) == h` for every hex in a test board
  at every supported tile size; projected board goldens LOOK regular
  (row/col extents asserted, aspect folded).
- Determinism: byte-identical outputs across platforms (integer math
  only in traversal paths).

## Progress checklist
- [ ] Coordinate types + neighbors/distance (square, hex) + goldens
- [ ] Line traversal (Bresenham cell walk + supercover; hex cube-round)
- [ ] Range/ring iteration
- [ ] Aspect-corrected projection + picking + goldens
- [ ] Docs page with the worked mini-board
