# Completed: public runtime capabilities accessor — apps cannot render honest key hints

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed — API gap, first-app finding, composer wave)
- Completed: 2026-07-22 (first-app fix wave, cycle 3; converged with media-av 0685 — one accessor, both consumers)

## ADR status
- Governing ADRs: None. ADR impact: none — additive read-only accessor.

## Context
`abstractcode-tui` wants its composer placeholder and `/help` to tell the
truth per terminal: "Shift+Enter newline" where the kitty protocol is
live, "Alt+Enter (Option as Meta)" elsewhere. That needs the POST-PROBE
`Capabilities` at runtime.

## Current code reality
- `Driver::caps()` exists but is unreachable from mounted app code (the
  driver lives in `App::run`'s loop; apps never hold it).
- `app::` exposes `current_theme`/`current_viewport`/
  `use_startup_notices` — no caps.
- The caps SUMMARY string rides a startup notice; parsing prose for a
  boolean is the fragile non-API.
- Consequence: apps ship static, capability-neutral hint text — either
  over-claiming (WezTerm stock config) or under-claiming (kitty users
  reading about Alt+Enter).

## Proposed direction (engine's call)
- `app::current_caps() -> Capabilities` (post-probe snapshot) or
  `use_caps(cx) -> Signal<Capabilities>` updated when probe replies land
  — same shape as the viewport accessor. Read-only; `#[non_exhaustive]`
  already protects the struct's evolution.
- SAME PRIMITIVE as media-av/0685 (probed-capabilities signal — its
  consumer is the images example's channel label; this item's is key
  hints). One accessor serves both: converge at design time, build
  once (cross-ref recorded both ends, convergence cycle 2).
- Nice-to-have while in the area: fold Ctrl+J (`0x0a`) into
  `SubmitPolicy::EnterSubmits` as a built-in newline chord so every app
  inherits the universal fallback (today each app must add its own
  shortcut + `TextAreaState::replace_range`).

## App-side state
Until this lands, `abstractcode-tui` ships static honest text listing the
terminal families; with it, the placeholder/help swap to per-terminal
truth in one dyn read.

## Completion report

- Completed: 2026-07-22 (first-app fix wave, cycle 3). Built ONCE with
  media-av 0685 as the design-time convergence recorded in both items.
- Shipped API (additive, prelude-exported):
  - `app::use_caps(cx) -> Signal<Capabilities>` — the reactive
    signal shape (the item's second option; the viewport-signal
    pattern): published by `Driver::new` with the env pass, updated by
    the probe fold whenever a reply ACTUALLY changed a field
    (equality-deduped — partial probes that never see DA1 still surface
    what they proved). `app::current_caps()` is the untracked snapshot.
  - Read-only contract documented on the module: writing capabilities
    stays the driver's job; the signal is a view.
- Hint honesty end to end: with 0293 shipped in the same wave, reading
  `kitty_keyboard == true` under the default run posture means the
  enhancement is LIVE (flags pushed at enter or at the probe upgrade),
  so "Shift+Enter newline" hints are truthful per terminal — and the
  WezTerm env over-claim is gone (evidence-gated, 0293 direction 2).
  `examples/transcript.rs` now renders its newline hint from the signal
  ("Shift+Enter newline" vs "Alt+Enter / Ctrl+J newline");
  `examples/images.rs` derives its channel label from it (0685's
  consumer) and dropped the "env pass; runtime probe may upgrade"
  hedging.
- The nice-to-have shipped too: `TextArea` now folds Ctrl+J into BOTH
  submit policies as a built-in newline chord (`0x0a` IS Ctrl+J on the
  legacy wire — `input::legacy::control_key`; kitty reports the same
  identity), so every app inherits the universal fallback without
  hand-rolled shortcuts.
- Tests: `app::caps::tests::publish_flips_subscribers_once_per_real_change`
  (the 0685 validation line at the signal layer: one flip per real
  change, identical re-publish deduped);
  `tests/wave_probe_caps.rs::use_caps_dyn_view_rerenders_channel_label_on_probe_upgrade`
  (CaptureTerm: frame 1 renders env-pass truth, probe replies flip the
  channel label AND the key hint in the re-rendered frame); Ctrl+J
  through the real wire in
  `tests/wave_composer.rs::submit_vs_newline_chords_on_both_wires_and_history_recall`.
- Docs: `docs/api.md` app-hooks bullet + term-section note; CHANGELOG
  under Unreleased.
