# Completed: 0142 — Markdown tables (GFM subset) in the md pipeline

- Status: completed (app-widgets wave 3, READER seat)
- Track: app-widgets
- Origin: seeded by extensions/0460 (mdpad-class reader enablement), cycle-4 handoff
- Depends on: none (coordinates with app-kits/0530 on `solve_columns` reuse)
- Completed: 2026-07-23

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

## Completion report

- Final path: docs/backlog/completed/app-widgets/0142_markdown_tables.md
- Date: 2026-07-23
- SEMVER CONSTRAINT SHAPED THE DESIGN: `md::Block` shipped exhaustive in
  0.2.x, so the table block lands in a NEW `#[non_exhaustive]` enum
  `md::DocBlock { Core(Block), Table(TableBlock), Image, Task }` with
  `md::parse_doc` as the entry (src/render/md_doc.rs). On sources
  without extended constructs, `parse_doc == parse` wrapped in `Core`
  (test-pinned). Verified additive vs published 0.2.2 with
  `cargo semver-checks` (196 checks pass).
- Recognition (the honest subset, documented in md_doc.rs): header = a
  plain-text line with an unescaped `|` whose NEXT line is a delimiter
  row (`:?-+:?` cells, ≥1 pipe so `---` stays a rule, cell count ==
  header count); body rows require a pipe (deviation from GFM's
  one-cell-row absorption — deliberately, for streaming cut-safety);
  blank/block/prose lines close the table; `\|` is a literal pipe;
  extra cells drop, missing cells pad (`TableBlock::new` normalizes).
- STREAMING (`md::DocStreamSession`, src/render/md_doc_stream.rs): the
  open/close semantics the item required — a table OPENS in
  `open_blocks()` once header + delimiter lines are complete, grows a
  row per pipe line, SEALS at the first non-pipe line; EOF closes like
  the fence recovery rule. Cut safety extends the core session's: no
  seal from an open table's header through its last row, and none past
  an UNRESOLVED header candidate (a complete pipe line whose successor
  has not arrived). The seal shares `doc_line_class`/`table_opens` with
  the batch parser (children of `md`, one implementation — no drift),
  and the batch/stream equivalence note: streamed-vs-batch was pinned
  against `DocStreamSession` (the doc vocabulary's session) rather than
  the item's literal `md::StreamSession`, which cannot carry table
  blocks (its `Block` return type is the frozen core enum).
- Typesetting (src/widgets/markdown_doc.rs): the Table widget's own
  `solve_columns` promoted to `pub(crate)` (src/widgets/table.rs) and
  SHARED — natural widths as `Cells` when they fit; when overflowing,
  columns beyond the fair share become `Flex(natural)` and truncate
  per-cell with a style-preserving ellipsis (`truncate_rich`). Bold
  header, border-ink separator rule across the solved width, 1-cell
  gaps (the solver's contract), `:--`/`:-:`/`--:` honored, inline spans
  keep their patches inside cells. `BlockTypesetter::push_doc_block`
  delegates core blocks to `push_block` verbatim (Feed's recipe is
  unchanged; adopting tables in Feed = switching to the doc session +
  `push_doc_block`, CONTENT2's call).
- Tests: md_doc_tests.rs (recognition/alignment/padding/escapes/inline
  spans/fence immunity, `parse_doc == Core-wrapped parse` pin, cell
  lexer edges incl. escape parity, hostile-corpus + markdown-soup
  fuzz), md_doc_stream_tests.rs (26-doc corpus × 7 chunkings
  equivalence, 250 randomized docs, hostile corpus streamed == batch,
  open/grow/seal semantics, O(open) cost pin, EOF idempotence),
  markdown_doc_tests.rs (typeset goldens: alignment/padding/separator,
  center + truncation, bold header + border ink through a real draw,
  inline code chip in a cell, zero/tiny width no-panic).
- Proof vehicle: `examples/reader.rs` renders a live table;
  `live_reader` pty smoke green.
