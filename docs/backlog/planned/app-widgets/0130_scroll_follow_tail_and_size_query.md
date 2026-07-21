# 0130 — Scroll: follow-tail idiom + optional content_size (size query)

## Metadata
- Created: 2026-07-21
- Status: Planned
- Track: app-widgets
- Completed: N/A

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
- [ ] follow_tail policy signal + disengage/re-engage semantics
- [ ] Layout measure/size-query surface
- [ ] Feed/List/MarkdownView exact answers
- [ ] Resize-with-tail-pinned acceptance
- [ ] Docs: retire the scroll.rs honesty note

## Field evidence (2026-07-21, first app)
`abstractcode-tui` implements follow-tail app-side: an effect recomputes
content height on every fold/viewport/theme change and writes the offset
signal when sticking, plus a second effect deriving stickiness from
offset-vs-max (its src/ui/mod.rs `wire_autoscroll`). It also re-measures its
whole transcript to feed `content_size` on each change. Both halves of this
item (follow-tail idiom + measured content size) are confirmed real needs.
