# Proposed: Capped preview blocks — width-aware `max_rows` + honest overflow marker on Text/Rich feed blocks

## Metadata
- Created: 2026-07-23
- Status: Proposed (API gap report — first-app finding, 0.2.6 adoption wave)
- Completed: N/A

## ADR status
- Governing ADRs: None. ADR impact: none — feed block typesetting knobs.

## Context
0102 shipped the span model (`FeedItem::rich/rich_lines/rich_block`)
that the app's Card system motivated, and the app adopted it for every
HEADER row on 2026-07-23 (multi-ink glyph + label + detail as one
`RichLine`; ~70 lines of custom header drawing deleted). What keeps the
REST of the Card system alive is the body half: every transcript
preview body is row-CAPPED with an honest overflow marker, and neither
Text nor Rich feed blocks can express that. This is the one feature
standing between the app and deleting its last transcript custom-draw
closure (images aside — 0280).

## Current code reality
- App (`abstractcode-tui src/ui/transcript_view.rs`): `CappedBody` +
  `wrap_capped` — wrap at draw width, cap the POST-WRAP row count, and
  append "… (+K more lines — full text in the run ledger)". Nearly
  every body uses a cap (user 200 / steer 40 / thinking 10 / tool
  result 6 / error 12 and 3 / info 6 / probe 14 / image states 2 and
  1), plus a hang-indented first-line prefix (the `· ` of info items).
- Engine: rich/text blocks wrap at draw width with NO row clamp; render
  closures run width-independent, so a consumer cannot precompute the
  cap (the row count depends on the width the engine wraps at — that
  is exactly why the cap must live where the wrap lives).
- Three concrete gaps, in priority order:
  1. Width-aware `max_rows` + overflow marker (the load-bearing one).
  2. Hanging-indent continuations (`RichText::wrap` has no concept).
  3. Truncate-to-width (ellipsis) as a per-line alternative to
     wrapping — the old custom header ellipsized a long detail to the
     remaining columns; rich lines can only wrap, so the app accepted
     wrap-on-overflow for tool args previews (bounded by an upstream
     200-char cap) when it adopted rich headers.
- Related rhythm finding from the same adoption (may belong here or in
  its own note, engine's call): the feed's block rhythm inserts one
  blank row before every non-list block after content
  (feed_typeset.rs `typeset_static`), unconditionally. Adopting rich
  headers grew every body-carrying card from 2 to 3 rows (header ·
  blank · body). That happens to match the markdown-body cards'
  long-standing shape, so the app accepted it as uniform typography —
  but a consumer wanting the old compact header+body adjacency has no
  knob; if capped preview blocks ship, a `tight` option (suppress the
  separator row between two blocks of one item) would restore the
  choice.

## Proposed direction (engine's call)
- `FeedBlock::Text`/rich blocks (or a builder on `FeedItem`) gain
  `max_rows(usize)` + an overflow-marker line rendered in the block's
  ink (marker text either fixed or a `Fn(usize) -> String` for the
  "+K more lines" wording), applied POST-WRAP at the width the engine
  typesets at.
- Optional: `hang_indent(cols)` / first-line prefix, if cheap in the
  same pass.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/transcript_view.rs` — `CappedBody` +
`wrap_capped` + the `wrap_capped_holds_at_degenerate_widths_and_unicode`
geometry pin (~90 lines): the last non-image custom-draw closure in the
transcript projection.
