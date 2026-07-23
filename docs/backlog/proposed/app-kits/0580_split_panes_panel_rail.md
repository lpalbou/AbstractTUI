# 0580 — Workspace chrome: resizable split panes + collapsible panel rail

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: nothing in-band (independent; rail headers use 0540
  badge slots, optional). Cross-band, non-blocking: control-plane
  0340 for the persistence recipe (two registered keys).
- Validator (0590): `examples/triage_shell` (SplitPane between thread
  and rail + the PanelRail itself — 0590 amended cycle 2 to include
  the split).
- Promotion trigger: the 0590 triage shell starting (right rail + the
  thread/rail split), or any master-detail surface in a dogfood app
  (a file-manager triptych — tree + list + preview, three panes, two
  dividers — is the fuller exercise if a fourth validator is added).
- Sibling (appended 2026-07-24): 0585's `app::Drawer` (completed) is
  the OVERLAY half of this family — summonable edge panels that cover
  the page. This item stays the DOCKED half (in-layout regions that
  consume space and resize neighbors); the two compose, neither
  absorbs the other.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (two composition widgets over the solver + signals).

## Context
Multi-pane workspaces are the spatial grammar of three brief classes:
the file manager's tree + list + preview triptych (D), the chat shell's
right-edge vertical rail of collapsible panels — Assistant / Members /
Files / Leaderboard / Desk (C), and the admin console's list + detail
panels (A). Monitoring dashboards (D) want the same two gestures the
moment they outgrow one screen: RESIZE the boundary between two
regions, and COLLAPSE a secondary region to reclaim space without
losing it. Both gestures are pure composition over shipped machinery
(flex solver, signals, `style_signal`), which is exactly why every app
will hand-roll them divergently unless the kit owns the policy: divider
hit-zones, keyboard resize parity, min-size floors, and
collapse-vs-dispose semantics each have one right answer worth encoding
once.

## Current code reality
- **No splitter/divider/rail widget**: grep for splitter/split_pane/
  divider — zero matches in src/ (verified this study). `Separator`
  (`src/widgets/mod.rs:36,63`) is a passive rule: no hit-zone, no
  drag, no keyboard.
- **The solver already speaks the layout**: fixed + growing panes are
  one row style (`docs/api.md:110-115` — the sidebar+content snippet);
  a resize is a signal-driven basis/width change via
  `Element::style_signal` (the mechanism `Scroll` uses to reposition
  content, `src/widgets/scroll.rs:3-6`), so a dragged divider is
  "write a signal per drag event", no new layout capability.
- **Drag machinery is proven**: `Scroll`'s thumb drags with pointer
  capture ("mouse-down auto-captures, so drags keep steering the thumb
  after the pointer leaves it", scroll.rs:36-38); `MouseKind::Drag`
  exists (`src/ui/event.rs:95-105`). A divider is a 1-cell-wide
  captured-drag strip.
- **Collapse semantics precedent**: Tabs' lazy panels DISPOSE on
  switch and the doc warns state must live outside the builder
  (`src/widgets/tabs.rs:3-8`); `Scroll` mounts content ONCE so state
  survives (scroll.rs:3-9). A collapsed panel must pick a side
  deliberately: the rail keeps panels MOUNTED (hidden, zero-size) so
  a chat assistant panel's scrollback/compose state survives collapse
  — the more expensive default, chosen for honesty, with a lazy opt-in
  for heavy panels (the Tabs pattern where the app prefers disposal).
- **Fold gesture + fixed-row law**: 0260's `▸/▾` disclosure gesture is
  the rail's header affordance (cross-referenced, not duplicated);
  header rows and dividers carry `shrink(0.0)` per the 0240 rule
  (`src/app/popups.rs:48-56,123-134` — declared fixed sizes are
  promises).
- **Focus geometry exists for pane hopping**: the dashboard wires
  Alt+arrows spatial focus via "REACT's cycle-7 `focus_next_in`"
  (`examples/dashboard/main.rs:241-260`) — pane navigation between
  splits composes with what ships; this item only standardizes the
  keybinding recipe in its docs, adding nothing to the focus engine.

## Problem
Resizable boundaries and collapsible side panels do not exist; a file
manager or a paneled chat shell must invent divider hit-zones, resize
keys, min-size clamping, persistence, and collapse state — five
policies per app, all divergent, some wrong (disposal eating panel
state is the predictable failure, already documented for Tabs).

## What we want
1. **`SplitPane`**: two children + one divider (row or column axis).
   - Position = `Signal<i32>` (cells of the FIRST pane; app-owned and
     bound in). Persistence honesty (corrected per PLATFORM cycle-2
     F3): signals are NOT "serializable by construction" — they are
     `Box<dyn Any>` arena cells with no reflection; what the signal
     substrate buys is REGISTRABILITY: the position is one `i32` the
     app can register as a control-plane 0340 key in one line.
   - Mouse: the divider is a 1-cell strip; hover affordance recolors
     to `accent` (theming state table, docs/theming.md:277-283);
     press + `Drag` with pointer capture resizes live; the strip
     renders `│`/`─` in `border`, `border_focus` while focused.
   - Keyboard parity: the divider is focusable; Left/Right (or
     Up/Down by axis) nudge by 1, Shift+ by 5; Enter toggles
     collapse-to-min of the secondary pane (a11y rule: nothing is
     drag-only).
   - Floors: per-pane `min` (cells); the clamp respects both mins and
     resolves conflicts toward the PRIMARY pane (declared by the app);
     terminal resize re-clamps (proportional vs. fixed-first policy is
     a builder knob with fixed-first default — the sidebar case).
   - Nesting composes (tree | (list / preview)) because each SplitPane
     is an ordinary element.
2. **`PanelRail`**: an edge-docked stack of titled, collapsible panels.
   - Model: `RailPanel { key, title, badge: Option<BadgeSpec>, build }`;
     expanded set = `Signal<HashSet<String>>` (bindable; "all
     collapsed" is the compact rail).
   - Rendering: collapsed = a 1-row header (`▸ title · badge`,
     0540 badge slot for counts — "Files (3)"); expanded = header
     (`▾`) + the panel body; bodies share the rail's cross-size;
     multiple panels may be open (accordion-exclusive is a knob).
     Collapsed-to-EDGE compact mode (the whole rail folds to a
     1-column strip of vertical glyph tabs) is v2 — evidence first.
   - State: panels stay mounted through collapse by default (signals,
     scroll positions, composers survive — the chat Assistant panel
     case); `lazy(true)` per panel opts into Tabs-style
     dispose-on-collapse for heavy content (both semantics named,
     neither silent).
   - Overflow: more expanded content than rail height → the rail body
     scrolls (`Scroll` composition); headers stay pinned
     (`shrink(0.0)`).
3. **Persistence recipe (via control-plane 0340)**: divider position +
   expanded set are two REGISTERED 0340 keys — the docs section shows
   the exact registration (`read_fn` samples the two values, `write_fn`
   applies them, one `u8` version) instead of the cycle-1 claim that
   the control-plane band "can serialize without new API" (false —
   no reflection exists; 0340's declared-keys registry IS the API, and
   kit value types being constrained is what makes registration
   one-line).

## Scope / Non-goals
Scope: SplitPane (mouse+keyboard resize, floors, collapse toggle),
PanelRail (fold, badges, mounted-vs-lazy), docs recipe, tests,
validator adoption (0590 triage shell: split + rail).
Non-goals: floating/undockable panels; drag-to-reorder rail panels;
golden-ratio/auto-layout policies; a full docking framework (three
fixed regions + rails cover every brief class; docking needs external
evidence); the compact glyph-strip rail (v2).

## Expected outcomes
Tree+list+preview is two nested SplitPanes with persistent positions;
the chat shell's right rail is one PanelRail with per-panel unread
badges; no app invents divider or collapse policy again. The roadmap's
deletion measure: the dashboard's fixed sidebar could adopt SplitPane
with one line when resize is wanted.

## Validation
- Unit: clamp math (mins both sides, primary-wins conflict, resize
  re-clamp under fixed-first and proportional policies); keyboard
  nudge/collapse parity with drag; rail expanded-set round-trips;
  mounted-through-collapse preserves a child signal's value while
  lazy(true) disposes it (both asserted).
- CaptureTerm acceptance: drag a divider with capture (pointer leaves
  the strip mid-drag, resize continues — the scroll.rs:36-38
  behavior); keyboard-only resize + collapse; a rail with three
  panels: expand two, scroll body, collapse all → 3 header rows; badge
  counts update without expanding (0540 reactivity); 40-col honesty
  (panes floor, rail still usable).
- Idle: settled dividers and parked rails cost zero (the toast
  idle-zero standard, `src/app/popups.rs:12-14` — extend the pin).

## Progress checklist
- [ ] SplitPane (drag capture, keyboard parity, floors, collapse)
- [ ] Resize-policy knob (fixed-first default) + re-clamp on resize
- [ ] PanelRail (fold headers, badges, mounted vs lazy semantics)
- [ ] Rail overflow scroll with pinned headers
- [ ] Persistence recipe doc (two registered 0340 keys, shown exactly)
- [ ] Acceptance + idle-cost tests; validator adoption
