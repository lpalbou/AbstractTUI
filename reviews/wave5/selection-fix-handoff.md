# Wave-5 handoff — 0285 selection click-through (field P0 fixed)

From: the 0285 fix lane. For: whoever holds the docs/backlog/overview.md
pen this wave (the file is deliberately not touched from this lane).

## What shipped

first-app/0285 is DONE and moved to `completed/first-app/` (full
completion report in the item). Two changes:

1. **Selection click-through** (`src/app/selection.rs` + driver): the
   selection layer owns the gesture only once it DRAGS. Plain Down/Up
   pass through (buttons clickable with select mode on — the consumer's
   approval-modal P0); the first drag off the anchor cell claims the
   gesture and the driver resolves the already-routed press via a
   release outside every rect (no click, no stuck pressed state, capture
   dropped); a click on a VISIBLE region stays consumed (dismissal, Esc
   parity, both halves). Click rules stated in docs/api.md (selection
   section); CHANGELOG entry under `## [Unreleased]` / Fixed.
2. **Pointer-capture heal** (`src/ui/tree.rs`): pre-existing defect
   found while tracing — Button's `pressed` write on Down disposes its
   own captured hit leaf, so EVERY button press stranded its capture and
   a release outside the button wedged the pressed visual. A stale
   capture now re-points at the press cell's current occupant
   (`validated_capture`), making the documented capture contract hold.

## The overview.md row (please add)

0285 has no existing row in either overview ledger (the field filing
only landed in the band README, which this lane already updated). Add to
the **Completed ledger**, alongside the 0281-0284 rows:

```
| 0285 | Selection click-through: layer claims only once the gesture DRAGS (plain clicks reach widgets; dismissal click consumed; claim releases the routed press) + pointer-capture heal (press re-render no longer strands the capture) — completed 2026-07-23 (wave-5 field fix) | completed/first-app/ |
```

## Consumer note (abstractcode-tui)

Their modal select-mode suspension workaround (`open_modal` →
`set_enabled(false)` / `close_modal` → `set_enabled(true)`) can be
deleted on the next engine bump; their SGR-click test assertions keep
passing unchanged. Engine-side twin proof:
`modal_buttons_are_clickable_while_select_mode_is_on`
(tests/adv_selection.rs) drives the same click through a real Modal with
select mode ON. Bonus in the same class: modal outside-press dismissal
now works in select mode too (those presses were also eaten).

## Semver

Additive only: no public item changed shape (`SelectionAct::Claim`,
`cancel_pointer_press`, `validated_capture`, `capture_pos` are all
crate-internal). Behavior deltas are the two fixes above.
