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
    /// Ground-vs-ground separation, REPORTED and not enforced — the
    /// default floor for [`super::ground_overlaps`]. Below this two
    /// grounds are close enough that a reader is unlikely to see an edge
    /// between them, so a panel raised onto a surface has none.
    ///
    /// Not a member of the enforced set above, and deliberately so: at
    /// this value it fires across most of the registry, and whether that
    /// is a defect or authored subtlety is DESIGN's ruling. Same standing
    /// as `TEXT_TARGET` — a number the engine will tell you about and
    /// will not fail you for.
    pub const GROUND_SEPARATION_REPORT: f32 = 1.10;
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

/// Two GROUNDS that a reader may not be able to tell apart.
///
/// Deliberately not a [`Violation`]: this is **reported, never enforced**,
/// and calling it a violation would state a verdict that is not mine to
/// state. See [`ground_overlaps`].
#[derive(Clone, Debug, PartialEq)]
pub struct GroundOverlap {
    /// Theme id the pair was measured in.
    pub theme: String,
    /// The two grounds, in `TokenSet::grounds()` order.
    pub a: TokenId,
    pub b: TokenId,
    /// Their WCAG contrast ratio, at TRUECOLOR — before any quantisation.
    pub measured: f32,
}

impl core::fmt::Display for GroundOverlap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "[{}] {} / {} measure {:.3} apart",
            self.theme,
            self.a.name(),
            self.b.name(),
            self.measured
        )
    }
}

/// Every pair of GROUNDS in `t` closer together than `floor`.
///
/// ## Why this exists as its own function
///
/// [`audit`] cannot ask this question. Every one of its rules is
/// INK-on-GROUND: `surface_raised` appears there only as a *background*
/// for text and syntax checks, never as a subject measured against
/// `surface` or `bg`. So a theme may author two grounds `1.006` apart —
/// one colour to any reader — and pass a clean audit with zero
/// violations. That is the same hole `quantize_pair_256` had against
/// `quantize_set_256`: a rule that protects a foreground from its own
/// background cannot protect two grounds from each other, because they
/// live in different cells and are never handed to it as a pair.
///
/// ## Why it REPORTS instead of failing
///
/// Whether a near-invisible ground pair is a defect or deliberate
/// subtlety is a DESIGN call, and at the default reporting floor it fires
/// on most of the registry — so wiring it into `audit` would redden
/// themes whose authors may well have meant it, which is the
/// valid-input-fails defect this module spent a day learning. The
/// engineering claim here is only that the question should be
/// ASKABLE. Set a floor in `audit` once DESIGN has ruled; until then the
/// honest instrument is a measurement anyone can run, not a verdict
/// nobody agreed.
///
/// Note the measurement is at truecolor: it says what the theme AUTHORED,
/// independent of what any depth downgrade later does with it.
pub fn ground_overlaps(theme_id: &str, t: &TokenSet, floor: f32) -> Vec<GroundOverlap> {
    let g = t.grounds();
    let mut out = Vec::new();
    for i in 0..g.len() {
        for j in (i + 1)..g.len() {
            let measured = contrast_ratio(g[i].1, g[j].1);
            if measured < floor {
                out.push(GroundOverlap {
                    theme: theme_id.to_string(),
                    a: g[i].0,
                    b: g[j].0,
                    measured,
                });
            }
        }
    }
    out
}

/// Where a theme's ground-separation DECLARATION contradicts the colours
/// it declared it about — one line per contradiction, empty when there
/// is none.
///
/// ## The one contradiction that exists, and why only that one
///
/// A pair declared [`Distinct`](crate::render::color::PairIntent::Distinct)
/// whose two grounds are BYTE-IDENTICAL. The author has said "these must
/// read as different surfaces" about one surface. It has a determinate
/// resolution — the colours are the artifact and the declaration is
/// metadata about them, so the bytes win and the two share an entry —
/// and until now that resolution happened in silence.
///
/// The mirror case is NOT a contradiction and is deliberately not
/// reported: `Same` over two colours a mile apart asks for a merge that
/// can never happen, because intent only releases a merge at a
/// collision. It is inert, not wrong, and an author may reasonably
/// declare it against a future re-tint of their own palette.
///
/// ## Why this is worth a function
///
/// `Distinct` changes no bytes: undeclared already keeps two grounds
/// apart, so declaring it is a statement for the record rather than an
/// instruction. That was reported as a cost when the three states were
/// ruled (`decision:ground-separation-intent-is-declared-by-the-theme`),
/// weighed, and kept. THIS is what it buys in exchange — a declaration
/// that can be checked against the artifact. An author who writes
/// `Distinct` and then tints both grounds to the same hex now hears
/// about it instead of believing an edge is protected.
///
/// Unlike [`ground_overlaps`], this is not a taste judgement about how
/// close two colours may be, so it does not need DESIGN to set a floor:
/// the theme contradicts ITSELF, at any floor, and the author is the
/// only one who can say which half they meant.
pub fn declaration_contradictions(
    theme_id: &str,
    t: &TokenSet,
    declaration: &[(TokenId, TokenId, crate::render::color::PairIntent)],
) -> Vec<String> {
    let g = t.grounds();
    let color_of = |id: TokenId| g.iter().find(|(k, _)| *k == id).map(|(_, c)| *c);
    declaration
        .iter()
        .filter(|(_, _, intent)| *intent == crate::render::color::PairIntent::Distinct)
        .filter_map(|(a, b, _)| {
            let (ca, cb) = (color_of(*a)?, color_of(*b)?);
            // RGB only: the grounds are opaque by definition, and this is
            // the same equality the separator resolves the case with.
            (ca.r == cb.r && ca.g == cb.g && ca.b == cb.b).then(|| {
                format!(
                    "[{theme_id}] {} / {} are declared Distinct and are the same colour \
                     (#{:02x}{:02x}{:02x}) — they will share one palette entry, because \
                     at truecolor they already are one surface",
                    a.name(),
                    b.name(),
                    ca.r,
                    ca.g,
                    ca.b
                )
            })
        })
        .collect()
}

/// An ink chosen for a ground, with the contrast it actually achieved.
///
/// The ratio is returned rather than swallowed on purpose: on some
/// theme/ground combinations NO authored ink clears [`floors::TEXT`], and
/// a bare `Rgba` return would hand the caller unreadable text that looks
/// like a considered choice. Check `contrast` against the floor you need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// The chosen colour — always a token the theme AUTHORED.
    pub color: Rgba,
    /// Which token it came from, for diagnostics and golden output.
    pub token: TokenId,
    /// Achieved WCAG ratio against the ground it was chosen for.
    pub contrast: f32,
}

/// The theme's most readable authored ink for an arbitrary `ground`.
///
/// ## The gap this closes
///
/// [`audit`] guarantees `text` reads on the theme's own grounds, and it
/// holds: across the built-in registry `text` clears [`floors::TEXT`] on
/// 128 of 130 theme/ground pairs. It says nothing whatsoever about a
/// ground the THEME never saw — and `RunConfig::extra_grounds` exists
/// precisely so an application can declare one. Measured on the registry,
/// a mid-dark declared panel leaves `text` below the floor in 8 of 26
/// themes (`solarized-light` reaches 1.01 — text the same colour as the
/// panel), and a bright declared panel in 19 of 26. An app that reaches
/// for `t.text` on its own panel is reading a coin flip, and the failure
/// is invisible until someone switches theme.
///
/// ## Why the candidates are exactly `text` and `bg`
///
/// Those are the theme's two POLES, and [`audit`]'s `text/bg` rule
/// already guarantees they are far apart. Whichever pole a ground is
/// near, the other one reads on it — which is the general form of "black
/// font on a bright panel, white font on a dark panel" expressed in
/// colours the theme's author chose, rather than in literal black and
/// white that belong to no palette. Nothing here MINTS a colour:
/// `decision:no-palette-fill-in-helper` rules that the engine does not
/// invent tokens, and selecting between two authored ones by a
/// measurable property is the opposite of inventing a meaning.
///
/// ## It can still fail, and says so
///
/// Best-of-the-two-poles clears [`floors::TEXT`] on 51 of 52 measured
/// theme/declared-ground combinations. The one that does not is
/// `everforest-light` on a bright yellow panel (best 3.49): a soft light
/// palette holds no ink dark enough, and no choice among authored tokens
/// can rescue it. That case is REPORTED through [`Ink::contrast`] rather
/// than papered over — the caller picks a different panel colour, or
/// accepts it knowingly.
pub fn ink_on(t: &TokenSet, ground: Rgba) -> Ink {
    let candidates = [(TokenId::Text, t.text), (TokenId::Bg, t.bg)];
    let mut best = Ink {
        color: candidates[0].1,
        token: candidates[0].0,
        contrast: contrast_ratio(candidates[0].1, ground),
    };
    for (token, color) in candidates.into_iter().skip(1) {
        let contrast = contrast_ratio(color, ground);
        if contrast > best.contrast {
            best = Ink {
                color,
                token,
                contrast,
            };
        }
    }
    best
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
