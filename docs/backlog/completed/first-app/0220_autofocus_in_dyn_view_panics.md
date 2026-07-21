# Completed: `autofocus` inside a dyn_view regeneration panics the reactive runtime

## Metadata
- Created: 2026-07-21
- Status: Completed (wave cycle 1, STABILITY seat)
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None. ADR impact: none — this is a defect in the interaction
  of two shipped mechanisms, not a policy change.

## Context
`abstractcode-tui` (the first real app) mounted its composer as
`TextInput::new()...element(scx, &tokens).autofocus().build()` inside a
theme-keyed `dyn_view_scoped` — the exact composition the widgets gallery
teaches (theme dyn wrapping the widget tree) plus the documented initial-focus
mechanism (`Element::autofocus`, src/ui/view.rs:204-207). The app panicked at
mount, every time, in release and debug:

```
abstracttui reactive: dependency cycle — a computation re-entered itself
while running. FIX: a memo/effect (transitively) reads its own output; ...
  at src/reactive/execute.rs:45
```

## Current code reality
The chain (all file:line against abstracttui 0.1.0):
- A dyn_view's inner subtree mounts INSIDE the dyn's own effect run
  (`mount_view` frames nested in `Effect::new` in the panic backtrace).
- The regenerated-subtree autofocus path (src/ui/mount.rs:170-173) takes
  `pending_autofocus` and calls `set_focus` immediately — still inside the
  running dyn computation.
- `set_focus` dispatches `FocusIn` synchronously; `Element::focus_signal`'s
  handler (src/ui/view.rs:258-264, used by every focusable widget incl.
  TextInput at src/widgets/input.rs:207) does `Signal::set(true)`.
- The set triggers `maybe_flush` → `flush_effects` → `update_if_necessary` →
  `begin_run` on a node whose `running` flag is still set (the dyn effect that
  is mid-mount) → the dependency-cycle panic (src/reactive/execute.rs:45).

The INITIAL-mount path is safe by construction: `UiTree::mount` takes
`pending_autofocus` after the mount completes, outside any computation
(src/ui/tree.rs:325-336) — which is why `Modal::open` content (mounted via
`layer_tree`'s fresh `tree.mount`) can carry autofocus without crashing.
Only the dyn REGENERATION path (mount.rs:170) fires inside a running effect.

## Problem or opportunity
The two mechanisms are individually documented and both appear in the shipped
examples; their composition is a guaranteed mount-time panic. Any app that
wraps its widget tree in a theme dyn (the gallery's own pattern) and marks an
input autofocus (the palette pattern named in view.rs:204's doc comment) hits
it on frame one. There is no diagnostic pointing at autofocus — the panic
blames a "dependency cycle", which sent the first app down a false trail of
its own effects.

## Proposed direction
Defer the regen-path autofocus out of the running computation: instead of
calling `set_focus` inline at mount.rs:170-173, post it (the engine's own
posted-job machinery, or process it where the initial-mount path does — after
the effect run completes, e.g. alongside damage application in the frame
phase that already runs post-update). Alternatively, make `set_focus`'s
FocusIn/FocusOut dispatch defer signal writes when a computation is running.
Either way, add a regression test: a `dyn_view` whose regenerated content
carries `.autofocus()` must mount without panicking and end focused.

## Why it might matter
First-frame panics in the composition of two documented features are the
worst first-five-minutes experience an engine can offer; the workaround
(below) costs every app real machinery.

## Workaround in the field (delete when fixed)
abstractcode-tui removed `.autofocus()` from the composer, holds a
`UiTree` handle (`app.tree().handle()`) in its UI context, orders the
composer as the root's first focusable, and calls `focus_first()` after
mount + deferred via `reactive::after(Duration::ZERO, ...)` after every
theme switch (src/ui/mod.rs `UiCtx::focus_composer` in that repo).

## Promotion criteria
Reproduction is deterministic and the fix is engine-internal — promote
whenever an engine work cycle opens; no external evidence needed.

## Validation ideas
- Regression test in `src/app/acceptance.rs` or `src/ui/mount.rs` tests:
  theme-signal write forces a dyn regen whose subtree carries autofocus;
  assert no panic and focus lands on the node.
- The first app's composer can then return to plain `.autofocus()`.

## Non-goals
No redesign of focus routing (0230 covers the modal-focus story); no changes
to the initial-mount autofocus path, which is correct today.

## Completion report
- Final path: docs/backlog/completed/first-app/0220_autofocus_in_dyn_view_panics.md
- Date: 2026-07-21
- Root cause: exactly as this item diagnosed — the dyn regeneration path
  (src/ui/mount.rs:170-173 in 0.1.0) took `pending_autofocus` and called
  `set_focus` INSIDE the running dyn effect; the FocusIn handler's
  `Signal::set(true)` triggered `maybe_flush` → `begin_run` on the
  still-running node → the reactive "dependency cycle" panic
  (src/reactive/execute.rs:45). The initial-mount path was safe because
  `UiTree::mount` consumes the slot after the mount returns.
- Fix: the item's first proposed direction — the request now stays PARKED
  in `pending_autofocus` (the dyn mount effect no longer touches focus;
  its `request_frame` guarantees a frame). Delivery happens at the two
  points that run outside every computation: `UiTree::mount` (initial
  mount, unchanged behavior) and `UiTree::layout` (frame phase L; also
  the dispatch/draw entry paths, which call `layout()` first) via the new
  `deliver_pending_autofocus` (src/ui/focus.rs). A target disposed
  between park and delivery is dropped without blurring current focus;
  a re-park during delivery (FocusIn handler forcing another regen) is
  preserved because the slot is taken before `set_focus` runs. The
  now-unused `UiTree::from_core` was removed.
- Reproduction verified: the exact composition (theme-keyed
  `dyn_view_scoped` wrapping `TextInput...autofocus()`) panicked with the
  item's exact message against the pre-fix tree (stash A/B run) and
  passes with the fix.
- Regression tests: `tests/wave_stability.rs::
  autofocus_inside_dyn_view_regeneration_mounts_and_focuses` (mount +
  regeneration both land focus, no panic, focus points at the REGENERATED
  node) and `repeated_dyn_regenerations_keep_autofocus_deterministic`
  (three generations, each ends focused on a live instance that consumes
  keys).
- Field workaround: abstractcode-tui's `UiCtx::focus_composer` machinery
  can be deleted; plain `.autofocus()` works again (validation idea 2).
