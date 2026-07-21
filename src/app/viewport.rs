//! Reactive viewport size (DESIGN cycle-4 nit): `Modal::open`/
//! `Toast::show` take a `viewport: Size`, which callers were
//! hand-tracking through resizes. `use_viewport(cx)` is the one source —
//! `App::set_viewport` (mount + every driver resize) publishes into it,
//! so a `dyn_view` reading it re-renders on resize and popup call sites
//! read the current size at open time.
//!
//! Same immortal-root pattern as the theme signal: one per thread,
//! deliberately leaked (disposing it would invalidate every captured
//! handle).

use std::cell::Cell;

use crate::base::Size;
use crate::reactive::{create_root, Scope, Signal};

thread_local! {
    static VIEWPORT_SIGNAL: Cell<Option<Signal<Size>>> = const { Cell::new(None) };
}

fn viewport_signal() -> Signal<Size> {
    VIEWPORT_SIGNAL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let (root, sig) = create_root(|cx| cx.signal(Size::ZERO));
        std::mem::forget(root);
        slot.set(Some(sig));
        sig
    })
}

/// The terminal viewport as a reactive signal. `Size::ZERO` before the
/// first `App` exists on this thread.
///
/// ```ignore
/// let vp = use_viewport(cx);
/// button.on_click(move || { Toast::show(&overlays, cx, vp.get_untracked(), "hi", d); })
/// ```
pub fn use_viewport(_cx: Scope) -> Signal<Size> {
    viewport_signal()
}

/// Untracked read for non-component code (popup call sites, plumbing).
pub fn current_viewport() -> Size {
    viewport_signal().get_untracked()
}

/// App-internal publisher (called from `App::set_viewport`).
pub(super) fn publish_viewport(size: Size) {
    let sig = viewport_signal();
    if sig.get_untracked() != size {
        sig.set(size);
    }
}
