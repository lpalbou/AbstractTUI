# 0570 — Tree view: virtualized hierarchy with keyed nodes + lazy children

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: nothing in-band (independent; 0540 badge slots are
  optional rendering). The 0250 ruling governs its event split.
- Validator (0590): `examples/triage_shell` (the notes-outline panel);
  a file-manager triptych validator remains the fuller exercise if a
  fourth validator is ever added.
- Activation semantics: per the 0250 ruling
  (reviews/study/platform-on-appkits.md "The 0250 ruling") — branch
  fold is a TOGGLE (Enter/Space coincide on branches, ruling §2); leaf
  Enter activates; §4 encodes it.
- 0170 coordination (PLATFORM cycle-2 F9): `Role` derives no
  `#[non_exhaustive]` (src/ui/access.rs:30-31), so adding
  `Tree`/`TreeItem` is technically breaking for downstream exhaustive
  matches — either `Role` gains `#[non_exhaustive]` in the 0.2
  budgeted batch (control-plane serializers want that too) or these
  variants ride that same batch. Named here so the item cannot land
  "additively" by accident.
- Promotion trigger: the file-manager or smart-note validator (0590)
  starting; or any product needing a browsable hierarchy (camera/object
  libraries, note outlines, grouped admin entities).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (new widget). One design ruling to record at
  implementation: flatten-to-list rendering (proposed) vs. nested
  elements — the item argues flatten below.

## Context
Hierarchy is the defining surface of two brief classes and appears in
two more: file managers browse directory trees of connected-camera /
3D-object libraries (D), the smart-note app's outline IS a tree of
thoughts/tasks/questions (D), the admin console groups entities under
providers/tenants (A), and chat sidebars group channels under
workspaces (C — shallow, but the same shape 0550's sections only
partially cover: sections are one level, trees are N). The engine has
no tree widget, and — unusually — no partial substitute either: `List`
is flat, 0260's `Disclosure` is a single fold whose own non-goals
say "no tree-view generalization (nested disclosures compose naturally
but are not designed for here)" (0260 "Non-goals"). The generalization
is therefore explicitly unowned; this item owns it.

## Current code reality
- **No tree/expand/hierarchy widget**: verified against the module list
  (`src/widgets/mod.rs:19-40`) and by grep (TreeView/tree_view/
  splitter/divider: zero matches in src/).
- **`List` owns the machinery a tree should ride, not fork**:
  virtualization with variable heights (prefix sums + binary search),
  sticky `selection_key`, `scroll_to` (`src/widgets/list.rs:1-13,
  99-120`). A tree IS a flat list of the *visible* (expanded) nodes —
  the classic flatten: expand/collapse edits the flattened window,
  virtualization stays one-dimensional. But `List` items are `String`s
  drawn on the first row only (list.rs:54-55,11-13) — the flatten
  cannot render indent/glyph/badge decorations through the current
  item type, so the tree either extends List's item model or owns its
  flatten + draws rows itself (the Table precedent: own draw closure,
  `src/widgets/table.rs:266-367`).
- **Disclosure semantics precedent**: 0260 defines the fold gesture
  (`▸/▾`, Enter/Space/click toggles, collapsed = 1 row, lazy content
  on first expand — the Tabs lazy-panel pattern, 0260 "Proposed
  direction" §1). The tree adopts the same gesture per node so the
  two widgets feel identical.
- **Activation semantics gap applies here too**: `List::on_select`
  fires on movement (0250); a file tree must distinguish
  browse (selection moves), expand/collapse (fold state), and OPEN
  (activation — Enter on a leaf) — three events, of which the engine
  vocabulary currently has one, and it fires wrong.
- **A11y vocabulary is missing a role**: `Role` has List/ListItem/
  Menu/MenuItem but no Tree/TreeItem (`src/ui/access.rs:31-51`) — the
  item adds them; NOT additive in practice (`Role` is public and not
  `#[non_exhaustive]`) — see the metadata's 0170-coordination line for
  the resolved handling.
- **Height/scroll integration**: `Scroll` measures mounted content or
  takes reactive extents (scroll.rs:11-24); the tree exposes
  `total_rows()`-style reactive height exactly as `Feed` does
  (`src/widgets/feed.rs:50-56` — the signal-written-outside-draw
  discipline documented there applies verbatim to expand/collapse
  height changes).

## Problem
Hierarchical data cannot be browsed: no indent model, no fold state, no
lazy loading for large directories, no keyed identity that survives a
refresh (a file tree refreshes constantly), and no three-way
browse/fold/open event split. Every consumer would fork List and
re-derive prefix-sum math around fold state — the 0260 report already
documents the per-app height-remeasure bug this produces.

## What we want
1. **Data model**: `TreeNode { key, label, badge: Option<BadgeSpec>,
   kind_glyph: Option<char>, has_children: HasChildren }` where
   `HasChildren::{No, Yes, Unknown}` — `Unknown` renders the expand
   affordance and triggers `on_expand(key)` for LAZY loading (the app
   fills children asynchronously; a spinner glyph rides the node until
   the app writes them; directories with unscanned contents are the
   motivating case). The app owns the tree data in a signal; fold
   state is likewise APP-OWNED and the widget CONSUMES the bound
   signal (`expanded: Signal<HashSet<String>>` — wording per PLATFORM
   cycle-2 F9: a bindable signal is the app's state, which is exactly
   what makes "collapse all" one write AND makes fold state a
   registrable control-plane 0340 key; the 0260 external-control rule
   generalized).
2. **Flatten + virtualize**: visible-node flatten with per-node depth,
   prefix-sum windowing (List's math, generalized in place or shared);
   10k visible nodes draw one screenful (the Feed/List standard,
   feed.rs:5-7). Reactive `total_rows()` for `Scroll` composition;
   fold toggles update it synchronously at known width (the feed.rs
   width-fixup discipline if width-dependent heights ever appear —
   v1 rows are height 1).
3. **Row rendering**: indent guides (`│ ├ └` optional, plain-space
   default), fold glyph (`▸/▾`), kind glyph slot, label
   (cluster-safe truncation), right-aligned 0540 badge slot —
   drawn by the widget (Table's own-draw precedent) so no List fork.
4. **Interaction (three events, explicit)**: Up/Down browse
   (`on_select`, movement semantics documented); Right expands /
   moves to first child, Left collapses / moves to parent (the
   universal tree keys); Enter/Space on a branch toggles fold, on a
   leaf fires `on_activate(key)`; click: fold glyph toggles, row
   selects, second click on the selected row activates (mirrors 0530's
   ruling; no double-click event exists — `src/ui/event.rs:95-105`).
   Callbacks disposal-safe (widget bookkeeping before user callbacks —
   the 0250 crash class).
5. **Identity discipline**: everything keyed by `key: String`
   (selection, expansion, lazy-load requests); a data refresh keeps
   selection + fold state for surviving keys and drops vanished ones
   silently (counted, exposed as a signal for apps that care —
   labeled degradation, roadmap principle 4).
6. **A11y**: `Role::Tree`/`Role::TreeItem` (new), `access_value` =
   "label, depth N, expanded/collapsed/leaf, X of Y" — the tree is
   keyboard-first by construction.

## Scope / Non-goals
Scope: model, flatten/virtualization, row draw, three-event
interaction, lazy children, keyed persistence, a11y roles, tests,
validator adoption (the 0590 triage-shell notes outline; a
file-manager triptych is the fuller exercise if a fourth validator is
added).
Non-goals: multi-select (promote from 0530's pattern when a validator
demands bulk file ops); drag-and-drop re-parenting; inline rename
editing (an app overlays a TextInput; a packaged affordance can follow
evidence); columns-in-tree (tree-table hybrid — real but rare; needs
its own item if a validator hits it); checkbox trees.

## Expected outcomes
A directory browser is `Tree::new(nodes).expanded(sig).on_expand(load)
.on_activate(open)` — lazy scan, fold persistence, refresh-stable
selection. The smart-note outline and admin entity groups ride the
same widget. 0260's "nested disclosures" pressure has a designed home
instead of N compositions.

## Validation
- Unit: flatten math under fold toggles (prefix sums stay consistent —
  property test over random fold sequences); keyed selection/expansion
  surviving refresh with insertions/deletions; lazy-load protocol
  (Unknown → on_expand fires once → children arrive → glyph settles);
  Left/Right semantics at root/leaf edges.
- CaptureTerm acceptance: browse a three-level lazy tree keyboard-only
  (expand, load, collapse, activate a leaf); 10k-node expanded tree
  scrolls with one-screenful draws (damage-proportional — assert the
  window, the feed.rs:5-7 standard); indent guides render; theme
  switch restyles.
- Regression: an `on_activate` that disposes the surrounding scope
  must not panic (0250's test, tree edition).

## Progress checklist
- [ ] Node model + HasChildren::Unknown lazy protocol
- [ ] Flatten + prefix-sum virtualization + reactive total_rows
- [ ] Row draw (indent guides, glyphs, badge slot, truncation)
- [ ] Three-event interaction + disposal-safe callbacks
- [ ] Keyed persistence across refresh (counted drops)
- [ ] Role::Tree/TreeItem + access values
- [ ] Property + acceptance tests; validator adoption
