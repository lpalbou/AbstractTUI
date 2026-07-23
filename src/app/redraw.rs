//! Public full-redraw verb + focus-regain repaint opt-in (backlog
//! first-app/0299).
//!
//! The damage contract trusts the terminal to keep every cell the
//! engine ever painted. When that assumption breaks EXTERNALLY —
//! Cmd+K in Terminal.app, `printf '\033c'` from a stray process, an
//! emulator glitch — model-side damage cannot heal the screen: a
//! repaint that produces byte-identical cells emits NOTHING (the diff
//! correctly suppresses equal cells), so the loss is permanent. Only
//! the driver's "screen is unknown" resync repairs it: poison the
//! previous-frame model, invalidate the presenter's cursor/pen,
//! damage every layer, re-place protocol images — the pair resize and
//! suspend-resume already run.
//!
//! [`request_full_redraw`] is the component-reachable form of that
//! verb (the Ctrl+L class): a thread-local flag the driver drains at
//! its next turn's phase U — the `mouse_capture()` request shape,
//! without the handle. [`set_redraw_on_focus_gained`] opts into the
//! same resync whenever the terminal reports focus-in (DEC 1004): an
//! externally-cleared terminal is nearly always followed by a focus
//! round-trip before the user looks again, so the failure heals
//! without a keybinding. Default OFF — a full-frame emission per
//! focus-in is bounded and human-paced, but it is real byte cost
//! (tmux pane switches fire focus events constantly), so existing
//! sessions stay byte-identical unless the app opts in.
//!
//! OWNER: REACT (verb surface) with KERNEL (driver drain).

use std::cell::Cell;

use crate::reactive::request_frame;

thread_local! {
    /// Pending full-redraw request, drained once per driver turn.
    static FULL_REDRAW: Cell<bool> = const { Cell::new(false) };
    /// Policy: resync on terminal focus-in (DEC 1004 FocusGained).
    static ON_FOCUS_GAINED: Cell<bool> = const { Cell::new(false) };
}

/// Repaint everything from scratch on the next frame — the Ctrl+L
/// verb. Use it when the TERMINAL's content can no longer be trusted
/// (an external clear, a corrupting burst from another process):
/// unlike `request_repaint`/tree invalidation — which damage the
/// MODEL and re-emit only cells whose bytes changed — this poisons
/// the engine's previous-frame model and re-anchors the presenter, so
/// the next frame re-emits EVERY cell and re-places protocol images.
///
/// Callable from any component handler or posted job on the app
/// thread; the driver drains the request at its next turn (a call
/// from a key handler is honored within the same turn). Bounded cost:
/// one full-frame emission, then idle returns to zero bytes.
pub fn request_full_redraw() {
    FULL_REDRAW.set(true);
    // A request from a posted job must wake an idle loop; from an
    // event handler this is a no-op frame the turn was running anyway.
    request_frame();
}

/// Driver drain (once per turn, phase U): the pending request, if any.
pub(crate) fn take_full_redraw_request() -> bool {
    FULL_REDRAW.replace(false)
}

/// Opt into a full redraw whenever the terminal reports FOCUS-IN
/// (DEC 1004 — backlog first-app/0299 ask 2). An externally-cleared
/// terminal is nearly always followed by a focus round-trip, so the
/// damage heals silently, without a keybinding. Costs one full-frame
/// emission per focus-in (bounded, human-paced); default OFF so
/// existing sessions stay byte-identical. Terminal focus events are
/// otherwise dropped by routing (they are distinct from widget
/// focus), so enabling this consumes nothing an app could observe.
pub fn set_redraw_on_focus_gained(on: bool) {
    ON_FOCUS_GAINED.set(on);
}

/// The current focus-regain policy (see [`set_redraw_on_focus_gained`]).
pub fn redraw_on_focus_gained() -> bool {
    ON_FOCUS_GAINED.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_one_shot_and_drained() {
        assert!(!take_full_redraw_request(), "clean start");
        request_full_redraw();
        assert!(take_full_redraw_request(), "request observed");
        assert!(!take_full_redraw_request(), "drain is one-shot");
    }

    #[test]
    fn focus_policy_defaults_off_and_toggles() {
        assert!(!redraw_on_focus_gained(), "default off (0299 ask 2)");
        set_redraw_on_focus_gained(true);
        assert!(redraw_on_focus_gained());
        set_redraw_on_focus_gained(false);
        assert!(!redraw_on_focus_gained());
    }
}
