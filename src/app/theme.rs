//! The app-level theme signal (damage contract §5: ONE signal, no
//! per-token signals).
//!
//! Widgets resolve tokens at VIEW BUILD time inside `Dyn` regions: a
//! theme switch writes this one signal, every `Dyn` that read it re-runs,
//! and exactly those regions damage. Styles stay resolved-`Rgba` POD
//! (DESIGN request 1) so the draw/diff hot path never chases a token
//! lookup.
//!
//! The signal lives under a deliberately-leaked per-thread root scope:
//! the active theme is process-lifetime state (like the runtime itself),
//! and parking it under any component scope would kill it on unmount.

use std::cell::Cell;

use crate::reactive::{create_root, Scope, Signal};
use crate::theme::{default_theme, Theme};

thread_local! {
    static THEME_SIGNAL: Cell<Option<Signal<&'static Theme>>> = const { Cell::new(None) };
}

fn theme_signal() -> Signal<&'static Theme> {
    THEME_SIGNAL.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        // One immortal root per thread holds exactly this signal. The
        // RootScope is forgotten on purpose: disposing it would invalidate
        // the handle every component captured.
        let (root, sig) = create_root(|cx| cx.signal(default_theme()));
        std::mem::forget(root);
        slot.set(Some(sig));
        sig
    })
}

/// The active theme as a reactive signal. Read it inside a `Dyn` (or any
/// tracked computation) and the region re-renders on theme switch:
///
/// ```ignore
/// let theme = use_theme(cx);
/// dyn_view(style, move || {
///     let tokens = &theme.get().tokens;
///     styled_label(tokens.get(TokenId::Text), ...)
/// })
/// ```
///
/// The `cx` parameter is the component idiom (and the future hook point
/// for scoped theme overrides); the cycle-2 theme is app-global.
pub fn use_theme(_cx: Scope) -> Signal<&'static Theme> {
    theme_signal()
}

/// Current theme without a scope at hand (app plumbing, diagnostics).
/// Untracked read — UI code should go through [`use_theme`].
pub fn current_theme() -> &'static Theme {
    theme_signal().get_untracked()
}

/// Switch the active theme. Every tracked reader re-runs; their regions
/// damage; the next frame repaints them. Returns the previous theme.
pub fn set_theme(theme: &'static Theme) -> &'static Theme {
    let sig = theme_signal();
    let prev = sig.get_untracked();
    sig.set(theme);
    prev
}

/// Convenience: switch by registry id (`theme::get` semantics). Returns
/// false (and changes nothing) for an unknown id.
pub fn set_theme_by_id(id: &str) -> bool {
    match crate::theme::get(id) {
        Some(t) => {
            set_theme(t);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::create_root;

    #[test]
    fn theme_defaults_and_switches_reactively() {
        assert_eq!(current_theme().id, crate::theme::DEFAULT_THEME_ID);
        let runs = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let r2 = runs.clone();
        let (_root, ()) = create_root(|cx| {
            let theme = use_theme(cx);
            cx.effect(move || r2.borrow_mut().push(theme.get().id));
        });
        let target = crate::theme::get("nord").expect("nord registered");
        let prev = set_theme(target);
        assert_eq!(runs.borrow().last().copied(), Some("nord"));
        // Restore for other tests on this thread (thread-local signal).
        set_theme(prev);
        assert_eq!(runs.borrow().len(), 3, "initial + switch + restore");
    }

    #[test]
    fn set_theme_by_id_rejects_unknown() {
        assert!(!set_theme_by_id("no-such-theme"));
        assert_eq!(current_theme().id, crate::theme::DEFAULT_THEME_ID);
    }
}
