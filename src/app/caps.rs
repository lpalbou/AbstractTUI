//! Reactive terminal capabilities (backlog 0295 / media-av 0685):
//! `use_caps(cx)` is the app-facing view of what the driver KNOWS about
//! the terminal — the env pass at enter, upgraded as active-probe
//! replies fold in. Apps read it to render honest capability-derived
//! UI: key hints ("Shift+Enter newline" only where the kitty protocol
//! is live), graphics-channel labels ("placed via sixel"), truecolor-
//! conditional art.
//!
//! Read-only contract: WRITING capabilities stays the driver's job —
//! this signal is a view, published by `Driver::new` (env pass) and by
//! the probe fold whenever a reply actually changed a field. Before an
//! `App` runs on this thread it reads `Capabilities::default()` (all
//! conservative).
//!
//! Same immortal-root pattern as the theme and viewport signals: one
//! per thread, deliberately leaked (disposing it would invalidate every
//! captured handle).

use std::cell::Cell;

use crate::reactive::{create_root, Scope, Signal};
use crate::term::Capabilities;

thread_local! {
    static CAPS_SIGNAL: Cell<Option<Signal<Capabilities>>> = const { Cell::new(None) };
}

fn caps_signal() -> Signal<Capabilities> {
    CAPS_SIGNAL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let (root, sig) = create_root(|cx| cx.signal(Capabilities::default()));
        std::mem::forget(root);
        slot.set(Some(sig));
        sig
    })
}

/// The driver's live [`Capabilities`] as a reactive signal: env-pass
/// values from session enter, upgraded when active-probe replies land
/// (usually within the first frames — never blocking first paint). A
/// `dyn_view` reading it re-renders on upgrade, so hint text and
/// channel labels stay truthful per terminal:
///
/// ```ignore
/// let caps = use_caps(cx);
/// dyn_view(LayoutStyle::line(1), move || {
///     text(if caps.get().kitty_keyboard {
///         "Enter sends · Shift+Enter newline"
///     } else {
///         "Enter sends · Alt+Enter newline"
///     })
/// })
/// ```
///
/// Honesty note: under the default run posture the driver pushes the
/// kitty keyboard enter-flags whenever `kitty_keyboard` is true (at
/// enter when env-claimed, at the probe upgrade otherwise — backlog
/// 0293), so reading `true` here means the enhancement is actually
/// live. Embedders that entered with explicit `RunConfig::enter`
/// options own their posture and know what they asked for.
pub fn use_caps(_cx: Scope) -> Signal<Capabilities> {
    caps_signal()
}

/// Untracked snapshot for non-component code (plumbing, diagnostics).
/// `Capabilities::default()` before the first `App` runs on this thread.
pub fn current_caps() -> Capabilities {
    caps_signal().get_untracked()
}

/// Driver-internal publisher (`Driver::new` + the probe fold). The
/// equality guard keeps no-op folds from waking subscribers.
pub(super) fn publish_caps(caps: &Capabilities) {
    let sig = caps_signal();
    if sig.get_untracked() != *caps {
        sig.set(caps.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::flush_effects;

    /// The 0685 validation line at the signal layer: the signal flips
    /// exactly once per REAL change — identical re-publishes (a probe
    /// reply proving something already proven) never wake subscribers.
    #[test]
    fn publish_flips_subscribers_once_per_real_change() {
        let (root, runs) = create_root(|cx| {
            let runs = cx.signal(0u32);
            let caps = use_caps(cx);
            cx.effect(move || {
                let _ = caps.get(); // subscribe
                runs.update(|r| *r += 1);
            });
            runs
        });
        flush_effects();
        let base = runs.get_untracked();

        let upgraded = Capabilities::with(|c| c.kitty_keyboard = true);
        publish_caps(&upgraded);
        flush_effects();
        assert_eq!(runs.get_untracked(), base + 1, "real change: one flip");

        publish_caps(&upgraded); // byte-identical: deduped
        flush_effects();
        assert_eq!(runs.get_untracked(), base + 1, "no-op publish: no flip");

        // Reset the immortal thread-local for sibling assertions in this
        // thread (tests run one-per-thread, but stay tidy anyway).
        publish_caps(&Capabilities::default());
        root.dispose();
    }
}
