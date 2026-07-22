# Completed: stale frame band above the live frame after a workflow-picker close (resize suspected)

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed — rendering defect report, live maintainer screenshot)
- Completed: 2026-07-22

## ADR status
- Governing ADRs: damage contract (docs/design/01-damage-contract.md).
  ADR impact: none if this is a missed-damage bug; the contract already
  promises truth.

## Context
Live maintainer report (2026-07-22, macOS, tabbed terminal, very tall
window, abstractcode-tui on engine 0.2.1): immediately after choosing a
new workflow in the `/workflow` picker (List-in-Modal, `on_activate`
closes the modal), the screen showed TWO stacked frames:

- absolute top: a STALE header row (`▲ AbstractCode  coder · endpoint:…`)
  from before the switch, followed by a band of blank rows;
- from roughly mid-screen: the complete LIVE frame (header now naming
  `basic-agent`, centered empty-state, status bar on the true bottom
  row) — fully correct in itself, just vertically offset.

The live frame occupied about the height of a typical previous window
size, anchored to the BOTTOM of the terminal; the stale band sat above.
This strongly suggests a viewport RESIZE (window grown taller, or a
terminal-tab reflow) was involved around the modal close: the engine
re-laid-out at some size while rows the previous frame owned were never
cleared/damaged, or the paint origin and the terminal's post-reflow
content offset disagreed.

## What the app does on that path (for reproduction)
- `/workflow` opens a `Modal` (overlay layer); `List::on_activate` sets
  a store signal, saves prefs, calls `close_modal` (layer removed
  synchronously, scope disposal deferred one tick) — all app-side state
  is signal-driven; the app never writes to stdout/stderr mid-session.
- The main tree is one full-viewport column inside a theme
  `dyn_view_scoped`; the transcript is Feed-in-Scroll.

## Problem
Whatever the trigger, a half-stale screen violates the damage contract's
truth promise. If resize is the trigger: a resize must damage the WHOLE
new viewport (including rows the old layout never owned) and re-anchor
the paint origin; if terminal reflow moves alternate-screen content, the
next frame must repaint from absolute (1,1).

## Repro attempts requested
Engine-side: drive a resize (grow rows) while a modal overlay is open,
close the modal via a List on_activate, assert the full screen repaints
(CaptureTerm + VtScreen at two sizes should catch a stale top band).
Also worth checking: `current_viewport()` staleness inside modal layers
across a resize, and whether layer removal damages only the modal rect
while a resize invalidated the base layer's geometry.

## App-side state
No app workaround shipped (none is clean — the app cannot know the
screen is stale). If a "force full repaint" verb exists or lands, the
app could call it defensively after resize; better is the engine fixing
the damage source.

## Completion report (2026-07-22, cycle-3 fix wave)

Root cause found by the house method — headless repro FIRST, then the
mechanism. The damage side was NOT the bug; the PRESENTER side was.

- **What was already correct** (verified by the new suite before the
  fix, against a garbage-prefilled referee): `Driver::apply_resize`
  resizes + `damage_all`s the root layer (`Overlays::ensure_root`),
  damage-alls the tree (`App::set_viewport`), resizes frame/prev and
  POISONS `prev` — so the first post-resize frame re-emits every CELL
  of the new viewport. Overlay retire damage is also sound:
  `LayerHandle::remove` damages the root under the layer's CURRENT
  bounds clipped to the root's CURRENT bounds (src/app/overlays.rs),
  and any same-turn resize collapses root damage to full anyway
  (`Surface::damage_all` on resize). The scroll optimization cannot
  misfire across a resize: poisoned rows share fingerprints but can
  never equal a real row (the poison colors are impossible by
  convention), so `detect_shift` finds no candidates and the plain
  full emission runs. The Modal has no dim/veil layer.
- **The actual defect** (src/app/driver.rs `apply_resize`,
  pre-fix lines ~700-728): the resize path repaired the CELL model
  (prev poison) but not the PRESENTER model. The presenter's virtual
  cursor still held the previous frame's park position
  (bottom-left of the OLD size, `Presenter::park_cursor`), and
  `move_cursor` (src/render/present.rs:351-373) emits RELATIVE motion
  when row or column matches — so the first post-resize run
  (row 0, col 0: same column as the park) went out as `CUU old_h-1`
  from wherever the emulator's reflow actually left the physical
  cursor. macOS Terminal's bottom-anchored growth (the field
  incident) moves the cursor down by the grown rows, so the first
  run painted offset by exactly that delta — the previous frame's
  band stayed visible where the repaint never landed, matching the
  screenshot (stale header band above, live frame anchored low).
  The item's own contract line named the missing half: "the next
  frame must repaint from absolute (1,1)". `boot::player` already
  invalidated its presenter on splash resize; the driver did not.
- **The fix (one line + rationale comment)**: `apply_resize` now calls
  `self.presenter.invalidate()` beside the prev-poison — cursor AND
  pen forgotten, so the post-resize frame opens with absolute CUP and
  a reset-based SGR. Both halves of "the screen is unknown after a
  resize" now travel together: cells (poison) and cursor/pen
  (invalidate). Steady-state frames keep their relative-motion byte
  economy untouched; the invalidate costs a few bytes once per resize.
- **Regression suite** (tests/adv_resize_modal.rs, real Driver +
  CaptureTerm + VtScreen): every interleaving of {resize, List-in-Modal
  close via on_activate} — resize-then-close (taller/shorter/wider/
  narrower), close-then-resize (both directions), resize+close in the
  SAME turn, shrink-then-grow-then-close — asserts the FINAL screen
  cell-for-cell (glyph + full paint) equals a fresh-driver oracle
  rendering the same state at the final size. The referee models the
  honest post-resize worst case: a garbage-prefilled grid ('X' on
  magenta) with the cursor moved to the new bottom-left (the reflow
  ghost). Byte-level pin `first_post_resize_frame_re_anchors_absolutely`
  asserts the first cursor motion of every post-resize frame is CUP,
  never CUU/CUD/CUF/CUB/CR. Four of six scenario tests FAILED before
  the fix (stale cells exactly where the offset first run landed);
  all seven tests green after. `docs/design/render.md` §2.4 now
  documents invalidate-on-resize beside invalidate-after-external-
  writes.
- Whole-tree battery after the fix: 1470 tests green (52 suites),
  clippy zero, fmt clean, alloc pins green, `cargo semver-checks`
  clean vs 0.2.1 (no API change — driver-internal).
