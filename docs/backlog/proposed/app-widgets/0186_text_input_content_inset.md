# 0186 — TextInput / TextArea: a configurable left/right content inset

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: app-widgets
- Completed: N/A
- Root link: AbstractFramework backlog 1055; consumer: AbstractCode
  composer ("› " prompt, its backlog item on tool-result rendering).

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (additive builder knob).

## Inventory (what exists)
`TextInput` (src/widgets/input.rs: `value`, `placeholder`, `masked`,
`layout`, `on_change/on_submit/on_paste`) and `TextArea`
(src/widgets/textarea.rs: `rows`, `submit_policy`, `placeholder`, …)
draw text from column 0 of their rectangle; the only inset is the
1-cell cursor margin of horizontal scroll (input.rs:492). Apps that
want "› text" or breathing room wrap the widget in a padded Row, which
moves the hit area and breaks mouse-to-caret at the padding.

## What we want
`.inset(left, right)` (cells, default 0,0) on both widgets, applied
INSIDE the widget's rectangle and honoured by every path: typed text,
placeholder, wrapped lines (TextArea), selection, cursor and caret
cell, mouse-to-caret (a click in the inset maps to the nearest caret),
horizontal scroll budget, paste. Safe at narrow widths: the inset
shrinks before the text area reaches 1 cell, never negative. Also
settable through the widget's style/token (no per-app arithmetic).

## Validation
Headless tests at inset 0, 1 and 2 (and a width smaller than the
inset): drawn cells, caret cell, mouse-to-caret, wrap width, scroll;
red without the knob.
