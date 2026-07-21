# 0100 — Feed/Transcript widget (virtualized, append-only, rich blocks)

## Metadata
- Created: 2026-07-21
- Status: Planned
- Track: app-widgets
- Completed: N/A

## ADR status
- Governing ADRs: None — this repo has no ADR system yet (see 0170).
  ADR impact: the widget's public shape should wait for 0170's ruling on
  the 0.2 breaking-change budget, because it lands on `List`'s own named
  churn point (multi-row item content).

## Context
An append-only feed of keyed rich items is the defining surface of every
app whose content arrives over time — chat rooms, log and event monitors,
agent transcripts, notification panes, REPL histories: each item a small
rich document (markdown paragraphs, code fences, badges, tool-call
cards), windowed so 10k items cost only the visible rows, with only the
tail item changing during streaming. Both cycle-11 evaluations
independently named this the #1 gap
(`reviews/cycle11/completeness-and-code-port.md` §2b P0-1;
`reviews/cycle11/robustness-and-chat-port.md` Part 2 P0-1) and converged
on the point that proves the generality: an agent-console transcript and
a chat message list are the **same widget**. The two port epics are its
first validators — no app in these classes can be built without it, and
building it per-app would be throwaway every time.

## Current code reality
- `src/widgets/list.rs:1-13` — `List` is virtualized with variable heights
  (per-item height callback, prefix-sum windowing, binary-search item
  lookup), sticky selection by key, and `scroll_to`. But items are
  `Vec<String>` (list.rs:55) and the module doc is explicit: "the label
  renders on the item's first row only — wrapped multi-row item CONTENT is
  a later decision". The windowing machinery is exactly what a feed needs;
  the content model is not.
- `src/widgets/markdown.rs:108-124` — `MarkdownView` typesets its whole
  source at draw width and caches per width **inside one element
  instance** (`cache: Option<(i32, Vec<Row>)>`); any source change means a
  new element and a full re-parse. `MarkdownView::rows()`
  (markdown.rs:86-88) runs the same whole-document fold again for the
  caller's scroll clamp. Measured cost ~1 ms per 1,000-line re-parse
  (completeness review §0) — per-token streaming multiplies that into
  whole-core burn at 30–100 events/s.
- `src/ui/view.rs:339-348` — `dyn_view` replaces its entire subtree per
  rebuild; there is no keyed reconciliation, so a naive `dyn_view` over a
  message vector rebuilds every message view on each arrival.
- `src/widgets/scroll.rs:1-14` — `Scroll` mounts content once and clips,
  but needs an explicit `content_size(w, h)` hint (see 0130).
- `src/render/rich.rs:1-19` — `RichText`/`RichLine` is the right row
  currency: owned spans, span-preserving wrap, "parsed once, rendered many
  frames", drawing allocates nothing beyond `Surface::draw_text`.
- The presenter already wins on the byte side: scroll-region emission is
  referee-verified at 7.8–9× byte reduction on log-append
  (tests/adv_scroll.rs) — the engine's costs are typesetting, not paint.

## Problem
There is no widget that owns "many rich items, appended over time, only
the last one hot". Applications must choose between `List` (single-row
strings — no rich content), one big `MarkdownView` (O(document) re-parse
per change), or a hand-rolled window over `MarkdownView::rows` +
`Scroll::content_size` (both reviews sketch this workaround; it is
feasible and throwaway). Every ingredient exists in-repo; the composition
does not.

## What we want
A `Feed` widget (working name; `Transcript` reads too console-specific):
1. **Keyed items**: `push(key, FeedItem)` / `update(key, …)` — items are
   identities, not indices; the tail item is mutable (streaming), earlier
   items are frozen by default. `FeedItem` content is a small block list
   reusing the `render::md::Block` vocabulary plus app-supplied custom
   blocks (a `View`-per-item escape hatch is acceptable v1 if measured
   heights stay honest).
2. **Per-item typeset cache**: each item owns its typeset rows
   (`RichLine`s cached per width, the `MarkdownView` recipe applied
   per-item). A width change re-typesets all (affordable: 800-paragraph
   full re-wrap measured at 9.8 ms); a content change re-typesets only
   that item.
3. **Windowing**: prefix-sum row index over item heights (lift the
   machinery from `List`); only visible items draw; item lookup by binary
   search. 10k-item feeds must cost only the window.
4. **Streaming tail**: the open tail item re-typesets per delta —
   integrating 0110's `md::StreamSession` so even the tail pays only its
   open block.
5. **Scroll composition**: the feed reports its content extent (rows) so
   it can live inside `Scroll` without a hand-maintained
   `content_size` hint — designed together with 0130 (size query +
   follow-tail). Sticky-bottom behavior itself belongs to 0130's idiom.
6. Optional selection by key (sticky, like `List`) for
   detail-panel/copy flows.

## Scope / Non-goals
Scope: the widget, its typeset cache, windowing, tail-streaming seam,
scroll integration, and a worked example (a fake streaming transcript).
Non-goals: mouse text selection/copy across items (command-copy ports
first; selection is a later item per the completeness review P1-6 — now
filed as 0160); clickable link hit-testing (P2-7 there — now filed as
0165); tool-call-card/approval-bar sugar (app crates first, upstream if
they generalize); replacing `List` (it stays for flat string lists).

## Expected outcomes
Appending one item to a 10k-item feed re-typesets one item and repaints
only damaged rows; streaming into the tail item costs O(open block) per
delta, not O(document); both port epics (0200, 0210) consume this widget
unmodified.

## Validation
- CaptureTerm + `Driver::turn` acceptance: append/update/stream, window
  correctness at scroll offsets, sticky selection across appends.
- Alloc-budget test: steady-state append (closed items) allocates no
  per-frame typeset work outside the appended item.
- Perf budget (release, `#[ignore]` like the existing perf suite): tail
  streaming at a fixed delta rate on a 10k-item feed stays under budget.
- VtScreen byte assertion: appended rows ride the scroll-region emitter.

## Progress checklist
- [ ] Item/block model + per-item typeset cache
- [ ] Prefix-sum windowing (lifted/shared with List)
- [ ] Keyed push/update + frozen/closed semantics
- [ ] Tail-stream seam (0110 integration point)
- [ ] Scroll composition (with 0130)
- [ ] Example + acceptance/alloc/perf tests

## Field evidence (2026-07-21, first app)
`abstractcode-tui` (the AbstractGateway coding-agent client) hand-rolled this
exact widget: per-item row measurement mirrored between a `measure_all` and a
builder, a `Scroll` whose `content_size` is recomputed per append, and a full
subtree rebuild on every transcript change (its src/ui/transcript_view.rs).
It works at agent-event cadence but confirms every cost this item names —
whole-transcript re-typeset per append, measurement/builder drift risk, and
the manual stick-to-tail machinery (two effects + a stickiness cell). That app
is the first migration target when this lands.
