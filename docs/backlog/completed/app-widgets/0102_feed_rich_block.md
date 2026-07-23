# 0102 — `FeedBlock::Rich`: span-model feed lines (multi-ink without a custom block)

- Status: Completed (content wave, CONTENT2 seat — 2026-07-23)
- Track: app-widgets (band 0100–0190; extends the 0100 Feed trunk)
- Origin: FIELD study 2 consumer-tensions report
  (`reviews/study2/field-consumer-tensions.md` §4.1 — ranked the
  **highest-leverage single addition** the first consumer's code argues
  for; also `reviews/study2/field-app-classes.md` class 5's
  severity-tinted log lines and class 3's multi-ink message headers).
  Filed by the convergence pass (cycle 2, 2026-07-22).
- Depends on: none (additive enum variant + one renderer wire). Family:
  media-av/0660 (image blocks) and first-app/0280 (widget-hosting
  blocks) press on the SAME `FeedBlock` enum — one design pass settles
  the block vocabulary; whichever executes first owns it (cross-ref
  recorded in 0660).
- Promotion trigger: any transcript/log consumer mixing inks inside one
  feed line — the first consumer already carries the workaround today.

## Problem

`FeedBlock` is Text / Markdown / Code / Custom (src/widgets/feed.rs:
74-94). Text is single-ink; the only way to render a line mixing colors
(a message header `role · model · time`, a severity-tinted log line, a
status row with a colored badge word) is `FeedBlock::Custom` — a draw
closure with a hand-written height callback and hand-rolled wrapping.

The engine already OWNS the span model this needs: `render::rich::
RichText` (patch-style spans: `fg: None` inherits the widget's base
ink, `bg: None` inherits the fill) and the `RichTextView` renderer
whose span walk is deliberately shared "one renderer, three faces"
(src/widgets/richtext.rs:1-20). Feed is the missing fourth face.

Consumer evidence (field-consumer-tensions.md §4.1, consumer paths):
abstractcode-tui's `Card` system — ~137 lines (transcript_view.rs:
41-177) whose only reason to exist is "colored chrome the theme-ink
Text block cannot express" (transcript_view.rs:34-35), carrying its own
height/draw honesty contract (transcript_view.rs:100-124). The report's
verdict: "every transcript, every log viewer" re-pays this cost.

## What we want to do

1. `FeedBlock::Rich(RichText)` — typeset through the SAME span walk
   `RichTextView` uses (`draw_rich_lines`; never a second renderer),
   wrapped at item width with an honest height function like the other
   block kinds, participating in Feed's windowing/prefix-sum extent
   exactly like Text.
2. Span styles stay PATCHES (the richtext.rs rule): `None` fields
   inherit the feed item's base ink/fill so theme-agnostic RichText
   lands themed.
3. Streaming stays out of scope: Rich blocks are replace-on-update like
   Text (streaming spans would need a `StreamSession` analog — separate
   item if a consumer appears).

## Non-goals

Widget-hosting blocks (first-app/0280's question); image blocks
(media-av/0660); per-span hit-testing/links (0165's lane); a markdown
shortcut (Markdown blocks already exist).

## Validation

- Golden: a Rich block with three inks renders the exact cells
  `RichTextView` would render for the same `RichText` at the same
  width (parity pin — one renderer, four faces).
- Wrap + windowing: heights honest under resize; extent math unchanged
  for Text-only feeds (no regression).
- Theme patch rule: `fg: None` spans pick up the item ink; explicit
  spans survive theme switches.
- Consumer deletion test (the real acceptance): the first consumer's
  `Card` draw closure for header lines becomes a `FeedBlock::Rich`
  construction (report cites transcript_view.rs:41-177 as the deletable
  mass).

## Completion report

- Final path: docs/backlog/completed/app-widgets/0102_feed_rich_block.md
- Date: 2026-07-23
- ONE PREMISE CORRECTION, gate-forced: the item's "additive enum
  variant" is FALSE against `cargo semver-checks` — `FeedBlock` shipped
  EXHAUSTIVE in 0.2.x, and `enum_variant_added` on an exhaustive public
  enum is MAJOR (probe-verified against the published 0.2.2 baseline
  before designing). The rich kind therefore ships as `FeedItem`
  constructors — `FeedItem::rich(RichText)`, `.rich_block(RichText)`
  (builder append, composes with `.block(...)` in any order), and
  `FeedItem::rich_lines(Vec<RichLine>)` (the one-styled-line
  convenience) — over a crate-private FLAT `ItemBlock` vocabulary
  (`src/widgets/feed_item.rs`) that IS the eventual 0.3 public enum;
  `From<FeedBlock> for ItemBlock` converts losslessly. The fold-back
  (`FeedBlock` + `#[non_exhaustive]` + true `Rich` variant, after which
  0660/0280's block kinds land additively) is entry 8 of the 0.3
  budget (planned/0002) — this pass owned the block-vocabulary design
  the item's family note asked for.
- Typesetting: `ItemBlock::Rich` wraps through the SAME span-preserving
  `RichText::wrap` and lands as `Row::plain` rows drawn by the shared
  `draw_rows` -> `print_span_clipped` walk — one renderer, one more
  face, zero new draw code. Spans store VERBATIM (no ink stamping), so
  the patch rule (`fg: None` inherits the item ink) survives theme
  rebinds; explicit inks are resolved `Rgba` by the widget-wide token
  posture (rebuild items to retint — documented). Separator policy
  mirrors `Text` (its sibling class). Streaming stayed out of scope per
  the item. File split for the size discipline: `feed_item.rs` (public
  model) + `feed_sync.rs` joined the `#[path]` siblings and the mod.rs
  lint list; `feed.rs` holds state + painter (546 lines).
- Tests (src/widgets/feed_tests.rs):
  `rich_block_matches_richtextview_pixels` (the parity pin — chars,
  inks, grounds AND attrs cell-exact vs `RichTextView` at the same
  width), `rich_item_consumers_severity_log_and_chat_header` (the two
  filed consumer shapes, incl. multi-block rich-header-over-markdown),
  `rich_blocks_rewrap_and_resync_extent_on_resize` (height honesty),
  `rich_span_patch_rule_binds_item_ink_per_theme` (fg-less spans wear
  each theme's `text`; explicit ink byte-stable across themes),
  `rich_blocks_render_in_fixed_box_mode` (both feed modes). Existing
  Text-only extent tests unchanged and green (no regression).
- Measured: 1k three-ink rich items pushed at width 40 typeset in
  ~3.1 ms median release (≈3.1 µs/item, incl. mount + dispose;
  `perf_rich_block_typeset_1k_items`, `#[ignore]`d perf-suite
  convention).
- Consumer-deletion acceptance: the in-repo consumer shapes are pinned
  as tests (severity log line + chat header); abstractcode-tui's Card
  deletion is that app's own follow-up — the constructor it needs now
  exists.
