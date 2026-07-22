# 0420 — Canvas/vector layer in core: sub-cell dot canvas + stroke primitives

## Metadata
- Created: 2026-07-22
- Status: Proposed (v1-able — the one engine-code item in this track;
  the substrate every diagram extension stands on)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: ADR-0001 (additive: new module, new public API);
  ADR-0003 (any config struct follows the classification rule). 0400
  informs the module's naming/home but does not gate the build — this
  layer is CORE by 0400's own decision table (a minimal app drawing a
  sparkline already contains most of it).
  ADR impact: none expected.

## Context
Every diagram-class surface — charts (shipped), node graphs
(0430/0440), mermaid (0450), plots, minimaps, signal traces — needs the
same substrate: draw sub-cell strokes (lines, curves) into terminal
cells. The engine owns this math today but keeps it PRIVATE and
single-purpose: `chart.rs` hand-rolls a braille dot grid with a
Bresenham line and nothing else, and no other code can reach it. The
class-level justification (roadmap principle 1): dashboards & monitors
(custom traces), viewers (diagrams), editors (minimaps, structure
views), games & toys (vector sprites) — at least four of the five app
classes want strokes the engine already half-owns.

## Current code reality
- `src/widgets/chart.rs:49-115` — `BrailleGrid` (private struct):
  2x4-dots-per-cell grid, `set(x, y)`, Bresenham `line(a, b)`
  (chart.rs:82-105), braille codepoint assembly via `braille_bit`
  (chart.rs:34-45) and `cell_char` (chart.rs:107-114). This is the v1
  canvas already — minus curves, minus public access.
- `src/widgets/chart.rs:326-328` — the per-cell color constraint,
  already understood and documented: "One dot grid per series so colors
  never merge in a cell: later series win overlapping cells (documented
  z-order)". A terminal cell carries ONE fg — any canvas API must carry
  this rule, not hide it.
- `src/gfx/mosaic_fit.rs:41-67` — quadrant/sextant/braille tables exist
  a second time for a DIFFERENT job: least-squares image fitting
  (luminance/2-color fit per cell, mosaic_fit.rs:5-14). Vector strokes
  and image fitting share glyph tables but not algorithms; the item
  dedups the tables, not the fitters.
- `src/ui/canvas.rs:47-77` — `Canvas`/`StyledCanvas` are the cell
  output traits every widget draws through (put/print/fill); the dot
  canvas BLITS into them, so it composes with clipping
  (`ClippedCanvas`, canvas.rs:261-322) and damage for free.
- `src/widgets/chart.rs:400` — `V_EIGHTHS` block ramp (bar charts):
  eighth-block fills are a third sub-cell vocabulary worth exposing
  beside dots.
- `src/theme/tokens.rs:341` — `TokenSet::chart(i)` slot ramp: strokes
  take resolved `Rgba` params (the widget token rule,
  src/widgets/mod.rs:8-15, forbids color invention in widgets/ — a
  canvas layer taking caller-resolved colors passes it, exactly as
  `BrailleGrid` does today).
- Tests to preserve: `src/widgets/chart_tests.rs` pins deterministic
  cell output (`sparkline_renders_deterministic_ramp`,
  `braille_bits_match_unicode_dot_order`, chart_tests.rs:10-49) — the
  chart refactor onto the shared layer must keep those goldens
  byte-identical.

## Problem
The stroke substrate is locked inside one widget: 0430/0440/0450 (and
any app wanting a custom trace) would each re-derive dot-grid math,
Bresenham, and the cell-color rule — the exact "re-deriving math the
engine owns" smell 0120 files against composers. And curves do not
exist at all: no bezier, no arc, so edges in any graph would be
segment chains hand-flattened by every caller.

## What we want
A small public module (working name `render::vector`; final home/name
at implementation — `render` because it is pure cell math like
`render::rich`, and widgets/ carries the lint list that would need
extending, src/widgets/mod.rs:123-148):
1. **`DotCanvas`** — the promoted `BrailleGrid`: modes Braille (2x4)
   and Quadrant (2x2, universal glyph coverage — the mosaic auto-pick
   rationale at src/gfx/mosaic.rs:47-51 applies to strokes too:
   braille glyphs can be missing/ugly in some fonts; quadrant is the
   degradation), `set/clear`, `line` (Bresenham, lifted verbatim),
   `polyline`.
2. **Curves**: quadratic + cubic bezier via adaptive flattening to
   segments (flatness tolerance in dot units; deterministic — same
   inputs, same dots, test-pinned like the charts), circle/ellipse
   arcs (midpoint or param-stepped, deterministic).
3. **Blit**: emit into any `StyledCanvas` at a cell origin with ONE
   stroke color per grid (the chart's documented answer to the
   cell-color constraint); multi-color pictures = multiple grids,
   z-ordered by blit order. The constraint is documented API contract,
   never worked around silently. A styled blit variant takes a
   `render::Style` patch instead of a bare color, so a stroke can
   carry attributes AND a link id (`Style::link`,
   src/render/style.rs:148) — which is how 0480-minted edge/node
   links ride canvas strokes (0430 §6) with zero extra machinery
   here.
4. **Eighth-block fills** (`V_EIGHTHS` promoted): horizontal/vertical
   partial fills for gauges/bars. `Progress` also owns an eighth-block
   ramp (`EIGHTHS`, src/widgets/progress.rs:24-25) — it is a SECOND
   refactor-onto-the-layer candidate after charts, or explicitly left
   in place; the completion report says which, so the dedup claim
   stays complete (peer note P3-12).
5. **Refactor `chart.rs` onto it** — deletion is the proof (roadmap
   "what best-in-class means": app/widget-side machinery approaches
   zero). **Migration gate: the existing chart goldens
   (src/widgets/chart_tests.rs — deterministic fixed-series pins) pass
   byte-identical on the refactored implementation; any intentional
   pixel change is a separate, justified commit, never smuggled into
   the refactor.**
6. Docs: one page (docs/) with the dot-space model (cells × 2x4),
   the color rule, and a worked custom-trace example.

Named consumers, precisely (who this substrate exists for):
- **In-repo now**: `chart.rs` (Sparkline/LineChart lines via the
  lifted BrailleGrid; the refactor above) and optionally `Progress`
  (eighth-block ramp, point 4).
- **Extension crates next**: 0430 (node-card edges: bezier strokes +
  rubber-band), 0440 (laid-out graph edges + arrowheads), 0450
  (flowchart/sequence connectors) — all via the sibling-crate public
  path, which is why this layer must be public API, not widget-private
  plumbing.
- **App-side**: custom traces/minimaps (the docs example).

Deliberately CORE vs deliberately NOT core (0400 decision table):
- Core: dots, strokes, beziers, arcs, fills, blit — small (~400-600
  lines by analogy with chart.rs's 100-line grid + flattening math),
  general, zero deps.
- Extension (0430/0440): edge ROUTING (obstacle avoidance, orthogonal
  channels), arrowhead vocabularies, graph layout, hit-testing of
  strokes — domain policy, not substrate.

## Scope / Non-goals
Scope: the module, curves, blit, chart refactor, docs + example, tests.
Non-goals: filled polygons/rasterization beyond eighth-blocks (the gfx
bitmap+mosaic path already rasterizes real rasters — a vector fill
engine duplicates it for no class need yet); sextant mode (font-risk,
opt-in later if a consumer proves it, mosaic.rs:48-51); anti-aliasing
(cells have no alpha ramp per dot); scene graph/retained vector docs
(the reactive Element tree IS the scene graph).

## Expected outcomes
0430/0440/0450 consume strokes instead of re-deriving them; charts
lose private plumbing; an app can draw a custom braille trace in ~10
lines against a documented, deterministic API.

## Validation
- Unit: dot-bit order (extend chart_tests.rs:10's pin), Bresenham
  goldens, bezier flattening determinism (fixed tolerance → fixed dot
  sets), arc symmetry, clip behavior at grid edges (set() clips like
  chart.rs:73-79).
- Refactor proof: existing chart goldens byte-identical after the
  chart moves onto the shared layer.
- Blit composes with `ClippedCanvas` (no writes outside clip) and the
  damage contract (blit into a damaged region only repaints that
  region).
- Doc example compiles (doctest).

## Progress checklist
- [ ] Module home/name ruled (render::vector proposed)
- [ ] DotCanvas (braille + quadrant) + line/polyline (lift BrailleGrid)
- [ ] Bezier flattening + arcs (deterministic, pinned)
- [ ] StyledCanvas blit + color-rule contract text
- [ ] Eighth-block fills
- [ ] chart.rs refactor with byte-identical goldens
- [ ] Docs page + doctest example
