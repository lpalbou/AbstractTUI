# Proposed: dirty-form tracking is hand-rolled per form (0510 evidence)

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build, cycle-2)
- Severity: P3 — evidence for app-kits 0510; the per-form snapshot
  pattern works but is ~25 lines of convention per form
- Class: capability gap (evidence)

## Context
Esc closing a modal form silently destroyed typed work (API keys, base
URLs, options JSON) — one keypress, no warning, and the wizard footer
actively teaches Esc as "back". The fix every form now carries: a
snapshot of initial field values taken at build, a closure comparing
every signal against it, an `esc_armed` signal, and a guard slot the
modal's Esc handler consults ("unsaved changes — press Esc again to
discard"). Three forms × the same ~25 lines, where the only
form-specific content is the field list — exactly the shape a form kit
owns.

## Current code reality
- Engine 0.2.9 has no field/form state container: value signals are
  individually bound to widgets, so "is anything dirty" has no owner.
  `TextInput`/`Select`/`Checkbox` know their values but not their
  baselines.

## Repro
Not a defect — a convention every form-owning app re-invents (this
console: `src/ui/mod.rs` `GuardSlot`/`open_form_guarded` + a guard
block per form in providers/routes/users).

## Workaround in the field (delete when 0510 ships)
The GuardSlot pattern above. The 0510 form-kit ask: a field-group
handle owning (value signal, initial value) pairs — `fields.dirty()`
for free, plus reset-to-initial — so the Esc-guard becomes one line
and the discard warning becomes kit chrome.
