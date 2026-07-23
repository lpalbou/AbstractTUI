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
use crate::theme::{default_theme, themes_by_mode, Theme, ThemeMode};

thread_local! {
    static THEME_SIGNAL: Cell<Option<Signal<&'static Theme>>> = const { Cell::new(None) };
    /// Last theme used per mode ([dark, light]), recorded by
    /// [`set_theme`] — the single signal-write choke point, so EVERY
    /// switch path (pickers, `set_theme_by_id`, [`toggle_mode`]) feeds
    /// it. Same lifetime class as the signal itself: process-lifetime
    /// per thread, dies with the app.
    static LAST_BY_MODE: Cell<[Option<&'static Theme>; 2]> = const { Cell::new([None, None]) };
}

fn mode_slot(mode: ThemeMode) -> usize {
    match mode {
        ThemeMode::Dark => 0,
        ThemeMode::Light => 1,
    }
}

fn remember(theme: &'static Theme) {
    LAST_BY_MODE.with(|slot| {
        let mut last = slot.get();
        last[mode_slot(theme.mode())] = Some(theme);
        slot.set(last);
    });
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
///
/// Also records `theme` as the last-used theme of its mode — the
/// memory [`toggle_mode`] restores when flipping back.
pub fn set_theme(theme: &'static Theme) -> &'static Theme {
    remember(theme);
    let sig = theme_signal();
    let prev = sig.get_untracked();
    sig.set(theme);
    prev
}

/// Flip dark ↔ light, keeping the user's theme CHOICE per mode: the
/// target mode's last-used theme is restored when one was ever set on
/// this thread; a never-visited mode falls back to its first listed
/// theme — the house palette, by the documented
/// [`themes_by_mode`] ordering. Returns the theme now active.
///
/// The memory is fed by [`set_theme`] itself (the one signal-write
/// choke point), so the round trip holds across any switch path:
/// `nord` → toggle → `abstract-light` → toggle → `nord`.
pub fn toggle_mode() -> &'static Theme {
    let target = current_theme().mode().other();
    let remembered = LAST_BY_MODE.with(|slot| slot.get()[mode_slot(target)]);
    let next = remembered.unwrap_or_else(|| {
        themes_by_mode(target)
            .first()
            .copied()
            .expect("both modes carry built-in themes (registry invariant)")
    });
    set_theme(next);
    next
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

    #[test]
    fn toggle_mode_defaults_to_the_house_theme_then_remembers_choices() {
        // Thread-local state: this test's thread starts cold.
        let anchor = current_theme();
        assert!(set_theme_by_id("nord")); // a dark, non-house choice
        let lit = toggle_mode();
        assert_eq!(
            lit.id, "abstract-light",
            "never-visited light mode falls back to the house palette \
             (first of mode in the documented ordering)"
        );
        assert_eq!(current_theme().id, "abstract-light");
        // The round trip restores the CHOICE, not the house default.
        assert_eq!(toggle_mode().id, "nord");
        // A light choice is remembered symmetrically.
        assert!(set_theme_by_id("catppuccin-latte"));
        assert_eq!(toggle_mode().id, "nord");
        assert_eq!(toggle_mode().id, "catppuccin-latte");
        set_theme(anchor); // restore for sibling tests on this thread
    }
}
