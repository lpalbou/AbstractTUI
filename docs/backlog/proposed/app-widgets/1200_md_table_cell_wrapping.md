# 1200 — Markdown tables: cell wrapping + grouped multi-line rows

## Metadata
- Created: 2026-07-25 (wave 13, ARCHITECT-MD lane; numbered from 1200
  by agreement — the peer BACKLOG seat is filing wave-12 items in the
  low bands concurrently, so this lane takes 1200+ to avoid collision)
- Status: Proposed
- Track: app-widgets
- Completed: N/A

## Problem

The md-table recipe never wraps cells (an explicit 0142 non-goal):
overwide cells truncate with an ellipsis. Wave 13 made the crush ladder
honest (per-column floors, then the record-layout fallback — see
ADR-0005 and `markdown_doc.rs`), but between "fits" and "records" a
prose-heavy table still loses words to `…` that a WRAPPING grid would
keep. mdpad (the operator's own viewer, the wave-13 quality bar)
wraps cells to their solved column width and inserts row separators
only when a row is multi-line — its tables read like a browser's.

## Proposal

Overturn the 0142 non-goal deliberately, as its own item:

1. In `push_table`, wrap each cell's spans to its solved column width
   (`RichText::wrap`, the shared wrapper) instead of `truncate_rich`;
   a table row becomes `max(cell line counts)` typeset rows, cells
   padded row-wise (top-aligned).
2. Word-minimum floors: the overflow floor becomes
   `min(natural, max(3, widest word))` per column (mdpad's minimum
   stage) so wrap points stay word-shaped where possible.
3. Separator rows between BODY rows only when any cell wrapped
   (mdpad's rule: grouping is necessary exactly then; otherwise the
   separator wastes a row).
4. The record fallback stays: when even word minimums overflow, the
   grid is still a lie.

Costs to rule on: table height becomes width-dependent in a stronger
way (Feed prefix sums and `MarkdownView::rows` already re-typeset per
width, so the FOLD is ready); streaming needs no change (tables seal
whole). Golden churn is deliberate and localized to table tests.

## Validation

- Goldens: a prose table at panel widths wraps instead of `…`-losing
  words; separators appear only for multi-line rows; alignment holds
  per wrapped line.
- Property: no input loses words between the wrap stage and the
  record stage (join of rendered rows ⊇ join of cell words, modulo
  whitespace).
- The existing crush tests (floors, records) stay green with adjusted
  thresholds.
