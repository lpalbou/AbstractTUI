# 0142 — Markdown tables (GFM subset) in the md pipeline

- Status: proposed
- Track: app-widgets
- Origin: seeded by extensions/0460 (mdpad-class reader enablement), cycle-4 handoff
- Depends on: none (coordinates with app-kits/0530 on `solve_columns` reuse)

## Problem

`render::md` parses headings/paragraphs/lists/code/quotes but not GFM
tables; a markdown reader (mdpad-class) and chat messages carrying
structured tables (the coordination-UI case) render them as raw pipes.

## What we want to do

Parse the GFM table block (header row, `---` separator with alignment
colons, body rows) into a table block in the md block vocabulary and
typeset it with column solving SHARED with the Table widget's
`solve_columns` (src/widgets/table.rs:374) — share the solver function,
never embed the widget (Feed items are draw-only). Overflow: per-column
truncation with ellipsis; alignment honored; inline spans (bold/code)
inside cells work.

## Non-goals

Cell merging, block elements inside cells, interactive
sorting/selection (that is the Table widget's job).

## Validation

Golden snapshots (alignment, truncation, inline spans in cells); parse
equivalence via `md::StreamSession` (a table streamed token-by-token
equals batch parse); fuzz the parser against the hostile corpus.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).
