# Proposed: Capped preview blocks — width-aware `max_rows` + honest overflow marker on Text/Rich feed blocks

## Metadata
- Created: 2026-07-23
- Status: Completed (2026-07-23, scroll/feed wave 4)
- Completed: 2026-07-23

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
- Related rhythm finding from the same adoption — corrected 2026-07-23
  and RE-corrected the same day after a pixel probe (a CaptureTerm
  harness rendering the six block sequences below; the first version
  claimed "unconditional", the second dropped the list exception and
  under-attributed the Markdown blanks). The rule as probe-verified:
  - Rich/Text after rows IN THE SAME RUN stack TIGHT (`typeset_static`
    inserts their blank only when `current.is_empty()`, i.e. right
    after a Custom flush — feed_typeset.rs:247/:265).
  - CUSTOM blocks get one blank whenever content precedes (:300-303).
  - Markdown/Code blocks after content get a blank through TWO
    mechanisms: the arm's own `any_content && current.is_empty()`
    blank (:274-276 / :286-288) AND `push_block`'s per-arm blank when
    `out` is non-empty — and `push_block`'s blank is per-arm:
    `ListItem` (and task items) never emit it, so a markdown body that
    STARTS with a list stacks TIGHT against a preceding rich header
    (probe-verified 2-row card; live-reachable today — an assistant
    answer beginning with a bullet). The engine comment at
    feed_typeset.rs:233-234 says "non-list", which is exactly this
    exception — the qualifier is load-bearing, not imprecision.
  - Probe-caught engine wart for this filing's design pass: after a
    Custom flush, BOTH Markdown mechanisms fire — Custom→Markdown
    renders a DOUBLE blank (probe: `CUST · blank · blank · PARA`;
    Custom→Text gets one). No engine test pins any of this rhythm
    (the closest asserts only `body_y > hdr_y`).
  App shape today: rich-header + CUSTOM-body cards are 3 rows
  (header · blank · body, matching its PARAGRAPH-led markdown cards —
  list-led answers are 2 rows per the exception above), while
  rich-header + Text-body would be 2 rows. Consequence FOR THIS
  FILING: if `max_rows` ships on Text/Rich blocks and the app converts
  its `CappedBody` custom blocks to Text+max_rows, every card silently
  drops to the TIGHT 2-row shape — reversing the uniform typography
  the app accepted. The design question is therefore WHICH rhythm a
  capped Text/Rich body gets (tight is the free default for Text/Rich;
  the separator is the Custom/Markdown-non-list behavior), whether the
  knob should be an explicit per-item/per-block `rhythm`/`tight`
  choice so consumers keep their shape across the conversion — and
  the double-blank wart above wants folding into the same pass.

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

## Completion report (2026-07-23, scroll/feed wave 4)

- **Shipped shape: builders on `FeedItem`, targeting the most recently
  appended block** (`FeedBlock` stays exhaustive through 0.2.x, so the
  cap rides `FeedItem` constructors — the 0102 precedent):
  - `FeedItem::max_rows(usize)` — caps the last appended Text/Rich
    block at `rows` TOTAL typeset rows, MARKER INCLUDED, applied
    post-wrap at the width the engine typesets at. Content wrapping to
    at most `rows` rows is unchanged; overflow shows the first
    `rows - 1` wrapped rows and spends the last on the marker. The
    name stays honest (`max_rows(6)` never renders 7 rows) and
    extent/windowing arithmetic stays exact (segment height = row
    count, marker counted). `rows` clamps to ≥ 1 — a zero-row cap
    over hidden content would vanish it without a trace.
  - `FeedItem::overflow_marker(impl Fn(usize) -> String)` — wording
    override, `hidden_wrapped_row_count -> text` (the app's "… (+K
    more lines — full text in the run ledger)"). Default wording:
    "… (+K more lines)". Inert without a cap on the same block.
  - Per-block chaining: `.block(a).max_rows(3).block(b).max_rows(8)`.
    Targeting a Markdown/Code/Custom last block (no cap support) is a
    debug_assert in debug builds, documented no-op in release.
  - Marker ink: `text_muted` (the task's ruling over the item's
    "block's ink" option), minted at typeset time from the bound
    tokens, so theme rebinds retint it with everything else. One row
    by design — overwide marker text clips at the item width through
    the shared row walk, never wraps.
  - K is computed AFTER wrap at the real width — the cap lives inside
    `typeset_static` (`push_capped`), exactly where the wrap lives, so
    K changes when the width does (test-pinned across a resize).
- **Streaming unaffected by construction**: caps live on static
  Text/Rich `ItemBlock`s only; `push_stream` entries never touch the
  capped path (test-pinned: ten streamed lines all render, no marker,
  full extent).
- **Rhythm decision**: a capped Text/Rich block keeps the TIGHT
  Text/Rich rhythm — the cap changes how many rows a block yields,
  never the separator policy around it. The item's rhythm-knob
  question (`rhythm`/`tight` choice for converted CappedBody cards)
  and the Custom→Markdown double-blank wart are DELIBERATELY not
  folded into this pass: they are separator-policy design (affecting
  all block kinds), not row-capping, and bundling them would have put
  a speculative knob on every item. If the app wants its 3-row card
  shape after converting, one explicit blank line in the body (or a
  follow-up rhythm filing) covers it — filed as an open follow-up in
  the wave-4 handoff.
- **Hang-indent NOT shipped** (the item's optional gap 2): not
  cheap-and-correct in this pass. `RichText::wrap` wraps every line at
  ONE width; honest hanging indent needs per-continuation-line width
  awareness inside the wrap (or a two-pass wrap that splits spans at
  the first-line boundary). The cheap approximation — wrap everything
  at `width - hang` and indent continuations — misrenders the first
  line by `hang` columns. Deferred with reasoning rather than shipped
  wrong; remains open in this item's gap list (gap 2, gap 3).
- **Tests** (`widgets::feed::rich_tests`):
  - `max_rows_caps_post_wrap_and_k_changes_with_width` — 6 logical
    lines at width 40 (K=4) then width 12 (each wraps to 2 rows,
    K=10); capped extent == 3 at both widths (marker row counted);
  - `overflow_marker_wears_muted_ink_and_custom_wording` — default
    wording + `text_muted` ink cell assert + custom closure wording;
  - `fitting_content_and_uncapped_blocks_render_unchanged` — cap ≥
    wrapped rows: no marker, exact rows;
  - `rich_blocks_cap_with_marker_and_keep_shown_inks` — Rich lines
    cap the same way, shown rows keep span inks;
  - `streaming_items_are_unaffected_by_row_caps`.
- **App workaround**: `CappedBody` + `wrap_capped` (~90 lines, the
  last non-image custom-draw closure) deletable on upgrade —
  `FeedItem::text(body).max_rows(n).overflow_marker(...)` replaces it.
- Gates at completion: whole-tree `cargo test` green, clippy
  `--all-targets` zero, fmt clean, alloc pins green,
  `cargo semver-checks` vs 0.2.6 additive-clean (the two new `FeedItem`
  methods are the wave's only public-surface change).
