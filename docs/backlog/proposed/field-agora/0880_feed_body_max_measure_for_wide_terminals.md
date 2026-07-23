# Proposed: FeedItem body max-measure — 150+ column wraps exceed comfortable reading width on wide terminals

## Metadata
- Created: 2026-07-24
- Status: Proposed (field-agora, agora-tui build — from the cycle-3 design review)
- Severity: P3 — cosmetic/readability; worst in the app's zoom (solo pane) mode
- Class: capability gap

## Context
agora-tui at 180×50 wraps message bodies at ~151 columns (pane width
minus chrome) — far beyond the ~90–100-cell measure where prose stays
comfortably scannable. Zoom mode (one solo pane, the app's long-read
surface) is where it hurts most: hub reports are multi-paragraph
markdown-ish text at full terminal width.

## Current code reality (0.2.11)
- Feed Text/Markdown blocks typeset at the item width — no per-block
  or per-feed maximum measure; the app cannot narrow a block without
  narrowing the whole pane (which would also narrow headlines, borders,
  and the follow-tail viewport).

## Repro
Any Text block in a 150+ column pane: lines wrap at the full width.

## Workaround in the field
None shipped (the app accepts the wide measure; centering the pane
column would waste the width for headlines too). An engine
`FeedItem::max_measure(cells)` (wrap bodies at `min(width, cells)`,
headlines unaffected) would let long-read surfaces keep a book-like
measure while chrome uses the full terminal.
