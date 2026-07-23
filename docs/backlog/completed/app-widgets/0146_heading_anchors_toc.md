# Completed: 0146 — Heading anchors + TOC extraction from markdown

- Status: completed (app-widgets wave 3, READER seat)
- Track: app-widgets
- Origin: seeded by extensions/0460, cycle-4 handoff
- Depends on: none; 0165 (hyperlink hit-testing) consumes the anchor ids
- Completed: 2026-07-23

## Problem

A document reader needs "jump to section" (TOC sidebar, `#anchor`
links); the md pipeline typesets headings but exposes no structure.

## What we want to do

(1) `md::outline(source) -> Vec<Heading { level, text, anchor_id, row }>`
— rows computed from the typeset result at a given width so a TOC can
scroll-to; (2) stable anchor-id slugging (GitHub-compatible:
lowercase, dashes, dedup suffixes); (3) intra-document link targets:
`[text](#anchor)` resolves to a row; activation rides 0165 when it
lands (until then, apps consume the outline directly for TOC lists).

## Validation

Slug golden table (unicode, dedup, punctuation); outline rows match
typeset rows across widths (property test: re-wrap then re-outline);
anchor links resolve.

Full analysis: docs/backlog/proposed/extensions/0460 (§seeds).

## Completion report

- Final path: docs/backlog/completed/app-widgets/0146_heading_anchors_toc.md
- Date: 2026-07-23
- ONE LAYERING PRECISION vs the item text: `row` cannot live on the
  render-layer `md::outline` — rows are a property of the WIDGET
  TYPESET FOLD (wrap width, spacing policy, the level-1 underline), and
  computing them in `render::md` would duplicate that policy (drift).
  So: `md::outline(source) -> Vec<Heading { level, text, anchor_id }>`
  (src/render/md_outline.rs) owns the source-level facts, and
  `MarkdownView::outline_rows(source, &tokens, width) -> Vec<OutlineEntry
  { heading, row }>` (src/widgets/markdown_doc.rs) pairs each heading
  with its typeset row from THE SAME FOLD the renderer draws
  (`layout_doc` — also serving `element`/`rows`/`find`, so TOC jumps
  can never drift from the pixels). `MarkdownView::resolve_anchor`
  answers `[text](#anchor)` (leading `#` optional).
- Slugging (`md::slugify`): lowercase (full Unicode, one-to-many
  expansions included: ẞ→ß stays, İ→i+dot); keep
  letters/digits/`_`; spaces and `-` → `-`; drop the rest
  (punctuation, symbols, emoji). Dedup: `-1`/`-2`… in reading order,
  probing past literal collisions (a real "setup-1" heading occupies
  the suffix). DOCUMENTED DEVIATION from GitHub: combining marks drop
  (std has no Unicode-category-M test) — decomposed accents slug
  differently; precomposed match GitHub exactly. Golden table pins 21
  cases incl. CJK, emoji, dedup, and the deviation itself.
- Property test (`outline_rows_match_the_typeset_fold_across_widths`):
  across widths 10..80, every entry's row is monotonic and the typeset
  row at that index IS the heading's text (headings typeset unwrapped —
  the equality is exact, not prefix-fuzzy); entry count and content
  equal `md::outline` at every width. Re-wrap then re-outline stays
  consistent, as specified.
- Anchors survive the doc vocabulary: headings inside fences and pipe
  rows never produce phantom entries (test-pinned); hostile-corpus fuzz
  pins unique ids on every input.
- Proof vehicle: `examples/reader.rs` TOC panel (List over
  `outline_rows`, Enter jumps) + an intra-doc `[link](#a-small-table)`
  resolved via `resolve_anchor`; `live_reader` pty smoke green. 0165
  consumes `Heading.anchor_id` when it lands, as planned.
