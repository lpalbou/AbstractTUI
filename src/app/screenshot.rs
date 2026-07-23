//! Public screenshot verb (backlog control-plane/0370).
//!
//! [`request_screenshot`] is the component-reachable capture: apps only
//! hold signals and handles after `App::run()` consumes the `App`, so —
//! exactly like [`super::redraw::request_full_redraw`] — the request is
//! a thread-local the driver drains once per turn in its phase-U
//! engine-verb section. The callback receives the **last presented
//! frame** (the screen as the user saw it when the request landed; a
//! render triggered by the same turn happens after the drain), with
//! byte-channel image placements stamped as
//! [`pixel_regions`](crate::render::Screenshot::pixel_regions).
//!
//! Capture is a pure read of the composed frame: no re-render, no
//! damage, and an idle app stays at zero bytes / zero wakeups — the
//! verb only costs anything on the turn that serves it.
//!
//! There is deliberately NO default key binding: apps bind their own
//! (see the recipe in docs/api.md "Screenshots & captures").
//!
//! OWNER: REACT (verb surface) with KERNEL (driver drain).

use std::cell::RefCell;

use crate::reactive::request_frame;
use crate::render::Screenshot;

type ShotCallback = Box<dyn FnOnce(Screenshot)>;

thread_local! {
    /// Pending capture callbacks, drained once per driver turn.
    static PENDING: RefCell<Vec<ShotCallback>> = const { RefCell::new(Vec::new()) };
}

/// Capture the screen as last presented and hand it to `f` on the app
/// thread (the driver's next turn — same turn when called from an event
/// handler). The callback runs in phase U, so it may write signals; keep
/// heavy work (encoding, disk IO beyond a quick write) off the loop.
///
/// ```no_run
/// use abstracttui::app::request_screenshot;
/// request_screenshot(|shot| {
///     let _ = shot.write_svg("/tmp/screen.svg");
/// });
/// ```
///
/// Before the first frame is presented the capture is honestly blank.
/// Embedders driving their own turns can call `Driver::screenshot()`
/// directly instead.
pub fn request_screenshot(f: impl FnOnce(Screenshot) + 'static) {
    PENDING.with(|p| p.borrow_mut().push(Box::new(f)));
    // A request from a posted job must wake an idle loop; from an event
    // handler this turn was running anyway (the served frame renders
    // nothing new — zero-damage frames emit zero bytes).
    request_frame();
}

/// Driver drain (once per turn, phase U): all pending callbacks. Taking
/// an empty `Vec` allocates nothing, so quiet turns stay allocation-free.
pub(crate) fn take_screenshot_requests() -> Vec<ShotCallback> {
    PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn requests_queue_and_drain_once() {
        assert!(take_screenshot_requests().is_empty(), "clean start");
        let served = Rc::new(Cell::new(0));
        let s = served.clone();
        request_screenshot(move |_| s.set(s.get() + 1));
        let pending = take_screenshot_requests();
        assert_eq!(pending.len(), 1);
        assert!(take_screenshot_requests().is_empty(), "drain is one-shot");
        let blank = Screenshot::from_surface(&crate::render::Surface::new(
            crate::base::Size::new(2, 1),
            crate::render::Cell::EMPTY,
        ));
        for cb in pending {
            cb(blank.clone());
        }
        assert_eq!(served.get(), 1);
    }
}
