# Completed: Modal content shortcuts are dead until focus enters the modal tree

## Metadata
- Created: 2026-07-21
- Status: Completed (wave cycle 1, STABILITY seat)
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None. ADR impact: none — a behavioral gap in Modal's focus
  story, not a policy change.

## Context
`abstractcode-tui`'s tool-approval modal registers `a` (approve), `d` (deny)
and `Esc` as shortcuts on its content root — the same shape as the shipped
dashboard example's help panel (examples/dashboard/main.rs:671-690, Esc/`?`
shortcuts on the panel content). With the modal open, none of those keys did
anything until the user pressed Tab first. In a tool-approval flow that reads
as "the app ignored my approval".

## Current code reality
- `Modal::open` builds `panel = Element ... .focus_trap() .child(content)`
  and mounts it as a fresh overlay tree (src/app/popups.rs:54-81 →
  `Overlays::layer_tree`, src/app/overlays.rs:182-206). Nothing focuses
  anything in that tree — `layer_tree` calls `tree.mount` and
  `pending_autofocus` is only taken when the CONTENT carries an explicit
  `.autofocus()`.
- Modal layers swallow every key: `overlays.rs:344-346` returns
  `tree.dispatch(event)` unconditionally for key/paste events on a modal
  tree.
- Key dispatch inside a tree targets `focus.or(root)` (src/ui/tree.rs:519-521)
  and the shortcut phase consults only elements ON THE PATH root→target
  (tree.rs:562-582). With nothing focused, the target IS the panel root, the
  path is `[panel]`, and shortcuts on the CONTENT element (the panel's child)
  are unreachable.
- Consequence: every `Modal` whose content root carries shortcuts — including
  the flagship dashboard's help panel — has dead keys until the user tabs
  focus into the modal. The dashboard masks it because `?` is also handled…
  nowhere else; its Esc/`?` inside the open modal are dead too (verified by
  reading the dispatch path; the live smoke closes it by other means).

## Problem or opportunity
A focus-trapped modal that starts with nothing focused contradicts the
pattern's promise ("input is fully owned while open, Tab cycles inside" —
popups.rs doc). Every consumer must discover the `.focusable().autofocus()`
workaround on the content root, and 0220 makes autofocus radioactive in dyn
contexts, so consumers may reasonably fear the safe variant too.

## Proposed direction
`Modal::open` should establish initial focus in its own tree right after
mount: `tree.focus_first()` when the content carried no autofocus node
(autofocus, when present, already wins via `tree.mount`). One line in
`layer_tree` callers or in `Modal::open` itself, plus a regression test:
open a modal whose content root has a shortcut and no focusable children;
send the chord; assert it fired. Consider also documenting (or changing)
the shortcut-path rule — "shortcuts live on the focus path" is the deeper
surprise here and deserves a line in docs/api.md's app section.

## Why it might matter
Approval/confirm modals are the highest-stakes surfaces a TUI has; keys that
silently do nothing there erode exactly the trust the engine's honesty
posture builds.

## Workaround in the field (delete when fixed)
abstractcode-tui marks every modal content root `.focusable().autofocus()`
(safe on the Modal initial-mount path) and autofocuses the List inside its
pickers so arrow keys work immediately (src/ui/modals.rs in that repo).

## Promotion criteria
Deterministic reproduction; engine-local fix; promote with the next engine
work cycle. Pairs naturally with 0220 (both are focus-initialization gaps).

## Validation ideas
- Unit/acceptance: modal with root shortcut, no focusables → chord fires
  without Tab. Modal with a TextInput carrying autofocus → input focused,
  root shortcuts still fire (path includes root).
- Re-run the dashboard live smoke asserting Esc closes the help modal.

## Non-goals
No change to the modal key-swallow rule (correct); no global shortcut
registry (Actions already exists for app-global chords).

## Completion report
- Final path: docs/backlog/completed/first-app/0230_modal_shortcuts_dead_until_focus.md
- Date: 2026-07-21
- Root cause: as diagnosed — `Overlays::layer_tree` mounted modal trees
  with nothing focused; key dispatch targets `focus.or(root)` and the
  shortcut phase walks only the root→target path, so with no focus the
  path was `[panel]` and every shortcut on the CONTENT element (the
  panel's child) was unreachable until Tab moved focus in.
- Fix (the ruling, encoded): opening a modal moves keyboard ownership to
  the modal tree immediately. `UiTree::focus_init` (src/ui/focus.rs)
  runs from `Overlays::layer_tree` for `modal = true` trees (so
  `Modal::open` AND direct modal `layer_tree` callers both get it):
  1. an autofocus node focused at mount wins (no-op); 2. else the first
  focusable in document order; 3. else the root's FIRST CHILD — the
  content element of a panel/content composition — as a programmatic
  focus anchor so the content's shortcuts sit on the dispatch path from
  frame one (programmatic focus does not require focusability; Tab moves
  on normally); 4. a childless root anchors on the root. Non-modal
  overlay trees stay unfocused (the cycle-5 rule: only a FOCUSED
  non-modal overlay owns keys) — ruling documented on `layer_tree`.
  Support surface added: `LayerHandle::tree()` exposes a live handle to
  a tree layer's `UiTree` (needed by tests and useful to apps; `None`
  for draw/manual layers).
- Reproduction verified: a modal whose content root carries an `a`
  shortcut and no focusables ignored the chord against the pre-fix tree
  (stash A/B run: fired 0) and fires on frame one with the fix.
- Regression tests: `tests/wave_stability.rs::
  modal_content_shortcut_fires_without_tab` (full driver path: bytes →
  overlay routing → modal tree; chord fires without Tab) and
  `modal_autofocus_wins_and_root_shortcuts_stay_reachable` (autofocus
  input owns typing from frame one AND the root's Escape still fires via
  the path). `src/ui/mod.rs::tests::
  focus_init_prefers_autofocus_then_focusable_then_content_anchor` pins
  the three-step policy at unit level.
- The item's "consider documenting the shortcut-path rule in
  docs/api.md" — docs/api.md is not this seat's file this wave; filed in
  reviews/wave/stability-cycle1.md for the integrator.
- Field workaround: abstractcode-tui's `.focusable().autofocus()` on
  every modal content root can be deleted.
