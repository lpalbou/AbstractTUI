# Proposed: Table consumes `s` (sort cycling) even when no sort handler is registered

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; found by the
  build's cycle-1 adversarial review)
- Severity: P3 — cost a screen-level key binding; discovered by reading
  engine source, not by symptom (which is the problem)
- Class: footgun

## Context
The console's screens bind single-letter actions over focused tables
(`a` add, `e` edit, `d` delete, `m` models, `t` test). `s` would be the
natural "sandbox test" or "sort" binding — but a focused `Table`
consumes `s` and calls `stop_propagation` even when the app registered
NO `on_sort_requested`, so a screen-level `s` shortcut is silently dead
exactly when a table has focus (which, with autofocused tables, is
always). Nothing renders, nothing logs — the keypress just vanishes.

## Current code reality
- `src/widgets/table.rs:193-208` (0.2.8): the `s` arm cycles the sort
  indicator and stops propagation unconditionally; the sort-request
  callback is optional but the key claim is not conditioned on it.

## Repro
Focused table without `on_sort_requested`; ancestor element carries
`.shortcut(KeyChord::plain(Key::Char('s')), …)` — the handler never
fires; the table's sort indicator cycles with no app-side effect.

## Workaround in the field (delete when fixed)
The console avoids `s` in its action vocabulary (uses `t`/`g`). Fix
wish: claim `s` only when `on_sort_requested` is bound (the vocabulary
rule the engine already applies elsewhere: "when `on_activate` is
unbound, Enter and Space pass through to your shortcuts unchanged" —
the same contract should govern `s`).
