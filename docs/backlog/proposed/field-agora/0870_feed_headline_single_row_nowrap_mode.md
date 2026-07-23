# Proposed: a FeedItem headline (single-row, no-wrap, clip-with-ellipsis) mode — folded cards should be one row by contract, not by luck

## Metadata
- Created: 2026-07-24
- Status: Proposed (field-agora, agora-tui build — from the cycle-3 design review)
- Severity: P3 — app-side caps mitigate; residual wrapping persists on long chip sets
- Class: capability gap

## Context
agora-tui's folded message cards are one `FeedItem::rich_lines` line
(the 0.2.11 message-card recipe's own shape). Rich lines WRAP at feed
width, so a folded "one-liner" whose chips + sender + title +
decorations exceed the pane width becomes 2–3 rows — and the wrap
continuations carry no status strip, no indent, and no distinct ink, so
they impersonate body rows (design review P1-2: a CJK continuation row
was indistinguishable from an expanded body line). The app now caps
titles at 96 chars and drops previews from titled cards, which makes
one-row folding the COMMON case — but long addressee lists or dense
chip sets still wrap, and the app cannot know the width to cap against
(the row count only exists post-wrap, as 0283 already ruled for
`max_rows`).

## Current code reality (0.2.11)
- `feed_typeset.rs` — `ItemBlock::Rich` typesets through the
  span-preserving wrap unconditionally; there is no per-block nowrap or
  row-cap-with-ellipsis for rich blocks (`max_rows` exists but spends
  its last row on the "+K more lines" marker — the right contract for
  bodies, the wrong one for headlines, which want CLIP `…` in place).
- `Disclosure` (0.2.11) solved exactly this for its own title row
  ("truncate-ellipsis title") — the primitive exists engine-side, just
  not for feed items.

## Repro
Any `FeedItem::rich_lines` single line wider than the pane: it wraps;
the continuation row starts at column 0 with no visual tie to the card.

## Workaround in the field (delete when fixed)
`src/ui/panes.rs` in agora-tui: `cap_chars(title, 96)` + preview only on
untitled posts + decorations-last ordering — probabilistic one-row
folding. An engine `FeedItem::headline(...)` (or `.nowrap()` on the last
rich block: clip at width with `…`, exactly Disclosure's title-row rule)
would make it a contract and delete the char-count guesswork.
