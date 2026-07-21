//! Reactive startup-notice store (DESIGN's `use_startup_notices` ask).
//!
//! Engine notices land AFTER mount (`App::run` records the input-path
//! degradation and caps summary once the driver exists), so a component
//! reading a plain `Vec` at build time sees nothing. The store is a
//! SIGNAL: mount-time readers subscribe, late pushes propagate, and the
//! notice bar just works:
//!
//! ```ignore
//! let notices = use_startup_notices(cx);
//! dyn_view(Style::default().height(Dimension::Cells(1)), move || {
//!     text(notices.with(|n| n.join(" | ")))
//! })
//! ```
//!
//! Same immortal-root pattern as the theme/viewport signals: one per
//! thread, deliberately leaked (disposing would invalidate captured
//! handles).

use std::cell::Cell;

use crate::reactive::{create_root, Scope, Signal};

thread_local! {
    static NOTICES_SIGNAL: Cell<Option<Signal<Vec<String>>>> = const { Cell::new(None) };
}

fn notices_signal() -> Signal<Vec<String>> {
    NOTICES_SIGNAL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let (root, sig) = create_root(|cx| cx.signal(Vec::new()));
        std::mem::forget(root);
        slot.set(Some(sig));
        sig
    })
}

/// The startup notices as a reactive signal: read it in a `dyn_view`
/// and the region re-renders when the engine (or the app) pushes a
/// late notice. Empty vec = clean start (so far).
pub fn use_startup_notices(_cx: Scope) -> Signal<Vec<String>> {
    notices_signal()
}

/// App-internal publisher (`App::push_startup_notice` fans in here).
pub(super) fn publish_notice(notice: String) {
    let sig = notices_signal();
    sig.update(|v| v.push(notice));
}

/// Test hook: clear the thread-global store (tests on one thread share
/// it; a test asserting exact contents resets first).
#[cfg(test)]
pub(crate) fn reset_notices_for_test() {
    notices_signal().set(Vec::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{create_root, flush_effects};

    #[test]
    fn late_pushes_propagate_to_mount_time_readers() {
        reset_notices_for_test();
        let seen: std::rc::Rc<std::cell::RefCell<Vec<usize>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = seen.clone();
        let (root, ()) = create_root(|cx| {
            // A component reads the store AT MOUNT — before the engine
            // pushed anything (DESIGN's exact failure case).
            let notices = use_startup_notices(cx);
            cx.effect(move || s.borrow_mut().push(notices.with(|n| n.len())));
        });
        flush_effects();
        assert_eq!(*seen.borrow(), vec![0], "clean at mount");
        // The engine pushes AFTER mount (App::run's degradation read).
        let mut app = crate::app::App::new(crate::base::Size::new(10, 3));
        app.push_startup_notice("input: degraded (stdin fallback)");
        flush_effects();
        assert_eq!(*seen.borrow(), vec![0, 1], "late push re-ran the reader");
        assert_eq!(app.startup_notices().len(), 1, "plain read keeps working");
        root.dispose();
    }
}
