# Proposed: no engine pattern for routing one-shot write completions back to forms (0510 evidence)

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; sharpened by
  the build's cycle-1 adversarial review)
- Severity: P3 — design evidence for app-kits 0510; the app-side
  correlation shape works but had to be invented and defended twice
- Class: capability gap (evidence)

## Context
Every console form modal submits a write to a background worker and
must learn the outcome: close on success, stay open with the verbatim
error on failure (losing a filled form to a 400 is real damage). The
engine's live-data lanes (`channel_source`/`latest_source`/
`bounded_source`) bind STREAMS to signals; what a form needs is a
one-shot, correlated, exactly-once completion. The app hand-rolled:
a process-global `form_id` counter, a `Signal<Option<(u64,
Result<String,String>)>>` single slot the worker's sink posts into,
and a per-modal effect that claims only its own id. The cycle-1
adversarial review immediately found the class hazards of that shape:
two completions landing in one drain lose the first (single slot);
a completion for a form closed in the meantime leaves a stale value in
the slot until someone reads it.

## Current code reality
- Engine 0.2.8 has no correlate-by-id completion primitive; the
  closest, `WakeHandle::post`, delivers closures (which is what the
  app's sinks use underneath) but the routing/claiming discipline is
  entirely app-side convention.
- `ChoicePrompt::on_resolve` proves the engine already owns the
  "resolve EXACTLY ONCE through a callback" vocabulary — for its own
  modal, not for app-defined async work.

## Repro
Not a defect — a shape every form-owning app will re-invent: submit →
async result → close-or-stay-with-error, with modal lifetime shorter
than the async work.

## Workaround in the field (delete when 0510 ships)
`worker::next_form_id()` + `UiState.write_done` single slot + per-form
claiming effects (set to None on claim), plus `in_flight` signals for
double-submit guarding. The 0510 form-kit ask: a submit-state handle —
`FormSubmit::begin() -> token`, worker resolves the token, the form
reads `state: Idle | InFlight | Failed(msg)` and a `on_success` that
may close the modal — so the correlation, the double-submit guard, and
the exactly-once claim are engine-owned instead of five signals of
app convention.
