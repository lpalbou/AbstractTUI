//! WCAG contrast measurement and the theme contrast audit.
//!
//! Every registered theme must pass [`audit`] with zero violations — this is
//! test-pinned in `registry.rs`. The floors encode the project's readability
//! contract (see `docs/design/theme-identity.md`, section 1.3):
//!
//! | pair                        | floor  | note                              |
//! | --------------------------- | ------ | --------------------------------- |
//! | text / bg, surface, raised  | 4.5:1  | target 7:1, floor is binding      |
//! | text_muted / bg             | 3.0:1  | secondary copy stays readable     |
//! | text_faint / bg             | 2.5:1  | decoration/disabled tier only     |
//! | accent, accent_alt / bg     | 3.0:1  | interactive marks                 |
//! | ok, warn, error, info / bg  | 3.0:1  | semantic marks                    |
//! | link / bg                   | 3.0:1  | link renders as text + underline  |
//! | selection_fg / selection_bg | 4.5:1  | selected text is still text       |
//! | border / bg                 | 1.5:1  | hairline visibility               |
//! | border_focus / bg           | 2.0:1  | focus must beat plain border      |
//! | cursor / bg                 | 3.0:1  | block cursor visibility           |
//!
//! Decisiveness: a theme's ground must be decisively dark or light —
//! `|L(bg) - 0.5| >= 0.15` (dark themes: L < 0.35, light: L > 0.65). A
//! mid-gray ground makes both text polarities marginal and breaks the
//! dark/light grouping downstream consumers rely on (lesson inherited from
//! the abstractcode registry invariants).
//!
//! OWNER: DESIGN.

use crate::base::Rgba;
use crate::theme::tokens::{TokenId, TokenSet};

/// WCAG 2.x contrast ratio between two colors, 1.0..=21.0.
///
/// Order-insensitive; alpha is ignored (tokens are audited as opaque —
/// derivation composites washes before they get here).
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
    let la = a.luminance();
    let lb = b.luminance();
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Decisiveness margin: how far the ground sits from the ambiguous middle.
/// Themes must keep this >= [`DECISIVENESS_MARGIN`].
pub fn decisiveness(bg: Rgba) -> f32 {
    (bg.luminance() - 0.5).abs()
}

/// Minimum `|L(bg) - 0.5|` for a theme ground to count as decisive.
pub const DECISIVENESS_MARGIN: f32 = 0.15;

/// One failed check, with everything needed to reproduce it by hand.
/// `theme` is owned so runtime-registered candidates (RT1-9a) can be
/// audited before any of their strings are leaked to `'static`.
#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    /// Theme id the violation was found in.
    pub theme: String,
    /// Human-readable rule name, e.g. `"text/bg"` or `"decisive-ground"`.
    pub rule: &'static str,
    /// The token under test.
    pub token: TokenId,
    /// Measured value (contrast ratio, or luminance margin for
    /// decisiveness checks).
    pub measured: f32,
    /// The floor that was missed.
    pub required: f32,
}

impl core::fmt::Display for Violation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "[{}] {} on {:?}: measured {:.2}, required {:.2}",
            self.theme, self.rule, self.token, self.measured, self.required
        )
    }
}

/// Named, documented audit exceptions: `(theme id, rule)` pairs the
/// registry test tolerates. Every entry must explain itself inline and is
/// checked for staleness (an exception that no longer fires fails the
/// test — no silent grandfathering). Keep this list as close to empty as
/// the faithful port allows.
pub const AUDIT_EXCEPTIONS: &[(&str, &str)] = &[
    // everforest-light: text (#5c6a72) on bg-tertiary (#e8e0cc) measures
    // ~4.25:1. Both values are verbatim theme.css ports (everforest is
    // deliberately soft), so neither may be "fixed" engine-side, and the
    // rule itself is our own extension beyond the mandated text/bg floor.
    // Raised chrome hosts short labels rather than body copy, and 4.25:1
    // still clears WCAG AA-large (3:1) with margin.
    ("everforest-light", "text/surface_raised"),
];

/// Floors, kept public so tests and future tooling audit against the same
/// numbers the engine documents.
pub mod floors {
    pub const TEXT: f32 = 4.5;
    pub const TEXT_TARGET: f32 = 7.0; // aspirational, reported not enforced
    pub const TEXT_MUTED: f32 = 3.0;
    pub const TEXT_FAINT: f32 = 2.5;
    pub const ACCENT: f32 = 3.0;
    pub const SEMANTIC: f32 = 3.0;
    pub const LINK: f32 = 3.0;
    pub const SELECTION_TEXT: f32 = 4.5;
    pub const BORDER: f32 = 1.5;
    pub const BORDER_FOCUS: f32 = 2.0;
    pub const CURSOR: f32 = 3.0;
    /// Syntax inks on `surface_raised` (the declared code ground).
    /// TARGETS, capped per theme at the body text's own contrast there
    /// (`registry::syntax_floor`) — code cannot out-read text, and soft
    /// palettes (everforest-light) set the honest ceiling.
    pub const SYNTAX: f32 = 4.5;
    pub const SYNTAX_COMMENT: f32 = 3.0;
}

/// Audit one token set against every documented floor. Returns an empty
/// vector for a compliant theme; each entry is independently actionable.
pub fn audit(theme_id: &str, t: &TokenSet) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut check = |rule: &'static str, token: TokenId, fg: Rgba, bg: Rgba, floor: f32| {
        let measured = contrast_ratio(fg, bg);
        if measured < floor {
            out.push(Violation {
                theme: theme_id.to_string(),
                rule,
                token,
                measured,
                required: floor,
            });
        }
    };

    // Text tiers must hold on the ground AND on both surface elevations —
    // panels are where most text actually renders.
    for (bg_name, bg) in [
        ("bg", t.bg),
        ("surface", t.surface),
        ("surface_raised", t.surface_raised),
    ] {
        let rule: &'static str = match bg_name {
            "bg" => "text/bg",
            "surface" => "text/surface",
            _ => "text/surface_raised",
        };
        check(rule, TokenId::Text, t.text, bg, floors::TEXT);
    }
    check(
        "text_muted/bg",
        TokenId::TextMuted,
        t.text_muted,
        t.bg,
        floors::TEXT_MUTED,
    );
    check(
        "text_faint/bg",
        TokenId::TextFaint,
        t.text_faint,
        t.bg,
        floors::TEXT_FAINT,
    );

    check("accent/bg", TokenId::Accent, t.accent, t.bg, floors::ACCENT);
    check(
        "accent_alt/bg",
        TokenId::AccentAlt,
        t.accent_alt,
        t.bg,
        floors::ACCENT,
    );
    check("ok/bg", TokenId::Ok, t.ok, t.bg, floors::SEMANTIC);
    check("warn/bg", TokenId::Warn, t.warn, t.bg, floors::SEMANTIC);
    check("error/bg", TokenId::Error, t.error, t.bg, floors::SEMANTIC);
    check("info/bg", TokenId::Info, t.info, t.bg, floors::SEMANTIC);
    check("link/bg", TokenId::Link, t.link, t.bg, floors::LINK);

    check(
        "selection_fg/selection_bg",
        TokenId::SelectionFg,
        t.selection_fg,
        t.selection_bg,
        floors::SELECTION_TEXT,
    );
    check("border/bg", TokenId::Border, t.border, t.bg, floors::BORDER);
    check(
        "border_focus/bg",
        TokenId::BorderFocus,
        t.border_focus,
        t.bg,
        floors::BORDER_FOCUS,
    );
    check("cursor/bg", TokenId::Cursor, t.cursor, t.bg, floors::CURSOR);

    // Syntax inks: audited against surface_raised (the code ground),
    // floors capped at the theme's own text ceiling there.
    let code_ground = t.surface_raised;
    let primary = crate::theme::registry::syntax_floor(floors::SYNTAX, t.text, code_ground);
    let secondary =
        crate::theme::registry::syntax_floor(floors::SYNTAX_COMMENT, t.text, code_ground);
    for (rule, token, ink, floor) in [
        (
            "syntax_keyword/raised",
            TokenId::SyntaxKeyword,
            t.syntax_keyword,
            primary,
        ),
        (
            "syntax_string/raised",
            TokenId::SyntaxString,
            t.syntax_string,
            primary,
        ),
        (
            "syntax_number/raised",
            TokenId::SyntaxNumber,
            t.syntax_number,
            primary,
        ),
        (
            "syntax_type/raised",
            TokenId::SyntaxType,
            t.syntax_type,
            primary,
        ),
        (
            "syntax_func/raised",
            TokenId::SyntaxFunc,
            t.syntax_func,
            primary,
        ),
        (
            "syntax_punct/raised",
            TokenId::SyntaxPunct,
            t.syntax_punct,
            primary,
        ),
        (
            "syntax_comment/raised",
            TokenId::SyntaxComment,
            t.syntax_comment,
            secondary,
        ),
    ] {
        let measured = contrast_ratio(ink, code_ground);
        if measured < floor {
            out.push(Violation {
                theme: theme_id.to_string(),
                rule,
                token,
                measured,
                required: floor,
            });
        }
    }

    // Chart ramp: every entry must be legible as a mark on the ground.
    for (i, c) in t.chart.iter().enumerate() {
        let measured = contrast_ratio(*c, t.bg);
        if measured < floors::SEMANTIC {
            out.push(Violation {
                theme: theme_id.to_string(),
                rule: "chart/bg",
                token: TokenId::chart(i as u8),
                measured,
                required: floors::SEMANTIC,
            });
        }
    }

    // Decisive ground (dark XOR light, never mid-gray).
    let margin = decisiveness(t.bg);
    if margin < DECISIVENESS_MARGIN {
        out.push(Violation {
            theme: theme_id.to_string(),
            rule: "decisive-ground",
            token: TokenId::Bg,
            measured: margin,
            required: DECISIVENESS_MARGIN,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_matches_known_wcag_anchors() {
        // Black on white is the canonical 21:1; identical colors are 1:1.
        assert!((contrast_ratio(Rgba::BLACK, Rgba::WHITE) - 21.0).abs() < 0.01);
        let g = Rgba::rgb(0x80, 0x80, 0x80);
        assert!((contrast_ratio(g, g) - 1.0).abs() < 1e-6);
        // Order-insensitive.
        let a = Rgba::rgb(0xe9, 0x45, 0x60);
        let b = Rgba::rgb(0x1a, 0x1a, 0x2e);
        assert_eq!(contrast_ratio(a, b), contrast_ratio(b, a));
    }

    #[test]
    fn known_pair_measures_as_expected() {
        // #767676 on white is the classic "just passes 4.5:1" gray.
        let g = Rgba::rgb(0x76, 0x76, 0x76);
        let r = contrast_ratio(g, Rgba::WHITE);
        assert!(r > 4.4 && r < 4.7, "measured {r}");
    }

    #[test]
    fn audit_flags_a_deliberately_broken_theme() {
        let mut t = TokenSet::default();
        t.text = t.bg; // unreadable by construction
        let violations = audit("broken", &t);
        assert!(violations.iter().any(|v| v.rule == "text/bg"));
        // Display formatting stays humanly debuggable.
        let msg = violations[0].to_string();
        assert!(msg.contains("broken") && msg.contains("required"));
    }

    #[test]
    fn audit_flags_indecisive_ground() {
        let t = TokenSet {
            bg: Rgba::rgb(0xbb, 0xbb, 0xbb), // L ~ 0.51: ambiguous mid-gray
            ..Default::default()
        };
        let violations = audit("mid", &t);
        assert!(violations.iter().any(|v| v.rule == "decisive-ground"));
    }
}
