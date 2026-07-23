# 0620 — `widgets::Meter` + `widgets::AudioScope`: live level rendering

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed)
- Track: media-av (band 0600–0690)
- Completed: 2026-07-23 (wave 3, INPUTAV) — `widgets::Meter`
  (`new(Signal<f32>)` / `bands(Signal<Vec<f32>>)`; `.db_floor(-60.0)`,
  `.decay(db_per_s)` default 20 dB/s, `.peak_hold(~1.5s)`, `.zones()`,
  `.vertical()`, `.bar(w, gap)`) + `widgets::AudioScope`
  (`new(Signal<Vec<f32>>)`, `.range`, `.slot` — braille strip through
  the chart substrate, axes off). Ballistics: instant attack; decay is
  a constant-rate fall in display space (`decay_db_per_s / span`),
  frame-clocked via an internal frame task, frame-rate-independence
  virtual-clock-pinned; peak holds then falls and lands EXACTLY on the
  display level (max-clamp arithmetic, no epsilon). **THE IDLE LAW
  shipped and pinned three ways**: the frame task returns false at the
  fixpoint (unit: `silent_meter_reaches_fixpoint_and_stops_requesting_
  frames`), driver-level turns go idle with zero frames/bytes
  (tests/wave_inputav.rs `meter_reaches_fixpoint_and_turns_go_idle`),
  and 16 idle turns with a parked meter allocate literal zero
  (tests/alloc_budget.rs `idle_turns_with_parked_meter_scope_and_key_
  state_allocate_nothing`). Zone inks are `ok`/`warn`/`error` TOKENS
  (the widgets color lint covers both new files). One deliberate
  simplification vs the draft: the scope does NOT own a second ring —
  its input signal IS the window (pair with `bounded_source` +
  `DropOldest`; the source's retained window is the ring, drop
  accounting included), which keeps one semantics and zero duplicated
  buffering. Band-frame reality: a wider frame grows the channel set; a
  shorter frame leaves missing channels decaying toward their old
  targets; non-finite samples are gaps.
- Depends on: nothing (chart substrate + live-data sources are shipped).
- Promotion trigger: any voice app (0650's mock counts) or a monitoring
  dashboard needing ballistic level bars.

## ADR status
- Governing ADRs: ADR-0001 (additive widgets). ADR impact: none.

## Context
Both production voice clients render live audio levels (grounding study,
2026-07-22): the assistant's recognizer emits an EMA-smoothed mic level
0..1 per ~30 ms chunk (`abstractvoice/recognition.py:278-302`), and its
TTS playback derives per-frame log-spaced FFT band levels 80 Hz–6 kHz
(`gateway_voice_manager.py:_emit_audio_meter_from_chunk` — a `list[f32]`
per frame). The DATA SHAPES the engine must render are therefore: one
f32 level stream, N-band f32 frames, and (for scopes) a rolling sample
window. All arrive cross-thread — exactly what `latest_source` and
`bounded_source` (src/reactive/source.rs:176-224, ingest.rs:370-) carry.

## Current code reality
- `widgets::Sparkline`/`LineChart`/`BarChart` (src/widgets/chart.rs)
  already draw braille dot-grids and eighth-block bars from `Vec<f32>`,
  theme-ramped by slot, gap-honest on NaN. A naive per-frame bar meter
  is `BarChart::new(bands)` in a `dyn_view` TODAY.
- What no widget owns: **meter ballistics** — instant attack /
  exponential decay, peak-hold with drop-off, and dB mapping. Feeding
  raw RMS frames to a bar chart flickers illegibly; every audio UI ever
  shipped solves this with ballistics, and every app would re-derive
  the same few lines of state wrong (frame-rate-dependent decay).
- The engine's animation clock (`reactive::animate`, frame tasks,
  driver.rs:267-271) is the right decay driver — decay must advance on
  FRAME time, not on data arrival (a stalled stream should show a
  falling bar, not a frozen one).

## Problem
Level rendering is the one voice-UI surface where "compose it from
charts" produces wrong-feeling results unless the app re-implements
ballistics; that state machine belongs in a widget so its decay is
frame-clocked and its saturation colors are theme tokens, not app hex.

## What we want
1. **`Meter`**: one channel or N bands; input `Signal<f32>` or
   `Signal<Vec<f32>>` (0..1 linear; optional `.db_floor(-60.0)` log
   mapping); instant attack, configurable decay (default ~20 dB/s),
   peak-hold marker (~1.5 s hold then fall); horizontal or vertical;
   eighth-block sub-cell resolution; theme zones (ok/warn/danger tokens,
   never hardcoded green/red).
2. **`AudioScope`**: rolling waveform over a bounded window
   (`Signal<Vec<f32>>` frames appended; the widget owns the ring), drawn
   on the braille grid like `LineChart`; zero idle cost when the signal
   stops (last frame stays; no animation without data — decay is the
   Meter's business, not the scope's).
3. Both render from DATA ONLY — no audio, no threads, no I/O in the
   widget; producers are app-side `latest_source`/`bounded_source`.

## Scope / Non-goals
Scope: the two widgets, ballistics, dB mapping, theme zones, docs.
Non-goals: FFT (the band split is app-side — the assistant already does
it in numpy; a Rust app uses its own DSP), audio capture, spectrograms.

## Expected outcomes
A voice app binds mic levels to `Meter` in three lines and gets
broadcast-feeling ballistics; 0650's mock demo is the first validator.

## Validation
- Unit: ballistics math on a virtual clock (attack instant; decay
  frame-rate-independent; peak holds then falls); dB mapping floors.
- CaptureTerm: bar cells advance/decay across scripted frames; NaN and
  empty frames render gaps, never panic; theme-token assertion (no
  color literals — the widgets lint already enforces this directory-wide).

## Progress checklist
- [x] Meter state machine (virtual-clock tested: attack instant, decay
      frame-rate-independent, peak holds then falls, re-anchor never
      integrates a parked gap)
- [x] Band mode + dB mapping (floors pinned: -60 dB → 0, 0 dB → 1,
      silence → 0, NaN passes through as a gap)
- [x] AudioScope braille render (ring = the `bounded_source` window by
      design — see the completion note)
- [x] Theme zone tokens + docs (ok/warn/error inks, lint-covered;
      docs/api.md + CHANGELOG)
