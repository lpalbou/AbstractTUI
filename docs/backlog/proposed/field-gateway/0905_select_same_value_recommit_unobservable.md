# Proposed: Select/Combobox same-value re-commit is unobservable — "re-pick to retry" semantics cannot be built

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; found by the
  build's cycle-2 adversarial review as a regression in an app fix)
- Severity: P2 — turned an app's retry affordance into a dead gesture;
  app-side button workaround holds
- Class: API gap

## Context
The console's route editor loads a model list when a provider is
picked. When that discovery FAILS, the app rendered the error with the
teaching "re-pick the provider to retry" — a natural recovery gesture:
open the select, commit the same provider again, expect the load
effect to re-run. It never does. The commit path early-returns when
the committed index equals the current value, so `on_change` does not
fire and the bound signal does not change — the same-value re-commit
is invisible to the app. With a single-provider list there is NO
in-select gesture that retries at all.

## Current code reality
- `src/app/select.rs:276` (0.2.9): `write_value` early-returns on
  equal index — correct for the documented "on_change fires on COMMIT
  only, and only when the value actually changed" contract.
- There is no companion `on_commit`/`on_close(Committed)` event, so an
  app cannot distinguish "popup opened and re-committed the same
  option" from nothing happening.

## Repro
```rust
let ix = cx.signal(1usize);
Select::new(opts).value(ix).on_change(|_| retry()).view(cx)
// open the popup, press Enter on the already-selected option:
// retry() never runs; ix never changes; no event of any kind.
```

## Workaround in the field (delete when fixed)
An explicit "Retry model discovery" Button in the discovery-failed row
(routes editor + sandbox modal) that re-inserts Loading and resends.
Fix wish, smallest first: (a) an `on_commit(usize)` that fires on every
popup commit (change or not) alongside the value-change-only
`on_change`; or (b) document the early-return as a contract and note
that retry semantics need an out-of-band affordance. (a) matches the
Popup's own `DismissReason::Committed` vocabulary that already exists
one layer down.
