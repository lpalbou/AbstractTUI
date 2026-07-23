//! Click-chain synthesis: multi-click counts from raw press events.
//!
//! Terminals report only raw SGR press/release — no terminal tells an
//! application "that was a double-click". The ENGINE synthesizes the
//! count: [`ClickChain`] is the pure state machine (same button, within
//! a time window, within a cell tolerance → the count grows), and the
//! tree embeds one per [`UiTree`](super::UiTree) so every mouse-Down
//! handler can read the press's chain position via
//! [`EventCtx::click_count`](super::EventCtx::click_count) — widgets
//! (`Table`) and hand-rolled rows alike, one tracker, one policy.
//!
//! ## Where time comes from (the no-wall-clock rule)
//!
//! The chain needs a clock, and `UiTree::dispatch` deliberately has no
//! time parameter. Time arrives AMBIENTLY: [`set_event_time`] publishes
//! the input-dispatch timestamp (thread-local, like the reactive
//! runtime's frame requester), and the driver writes it every turn from
//! its own injectable clock (`Driver::set_clock`) — so one injected
//! clock drives animations, timers, AND click chains in tests. Without
//! a published time (a bare `UiTree` driven directly, no
//! `set_event_time` call), presses stay ISOLATED: every Down counts 1,
//! deterministically. The chain never falls back to `Instant::now()` —
//! an implicit wall clock would make direct-dispatch tests
//! wall-time-dependent (two quick programmatic clicks silently becoming
//! a double-click on a fast machine, staying single on a loaded CI
//! runner — the flake class this rule exists to prevent).
//!
//! ## Semantics (the web convention, cell-grid edition)
//!
//! A double-click DELIVERS BOTH PRESSES normally — click 1 is an
//! ordinary press (selection happens), click 2 is an ordinary press
//! that ADDITIONALLY carries `click_count() == 2`. Nothing is delayed
//! or suppressed waiting for a possible second click. The chain resets
//! on: window exceeded, position beyond tolerance (Chebyshev cell
//! distance), a different button, any wheel event (content scrolls
//! under the pointer — the "same cell" is a different row now), any
//! drag (the gesture became something else), and a selection-layer
//! gesture claim (`UiTree::cancel_pointer_press`). Modifiers do NOT
//! break the chain (Shift+click then click chains — the DOM `detail`
//! behavior). Counts saturate at `u8::MAX`; a triple-click reads 3.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::base::Point;

use super::event::{MouseButton, MouseEvent, MouseKind};

/// Default chain window: X11/GNOME multi-click tradition (~400 ms;
/// macOS/Windows system defaults sit near 500 ms). Configurable via
/// [`ClickChain::window`].
pub const DEFAULT_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// Default position tolerance in cells (Chebyshev distance — the max of
/// the per-axis deltas). 1 cell absorbs the physical wiggle of a real
/// double-click (terminal cells quantize sub-cell motion away; browsers
/// use a few PIXELS of slop, which is sub-cell here). Consumers whose
/// logical target is a ROW should still guard cross-row chains — see
/// `Table`'s already-selected guard.
pub const DEFAULT_CLICK_TOLERANCE: i32 = 1;

#[derive(Copy, Clone, Debug)]
struct ChainState {
    at: Instant,
    pos: Point,
    button: MouseButton,
    count: u8,
}

/// The multi-click state machine. Pure over the caller's clock: no
/// internal time source, no globals — feed it `(now, event)` pairs and
/// it answers the press's chain count. The tree embeds one per
/// `UiTree`; apps synthesizing clicks OUTSIDE tree dispatch (custom
/// input paths) can embed their own with their own clock and policy.
#[derive(Debug)]
pub struct ClickChain {
    window: Duration,
    tolerance: i32,
    state: Option<ChainState>,
}

impl Default for ClickChain {
    fn default() -> Self {
        ClickChain::new()
    }
}

impl ClickChain {
    /// A chain with the default window (400 ms) and tolerance (1 cell).
    pub fn new() -> ClickChain {
        ClickChain {
            window: DEFAULT_CLICK_WINDOW,
            tolerance: DEFAULT_CLICK_TOLERANCE,
            state: None,
        }
    }

    /// Maximum time between chained presses (inclusive: a press landing
    /// EXACTLY `window` after the previous one still chains).
    pub fn window(mut self, window: Duration) -> ClickChain {
        self.window = window;
        self
    }

    /// Maximum Chebyshev cell distance between chained presses
    /// (inclusive). `0` = exact same cell.
    pub fn tolerance(mut self, cells: i32) -> ClickChain {
        self.tolerance = cells.max(0);
        self
    }

    /// Fold one mouse event at time `now`. Returns the chain count for
    /// `Down` events (1 = isolated press, 2 = double-click's second
    /// press, 3 = triple's third, saturating at 255) and 0 for every
    /// other kind. Wheel and drag events RESET the chain; `Up`/`Move`
    /// leave it untouched (motion between the presses of a double-click
    /// is normal — the tolerance check at the next press decides).
    /// Modifiers are ignored (chain identity is button + time + place).
    pub fn observe(&mut self, now: Instant, ev: &MouseEvent) -> u8 {
        match ev.kind {
            MouseKind::Down(button) => {
                let chained = self.state.as_ref().is_some_and(|s| {
                    s.button == button
                        // saturating: a frozen test clock (dt = 0) and a
                        // clock stepping backwards both read as zero —
                        // never a panic, never a chain-breaking surprise.
                        && now.saturating_duration_since(s.at) <= self.window
                        && chebyshev(ev.pos, s.pos) <= self.tolerance
                });
                let count = match (chained, &self.state) {
                    (true, Some(s)) => s.count.saturating_add(1),
                    _ => 1,
                };
                self.state = Some(ChainState {
                    at: now,
                    pos: ev.pos,
                    button,
                    count,
                });
                count
            }
            MouseKind::Drag(_)
            | MouseKind::ScrollUp
            | MouseKind::ScrollDown
            | MouseKind::ScrollLeft
            | MouseKind::ScrollRight => {
                // A drag re-interprets the gesture; a wheel moves the
                // content under the (stationary) pointer — either way
                // the next press starts a fresh chain.
                self.state = None;
                0
            }
            MouseKind::Up(_) | MouseKind::Move => 0,
        }
    }

    /// Forget the chain: the next press counts 1. Used by the tree when
    /// a press gesture is re-interpreted (selection-layer claim) and by
    /// the no-time-source dispatch path.
    pub fn reset(&mut self) {
        self.state = None;
    }
}

/// Chebyshev (chessboard) cell distance — the max per-axis delta, so
/// the tolerance is a square box around the previous press.
fn chebyshev(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

thread_local! {
    /// The ambient input timestamp (see the module docs): written by the
    /// driver each turn from its injectable clock, read by every tree's
    /// embedded chain during dispatch. `None` = no time source — presses
    /// stay isolated (never a silent `Instant::now()` fallback).
    static EVENT_TIME: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Publish the timestamp for subsequently dispatched input events (the
/// driver does this every turn from its `set_clock`-injectable clock).
/// Harnesses driving a bare `UiTree` set this to script double-click
/// timing; `None` restores the no-time-source posture (every press
/// counts 1).
pub fn set_event_time(now: Option<Instant>) {
    EVENT_TIME.with(|t| t.set(now));
}

/// The published input timestamp, if any. Custom input paths embedding
/// their own [`ClickChain`] read the same clock the engine's chains use.
pub fn event_time() -> Option<Instant> {
    EVENT_TIME.with(|t| t.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Mods;

    fn press_at(x: i32, y: i32) -> MouseEvent {
        MouseEvent {
            pos: Point::new(x, y),
            kind: MouseKind::Down(MouseButton::Left),
            mods: Mods::NONE,
        }
    }

    fn ev(kind: MouseKind, x: i32, y: i32) -> MouseEvent {
        MouseEvent {
            pos: Point::new(x, y),
            kind,
            mods: Mods::NONE,
        }
    }

    #[test]
    fn presses_chain_and_triple_counts_three() {
        let mut chain = ClickChain::new();
        let t0 = Instant::now();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(100), &press_at(5, 5)),
            2
        );
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(200), &press_at(5, 5)),
            3,
            "triple-click reads 3 — no artificial cap below saturation"
        );
    }

    #[test]
    fn window_boundary_is_inclusive_and_one_past_resets() {
        let mut chain = ClickChain::new().window(Duration::from_millis(400));
        let t0 = Instant::now();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        // EXACTLY the window: still chained (inclusive by contract).
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(400), &press_at(5, 5)),
            2
        );
        // The next press measures from the SECOND press; one past the
        // window resets to an isolated press.
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(801), &press_at(5, 5)),
            1
        );
    }

    #[test]
    fn tolerance_boundary_is_inclusive_and_one_past_resets() {
        let mut chain = ClickChain::new().tolerance(1);
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(50);
        let t2 = t0 + Duration::from_millis(100);
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        // Chebyshev distance exactly 1 (diagonal wiggle): chains.
        assert_eq!(chain.observe(t1, &press_at(6, 6)), 2);
        // Distance 2 from the LAST press: fresh chain.
        assert_eq!(chain.observe(t2, &press_at(8, 6)), 1);
        // Exact-cell policy: tolerance 0 rejects a 1-cell drift.
        let mut strict = ClickChain::new().tolerance(0);
        assert_eq!(strict.observe(t0, &press_at(5, 5)), 1);
        assert_eq!(strict.observe(t1, &press_at(6, 5)), 1);
        assert_eq!(strict.observe(t2, &press_at(6, 5)), 2);
    }

    #[test]
    fn different_button_starts_its_own_chain() {
        let mut chain = ClickChain::new();
        let t0 = Instant::now();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        let right = MouseEvent {
            pos: Point::new(5, 5),
            kind: MouseKind::Down(MouseButton::Right),
            mods: Mods::NONE,
        };
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(50), &right),
            1,
            "a right press never continues a left chain"
        );
        // And the left press after it measures against the RIGHT press's
        // stored state (button mismatch): fresh again.
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(100), &press_at(5, 5)),
            1
        );
    }

    #[test]
    fn wheel_and_drag_reset_up_and_move_do_not() {
        let t0 = Instant::now();
        let step = Duration::from_millis(30);
        // Wheel between presses: content moved under the cell — reset.
        let mut chain = ClickChain::new();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        assert_eq!(
            chain.observe(t0 + step, &ev(MouseKind::ScrollDown, 5, 5)),
            0
        );
        assert_eq!(chain.observe(t0 + step * 2, &press_at(5, 5)), 1);
        // Drag: the gesture became something else — reset.
        let mut chain = ClickChain::new();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        assert_eq!(
            chain.observe(t0 + step, &ev(MouseKind::Drag(MouseButton::Left), 9, 5)),
            0
        );
        assert_eq!(chain.observe(t0 + step * 2, &press_at(5, 5)), 1);
        // Up and Move leave the chain intact (a normal double-click has
        // an Up between its presses; mode-1003 terminals stream Moves).
        let mut chain = ClickChain::new();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        chain.observe(t0 + step, &ev(MouseKind::Up(MouseButton::Left), 5, 5));
        chain.observe(t0 + step, &ev(MouseKind::Move, 5, 5));
        assert_eq!(chain.observe(t0 + step * 2, &press_at(5, 5)), 2);
    }

    #[test]
    fn mods_do_not_break_the_chain() {
        let mut chain = ClickChain::new();
        let t0 = Instant::now();
        let shifted = MouseEvent {
            pos: Point::new(5, 5),
            kind: MouseKind::Down(MouseButton::Left),
            mods: Mods::SHIFT,
        };
        assert_eq!(chain.observe(t0, &shifted), 1);
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(50), &press_at(5, 5)),
            2,
            "chain identity is button+time+place, never modifiers"
        );
    }

    #[test]
    fn count_saturates_at_u8_max() {
        let mut chain = ClickChain::new();
        let t0 = Instant::now();
        let mut last = 0;
        for _ in 0..300 {
            last = chain.observe(t0, &press_at(5, 5));
        }
        assert_eq!(last, u8::MAX, "saturating, never wrapping to 0/1");
    }

    #[test]
    fn reset_isolates_the_next_press() {
        let mut chain = ClickChain::new();
        let t0 = Instant::now();
        assert_eq!(chain.observe(t0, &press_at(5, 5)), 1);
        chain.reset();
        assert_eq!(
            chain.observe(t0 + Duration::from_millis(10), &press_at(5, 5)),
            1
        );
    }

    #[test]
    fn ambient_event_time_roundtrips_and_clears() {
        let t = Instant::now();
        set_event_time(Some(t));
        assert_eq!(event_time(), Some(t));
        set_event_time(None);
        assert_eq!(event_time(), None);
    }
}
