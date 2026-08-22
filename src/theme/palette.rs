//! The public seed door: a consumer's house colors → the engine's own
//! derivation → a [`ThemeCandidate`] the existing audit judges.
//!
//! ## Why this exists
//!
//! Before this module, `abstracttui` had a public audit
//! ([`register`](crate::theme::register())) sitting behind a PRIVATE
//! derivation (`registry::build`). [`ThemeCandidate`] wants all 28 tokens,
//! so an app with a house palette had to produce `border`,
//! `selection_bg`, the 8 chart inks and the 7 syntax inks itself — and the
//! primitives to do that are public ([`derive`](crate::theme::derive)), so
//! it was an afternoon's work that `register` would happily accept.
//!
//! That is the trap: the app's tokens would come from ITS copy of the
//! transform and drift from the engine's on the next floor change, with
//! nothing anywhere to notice. A public audit behind a private derivation
//! invites exactly the failure the audit exists to prevent.
//!
//! So: one transform, two front doors. The built-in table
//! (`seeds::ThemeSeed`, `&'static` hex) and a [`Palette`] (owned strings
//! from a config file) both parse into the same `SeedColors` and go
//! through the same `derive_tokens`. `palette_derives_like_the_builtin_table`
//! below pins that byte-for-byte across all 26 built-ins — if the two paths
//! ever diverge, that test is what says so.
//!
//! ## Fallible where the input is
//!
//! The built-in table PANICS on bad hex, which is right: it is compile-time
//! data audited in CI, and a typo there is a programmer error that should
//! never reach a user. A palette read from a config file is the opposite
//! case, so [`Palette::derive`] returns [`PaletteError`] listing EVERY
//! malformed field, not just the first — a config with three typos should
//! cost the user one round trip.
//!
//! ## The whole flow
//!
//! ```no_run
//! use abstracttui::theme::{Palette, RegisterMode, register};
//!
//! let mut palette = Palette::new("acme", "Acme", true);
//! palette.bg = "#101014".into();
//! palette.accent = "#ff6188".into();
//! // ... the other ten authored colors ...
//!
//! let candidate = palette.derive()?;          // 12 colors -> 28 tokens
//! let reg = register(candidate, RegisterMode::Strict)?;  // audited
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! `derive` does NOT audit and does NOT validate the id: it is the
//! transform only. [`register`](crate::theme::register()) remains the single
//! place a theme is judged, so there is no second audit to keep in step.
//!
//! OWNER: DESIGN.

use crate::theme::register::ThemeCandidate;
use crate::theme::registry::{derive_tokens, SeedColors};

/// A consumer's authored colors: the same twelve the built-in seed table
/// carries, as owned strings (a palette read at runtime is not `&'static`).
///
/// Every field is required, and there is deliberately no "fill the rest
/// from `bg` and `accent`" shortcut. The first real consumer of this API
/// is what settled that: they arrived with **seven** of the twelve, and
/// every one of the five they lacked was a *semantic* ink —
/// `accent_alt`, `ok`, `warn`, `error`, `info`.
///
/// That is the shape of the gap in general, and it is the argument. The
/// tokens a consumer is most likely to be missing are the ones a
/// derivation has the least business guessing: "which green means
/// resolved in this product" is a decision, not a shade, and a second
/// brand accent that does not exist yet must not be minted by algorithm.
/// A helper aimed at this gap would invent meaning, and the first thing
/// it would do is put a green in someone's product that nobody chose.
///
/// So this type only claims to run the derivation the built-ins run.
/// Choosing the authored colors stays with whoever owns the brand.
///
/// Hex accepts `#rgb`, `#rrggbb` and `#rrggbbaa`, with or without the `#`
/// (the grounds should be opaque; alpha is not rejected, but the audit
/// judges the composited result).
///
/// Adding a field here is a breaking change, and that is intentional: this
/// type IS the seed contract, and the built-in table would gain the same
/// field on the same day.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    /// Kebab-case machine id. Validated by
    /// [`register`](crate::theme::register()), not here.
    pub id: String,
    /// Human label for pickers (empty falls back to the id at register).
    pub label: String,
    /// Declared polarity. Audited against the measured ground at register,
    /// and it selects the shadow rule here.
    pub dark: bool,
    /// Application ground.
    pub bg: String,
    /// Panel/card ground.
    pub surface: String,
    /// Raised ground; also the declared code ground for the syntax inks.
    pub surface_raised: String,
    /// Body ink. Drives border, shadow (light), and every "walk toward
    /// text" step in the derivation.
    pub text: String,
    /// Secondary ink.
    pub text_muted: String,
    /// Tertiary ink.
    pub text_faint: String,
    /// Brand accent: also `border_focus`, `cursor`, and chart slot 0.
    pub accent: String,
    /// Curated second accent; contrast-guarded during derivation.
    pub accent_alt: String,
    /// Success.
    pub ok: String,
    /// Warning.
    pub warn: String,
    /// Error.
    pub error: String,
    /// Info: also `link` and chart slot 1.
    pub info: String,
}

/// One field that did not parse as hex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadHex {
    /// The [`Palette`] field name, e.g. `"surface_raised"`.
    pub field: &'static str,
    /// What the caller supplied, echoed back so config errors are
    /// greppable without the caller re-reading its own file.
    pub value: String,
}

impl std::fmt::Display for BadHex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: '{}' is not hex (#rgb, #rrggbb or #rrggbbaa)",
            self.field, self.value
        )
    }
}

/// Why a [`Palette`] could not be derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteError {
    /// One entry per malformed color, in declaration order. Never empty.
    BadHex(Vec<BadHex>),
}

impl std::fmt::Display for PaletteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaletteError::BadHex(bad) => {
                write!(f, "palette: {} malformed color(s)", bad.len())?;
                for b in bad {
                    write!(f, "\n  {b}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for PaletteError {}

impl Palette {
    /// An empty palette with identity set. Every color is still `""` and
    /// will fail [`derive`](Palette::derive) until filled — a palette you
    /// forgot to populate must not silently become black-on-black.
    pub fn new(id: impl Into<String>, label: impl Into<String>, dark: bool) -> Palette {
        Palette {
            id: id.into(),
            label: label.into(),
            dark,
            ..Palette::default()
        }
    }

    /// Run the engine's derivation over these colors and return a candidate
    /// for [`register`](crate::theme::register()).
    ///
    /// Errors list EVERY malformed field, never just the first.
    pub fn derive(&self) -> Result<ThemeCandidate, PaletteError> {
        let mut bad: Vec<BadHex> = Vec::new();
        let mut parse = |field: &'static str, value: &str| {
            crate::base::Rgba::from_hex(value).unwrap_or_else(|| {
                bad.push(BadHex {
                    field,
                    value: value.to_string(),
                });
                crate::base::Rgba::BLACK
            })
        };
        // Order matches the struct so the error list reads top-to-bottom
        // against the caller's own config file.
        let colors = SeedColors {
            bg: parse("bg", &self.bg),
            surface: parse("surface", &self.surface),
            surface_raised: parse("surface_raised", &self.surface_raised),
            text: parse("text", &self.text),
            text_muted: parse("text_muted", &self.text_muted),
            text_faint: parse("text_faint", &self.text_faint),
            accent: parse("accent", &self.accent),
            accent_alt: parse("accent_alt", &self.accent_alt),
            ok: parse("ok", &self.ok),
            warn: parse("warn", &self.warn),
            error: parse("error", &self.error),
            info: parse("info", &self.info),
        };
        if !bad.is_empty() {
            return Err(PaletteError::BadHex(bad));
        }
        Ok(ThemeCandidate {
            id: self.id.clone(),
            label: self.label.clone(),
            dark: self.dark,
            tokens: derive_tokens(&colors, self.dark),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::register::{register, RegisterMode};
    use crate::theme::registry::themes;
    use crate::theme::seeds::{ThemeSeed, SEEDS};

    fn palette_of(seed: &ThemeSeed) -> Palette {
        Palette {
            id: seed.id.to_string(),
            label: seed.label.to_string(),
            dark: seed.dark,
            bg: seed.bg.to_string(),
            surface: seed.surface.to_string(),
            surface_raised: seed.surface_raised.to_string(),
            text: seed.text.to_string(),
            text_muted: seed.text_muted.to_string(),
            text_faint: seed.text_faint.to_string(),
            accent: seed.accent.to_string(),
            accent_alt: seed.accent_alt.to_string(),
            ok: seed.ok.to_string(),
            warn: seed.warn.to_string(),
            error: seed.error.to_string(),
            info: seed.info.to_string(),
        }
    }

    /// THE test this module exists for. Not "the derivation runs" — that
    /// would pass with a second, subtly different transform. This asserts
    /// the public path produces the SAME 28 tokens the private table path
    /// produces, for every built-in. If it ever fails, the two derivations
    /// have forked and the public one is lying about being the engine's.
    #[test]
    fn palette_derives_like_the_builtin_table() {
        assert_eq!(SEEDS.len(), themes().len(), "seed/theme count drifted");
        for (seed, theme) in SEEDS.iter().zip(themes()) {
            assert_eq!(seed.id, theme.id, "seed order drifted from theme order");
            let candidate = palette_of(seed)
                .derive()
                .unwrap_or_else(|e| panic!("{}: public path refused its own seed: {e}", seed.id));
            assert_eq!(
                candidate.tokens, theme.tokens,
                "{}: public derivation diverged from registry::build",
                seed.id
            );
            assert_eq!(candidate.dark, theme.dark, "{}: polarity", seed.id);
        }
    }

    /// The guard above is only worth its line count if it can go red.
    /// Perturb one authored color by one unit and the tokens must differ —
    /// otherwise the comparison is not reaching the derivation at all.
    #[test]
    fn the_identity_guard_fails_when_a_seed_is_perturbed() {
        let seed = &SEEDS[0];
        let mut p = palette_of(seed);
        p.accent = "#e94561".into(); // abstract-dark's #e94560, +1 blue
        let perturbed = p.derive().expect("valid hex").tokens;
        assert_ne!(
            perturbed,
            themes()[0].tokens,
            "a one-unit accent change produced identical tokens: the \
             identity test is comparing something other than the derivation"
        );
    }

    #[test]
    fn malformed_hex_errors_rather_than_panicking() {
        let mut p = palette_of(&SEEDS[0]);
        p.surface = "not-a-color".into();
        p.warn = "#12345".into(); // 5 digits: no valid length
        let err = p.derive().expect_err("malformed hex must not derive");
        let PaletteError::BadHex(bad) = &err;
        // EVERY bad field, in declaration order — a config with two typos
        // costs one round trip, not two.
        assert_eq!(
            bad.iter().map(|b| b.field).collect::<Vec<_>>(),
            ["surface", "warn"]
        );
        assert_eq!(bad[0].value, "not-a-color");
        assert!(err.to_string().contains("surface"), "{err}");
        assert!(err.to_string().contains("warn"), "{err}");
    }

    #[test]
    fn an_empty_palette_is_an_error_not_a_black_theme() {
        let err = Palette::new("blank", "Blank", true)
            .derive()
            .expect_err("empty colors must not derive to black-on-black");
        let PaletteError::BadHex(bad) = &err;
        assert_eq!(bad.len(), 12, "every unfilled color should be named");
    }

    /// The composition claim in the module docs: seed → derive → register
    /// with no second audit and no new registration path. A consumer's own
    /// palette must be able to reach the registry through the existing
    /// door — and be judged by it.
    #[test]
    fn a_derived_palette_registers_through_the_existing_audited_door() {
        // A house palette that is not a built-in: abstract-dark's own
        // colors would collide on id, so re-label under a private id.
        let mut p = palette_of(&SEEDS[0]);
        p.id = "palette-door-probe".into();
        p.label = "Door probe".into();
        let reg = register(p.derive().expect("valid hex"), RegisterMode::Strict)
            .expect("abstract-dark's colors pass the audit they were built against");
        assert_eq!(reg.theme.id, "palette-door-probe");
        assert!(reg.warnings.is_empty(), "strict mode: {:?}", reg.warnings);
        assert_eq!(
            reg.theme.tokens,
            themes()[0].tokens,
            "registration must not alter the derived tokens"
        );
    }

    /// A palette the audit should refuse still refuses: the public seed
    /// path adds a transform, never an exemption.
    #[test]
    fn the_audit_still_refuses_an_unreadable_palette() {
        let mut p = palette_of(&SEEDS[0]);
        p.id = "palette-unreadable-probe".into();
        p.text = p.bg.clone(); // text on its own ground: 1:1
        let err = register(p.derive().expect("valid hex"), RegisterMode::Strict)
            .expect_err("text == bg must not register in strict mode");
        assert!(
            matches!(err, crate::theme::RegisterError::Rejected { .. }),
            "{err}"
        );
    }
}
