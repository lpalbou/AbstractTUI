# 0190 — Time-axis charts + history windows (`TimeSeries` model)

- Status: Completed (content wave, CONTENT2 seat — 2026-07-23)
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

## Completion report

- Final path: docs/backlog/completed/app-widgets/0190_time_axis_history_charts.md
- Date: 2026-07-23
- Model (src/widgets/chart_time.rs, `#[path]` sibling of chart):
  `TimeSeries` quantizes `push(t: Duration, v)` into CADENCE SLOTS
  (slot = t / cadence) over a bounded `VecDeque` ring — `new(cadence,
  window)` derives slot count from a window duration (drop-by-age),
  `with_slots` takes it directly (drop-by-count). Missed slots pad
  `NAN` at the next push, so a sampling pause flows into the charts'
  EXISTING cell contract ("non-finite = gap") and draws a hole with
  the pause still occupying x-width — sample spacing is time-linear by
  construction, which is what makes the axis honest. Bounded work:
  same-slot pushes coalesce (latest wins), out-of-order writes land in
  retained slots or drop, and a pause longer than the whole window
  restarts the ring instead of looping padding. Time is a value on the
  app's clock (the `anim::Clock` rule) — no wall-clock reads anywhere,
  so tests and the dashboard drive virtual timelines. `TimeSeriesState`
  is the reactive handle (`FeedState` shape): tracked `samples()` /
  `span()` / `last()` re-render reader dyns per push; disposal-safe.
- Axis (`time_ticks` + `draw_time_labels`, pure over (span, width)):
  `LineChart::time_axis(span)` embeds labels IN the existing axis rule
  row (zero extra height) — "now" right-aligned at the plot edge, nice
  steps from a 1s..24h ladder admitted only when labels keep breathing
  room (widest-label + 4 cells), centered on their tick columns,
  right-to-left collision skipping; `Sparkline::time_axis(span)` adds
  an optional label row (default layout grows to 2; one-row rects
  degrade to the bare trend). `span` comes from the ring, so warmup
  labels the REAL covered time, never the target window. The
  value-axis hi/lo labels were already present and compose unchanged.
- Dashboard migration (the acceptance): the traffic panel's hand-walked
  `(0..WINDOW)` rx/tx sample vectors are DELETED — two seeded
  `TimeSeriesState`s, one push per data tick (timestamps derived from
  the tick count, capture-friendly), tracked reads in the panel dyn,
  `.time_axis(rx.span())` on the chart. Before/after captures:
  identically-or-better — the old still showed a mostly-flat lead-in
  (back-history before tick 0 collapses to `f(0)` in the pure-function
  walk) while the seeded ring shows the full deterministic waveform +
  the `└──-15s──-10s──-5s──now` rule. dashboard-dark/dawn regenerated;
  all OTHER captures verified unchanged (the `apps` family regenerated
  byte-identical — proof the feed/chart changes altered no existing
  rendering; wobble-only pty shots restored from the pre-run backup).
  The load panel's sparkline walk stays deliberately: it samples a
  pure demo function, not a maintained ring — no workaround mass there.
- Tests: ring goldens (`by_count_retention_evicts_oldest`,
  `by_age_retention_derives_slot_count_from_window`,
  `missed_slots_pad_with_nan_and_never_compress_time`,
  `pause_longer_than_the_window_restarts_bounded`,
  `same_slot_coalesces_and_stale_slots_drop`,
  `soak_push_loop_never_grows_the_ring_allocation` — 10k-push soak,
  ring capacity byte-stable), axis math
  (`tick_labels_adapt_density_and_anchor_now_right`,
  `tick_math_is_deterministic_and_minute_labels_format`), reactive
  handle (`reactive_handle_tracks_pushes`), and chart-level goldens in
  chart_tests.rs (`line_chart_time_axis_anchors_now_and_adapts_density`
  — two widths, determinism, label/rule inks;
  `sparkline_time_axis_labels_below_and_one_row_degrades`;
  `time_series_pause_renders_a_hole_never_compression` — gap honesty
  end-to-end through real chart cells).
- Non-goals honored: no gauges, no alert recipes, no wall-clock/calendar
  formatting (relative offsets only), no transport policy.

## Post-completion fix (wave-3 cycle-3 close, CLOSER — 2026-07-23)

Cycle-2 review C-4 + C-6 (`reviews/wave3/review-cycle2.md`): the
`missed >= capacity` RESTART contradicted this item's own gap claim by
one slot — `missed == capacity` is exactly the window (not "longer"),
and the restart collapsed the display to a lone zero-span dot, which
IS an x-axis compression (the thing the module doc says it avoids).
Fixed by the review's uniform option: padding is now capped at
`capacity - 1` NANs per push (`missed.min(capacity as u64 - 1)`, the
cap applied in u64 space BEFORE the usize cast — closing C-6's 32-bit
wrap for free) and the restart path is DELETED. A pause of any length
≥ the window lands as a full window of hole ending in the fresh
sample: same bounded work, no boundary discontinuity, and the module's
gap contract is now uniformly true. Out-of-order writes into the hole
region now land (the restart used to drop them). Tests updated
deliberately: `pause_longer_than_the_window_restarts_bounded` →
`pause_longer_than_the_window_pads_a_full_window_of_hole_bounded`;
review probe `timeseries_restart_boundary_sits_at_exactly_capacity_
missed_slots` → `timeseries_pause_padding_is_uniform_at_and_past_the_
window` (cap-1, cap, and 1h pauses all pin the same shape).
