# 0685 — Probed-capabilities signal: apps see what the driver learned

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed)
- Track: media-av (band 0600–0690)
- Completed: 2026-07-22 (discharged by first-app 0295 — the recorded design-time convergence: one accessor built once, serving both consumers)
- Depends on: nothing (the viewport signal is the shipped pattern).
- Same primitive as first-app/0295 (public runtime caps accessor — its
  consumer is composer key hints; this item's is the graphics-channel
  label). One accessor serves both; converge at design time (cross-ref
  recorded both ends, convergence cycle 2).
- Promotion trigger: any app wanting an honest "images via X" label or
  capability-conditional UI — the images example is the first consumer
  waiting (its label is env-pass-only today, marked in the source).

## ADR status
- Governing ADRs: ADR-0001 (additive). ADR impact: none.

## Context
The driver's active probe upgrades capabilities AFTER first paint
(kitty graphics proof, sixel via DA1, cell pixel size, tmux passthrough
verification — src/term/probe.rs). Components have no way to observe
the upgrade: `Driver::caps()` exists but apps built on `App::run` never
touch the driver, so anything an app renders about capabilities is
env-pass folklore, exactly what the detection design forbids elsewhere.

## Current code reality
- `app::viewport` (src/app/viewport.rs) is the shipped pattern: one
  immortal thread-local signal, published by the driver, read via
  `use_viewport(cx)` — resize-reactive components for free.
- The upgrade moment is a single call site:
  `Driver::apply_caps_upgrade` (src/app/driver.rs) — it already
  re-poisons and re-dirties; publishing a snapshot there is one line.
- `examples/images.rs` documents the gap in its channel label
  ("env pass; runtime probe may upgrade" — study-2 honesty patch).

## Problem
Apps cannot truthfully answer "which graphics channel will my image
use?", "is the kitty keyboard active?", or "did tmux passthrough
verify?" — the engine knows and keeps it to itself.

## What we want
1. `app::use_caps(cx) -> Signal<Capabilities>` (clone-on-publish;
   `Capabilities` is Clone + non_exhaustive already — additive-safe),
   published at driver start (env pass) and on every probe fold that
   changes it.
2. The images example's footer goes truthful: derive the channel from
   the signal via `choose_channel`.
3. Documented read-only contract: writing capabilities stays the
   driver's job; the signal is a view.

## Scope / Non-goals
Scope: the signal, publish points, example consumption, docs.
Non-goals: capability change CALLBACKS beyond signal reactivity; live
re-probing APIs (0670 owns the resize refresh).

## Expected outcomes
Capability-honest UI text everywhere; the example demo stops hedging.

## Validation
- Driver test: probe reply folding flips the signal exactly once per
  real change; a dyn_view reading it re-renders with the new channel
  label (CaptureTerm).

## Progress checklist
- [x] Signal + publish points
- [x] Example consumption
- [x] Docs note (read-only view)

## Completion report

- Completed: 2026-07-22, fully discharged by the first-app 0295 build
  (the convergence both items recorded at filing time — one accessor,
  built once).
- Want 1 shipped as specified: `app::use_caps(cx) ->
  Signal<Capabilities>` (clone-on-publish, `#[non_exhaustive]` struct),
  published at driver start (env pass, `Driver::new`) and on every
  probe fold that changes it (equality-deduped inside
  `publish_caps` — the probe's reply handler compares before/after, so
  partial probes surface upgrades without waiting for the DA1
  sentinel). `app::current_caps()` rides along for plumbing.
- Want 2 shipped: `examples/images.rs`'s footer derives its channel
  from the signal via `choose_channel` — the "env pass; runtime probe
  may upgrade" hedge is deleted; the label flips live when the probe
  proves kitty graphics / sixel.
- Want 3 shipped: the read-only contract is documented on the module
  and in `docs/api.md` (writing capabilities stays the driver's job;
  the signal is a view).
- Validation delivered:
  `app::caps::tests::publish_flips_subscribers_once_per_real_change`
  (exactly one flip per real change; identical re-publish never wakes
  subscribers) and `tests/wave_probe_caps.rs::
  use_caps_dyn_view_rerenders_channel_label_on_probe_upgrade`
  (CaptureTerm: a dyn_view reading the signal re-renders with the new
  channel label when probe replies fold in).
- Bonus from the same wave (0293): `kitty_keyboard` in the signal is
  ACTIONABLE — the driver now pushes the enter-flags whenever the
  probe proves the protocol, so capability-conditional key hints are
  truthful, not aspirational.
