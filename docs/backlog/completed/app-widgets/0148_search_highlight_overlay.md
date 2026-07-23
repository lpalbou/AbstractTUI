# Completed: 0148 — Search-highlight overlay for rich/markdown views

- Status: completed (app-widgets wave 3, READER seat)
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: design coordination with 0160 (content selection) — both
  need a "typeset text ↔ screen cells" mapping; build the mapping once
- Completed: 2026-07-23

## Problem

A reader (mdpad-class) needs find-in-document: highlight all matches,
jump next/previous, live count — over typeset (wrapped, styled) text
whose screen positions differ from source offsets.

## What we want to do

(1) A query→match API over the typeset result (source-offset matches
mapped to cell rectangles per line fragment); (2) a non-destructive
highlight pass (style patch at draw time — background tint from theme
tokens; current-match distinct from other-matches); (3) scroll-to-match
composing with Scroll (and follow-tail disengage semantics); (4) match
count as a signal for the search bar.

The text↔cells mapping is the shared substrate with 0160 selection;
whichever lands first builds it and the other consumes.

## Validation

Highlight rectangles correct across wrapping/styled spans (goldens);
zero idle cost when the query is empty; large-document search latency
measured; scroll-to-match honors the damage contract.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).

## Completion report

- Final path: docs/backlog/completed/app-widgets/0148_search_highlight_overlay.md
- Date: 2026-07-23
- THE SHARED SUBSTRATE (0160 contract, documented in
  src/widgets/markdown_search.rs header): one typeset Row = one line
  fragment; its logical text = `RichLine::plain()` byte offsets; its
  cells = columns from `row.indent` (exactly where `draw_rows` puts the
  first cluster — same spans, same `text::segments`, same widths, so
  text→cells cannot drift from pixels). Both directions shipped:
  `row_col_at_byte` (text→cells: search rects, selection draw) and
  `row_byte_at_col` (cells→text: the 0160 mouse-hit direction, built
  now per the whichever-lands-first pact, round-trip test-pinned).
  Offsets snap OUT to grapheme clusters (a hit inside é/emoji covers
  the cluster — cells hold clusters, not bytes); matches never span
  wrapped rows (search lives in what the eye sees).
- Query API: `MarkdownView::find(source, &tokens, width, query,
  case_insensitive) -> Vec<MdSearchMatch { row, bytes, cells }>` —
  literal match (regex not required, per the item); case folding =
  full Unicode lowercasing with a per-byte offset map back to original
  char ranges (ß→ss and İ-expansion safe; pinned by
  `case_insensitive_folds_unicode_with_true_offsets`). Table rows are
  searchable as their TYPESET text (padded/aligned — honest: find in
  what is visible).
- Highlight pass: `MarkdownView::highlights(matches, current)` paints
  AFTER `draw_rows` by re-printing matched slices at their own columns
  (allocation-free span-slice walk) — glyphs stay, tones change. Token
  choice (the item's "check the token set" step): the audited set has
  NO dedicated search token, so matches wear the documented
  selection pair (`selection_fg` on `selection_bg`) and the CURRENT
  match adds BOLD+UNDERLINE as its distinct treatment — recorded here
  as the deliberate mapping. Zero idle cost: empty matches never enter
  the pass (and `find("")` is free) — the reader keeps the engine's
  0-byte idle frames.
- Scroll composition: matches carry absolute rows; the reader centers
  `matches[i].row` via `MarkdownView::scroll_offset` (clamped by
  `MarkdownView::rows` — the same fold). With the `Scroll` widget the
  identical rows drive `offset_y` + `follow_tail.set(false)` (the 0130
  disengage signal is app-visible both ways by design); the reader
  example uses the offset path since a document is not a tail-follow
  surface.
- Match count + current index: exposed as the reader's signals
  (`matches` memo + `current` signal → "match i/N" live in the footer;
  n/N wrap around).
- Tests (markdown_search_tests.rs): wrap-aware rect goldens, indent +
  styled-span column truth, unicode fold offsets, cluster snapping,
  the 0160 round-trip pin, highlight tones + current-match distinct +
  scroll-offset clipping through real draws, hostile-source fuzz, and
  the table-text searchability pin.
- Proof vehicle: `examples/reader.rs` `/` search bar (TextInput) with
  live count, Enter jump, n/N navigation; `live_reader` pty smoke
  green. Latency note: `find` re-folds the document per query change
  (explicit API cost, not per-frame); at mdpad scale (100 KB docs)
  parse+wrap is ~ms-class — no budget assertion added, matching the
  repo's counters-over-timings testing rule.
