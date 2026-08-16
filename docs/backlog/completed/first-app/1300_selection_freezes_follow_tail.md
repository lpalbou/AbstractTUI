# 1300 — A live selection must freeze follow-tail (screen-space copy over a streaming transcript)

Status: completed 2026-08-16
Owner: engine (widgets/scroll + app/selection)
Effort: S

First item of the first-app CONTINUATION band: 0220–0299 is full (0299
was itself renumbered out of control-plane's range), so per the
next-free-fifty rule this track continues at 1300–1340.

## The field failure

abstractcode-tui, 2026-08-16, maintainer-reported as two bugs:

1. "I cannot copy text while it works (in a react cycle)."
2. "When it is not in a react cycle I can select, but it says it can't
   copy" — screenshot of the engine's own notice, `clipboard: OSC 52 not
   advertised by this terminal — copies may be ignored`.

Symptom 2 was a version fact, not a live defect: that app pinned
`abstracttui` 0.2.20, where OSC 52 was the ONLY copy route. 0.2.25's
host-clipboard fallback (`platform_clipboard`, default on) fixes it on
the bump — see CHANGELOG 0.2.25/0.3.0.

Symptom 1 is this item. The selection region is SCREEN space (module
docs say so plainly), and the app's transcript is a `Scroll` with
`follow_tail` pinned while a run streams. So during a run:

- the highlight paints on the cells the user dragged over;
- every append re-pins the scroller to the content bottom, sliding the
  text UP under a highlight that does not move;
- release extracts the glyphs on those screen cells NOW.

The user highlights one thing and the clipboard receives another —
worse than a failed copy, because it is a silent, plausible wrong
answer. No app-side workaround exists that keeps the transcript live:
the app can only stop following entirely, and it cannot see a drag
start (`selection().is_active()` is not reactive).

## The rule

A copy must return the text the highlight covered. Since the region is
screen space, that means: **while a region is visible, screen-space
content under it does not move.**

## Shipped

- `widgets::scroll::freeze_follow_tail(bool)` / `follow_tail_frozen()`
  — one thread-local reactive flag under the leaked-root pattern
  (`app::theme`'s). Frozen, a pinned `Scroll` drops its bottom anchor
  and returns to `top: -oy` at the offset the pin last wrote: visible
  rows hold, appends grow below the viewport, the wheel steps from `oy`
  instead of the live bottom. Thawing re-runs the pin effect, which
  re-tracks extent/viewport and lands on the tail as it stands then.
- `app::selection` publishes `region.is_some()` at every transition
  that can change it (drag claim, dismissal, release-copy, key-copy,
  Esc, disable, resize, session reset). Layering holds: the app layer
  writes DOWN into a widgets-layer flag; `widgets` never learns about
  `app`.
- `follow` itself is never touched, so "following / scrolled" chrome
  does not flicker for the length of a drag, and a user who wheels away
  mid-drag still disengages normally.

Cost when nothing is selected: one extra signal read per pinned
scroller per settle, and one `set_if_changed` per selection edge.

## Evidence

- `tests/adv_selection.rs::a_live_selection_freezes_the_streaming_tail_and_copies_what_it_shows`
  — real driver, SGR mouse bytes, `FeedState` appends arriving mid-drag:
  the highlighted row holds, the release copies exactly that row, and
  clearing the region re-pins to the new tail.
- `widgets::scroll::tests::frozen_follow_tail_holds_its_rows_and_repins_on_thaw`
  — the widgets half alone (text columns hold; the scrollbar thumb still
  moves, which is the honest report that content grew).

## Not in scope

Logical-content selection (copying a widget's text rather than rendered
screen rows) remains app-widgets/0160. This item makes the screen-space
answer TRUE, it does not replace it.
