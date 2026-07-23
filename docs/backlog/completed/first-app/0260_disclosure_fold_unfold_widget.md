# Proposed: Disclosure widget — graphical fold/unfold for transcript sections

## Metadata
- Created: 2026-07-21
- Status: Completed (2026-07-24 disclosure wave — promoted STANDALONE
  under this filing's own promotion criterion: field-agora 0850 was the
  second independent consumer, arriving before 0100 started; see the
  completion report at the bottom)
- Completed: 2026-07-24

## ADR status
- Governing ADRs: None. ADR impact: none — an additive widget.

## Context
The maintainer, using `abstractcode-tui` live, asked for "a clean and simple
way to fold/unfold" the agent's underlying reasoning cycles and tool results
"graphically". The app shipped the workaround the engine allows today: a
GLOBAL details toggle (Ctrl+D — thinking blocks vanish, tool cards collapse
to their header). What the request actually describes is PER-ITEM disclosure:
a `▸ cycle 3 (12 lines)` row that expands to `▾ …` in place — click or
Enter, with the collapsed row summarizing what is hidden.

## Current code reality
- No collapsible/disclosure/accordion widget exists in the catalog
  (src/widgets/: block, button, input, list, table, tabs, scroll, checkbox,
  radio, progress, spinner, badge, separator, charts, grid, image, viewport3d,
  markdown, richtext, code, logo — verified).
- The ingredients exist: `dyn_view` re-renders a region on a `Signal<bool>`,
  `Element::focusable()` + Enter/Space activation defaults, `Block` renders
  headers. Every app can hand-roll one disclosure — and every app will,
  differently, with the height-remeasure bug each time (a fold changes the
  transcript's scroll math; abstractcode-tui's measure/build split has to be
  updated in two places per item type).
- The 0100 Feed/Transcript widget plan (planned/app-widgets) does not
  currently name per-item collapse in its item model.

## Problem or opportunity
Agent transcripts are the flagship use case (ports track 0200/0210): they
interleave high-value content (answers) with high-volume detail (reasoning,
tool output). Without disclosure, apps choose between noise and blindness;
the global-toggle workaround is all-or-nothing and loses the "peek at just
this cycle" gesture users reach for first.

## Proposed direction
1. A `Disclosure` widget: header row (glyph ▸/▾ + title + summary hint,
   focusable, Enter/Space/click toggles) + a content `View` mounted lazily on
   first expand (Tabs' lazy-panel pattern). Collapsed height = 1; expanded =
   content height. Bound `Signal<bool>` for external control (a global
   "collapse all" writes every signal).
2. Fold it into 0100's item model: feed items get an optional collapsed
   state with a summary line, so transcript virtualization and scroll math
   account for disclosure natively instead of every app re-deriving heights.
3. Keyboard story: the transcript needs focus to reach disclosure headers —
   which meets 0100's selection model (j/k between items, Enter toggles).

## Why it might matter
Direct maintainer request from the first live session with the first app;
every agent-transcript consumer (0200 console, 0210 chat) needs the same
gesture.

## Workaround in the field (delete when this lands)
abstractcode-tui: global `show_details` signal (Ctrl+D / `/details`,
persisted) — thinking items render zero rows, tool cards drop their result
preview; errors stay visible in both modes (honesty over tidiness).

## Promotion criteria
Fold into 0100's design (preferred — the item model should be born with
collapse semantics), or promote standalone if a second app needs it before
0100 starts.

## Validation ideas
- Widget test: collapsed renders 1 row + summary; expand mounts content once
  (state inside survives re-collapse); Enter/Space/click all toggle.
- Layout test: a Scroll containing N disclosures re-measures correctly on
  toggle (the exact math abstractcode-tui hand-rolls today).

## Non-goals
No animation requirements (a tween is optional polish); no tree-view
generalization (nested disclosures compose naturally but are not designed
for here).

---

## Completion report (2026-07-24, disclosure wave — with field-agora 0850)

**Scope**: the standalone `Disclosure` widget shipped (promotion
criterion satisfied: 0850 was the second consumer before 0100
started), plus the enablers both consumers' hand-rolls needed
(`Feed::on_item_press` + `FeedState::item_at_row`,
`Scroll::extent_signal` + `Scroll::scrollbar_auto_hide`) and the
documented message-card recipe. Direction 2 of this filing (fold
collapse into 0100's item model as a native feed-item KIND) is
deliberately NOT shipped: `FeedBlock::Custom` blocks are draw-only
cell closures (0280's block boundary — no widget lifecycle inside
feed items), so an engine-owned interactive card inside the feed
would need the 0280 resolution first. Inside a Feed the supported
shape is the recipe (api.md "The message-card recipe"); the widget
covers the standalone card.

**What shipped** (`src/widgets/disclosure.rs`):

- Header row exactly as asked: `▸`/`▾` glyph (accent ink), title
  (truncate-ellipsis), optional right-aligned muted `detail` slot
  (renders whole or drops when < 4 title cells would remain — the
  title wins). One focusable tab stop; Enter/Space toggle while
  focused; click on the title row toggles (and focuses). Focus wears
  the selection pair (§3.2 borderless rules); hover is accent
  garnish. Chrome decision: BORDERLESS two-tone (header
  `surface_raised`, body `surface`) — cards stack at transcript
  scale where per-card borders read as noise; `Block` composes
  around one for a frame.
- Body mounts lazily on first expand and UNMOUNTS on fold (a
  `dyn_view_scoped` generation — zero idle cost folded, remount per
  expand; the filing's "state inside survives re-collapse" idea
  yielded to the maintainer's zero-cost ask: durable state belongs
  in app signals outside the builder closure, per-expansion
  internals die with the fold). `Disclosure::text`/`::markdown`
  bodies typeset ONCE in a one-item feed kept across folds.
- `max_body_rows(n)` (default 8): the body region is
  `min(content, n)` rows; overflow scrolls inside a `Scroll` with a
  visible, auto-hiding scrollbar (the maintainer's explicit
  scrollbar ask); `n <= 0` = uncapped. The region sizes itself from
  the Scroll's measured extent (one settle turn on content change —
  the documented Scroll contract).
- Uncontrolled (`initially_folded`, default FOLDED) AND controlled
  (`folded(Signal<bool>)`, two-way) state; `on_toggle(bool = new
  folded state)` runs after the state write (0297 disposal law,
  test-pinned). A11y: `region`(title) wrapping `button` with
  value "collapsed"/"expanded" (Role enum frozen till 0.3 — the
  Select precedent).

**Validation ideas from this filing, landed as tests**: collapsed =
1 row / expand mounts / Enter+Space+click toggle
(`disclosure_tests.rs`: 14 tests incl.
`folded_body_unmounts_and_every_unfold_remounts`,
`enter_and_space_toggle_only_while_focused`); the Scroll-of-N-
disclosures re-measure math is exercised end-to-end in
`tests/wave_disclosure.rs` (toggle damage contained, sibling rows
hold position, extent never drifts).

**Consumer note (abstractcode-tui)**: the global Ctrl+D details
toggle can now be per-item — transcript sections as
`Disclosure::text("cycle 3", body).detail("12 lines")` standalone,
or the recipe inside its Feed; the height-remeasure bookkeeping this
filing complained about is engine-owned in both shapes.

**Gates** (2026-07-24, whole tree): see the wave handoff
(`reviews/wave7/disclosure-handoff.md`) — tests green, clippy zero,
fmt clean, `cargo semver-checks` vs 0.2.10 additive-clean.
