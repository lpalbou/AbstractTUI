//! Theme polarity as a first-class vocabulary: [`ThemeMode`],
//! [`Theme::mode`], and the mode-grouped listing [`themes_by_mode`].
//!
//! The registry has always carried polarity (`Theme::dark`, audited
//! against measured ground luminance — a theme cannot lie about its
//! mode and pass the registry tests). This module gives that fact a
//! closed two-value type so pickers, toggles and polarity-conditional
//! code stop hand-rolling `if t.dark` filters. There is deliberately
//! NO third value: the decisive-ground invariant
//! (`|L(bg) − 0.5| ≥ 0.15`, `contrast::DECISIVENESS_MARGIN`) makes
//! "mid" structurally unrepresentable, so the enum is closed by
//! construction, not by hope.
//!
//! `mode()` derives from the audited `dark` flag — the ONE source; it
//! never re-measures luminance (a second copy of the threshold would
//! be the drift seed the registry tests exist to prevent).
//!
//! OWNER: DESIGN.

use crate::theme::registry::{themes, Theme};

/// A theme's polarity. Closed by construction: the registry's
/// decisive-ground invariant leaves no room for a third value.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    /// `true` for [`ThemeMode::Dark`] — the bridge to the registry's
    /// boolean vocabulary (`Theme::is_dark`, `list()`'s third tuple
    /// field).
    pub fn is_dark(self) -> bool {
        self == ThemeMode::Dark
    }

    /// The opposite mode (what a dark/light toggle flips to).
    pub fn other(self) -> ThemeMode {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }

    /// Human label for group headers and pickers: `"Dark"` / `"Light"`.
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
        }
    }
}

impl Theme {
    /// This theme's polarity as the closed [`ThemeMode`] vocabulary.
    ///
    /// Derived from the declared `dark` flag — the one source, itself
    /// test-pinned against measured ground luminance in the registry
    /// audit (`ids_unique_and_polarity_decisive`), so the method never
    /// re-measures anything.
    pub fn mode(&self) -> ThemeMode {
        if self.dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }
}

/// Every visible theme of one mode, in the same curated order
/// [`super::registry::list`] presents: built-ins in registry order
/// (the house theme of each mode first, then families, then the
/// AbstractTUI originals), followed by runtime registrations (newest
/// per id). The order is stable and documented — pickers rely on
/// "first of mode" being the house palette (`abstract-dark` /
/// `abstract-light`), and so does [`crate::app::toggle_mode`]'s
/// cold-start default.
pub fn themes_by_mode(mode: ThemeMode) -> Vec<&'static Theme> {
    themes()
        .iter()
        .chain(crate::theme::register::user_list())
        .filter(|t| t.mode() == mode)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::registry::get;

    #[test]
    fn every_theme_classifies_and_the_partition_is_exact() {
        let dark = themes_by_mode(ThemeMode::Dark);
        let light = themes_by_mode(ThemeMode::Light);
        assert!(!dark.is_empty(), "dark mode must never be empty");
        assert!(!light.is_empty(), "light mode must never be empty");
        // Built-ins partition exactly: every registry theme lands in
        // exactly one group, and mode() agrees with the audited flag.
        // (User registrations from sibling tests may also be present;
        // the partition is checked over the built-in set.)
        let builtin_dark: Vec<&str> = dark
            .iter()
            .filter(|t| themes().iter().any(|b| b.id == t.id))
            .map(|t| t.id)
            .collect();
        let builtin_light: Vec<&str> = light
            .iter()
            .filter(|t| themes().iter().any(|b| b.id == t.id))
            .map(|t| t.id)
            .collect();
        assert_eq!(builtin_dark.len() + builtin_light.len(), themes().len());
        for t in themes() {
            assert_eq!(t.mode().is_dark(), t.is_dark(), "[{}] mode vs flag", t.id);
            let group = if t.is_dark() {
                &builtin_dark
            } else {
                &builtin_light
            };
            assert!(group.contains(&t.id), "[{}] missing from its group", t.id);
        }
    }

    #[test]
    fn known_themes_pin_their_modes() {
        for (id, mode) in [
            ("abstract-dark", ThemeMode::Dark),
            ("tokyo-night", ThemeMode::Dark),
            ("dracula", ThemeMode::Dark),
            ("observer-night", ThemeMode::Dark),
            ("abstract-midnight", ThemeMode::Dark),
            ("abstract-light", ThemeMode::Light),
            ("abstract-dawn", ThemeMode::Light),
            ("abstract-paper", ThemeMode::Light),
            ("rose-pine-dawn", ThemeMode::Light),
            ("catppuccin-latte", ThemeMode::Light),
            ("one-light", ThemeMode::Light),
            ("everforest-light", ThemeMode::Light),
            ("solarized-light", ThemeMode::Light),
        ] {
            assert_eq!(get(id).expect(id).mode(), mode, "{id}");
        }
    }

    #[test]
    fn ordering_is_registry_order_and_house_themes_lead() {
        // The documented contract: registry order filtered by mode —
        // so the first theme of each mode is the house palette.
        assert_eq!(themes_by_mode(ThemeMode::Dark)[0].id, "abstract-dark");
        assert_eq!(themes_by_mode(ThemeMode::Light)[0].id, "abstract-light");
        // Relative order within a mode matches themes() (stable
        // subsequence, no reordering).
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let ids: Vec<&str> = themes_by_mode(mode)
                .iter()
                .filter(|t| themes().iter().any(|b| b.id == t.id))
                .map(|t| t.id)
                .collect();
            let expected: Vec<&str> = themes()
                .iter()
                .filter(|t| t.mode() == mode)
                .map(|t| t.id)
                .collect();
            assert_eq!(ids, expected, "{mode:?} order drifted from registry");
        }
    }

    #[test]
    fn mode_helpers_round_trip() {
        assert_eq!(ThemeMode::Dark.other(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.other(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.other().other(), ThemeMode::Dark);
        assert!(ThemeMode::Dark.is_dark());
        assert!(!ThemeMode::Light.is_dark());
        assert_eq!(ThemeMode::Dark.label(), "Dark");
        assert_eq!(ThemeMode::Light.label(), "Light");
    }

    #[test]
    fn runtime_registrations_join_their_mode_group() {
        use crate::theme::register::{register, RegisterMode, ThemeCandidate};
        let base = get("abstract-light").expect("house light");
        let reg = register(
            ThemeCandidate {
                id: "mode-test-light".into(),
                label: "Mode Test Light".into(),
                dark: false,
                tokens: base.tokens,
            },
            RegisterMode::Strict,
        )
        .expect("clean candidate registers");
        let light = themes_by_mode(ThemeMode::Light);
        assert!(
            light.iter().any(|t| t.id == reg.theme.id),
            "user theme visible in its mode listing"
        );
        // Registrations trail the built-ins (curated order first).
        let pos_user = light.iter().position(|t| t.id == reg.theme.id).unwrap();
        let pos_house = light.iter().position(|t| t.id == "abstract-light").unwrap();
        assert!(pos_house < pos_user, "built-ins lead, registrations trail");
        assert!(
            !themes_by_mode(ThemeMode::Dark)
                .iter()
                .any(|t| t.id == reg.theme.id),
            "a light registration never appears in the dark group"
        );
    }
}
