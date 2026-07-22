# 0530 — Data-table upgrades: rich cells, row actions, activation, identity

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: 0540 (badge-cell rendering) — non-blocking for the
  text/actions half; 0500's substrate for row-action overflow menus.
- Validator (0590): `examples/admin_console` (the rotate-a-key journey
  is this item's acceptance surface).
- Activation semantics: per the 0250 ruling as proposed by PLATFORM
  (reviews/study/platform-on-appkits.md "The 0250 ruling") — adopted
  here; §3/§5 encode it.
- Promotion trigger: the first admin/console-class dogfood surface (the
  0590 admin-console validator, or any product table needing a badge or
  a button in a row).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none expected. The cell-content enum is public API and
  should land under 0170's breaking-budget discipline if `Table`'s
  existing `rows(Vec<Vec<String>>)` signature changes (an additive
  `rich_rows` avoids the break — design call recorded below).

## Context
Reference UI A is a table of tables: every admin screen is rows of
routes/users/entities with STATE BADGES (configured / covered /
not-configured / linked; enabled / asleep) and PER-ROW ACTION BUTTONS
(Edit / Clear / Override / Configure; Rotate / Disable / Delete; Talk /
Manage). Monitoring dashboards (D) want status-tinted cells and row
drill-in; file managers want name/size/date rows with activation;
triage lists (C's message tables, D's smart-note inbox) want selection
that survives data refresh. This is one class-level need: the table
stops being a grid of strings and becomes a grid of *meaningful cells
with acts attached*. The shipped `Table` was built for the dashboard
demo's read-only sessions pane and is honest about that scope.

## Current code reality
- **Cells are plain `String`s**: `Table::rows(Vec<Vec<String>>)`
  (`src/widgets/table.rs:73-76`); the draw truncates each cell with
  `truncate_ellipsis` and prints in one style per row
  (table.rs:347-358). No per-cell tones, no badges, no buttons, no
  custom draw. (`text::truncate_ellipsis` is cluster-safe —
  `src/text/truncate.rs:19` — keep it as the overflow authority.)
- **One selection, movement-fires-callback**: `selection:
  Signal<usize>` + `on_select` invoked from `select()` on every
  selection CHANGE (table.rs:153-174) — the same footgun 0250 reported
  on `List`, whose report already names Table as "same shape... likely
  the same hazard" (0250 "Problem" section). No activation event, no
  multi-select, no selection-by-key: `List` has sticky `selection_key`
  + `key_fn` (`src/widgets/list.rs:99-111`) but `Table` holds only the
  index (table.rs:48-57) — a refresh that reorders rows silently moves
  the selection to a different entity (the exact preference-corruption
  class 0250 documents).
- **Header interactions exist and stay**: header click → 
  `on_sort_requested(col)`, 's' round-robins (table.rs:177-247);
  the app owns the ordering (table.rs:83-88 `sorted` indicator).
- **Row painting is one draw closure, not elements**: the body renders
  inside a single `dyn_view`+`draw` (table.rs:266-367), so cells
  cannot host real `Button` widgets without a cell-element model.
  Precedent for the honest alternative is IN the widget already: the
  header maps clicks to columns geometrically (table.rs:222-243) —
  action cells can map clicks to hit-zones the same way. `Feed` chose
  the same road for rich content (`CustomBlock` draw + height callback,
  `src/widgets/feed.rs:87-100`).
- **Mouse vocabulary**: `MouseKind` has Down/Up/Move/Drag/Scroll only
  (`src/ui/event.rs:95-105`) — no double-click synthesis exists; an
  activation gesture must be Enter/Space + single-click policy, not
  double-click (or this item adds click-count synthesis; recorded as a
  design question).
- **Tokens for cell states are ready**: `Badge` tones
  (`src/widgets/badge.rs:22-30`, `Tone::{Accent,Ok,Warn,Error,Info,
  Muted}`) and the selection-pair/hover rules (docs/theming.md:266-288).

## Problem
An admin console cannot be built on `Table` today: no state badges, no
per-row actions, no "open this row" event, and selection that breaks on
refresh. Building it app-side means forking the whole widget (the row
draw is one closure), so every product would carry a diverged table —
the exact outcome the widget library exists to prevent.

## What we want
1. **Cell content model**: `Cell::Text(String)` (today's behavior,
   default), `Cell::Styled { text, tone, attrs }` (status-tinted text),
   `Cell::Badge { label, tone }` (renders the 0540/badge chip),
   `Cell::Actions(Vec<RowAction>)` (see 2), `Cell::Custom` (draw
   closure + width hint — the Feed CustomBlock pattern scoped to a
   cell). Additive API: `rich_rows(Vec<Vec<Cell>>)` beside `rows(...)`
   (which becomes sugar for all-Text) — no break, 0170-friendly.
2. **Row actions**: `RowAction { id, label, tone, disabled }` rendered
   as compact `[label]` hit-zones in their cell; click OR (while the
   row is selected) a bound key fires `on_action(row_key, action_id)`.
   Keyboard path: Left/Right (or Tab-like cycling within the selected
   row — design call at implementation) moves an action highlight;
   Enter fires the highlighted action; plain Enter with no action
   highlighted fires row activation (3). Hit-zones map geometrically in
   the existing bubble handler (the header-click precedent); actions
   render disabled in `text_faint` and are skipped.
3. **Activation vs. selection** (the 0250 ruling applied): keep
   `on_select` as the selection-changed NOTIFICATION (never wire
   commitment to it); add `on_activate(row)` on Enter (always — the
   universal commit key) + click policy (single-click selects, a
   second click on the selected row activates — no double-click event
   exists, event.rs:95-105; click-count synthesis would be its own
   engine item). **Space follows the ruling's toggle-first rule
   (PLATFORM cycle-2 F5)**: in single-select mode Space aliases Enter
   (no toggle meaning exists); with multi-select enabled (§5) Space
   TOGGLES the row mark and never activates — one key, one meaning per
   mode, both tested. Callbacks must be disposal-safe per ruling §4:
   the widget finishes its own bookkeeping (ensure-visible offset
   math, table.rs:163-172) BEFORE invoking user callbacks — the 0250
   crash class designed out, test-pinned.
4. **Row identity**: `row_key_fn` + `selection_key: Signal<String>`
   mirroring `List` (list.rs:99-111) so refresh/sort keeps the same
   logical entity selected; `on_action`/`on_activate` report the KEY,
   not the index (indices lie across refreshes).
5. **Multi-select**: opt-in `selected_keys: Signal<Vec<String>>` +
   Space-toggles + a leading checkbox column rendered by the table
   (header checkbox = all/none); the admin bulk-ops surface. Selection
   pair stays the single-focus row; multi-marks render as `[x]` cells
   (one selection-pair meaning per screen — docs/theming.md:274-276).
6. **Column polish, evidence-scoped**: per-column `min` width honored
   by `solve_columns` (today a squeezed fixed column just clamps,
   table.rs:374-408) + per-column ellipsis vs. clip choice. Horizontal
   scroll stays OUT (non-goal) until a validator demands it.
7. **Empty state**: a `View` slot rendered when rows are empty (every
   console shows "no routes configured — Create one"); today the body
   just paints ground.
8. **Semantic visibility of actions (PLATFORM cycle-2 F6)**: hit-zones
   are not elements — they carry no role/label and are invisible to
   `accessibility_tree()`, which is the machine-readable UI state the
   control-plane band exports (automation bus / wire / MCP). The
   table's `access_value` must therefore carry, for the selected row:
   the row key/label, the available action ids, and the currently
   highlighted action — so an agent (or screen-reader bridge) can
   discover Edit/Rotate/Delete without pixel knowledge. This is the
   band rule (README "Band rules"): every interactive affordance a kit
   widget renders is represented in the accessibility snapshot.

## Scope / Non-goals
Scope: cell model, actions + activation + identity + multi-select,
column min/ellipsis, empty slot, tests, gallery + 0590 validator use.
Non-goals: inline cell EDITING (a 0500 select in a cell is opened via
`on_action` into a popup — cell editors are a later item if validators
demand); horizontal scroll / frozen columns; grouping/tree rows (0570
owns hierarchy); markdown-in-cells (md has no tables anyway,
`src/render/md.rs:14-17` — and the honest recipe for a table inside a
Feed message is a custom-DRAWN static table via `FeedBlock::Custom`,
which is a draw closure, not an element mount (feed.rs:87-90): it can
paint table cells but can never host THIS interactive widget; when the
app-widgets band lands md-table typesetting, that block type replaces
the hand-drawn recipe. Corrected cycle 2 — the cycle-1 text claimed "a
real Table via FeedBlock::Custom", which overstated what a draw-only
block can carry).

## Expected outcomes
The reference admin console's route/user/entity screens compose from
`Table` directly: badges and action buttons per row, Enter opens,
bulk-select works, refresh never mis-selects. Dashboards get tinted
cells for free. No product forks the table.

## Validation
- Unit: key-sticky selection across reorder (port List's semantics);
  action hit-zone geometry incl. truncated cells; disabled actions
  skipped by highlight + unclickable; activation fires once (Enter,
  second-click) and never on arrows (the 0250 regression, table
  edition); callback-disposes-scope safety test.
- CaptureTerm acceptance: an admin-style table (state badge column +
  three actions) driven keyboard-only: navigate → highlight action →
  fire → data refresh (reorder) → same entity still selected;
  multi-select three rows via Space (which must NOT activate) → header
  checkbox clears; single-select mode: Space activates like Enter;
  empty state renders the slot.
- A11y/automation: before an action fires, the accessibility snapshot
  names the selected row and the highlighted action id (the F6
  discoverability test — an agent can read what it is about to do).
- Theme: badge/action tones restyle on theme switch (tokens only —
  the widget lint applies, `src/widgets/mod.rs:8-15`).

## Progress checklist
- [ ] Cell enum + rich_rows (additive; rows() = all-Text sugar)
- [ ] Action cells: hit-zones, keyboard highlight, on_action(key, id)
- [ ] on_activate + disposal-safe callback ordering (with 0250)
- [ ] row_key_fn + selection_key parity with List
- [ ] Multi-select + checkbox column (Space = toggle, ruling F5)
- [ ] Column min/ellipsis-policy + empty-state slot
- [ ] access_value action discoverability (F6) + snapshot test
- [ ] Acceptance suite + gallery/admin-validator adoption
