# Proposed: selection region lingers after the release-copy — `c`/Enter keep being swallowed

## Metadata
- Created: 2026-07-22
- Status: Proposed (UX footgun report — first-app finding, 0.2.0 adoption wave)
- Completed: N/A

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
