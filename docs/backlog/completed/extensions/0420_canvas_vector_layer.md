# 0420 — Canvas/vector layer in core: sub-cell dot canvas + stroke primitives

## Metadata
- Created: 2026-07-22
- Status: Completed (extensions wave, CANVAS seat)
- Track: extensions
- Completed: 2026-07-24

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
- [x] Module home/name ruled (`crate::canvas` — see the completion report)
- [x] DotCanvas (braille + quadrant) + line/polyline (lift BrailleGrid)
- [x] Bezier flattening + arcs (deterministic, pinned)
- [x] StyledCanvas blit + color-rule contract text
- [x] Eighth-block fills
- [x] chart.rs refactor with byte-identical goldens
- [x] Docs page + doctest example (api.md section per wave instruction)

## Completion report (2026-07-24)

**Module home: `crate::canvas`, top level** — the item's working name
`render::vector` is untenable in the layer map: blit targets
`ui::StyledCanvas`, and `ui` already imports `render` (`ui/canvas.rs`
uses `render::Style`), so a render-side home would cycle. The item's
two reasons for render (pure cell math; keep widgets/ lint-free) are
both satisfied by a top-level module between `ui` and `widgets`, and
`abstracttui::canvas` is the spelling ADR-0004's anchor list ("the
canvas/vector layer") reads naturally as. Files:
`src/canvas/{mod,glyphs,curves,fill,tests}.rs` (largest ~330 lines).
Layer edges added: `canvas -> {base, render, ui}`, `gfx -> canvas`
(tables); `ui` imports no `gfx`, so the graph stays acyclic.

**Shipped** (all additive; semver-checks 196/196 vs published 0.2.12):
- `DotCanvas` (`DotMode::{Braille, Quadrant}`, `#[non_exhaustive]`
  per ADR-0003 §3 — sextant is a known growth candidate):
  `set/clear/get/clear_all`, `cell_char`, dims accessors.
- `line` — the chart Bresenham lifted verbatim, PLUS a parametric
  (Liang-Barsky) pre-clip to the grid box inflated by one dot. This
  was a robustness find during promotion, not in the item: the
  private grid only ever saw in-grid endpoints, but a public API
  fed by flattened beziers or panned diagram edges walks O(segment
  length) — up to billions of dots — and `x1 - x0` can overflow
  i32. In-box segments (all chart traffic) skip the clip and walk
  EXACTLY as before; clipped segments cost O(grid) and may differ
  from an unclipped ideal by one boundary dot (deterministic,
  documented on the method).
- `polyline`; `bezier_quad`/`bezier_cubic` (adaptive de Casteljau
  flattening, flatness tolerance in dot units, depth cap 12 → ≤4096
  segments per curve — bounded, documented); `ellipse_arc`
  (parameter-stepped ~1 dot/segment, cap 2048; sin/cos are an
  in-crate f64 polynomial with quarter-turn range reduction so dot
  sets are bit-identical across platforms — std trig goes through
  the platform libm, whose last-ulp differences could flip a rounded
  dot on one CI OS and not another). Non-finite inputs draw nothing
  (the chart sample-skip contract).
- `blit` (any `Canvas`; one color per grid, empty cells skipped,
  later blits win overlapping cells — the chart.rs:326 rule as
  documented API contract) and `blit_styled` (full `render::Style`
  patch: attributes + `Style::link` ride strokes, the 0430/0480
  edge-activation path).
- `fill_v`/`fill_h` eighth-block fills; glyph vocabularies exported
  (`braille_bit`, `QUADRANT_CHARS`, `V_EIGHTHS`, `H_EIGHTHS`).
- Dedup executed as specified (tables, not fitters):
  `gfx/mosaic_fit.rs` now re-exports `QUADRANT_CHARS` from canvas
  and derives braille bits from `canvas::braille_bit`; its private
  `BRAILLE_BITS` table is deleted; `SEXTANT_CHARS` stays put (no
  stroke mode uses it — its only home).
- Prelude: `DotCanvas`, `DotMode` (fills + tables stay behind
  `canvas::`). Docs: module docs with the dot-space model + color
  rule + a compiling ~10-line doctest; `docs/api.md` gained the
  "Canvas & vector strokes" section (the item's docs page landed as
  that section per the wave instruction); CHANGELOG under
  `[Unreleased]`.

**Refactors (deletion is the proof)**: `chart.rs` lost `braille_bit`,
`BrailleGrid` and `V_EIGHTHS` (~95 lines) — Sparkline/LineChart build
`DotCanvas::braille` and `blit`; BarChart bars are one `fill_v` call.
**Point-4 decision (peer note P3-12): `Progress` WAS refactored onto
the layer** (not left in place) — its 7-glyph `EIGHTHS` const is
deleted and the fill run is one `fill_h` call; `H_EIGHTHS[..7]` is
that exact ramp. Byte-identity: `chart_tests.rs` and the progress
tests were NOT touched (chart_tests reaches `braille_bit` through a
`#[cfg(test)]` re-import in chart.rs, so even its `use super::*`
works unchanged) and every golden passes — including the exact-string
pins `sparkline_renders_deterministic_ramp` ("⡠⠊"),
`sparkline_flat_series_centers_and_gaps_skip` ("⠤⠤"),
`bar_chart_eighth_precision_and_cycling_colors` ('█'/'▁' cells) and
progress "█████▋". The canvas suite additionally cross-pins the
"⡠⠊" ramp drawn through the public API
(`bresenham_matches_the_shipped_chart_ramp`).

**Validation** (gate numbers at completion): workspace
`cargo test --workspace` 1,982 passed / 0 failed (root ≈ 1,935 incl.
16 new canvas unit tests, the new alloc pin and the canvas doctest;
`abstracttui-graph` ≈ 47, green at this run mid-build by its owner);
`cargo clippy --workspace --all-targets -- -D warnings` zero;
`cargo fmt --all --check` clean; `cargo semver-checks
--baseline-version 0.2.12 -p abstracttui` 196 pass / 0 fail ("no
semver update required" — additive). Alloc discipline:
`tests/alloc_budget.rs::dot_canvas_stroke_and_blit_paths_allocate_nothing`
— 8 frames of clear_all + line + polyline + both beziers + full-turn
arc + far-off-grid line + blit + both fills = 0 allocs / 0 reallocs.
Canvas unit tests: dot-order pin (all 8 bits), vocabulary pins,
edge-clip behavior, quadrant mode, the shipped-ramp cross-pin,
bezier determinism + curve-tracking (512-sample analytic envelope,
both directions), bounded flattening on pathological inputs,
non-finite rejection, arc determinism + mirror symmetry (±1 dot —
Bresenham chords are not exactly reversal-symmetric, so proximity is
the contract, not set equality) + radius envelope + quadrant
containment, the color-rule pin, ClippedCanvas composition,
styled-blit attrs, fill goldens, far-segment boundedness.

**Follow-ups revealed** (none blocking):
1. Charts hardcode braille mode; a chart-side `DotMode` knob (the
   mosaic-style degradation for braille-poor fonts) is additive
   whenever a consumer names it.
2. Windows CI stays root-lib-only — the extension family rides the
   unix `--workspace` jobs; widening Windows coverage to the family
   is an integrator call.
3. The semver CI job is scoped `package: abstracttui`; each family
   crate joins the gate at its first crates.io release (ADR-0004's
   consequence line, recorded in the workflow comment).
4. 0480 (`register_link`) composes as designed: `blit_styled`
   already carries `Style::link`; only the surface-side mint is
   pending, no canvas change needed.
