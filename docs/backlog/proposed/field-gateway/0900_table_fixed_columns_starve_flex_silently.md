# Proposed: Table — oversubscribed fixed columns silently starve the Flex column to zero

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P2 — cost ~30min of "where did my model column go"; app-side workaround holds
- Class: footgun

## Context
The routes screen of `abstractgateway-console`: a `Table` over the
gateway's capability routes with five fixed columns and one
`ColWidth::Flex(1.0)` column for the model id (the one cell whose
content matters most — `AbstractFramework/wan2.2-i2v-a14b-diffusers-8bit`
class strings). On a 110-col terminal the table rendered with the flex
column **completely absent** — header and cells — with no warning. The
declared fixed widths summed to 118 (32+23+22+15+26), i.e. more than the
pane; every fixed column took its cells in declaration order and the
flex column received the nothing that remained. To a reader the table
just looks like the column was never declared.

## Current code reality
- `src/widgets/table.rs:386-420` (`solve_columns`, 0.2.8): fixed
  `Cells(c)` columns clamp against `remaining` in declaration order
  (`out[i] = c.clamp(0, remaining.max(0))`), and flex columns share
  `remaining` only `if any_flex && remaining > 0` — an oversubscribed
  fixed set leaves `remaining <= 0` and the flex share is zero.
- Two silent degradations compound: LATER fixed columns also clamp to
  whatever is left (a declared `Cells(26)` can render at 3 cells), so
  the damage depends on declaration order, and nothing surfaces —
  debug builds log nothing, the header row simply omits the column.

## Repro
```rust
use abstracttui::widgets::{ColWidth, Column, Table};
// total width 110 → usable ≈ 104 after gaps; fixed sum = 118.
Table::new(vec![
    Column::new("a", ColWidth::Cells(32)),
    Column::new("b", ColWidth::Cells(23)),
    Column::new("c", ColWidth::Cells(22)),
    Column::new("d", ColWidth::Cells(15)),
    Column::new("model", ColWidth::Flex(1.0)), // renders 0 wide
    Column::new("e", ColWidth::Cells(26)),     // renders truncated
]);
```

## Workaround in the field (delete when fixed)
Keep the fixed sum comfortably under the narrowest supported terminal
(the console's routes table dropped a column and slimmed the rest:
30+22+14+20 fixed + Flex model). What the engine could offer instead,
in preference order: (a) proportional shrink of fixed columns when
oversubscribed (flex keeps a floor), (b) a debug-build log line naming
the starved column, (c) a documented `min` on `ColWidth::Flex`. Any of
the three deletes the app-side arithmetic.
