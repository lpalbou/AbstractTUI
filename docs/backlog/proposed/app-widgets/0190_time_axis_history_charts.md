# 0190 — Time-axis charts + history windows (`TimeSeries` model)

- Status: proposed
- Track: app-widgets (band 0100–0190; 0190 was the band's named free id)
- Origin: FIELD study 2 app-class gap check
  (`reviews/study2/field-app-classes.md` class 5, the one NEEDS-ITEM of
  the realtime-monitoring class — "integrator to file", handed to the
  convergence pass). Filed cycle 2, 2026-07-22.
- Depends on: none (a small model type + additive chart options).
  Shares column/label discipline with 0142's table work only if
  natural — do not gate on it (the report's own instruction).
- Promotion trigger: the first production-monitor consumer, or the
  dashboard example graduating from its hand-rolled ring (evidence
  below — the flagship example already pays the cost).

## Problem

Every shipped chart labels the VALUE domain only: `finite_range`
computes min/max over samples (src/widgets/chart.rs:117-121) and the
axis/range labels render from it; nothing anywhere renders a TIME axis
(grep verified: no time formatting or axis-tick code in
src/widgets/chart.rs). And nothing owns the history buffer a monitor
needs: the engine's own flagship example hand-rolls the ring — `const
TICK: Duration = 250ms` + `const WINDOW: usize = 72` and hand-walked
sample vectors (examples/dashboard/main.rs:40-42) — which is exactly
the code every production monitor re-writes, with the same off-by-one
and drop-by-age bugs.

What already exists and must be composed with, not duplicated: the
cell-level gap contract — "non-finite samples are SKIPPED (gap, not
zero)" (src/widgets/chart.rs:20, honored by the line walk at
chart.rs:201-202 and the braille walk at chart.rs:340) — models the
RENDER half of "samples paused"; the missing half is the model knowing
WHEN samples are from so a pause becomes a gap instead of a squeezed
x-axis.

## What we want to do

One cohesive item (the report's scoping): a `TimeSeries` model + chart
time-axis support.

1. **`TimeSeries` buffer**: `push(t, v)` with drop-by-age OR by-count
   retention (both bounded; monitor-friendly), backed by the loop's
   clock discipline (injected `Instant`s in tests — the anim::Clock
   rule). Reads produce the sample slice + the time window for the
   axis.
2. **Time-axis rendering**: opt-in x-axis labels on LineChart/Sparkline
   ("now", −30s, −1m tick style; label density adapts to width the way
   value labels already clamp). Rendering stays deterministic (same
   samples + same window = same cells — the chart determinism
   discipline, chart.rs:22-23).
3. **Gap honesty end-to-end**: a sampling pause materializes as
   non-finite padding (or explicit gap spans) so the existing cell
   contract draws a hole, never a time-compressed lie.
4. The dashboard example migrates (deletes its WINDOW ring) — the same
   in-tree-consumer-deletes-its-workaround acceptance the band always
   uses.

## Non-goals

A radial/arc gauge (report: minor, wait for extensions 0420's public
dot canvas — filing it now would invert the dependency); alert-state
recipes (threshold → banner + notify is a composition note for 0560's
validator journey / docs, per the report); general time-zone/calendar
formatting (relative offsets only — "−30s", not wall-clock dates);
streaming ingestion policy (live-data 0010/0020 own transport-to-signal;
this consumes a signal).

## Validation

- Ring: by-count and by-age retention goldens under a virtual clock
  (push N, advance, assert evictions); no allocation growth over a
  soak-shaped push loop.
- Axis: golden cells at two widths (label density adapts); "now"
  anchors the right edge; deterministic across runs.
- Gap: a scripted sampling pause renders a hole (cell parity with the
  existing non-finite contract), never x-compression.
- Example: dashboard renders identically-or-better with the ring
  deleted (before/after capture).
