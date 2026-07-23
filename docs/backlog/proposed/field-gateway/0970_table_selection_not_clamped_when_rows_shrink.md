# Proposed: Table never clamps a bound selection when rows shrink — stale selection goes silently dead

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; confirmed by
  the build's cycle-1 adversarial review)
- Severity: P2 — every CRUD table hits it on delete-last-row; app-side
  clamp effects hold
- Class: API gap (0530 table-upgrades evidence)

## Context
Every console table binds `Table::selection(Signal<usize>)` and derives
row actions from `rows.get(sel.get())`. Delete the last row: the data
shrinks, the signal still points past the end, the table renders NO
highlighted row, and every row action (`e`/`d`/`m`/`t`) is a silent
no-op until the user presses an arrow key. The dangerous variant is the
app "fixing" it by clamping at read time (`idx.min(len-1)`) — then the
action targets a row the user never saw highlighted; for a delete
action that is a destructive misfire.

## Current code reality
- `src/widgets/table.rs` (0.2.8): selection is clamped during key
  navigation only; `rows()` at build never writes the bound signal back
  into range (a build closure writing signals would be its own hazard —
  the fix belongs inside the widget's update path, not the app's).

## Repro
```rust
let sel = cx.signal(2usize);
// rows shrinks from 3 to 1 across a dyn_view rebuild:
Table::new(cols).rows(one_row).selection(sel).view(cx)
// sel == 2: no highlight rendered, sel is out of range for the app.
```

## Workaround in the field (delete when fixed)
`util::clamp_selection(cx, sel, len_of)` — a per-screen effect reading
the domain signal and clamping the selection signal (0 for empty,
len-1 overflow). Fix wish: the Table (and List) clamp the bound
selection to the new row count when rows are supplied — the widget owns
the invariant "the selection points at a real row or there are no
rows", exactly like it already owns clamping during navigation.
