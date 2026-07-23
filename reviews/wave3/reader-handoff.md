# Wave 3 — READER seat handoff (0142 / 0144 / 0146 / 0148)

Date: 2026-07-23. Seat: READER. All four items completed and moved to
`docs/backlog/completed/app-widgets/`; gates green (whole-tree tests,
clippy zero, fmt clean, alloc pins, `cargo semver-checks` vs published
0.2.2: 196 checks pass — everything additive).

## Ledger rows (for CONTENT2 — the single overview.md writer)

Append-ready rows, matching the overview table shape:

| item | title | state | note |
| --- | --- | --- | --- |
| app-widgets/0142 | Markdown tables (GFM subset) | completed (wave 3, READER) | `md::DocBlock`/`parse_doc` + `DocStreamSession`; typeset shares Table's `solve_columns` |
| app-widgets/0144 | Markdown images: in-flow mosaic | completed (wave 3, READER) | probe-at-typeset (`gfx::probe_dimensions`), lazy decode at first draw, LRU by (path, sig, size); protocol-in-flow stays a named open question |
| app-widgets/0146 | Heading anchors + TOC | completed (wave 3, READER) | `md::outline`/`slugify` + `MarkdownView::outline_rows`/`resolve_anchor` (rows from the ONE typeset fold) |
| app-widgets/0148 | Search-highlight overlay | completed (wave 3, READER) | `MarkdownView::find` + `.highlights()`; text↔cells mapping built BOTH directions — 0160's substrate is ready |

## What CONTENT2 needs to know (feed/chart consumption)

- `BlockTypesetter::push_block` and `StreamSession` are BYTE-UNCHANGED —
  feed compiles and renders exactly as before. Core sources typeset
  identically through the new doc fold (test-pinned:
  `core_sources_typeset_identically_through_the_doc_fold`).
- To get tables/images/tasks in Feed items: switch the item's parser to
  `md::DocStreamSession` (same API shape as `StreamSession`:
  append/closed_blocks/closed_revision/open_blocks/finish) and typeset
  via `BlockTypesetter::push_doc_block(out, &DocBlock, width, separate)`
  (crate-internal, delegates core blocks to `push_block` — one recipe).
  `Row` gained one field (`image: Option<MdImageSlice>`, always `None`
  from `Row::plain` and every core recipe); `draw_rows` handles image
  rows internally — no feed-side draw changes needed.
- Image rows in a Scroll context are mosaic cells — safe under partial
  visibility by construction. Decode is lazy (first draw) and cached
  across rebuilds; a feed with 100 images decodes only what scrolls
  into view.
- Strikethrough (`~~x~~`) joined the CORE inline vocabulary
  (`Attrs::STRIKE`, attribute-only). Existing goldens unaffected unless
  they contain literal `~~` (tree-wide grep found none).

## The text↔cells mapping contract (0160 reuse — 4 lines)

1. One typeset `Row` = one line fragment; logical text = `RichLine::plain()` byte offsets; cells = columns from `row.indent`, exactly where `draw_rows` paints (same spans, same `text::segments`).
2. `markdown_search::row_col_at_byte(row, byte) -> col` (text→cells) and `row_byte_at_col(row, col) -> byte` (cells→text, the mouse-hit direction) are the two queries; both crate-internal in src/widgets/markdown_search.rs, round-trip test-pinned.
3. Offsets snap OUT to grapheme-cluster boundaries (cells hold clusters); ranges never span rows — multi-row selection = per-row ranges.
4. 0160 consumes these directly (the whichever-lands-first pact is discharged); highlight painting shows the draw idiom: re-print slices at their columns with a style patch, glyphs stay.

## Deferred / named open questions

- Pixel-protocol images inside scrollable flow: NOT attempted, per the
  0144 open-design note (placement/eviction under partial visibility is
  damage-contract territory). Mosaic-only shipped; the module header in
  markdown_image.rs documents the seam.
- Feed adoption of the doc vocabulary (above) is deliberately left to
  the feed owner — the pieces are ready, the wiring is a product call.
- `md::outline` carries no `row` field (layering: rows are the widget
  fold's); `MarkdownView::outline_rows` is the width-resolved form.
- The token set has no dedicated search/highlight token; matches wear
  the documented selection pair, current match adds BOLD+UNDERLINE. If
  a future theme wave mints a `search` token, the mapping lives in ONE
  place (`markdown_search::draw_highlights`).

## New/changed files (mine)

- src/render/{md_doc,md_doc_stream,md_outline}.rs (+ _tests siblings);
  md.rs (module wiring, `~~`, escape `\~`); md_stream.rs (three
  `pub(super)` promotions, zero behavior).
- src/gfx/probe.rs (+ gfx/mod.rs export).
- src/widgets/{markdown_doc,markdown_image,markdown_search}.rs (+ _tests
  siblings); markdown.rs (Row.image, doc fold switch, new APIs);
  table.rs (`solve_columns` → pub(crate)); image.rs (from_path
  widening); mod.rs (lint-list + re-export appends).
- src/prelude.rs, docs/api.md, CHANGELOG.md, examples/README.md
  (appends); examples/reader.rs; tests/live_smoke.rs (`live_reader`).
