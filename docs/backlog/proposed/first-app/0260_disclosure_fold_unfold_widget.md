# Proposed: Disclosure widget — graphical fold/unfold for transcript sections

## Metadata
- Created: 2026-07-21
- Status: Proposed (feature request — first-app finding, maintainer-requested UX)
- Completed: N/A

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
