# Completed: 0130 — Scroll: follow-tail idiom + optional content_size (size query)

## Metadata
- Created: 2026-07-21
- Status: Completed (app-widgets wave, CONTENT seat — cycle 3)
- Track: app-widgets
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: coordinate with 0170 — `Scroll::content_size` is one of the
  crate's own named 0.2 churn points; batch any signature change into the
  0.2 breaking budget rather than trickling it.

## Context
Logs, transcripts, and chat rooms all want the same two behaviors: "stay
pinned to the bottom unless the user scrolled up" and "don't make me
hand-compute my content height". The robustness review (Part 2, P1-3 and
the `Scroll` row of its gap table) predicts every consumer will write the
same edge-cased follow-tail code; the crate's own module doc already files
the size-query request. This item pays both down before the ports copy the
workaround twice.

## Current code reality
- `src/widgets/scroll.rs:11-14` — the module's v1 honesty note, verbatim:
  "the content extent comes from `content_size(w, h)` — an explicit hint,
  because handlers have no layout-query surface yet. When one lands, the
  hint becomes optional and defaults to measured content (request
  filed)." This item is that filed request.
- `src/widgets/scroll.rs:31-58` — content is mounted once; offsets drive a
  reactive layout style (negative absolute insets); external
  `offset_y`/`offset_x` signals can be bound (scroll.rs:67-75). All the
  state a follow-tail policy needs is already reactive.
- `src/widgets/list.rs:8-9` — `List` has `scroll_to` (a command signal),
  but that is jump-once, not a standing "follow growth" policy.
- `src/widgets/markdown.rs:86-88` — `MarkdownView::rows(source, t, width)`
  is the kind of per-widget height fold callers currently run by hand to
  feed `content_size` (the reviews' sketched workaround).

## Problem
Two gaps, one module. (a) A feed that grows must manually: track whether
the user is at the bottom, recompute content height, and bump the offset
signal on every append — subtle around resize (the bottom row index
changes with width) and around the moment the user scrolls up mid-append.
(b) `content_size` forces every composite content to expose and maintain
a height fold; forgetting to update it clamps scrolling wrongly and the
error is silent.

## What we want
1. **`Scroll::follow_tail(Signal<bool>)`**: while true, the offset tracks
   the content's bottom edge across appends *and* resizes; any
   user-initiated upward scroll (wheel, keys, thumb drag) sets it false;
   scrolling back to the bottom edge sets it true. The signal is
   app-visible both ways (a "jump to latest ↓" affordance reads it; the
   app can force-set it).
2. **Layout size query**: a way for `Scroll` to measure its mounted
   content instead of demanding a hint — plumbed through the layout
   solver as an intrinsic-size query on the content wrapper (the solver
   already computes solved rects; the missing piece is exposing a
   measure pass for unconstrained axes). `content_size` becomes an
   optional override; the builder without it defaults to measured
   content. Widgets that can answer cheaply (Feed's prefix-sum total,
   List, MarkdownView via its cached rows) answer exactly; arbitrary
   element trees answer from a solve at the viewport width.
3. **Deprecation posture**: keep `content_size` working through 0.x;
   fold any removal into 0170's 0.2 budget.

## Scope / Non-goals
Scope: the follow-tail policy, the size query, Feed/List/MarkdownView
answering it, docs update of the honesty note. Non-goals: horizontal
follow (no consumer); virtual scrolling inside `Scroll` itself
(virtualization stays in Feed/List — Scroll remains the clipped-viewport
primitive); smooth/animated scrolling (compose with `Tween` app-side if
wanted).

## Expected outcomes
A chat room or log pane is `Scroll::new(feed).follow_tail(pinned)` with no
height bookkeeping; the honesty note in scroll.rs is retired; the ports
write zero follow-tail edge cases.

## Validation
- CaptureTerm acceptance: appends keep the bottom row visible; wheel-up
  disengages; returning to the bottom re-engages; resize keeps the tail
  pinned when engaged (the width-change row-count case).
- Size-query test: Scroll over a Feed/MarkdownView scrolls to the true
  last row with no hint; the hint, when given, still wins.
- Regression: existing Scroll tests unchanged (hint path stays green).

## Progress checklist
- [x] follow_tail policy signal + disengage/re-engage semantics
- [x] Layout measure/size-query surface
- [x] Feed/List/MarkdownView exact answers (see the report: MarkdownView
      answers through a Feed wrapper by design)
- [x] Resize-with-tail-pinned acceptance
- [x] Docs: retire the scroll.rs honesty note

## Field evidence (2026-07-21, first app)
`abstractcode-tui` implements follow-tail app-side: an effect recomputes
content height on every fold/viewport/theme change and writes the offset
signal when sticking, plus a second effect deriving stickiness from
offset-vs-max (its src/ui/mod.rs `wire_autoscroll`). It also re-measures its
whole transcript to feed `content_size` on each change. Both halves of this
item (follow-tail idiom + measured content size) are confirmed real needs.

## Completion report
- Final path: docs/backlog/completed/app-widgets/0130_scroll_follow_tail_and_size_query.md
- Date: 2026-07-21
- Design, in four moves. (1) MEASURED EXTENT: without a hint the content
  wrapper's scroll axis is `Auto`, so the solver's absolute-placement
  path answers its intrinsic size every solve (`place_absolute` →
  `intrinsic_size`; the standalone query is `layout::measure`, landed
  with its own test) — Feed answers O(1) through its reactive
  `total_rows` height style, text trees answer wrap-aware at the
  viewport width; the solved size is read back by a draw-probe +
  latched `after(0)` into an extent signal (the RT1-2-lawful Feed
  width-fixup pattern) that clamps offsets, sizes the thumb, and feeds
  the pin. `content_size(w, h)` is now `Option` internally: when given
  it WINS verbatim and nothing is measured (the old wrapper byte-for-
  byte). (2) FOLLOW-TAIL: `follow_tail(Signal<bool>)`; while true a pin
  effect keeps `offset = max(0, content_h − view_h)` (extent + a
  viewport probe, both reactive), and while actually scrolled the
  wrapper anchors its BOTTOM inset to the viewport — the solver keeps
  the tail glued through appends/shrinks/resizes pixel-exact the same
  frame with zero extent knowledge, and the wrapper can never leave
  the clip (which would starve the probe; a clear()-rebuild deadlocked
  exactly there before this). (3) DISENGAGE/RE-ARM: only user gestures
  derive the signal from geometry (`new_offset >= max_offset` after
  wheel/keys/thumb-drag) — programmatic offset writes never disengage;
  the app force-sets true to jump to the latest. (4) The default
  `Scroll` layout gained `basis(Cells(0))` beside its `grow(1.0)` —
  the 0240 completion report's follow-up #1 (a content-sized basis let
  long content starve fixed siblings to zero rows).
- Tests (unit, src/widgets/scroll_tests.rs — the four v1 tests
  unchanged, plus): `measured_extent_scrolls_to_the_true_last_row_without_a_hint`,
  `content_size_hint_wins_over_measurement`,
  `default_layout_takes_leftover_not_content_basis`,
  `follow_tail_pins_growth_disengages_on_wheel_and_rearms_at_bottom`,
  `app_can_force_follow_to_jump_to_latest`,
  `follow_tail_repins_across_resize`. Acceptance through the real loop
  (tests/wave_content.rs, real SGR wheel bytes + a scripted resize):
  `follow_tail_acceptance_appends_wheel_and_resize`,
  `clear_rebuilds_a_bounded_window_and_follow_repins`,
  `feed_10k_inside_measured_scroll_draws_only_a_screenful`.
- Measured: a 10k-item feed pinned inside a measured Scroll draws 171
  puts against the 900-put window budget; steady streaming under
  follow emits ~104 bytes/token (max 1,000) with chrome rows
  byte-identical; whole-tree suite green (958 lib + all integration).
- Semantics pinned by tests, worth naming: re-arm happens ON the
  bottom edge only (one wheel step short stays disengaged); a
  disengaged view holds byte-identical rows under further appends;
  when content fits the viewport the state is trivially "at bottom"
  and stays following; `Home`/`End` participate like any gesture.
- Exact-answer matrix vs the item text: Feed exact (O(1)); List keeps
  its own virtualization and offset — putting a List inside a Scroll
  stays a non-goal, so "List answers" is N/A by design; a bare
  `MarkdownView` is a draw-only widget with NO intrinsic height — the
  documented recipe (scroll.rs module doc) is a one-item Feed (same
  typeset fold) or the explicit hint via `MarkdownView::rows`.
- Deprecation posture: `content_size` is NOT deprecated — it is the
  override half of the contract ("the hint, when given, still wins"),
  so ADR-0001's named 0.2 churn point for `Scroll::content_size` can
  be re-evaluated at 0170: the additive path landed without a break.
- Follow-ups filed by 0240's report and still open (not this item's
  scope): one-row controls defaulting `shrink(0.0)`, the layout
  zero-collapse debug notice, the docs "modal with scrollable middle"
  recipe. Note: 0240 pointed at reviews/wave/stability-to-content.md
  for their precise specs, but that file was never written — the
  completion report's own list is the authoritative record.
- CLOSURE UPDATE (2026-07-21, final wave cycle): the three remaining
  0240 follow-ups landed. #2 — `shrink(0.0)` on the DEFAULT layouts of
  button, checkbox, radio, input, progress, badge, spinner, separator
  and the tabs bar (caller-provided layouts untouched); representative
  pin `tests/wave_content.rs::one_row_controls_survive_overflow_pressure`
  (verified discriminating: fails with the old defaults). #3 —
  debug-build zero-collapse diagnostic: the solver watches children
  that DECLARED a fixed `Cells` main-axis size (explicit min — even
  min 0, the 0240 opt-out — unwatches; percent/intrinsic never watch)
  and reports once per node via `LayoutTree::take_collapse_notices` +
  stderr; pinned by
  `layout::tests::zero_collapse_emits_a_debug_notice_once_with_opt_outs`.
  #4 — docs/api.md gained "Modal content that can overflow" beside the
  Scroll follow-tail section.
