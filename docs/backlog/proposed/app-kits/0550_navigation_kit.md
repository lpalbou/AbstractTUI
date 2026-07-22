# 0550 — Navigation kit: sidebar nav + filter tab strip with counts

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: 0540 (count/dot badge vocabulary for items and tabs).
- Validator (0590): `examples/triage_shell` (channel NavList + filter
  tabs with live counts) + `examples/admin_console` (section sidebar).
- Activation semantics: per the 0250 ruling
  (reviews/study/platform-on-appkits.md "The 0250 ruling") — the
  cycle-1 deferred default is now RESOLVED by ruling §3; §1 encodes it.
- Promotion trigger: the 0590 admin-console or triage-shell validator
  starting; or the 0210 chat epic reaching its room-list phase (its
  sidebar is this widget).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none. Stays inside the cycle-7 router ruling
  (`src/ui/compose.rs:136-155`): navigation state is a signal; these
  widgets render and mutate it, they never own routing.

## Context
Both "which area am I in" surfaces recur across every reference class
and are hand-rolled today. (1) The SIDEBAR: the admin console's section
nav (A), the chat shell's channels + DMs list with unread badges (C),
the file manager's places/libraries rail and the smart-note notebook
list (D). (2) The FILTER TAB STRIP: the chat thread view's
All / Unread / Asks / Needs-vigilance / FYI / Resolved tabs with live
counts (C), the admin console's per-panel scope tabs (A), the
smart-note inbox/triage states (D). Both are subset-selection: a list
of destinations/filters, one active, with counts announcing where
attention is owed. They share the count vocabulary (0540) and the
selection semantics, which is why they are one item.

## Current code reality
- **The sidebar is hand-rolled in the flagship example**:
  `examples/dashboard/main.rs:342-362` — `Block::new().title("nav")` +
  `List::new(items).selection(nav).focus_signal(nav_focus)`, items
  pre-formatted as strings. It has no sections, no badges, no
  collapse, and the on_select-fires-on-arrow footgun (0250) means
  merely browsing the nav "navigates" — with `dyn_view_scoped` page
  switching that disposes and rebuilds the page per arrow keystroke.
- **`List` carries most of the machinery to reuse**: variable-height
  items, sticky `selection_key`, `scroll_to`
  (`src/widgets/list.rs:1-13,99-120`) — but items are `String`s
  (list.rs:54-55, label drawn on the first row only, list.rs:11-13),
  so badges/section headers/indent cannot render without the same
  string-concat hacks the dashboard uses.
- **`Tabs` is the wrong shape for filter tabs**: titles are
  `Vec<String>` (`src/widgets/tabs.rs:26-32` — no count slot, no
  overflow when the strip exceeds the width: the span walk at
  tabs.rs:186-208 just runs off the rect), and Tabs OWNS panel
  mounting (lazy `dyn_view` panels, tabs.rs:214-221) — a filter strip
  filters ONE surface (the same Feed/List re-queried), it does not
  switch panels. Reusing Tabs would mount/dispose the thread view per
  filter switch, exactly what a triage UI must not do.
- **Both existing widgets prove the interaction contracts to copy**:
  one focusable element per strip/list, arrows move, click maps
  geometrically (tabs.rs:114-144; list keyboard family), `focus_signal`
  for pane strokes (`src/ui/view.rs:279-280`; Table's version at
  `src/widgets/table.rs:100-105`).
- **Count/badge rendering is 0540's** (Badge::count, dot, clamping) —
  this item consumes, never re-invents.
- **Collapse glyph precedent**: first-app 0260 (Disclosure) defines the
  `▸/▾` fold gesture for transcript items; sidebar section collapse is
  the same gesture applied to nav groups (0260's non-goals leave
  generalization open — cross-reference, no duplication).

## Problem
Navigation surfaces are string-formatted lists today: no sections, no
unread/attention counts, no collapse, browse-fires-navigation (0250),
and no count-bearing filter strip at all. Chat/triage UIs — the
brief's C class — cannot render their defining chrome without forking
List and Tabs.

## What we want
Two widgets, one selection philosophy (browse ≠ commit where a page
rebuild is at stake; commit-on-move where filtering is cheap):
1. **`NavList`**: a sectioned destination list.
   - Item model: `NavItem { key, label, badge: Option<BadgeSpec>,
     disabled }`; `NavSection { title, collapsible, items }` — badges
     are 0540 count/dot specs (unread counts, attention dots),
     rendered right-aligned, truncation eating the LABEL first, never
     the badge (`text::truncate_ellipsis` on the label span).
   - Selection: bound `Signal<String>` (key, not index — sections make
     indices lie); sticky across data refresh (List's selection_key
     semantics, list.rs:99-111).
   - Activation policy (RESOLVED, ruling §3 — commit-on-move is
     opt-in, never a default): arrows BROWSE (`on_select`
     notification only); `on_navigate(key)` fires on Enter/click —
     the domain alias for activate (ruling §5). `activate_on_move`
     exists as an opt-in knob ONLY where the committed act is cheap,
     idempotent, and non-destructive; for sidebars driving
     `dyn_view_scoped` pages the disposal cost (page state dies per
     arrow keystroke) is exactly why the default is OFF. Section
     headers fold on Enter/Space/click (`▸/▾` — a fold is a toggle,
     so the keys coincide per ruling §2), skipped by item movement
     when collapsed.
   - Compact mode: at widths under a threshold the list renders
     badge-only rows (the chat rail squeeze); explicit builder knob,
     no hidden breakpoints.
2. **`FilterTabs`**: a count-bearing tab strip WITHOUT panels.
   - Model: `FilterTab { key, label, count: Option<Signal<i32>> }`;
     active = `Signal<String>`; counts render via 0540 (`99+`,
     hide-zero option) and update reactively without rebuilding the
     strip (each count cell is its own fine-grained region — the
     tabs.rs:167-212 single-dyn-bar recipe, but per-tab dyn for
     counts).
   - Interaction: exactly Tabs' contract (one tab stop, Left/Right,
     click via span mapping, underline strip in `border_focus` —
     tabs.rs:76-84 token law) — switching writes the signal; the app
     re-queries its one surface. No panel mounting.
   - Overflow: when tabs exceed the width, scroll the strip (left/right
     ellipsis affordances) keeping the active tab visible — honest
     degradation, tested at narrow widths (the tabs.rs walk-off-rect
     gap closed).
3. **Composition doc**: sidebar + filter strip + content = the triage
   shell recipe (a `docs/` section with the signal wiring:
   nav key signal → page dyn; filter key signal → memo re-query — the
   compose.rs:87-99 derived-state cookbook applied).

## Scope / Non-goals
Scope: NavList, FilterTabs, compact mode, overflow, the composition
recipe, tests, validator adoption (0590 triage shell).
Non-goals: routing/history (the router ruling stands); breadcrumbs
(evidence-thin in the brief's classes — file managers may promote it
later); drag-reorder of nav items; Tabs deprecation (Tabs keeps the
panel-switching job it does well, tabs.rs:1-13 — FilterTabs is a
sibling, and the doc says when to use which).

## Expected outcomes
The dashboard's hand-rolled sidebar is replaceable by `NavList` (the
roadmap's deletion measure applied in-repo); the chat shell's channel
list with unread badges and its six filter tabs with live counts are
each one widget call; arrowing through nav never rebuilds pages unless
the app opted in.

## Validation
- Unit: key-sticky selection across section collapse and data refresh;
  collapsed sections skipped by movement; badge-preserving truncation;
  count reactivity without strip rebuild (signal write → one region
  re-renders — assert via render counting, the damage-economy
  discipline of `examples/dashboard/main.rs:6-8`).
- CaptureTerm acceptance: triage-shell wiring — switch filter tab →
  count updates + list re-queries while the FOCUS and scroll of the
  content survive (no remount); sidebar compact mode at narrow width;
  overflow strip keeps active tab visible at 40 cols.
- A11y: NavList reports List/ListItem with labels incl. badge values;
  FilterTabs reports Tabs/Tab with the active tab's label+count
  (`access_value` pattern, tabs.rs:158-164).

## Progress checklist
- [ ] NavList (sections, badges, sticky keys, collapse, compact mode)
- [ ] Activation policy per the 0250 ruling (browse ≠ commit;
      activate_on_move opt-in only — resolved cycle 2)
- [ ] FilterTabs (counts via 0540, overflow scroll, no panels)
- [ ] Composition recipe doc (nav + filter + content wiring)
- [ ] Acceptance + damage-economy tests; validator adoption
