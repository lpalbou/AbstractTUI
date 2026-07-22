# Completed: selection region lingers after the release-copy — `c`/Enter keep being swallowed

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed — UX footgun report, first-app finding, 0.2.0 adoption wave)
- Completed: 2026-07-22

## ADR status
- Governing ADRs: None. ADR impact: none — input-routing semantics of the
  selection layer.

## Context
`abstractcode-tui` enabled always-on drag selection (0270 tier 3) the way
`examples/feed.rs` demonstrates. The app is composer-centric: after any
drag-select the user's next act is usually TYPING. Live behavior: the mouse
RELEASE already copies (`SelectionAct::Copy`, "release copies; region stays
visible" — src/app/selection.rs:275-284), but the region stays visible, and
while any region is visible the key hook consumes `Esc`, `Enter`, `c`, and
`Ctrl+C` (selection.rs:288-305). Copy does NOT clear the region. Net effect:
after a drag, typing "cargo check" into the focused composer loses both
`c`s (each swallowed as a re-copy of the same region) and Enter submits
nothing (swallowed as another copy) until the user clicks or presses Esc.

## Current code reality
- `Event::Key` handling passes only when `st.region.is_none()`
  (selection.rs:289-292); `KeyCode::Enter` and `Char('c')` (bare or Ctrl)
  return `SelectionAct::Copy` (selection.rs:300-303), and no path clears the
  region on copy — only Esc and a fresh click clear.
- The release path ALREADY copied, so the retained region's remaining value
  is "copy the same thing again" — which is what it costs every subsequent
  `c`/Enter keystroke.

## Problem or opportunity
For read-mostly apps (the dashboard shape) the retained region is harmless.
For any app with a text input (the transcript shape — the engine's own
`examples/transcript.rs` composer), the retained region turns the two most
common keys into silent no-ops. The first typed character after a selection
is eaten with no feedback.

## Proposed direction (engine's call)
- Make the key-copy one-shot: `Enter`/`c` copy AND clear (the release
  already copied, so key-copy is a deliberate second act — clearing after
  it matches user intent), or
- clear the region on the first key event that is NOT a selection key
  (pass the key through after clearing), or
- expose the retention as a policy knob on `Selection`.

## App-side workaround to delete when this lands
`abstractcode-tui src/ui/chrome.rs` composer `on_change`: clears a lingering
region on the first composer text change. HONESTY CORRECTION (second
adversary pass, same day): this mitigation is INEFFECTIVE for exactly the
swallowed keys — the selection layer consumes `c`/Enter BEFORE tree
dispatch (`app/driver.rs:595-600`), so `on_change` never fires for them.
Only a non-selection key (any letter but `c`, etc.) reaches the widget and
clears the region; typing "check…" still loses every LEADING `c`, and
Enter-to-submit is fully eaten until some other key lands first. There is
NO effective app-side workaround; the fix must be engine-side.

## Completion report (2026-07-22, cycle-3 fix wave)

Shipped the strongest of the proposed directions, uniformly: **every
copy ends the gesture** — release-copy AND the mid-drag key-copies
(Enter / `c` / Ctrl+C) clear the region together with the copy.

- **Why not key-copy-one-shot alone** (the item's first option): with
  the region retained after release, the FIRST typed `c` or Enter
  after any drag is still swallowed as a redundant re-copy — the
  reported composer failure survives at one-keystroke strength, and
  the engine's own transcript shape (`examples/transcript.rs`
  composer) hits it. The release already copied, so the retained
  region's only remaining power was eating keys; clearing on copy
  loses nothing of value. The clear-on-non-selection-key option keeps
  `c`/Enter as selection keys and fails the same way; the policy-knob
  option defers the decision to every app — footgun by default.
- **Mechanics** (src/app/selection.rs + src/app/driver.rs):
  `SelectionAct::Copy` now CARRIES the `Region` (crate-internal enum) —
  the layer takes its own state (`clear_locked`) BEFORE answering, so
  no post-copy region can exist by construction; the driver extracts
  the carried region's screen text from the last composed frame and
  queues the OSC 52 payload (`Selection::active_region` +
  `Driver::queue_selection_copy` deleted — unreachable states removed
  rather than guarded). The highlight cells recompose from truth on
  the copy frame (the existing painted-rect repair damage). Esc still
  cancels a LIVE drag without copying (and the release after an
  Esc-cancelled drag copies nothing — pinned); a fresh click
  re-anchors; wheel/motion/other keys route normally throughout;
  Ctrl+C with no visible region stays the default quit.
- **Semantics after the fix**: a region is visible only mid-drag, so
  the copy/clear keys exist only mid-drag; after the release the app
  owns every key immediately. Public API unchanged
  (`cargo semver-checks` clean); `Selection::is_active` now reads
  false after release — documented on the method.
- **Tests**: unit (src/app/selection_tests.rs)
  `selection_claims_left_drag_only_wheel_and_buttons_pass` (release
  copies AND clears; post-copy Enter/`c` pass; a click's paired
  release copies nothing) and
  `copy_keys_are_one_shot_and_exist_only_while_a_region_is_visible`
  (each of `c`/Ctrl+C/Enter copies once then passes; Esc cancels
  without copying). Integration (tests/adv_selection.rs, real Driver +
  SGR mouse bytes + VtScreen + OSC 52 capture): new
  `release_copy_frees_enter_and_c_for_the_app` — after a release-copy
  the very next `c` and Enter reach tree dispatch, the clipboard does
  NOT change (no silent re-copy), and wheel still reaches the tree;
  `release_copies_multi_row_and_clears_highlight` (highlight clears
  itself on the copy frame, cells restored);
  `ctrl_c_copies_with_selection_and_quits_without` (the copy clears,
  so the NEXT Ctrl+C quits — no click/Esc dance). Docs: api.md
  selection section rewritten with an explicit key table;
  `examples/feed.rs` comment updated.
- Whole-tree battery: 1470 tests green (52 suites), clippy zero, fmt
  clean, alloc pins green, semver-checks clean vs 0.2.1.
- The abstractcode-tui `on_change` mitigation can now be deleted: the
  engine clears on every copy, and no keys are swallowed post-release.
