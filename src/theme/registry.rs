//! Theme registry: the AbstractUIC family, built from faithful seeds plus
//! documented derivations, contrast-audited as a test invariant.
//!
//! ## Derivation rules (tokens `theme.css` does not define per theme)
//!
//! - `border` — theme.css draws hairlines as a 12% ink wash
//!   (`--border-default`). Terminals need opaque cells, so we composite:
//!   mix `bg` toward `text` starting at t=0.12, stepping +0.02 until the
//!   1.5:1 border floor holds. Using the theme's own text as ink keeps
//!   borders hue-honest (gruvbox gets warm cream strokes, not clinical
//!   white).
//! - `border_focus` — `accent`, full strength. Deliberate terminal
//!   adaptation: theme.css softens focus rings to a 55% wash because 1px
//!   web hairlines glare; glyph-drawn borders are already visually thin,
//!   and the focused pane must win at a glance.
//! - `overlay` — `rgba(0,0,0,0.45)`, the exact `.af-connect-overlay` scrim,
//!   both polarities (the web kit uses black over light themes too).
//! - `shadow` — dark: black @ 0.35 (the `--ui-shadow-1` color); light: the
//!   theme's text ink @ 0.22 (theme.css uses ink @ 0.12 *blurred*; hard
//!   cell shadows have no blur, so equal perceptual weight needs more
//!   alpha — value chosen by eye, revisit on the real compositor).
//! - `selection_bg` — accent tint over `bg`, starting at t=0.30 (the web
//!   `--accent-subtle` 12% wash scaled up: terminals have no ::selection
//!   chrome, the tint is the only affordance), walking *down* by 0.02
//!   until `text` reads at 4.5:1 on it. `selection_fg` = `text`.
//! - `cursor` — `accent` (the caret is brand-colored).
//! - `link` — `info` (links are informational affordances; underline comes
//!   from the style attr, not the color).
//! - `accent_alt` — seeded from the theme.ts swatch strip (curated second
//!   accent); nudged toward `text` in 4% steps if it misses the 3:1 accent
//!   floor (solarized-light's cyan is the one case in the family).
//! - `chart[0..8]` — `[accent, info, ok, warn, error, violet, teal,
//!   orange]` where violet = accent<->info midpoint, teal = ok<->info
//!   midpoint, orange = error<->warn midpoint. Midpoints inherit the 3:1
//!   floor (sRGB lerp keeps luminance between endpoints). A near-duplicate
//!   pass then nudges colliding slots toward `text` so every series stays
//!   tellable-apart (several families curate accent_alt == info, which
//!   would otherwise clone a slot).
//! - `shadow_ground` (cycle 6) — `shadow` composited over `bg` once at
//!   theme build: the OPAQUE ground for cell-level elevation strips
//!   (`Block::shadow`), so widgets never run `over()` themselves.
//! - `syntax_*` (cycle 6) — code inks over `surface_raised` (the declared
//!   code ground): keyword <- `accent`, string <- `ok`, number <- `warn`,
//!   type <- `accent_alt`, func <- `info`, punct <- `text_muted`,
//!   comment <- `text_faint`. Each walks toward `text` until it clears
//!   its floor against `surface_raised`: primary inks 4.5:1, comment
//!   3:1 (comments recede by design) — CAPPED at what the theme's own
//!   `text` achieves there (code can never be more readable than body
//!   text; everforest-light's soft 4.25:1 ceiling is the motivating
//!   case). A chart-style separation pass then de-clones near-identical
//!   inks (green-on-green families like everforest) by stepping later
//!   ones toward `text`.
//!
//! OWNER: DESIGN.

use std::sync::OnceLock;

use crate::base::Rgba;
use crate::theme::contrast::floors;
use crate::theme::derive::{mix, mix_until_contrast, tint_until_readable};
use crate::theme::seeds::{ThemeSeed, SEEDS, UPSTREAM_ALIASES};
use crate::theme::tokens::TokenSet;

/// A registered theme: identity, polarity, resolved tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Stable kebab-case id (config files, `get`, upstream parity).
    pub id: &'static str,
    /// Human label for pickers.
    pub label: &'static str,
    /// Decisive polarity; audited against `L(bg)`.
    pub dark: bool,
    /// The resolved palette.
    pub tokens: TokenSet,
}

impl Theme {
    /// Decisive polarity (audited: the flag always agrees with measured
    /// ground luminance). Method form so call sites read as a question.
    pub fn is_dark(&self) -> bool {
        self.dark
    }
}

/// The engine default: the AbstractFramework house dark palette.
pub const DEFAULT_THEME_ID: &str = "abstract-dark";

static REGISTRY: OnceLock<Vec<Theme>> = OnceLock::new();

/// The built-in family, registry order (house pair first, then dark
/// families, then light — mirrors the upstream picker order). Runtime
/// registrations are separate (`register.rs`); [`get`]/[`list`] unify the
/// two so consumers see one registry.
pub fn themes() -> &'static [Theme] {
    REGISTRY.get_or_init(|| SEEDS.iter().map(build).collect())
}

/// `(id, label, dark)` for every visible theme — built-ins first, then
/// runtime registrations (newest per id). The picker surface.
pub fn list() -> Vec<(&'static str, &'static str, bool)> {
    themes()
        .iter()
        .map(|t| (t.id, t.label, t.dark))
        .chain(
            crate::theme::register::user_list()
                .into_iter()
                .map(|t| (t.id, t.label, t.dark)),
        )
        .collect()
}

/// Exact lookup by id, honoring upstream AbstractUIC aliases
/// (`"dark"`/`"light"` map to the house pair). Built-ins win over runtime
/// registrations (an app theme can never shadow `nord`; the register path
/// also refuses reserved ids up front). Whitespace-trimmed, case-sensitive
/// by design (ids are machine identifiers).
pub fn get(id: &str) -> Option<&'static Theme> {
    let id = id.trim();
    let canonical = UPSTREAM_ALIASES
        .iter()
        .find(|(alias, _)| *alias == id)
        .map(|(_, target)| *target)
        .unwrap_or(id);
    themes()
        .iter()
        .find(|t| t.id == canonical)
        .or_else(|| crate::theme::register::user_get(canonical))
}

/// Lookup with fallback: unknown ids resolve to the default theme and
/// return a labeled warning (the `#FALLBACK` convention — callers surface
/// it, never swallow it).
pub fn resolve(id: &str) -> (&'static Theme, Option<String>) {
    match get(id) {
        Some(t) => (t, None),
        None => (
            default_theme(),
            Some(format!(
                "#FALLBACK: unknown theme id '{}', using default '{}'",
                id.trim(),
                DEFAULT_THEME_ID
            )),
        ),
    }
}

/// The default theme (also backs `TokenSet::default()`).
pub fn default_theme() -> &'static Theme {
    get(DEFAULT_THEME_ID).expect("default theme must be registered")
}

/// Within-theme role hygiene: grounds and inks must never share a value,
/// text tiers must be pairwise distinct, selection must not be invisible.
/// Deliberate aliases (`border_focus`/`cursor` = accent, `link` = info,
/// chart slots reusing accents) are exempt — they are one role wearing its
/// documented color, not two roles colliding.
pub fn hygiene_violations(theme: &Theme) -> Vec<String> {
    let t = &theme.tokens;
    let mut out = Vec::new();

    let grounds = [
        ("bg", t.bg),
        ("surface", t.surface),
        ("surface_raised", t.surface_raised),
    ];
    let inks = [
        ("text", t.text),
        ("text_muted", t.text_muted),
        ("text_faint", t.text_faint),
        ("accent", t.accent),
        ("accent_alt", t.accent_alt),
        ("ok", t.ok),
        ("warn", t.warn),
        ("error", t.error),
        ("info", t.info),
        ("link", t.link),
        ("border_focus", t.border_focus),
        ("cursor", t.cursor),
    ];
    for (gn, g) in grounds {
        for (in_, i) in inks {
            if g == i {
                out.push(format!(
                    "[{}] ground '{}' equals ink '{}'",
                    theme.id, gn, in_
                ));
            }
        }
    }

    let tiers = [
        ("text", t.text),
        ("text_muted", t.text_muted),
        ("text_faint", t.text_faint),
    ];
    for a in 0..tiers.len() {
        for b in (a + 1)..tiers.len() {
            if tiers[a].1 == tiers[b].1 {
                out.push(format!(
                    "[{}] text tiers '{}' and '{}' are identical",
                    theme.id, tiers[a].0, tiers[b].0
                ));
            }
        }
    }

    if t.selection_bg == t.bg {
        out.push(format!(
            "[{}] selection_bg is invisible (equals bg)",
            theme.id
        ));
    }
    if t.selection_bg == t.selection_fg {
        out.push(format!("[{}] selection_fg equals selection_bg", theme.id));
    }
    if t.border == t.bg {
        out.push(format!("[{}] border is invisible (equals bg)", theme.id));
    }
    out
}

/// Scrim alpha, ported from `.af-connect-overlay` (`rgba(0,0,0,0.45)`).
const OVERLAY_ALPHA: u8 = 115; // 0.45 * 255
/// Dark shadow alpha, ported from `--ui-shadow-1` (`rgba(0,0,0,0.35)`).
const SHADOW_ALPHA_DARK: u8 = 89; // 0.35 * 255
/// Light shadow alpha (adapted: blurless cell shadows need more than the
/// web's 0.12 — see module docs).
const SHADOW_ALPHA_LIGHT: u8 = 56; // 0.22 * 255

/// Two chart entries closer than this (max channel delta) count as clones
/// and get separated. 24/255 is roughly the point where adjacent series in
/// a legend stop being tellable-apart at cell size.
const CHART_MIN_CHANNEL_DELTA: u8 = 24;

fn hex(field: &'static str, seed_id: &str, s: &str) -> Rgba {
    // Seeds are compile-time data; a bad hex is a programmer error and the
    // seed unit test names it before anything gets here.
    Rgba::from_hex(s)
        .unwrap_or_else(|| panic!("theme seed {seed_id}: field {field} has bad hex {s}"))
}

fn build(seed: &ThemeSeed) -> Theme {
    let bg = hex("bg", seed.id, seed.bg);
    let surface = hex("surface", seed.id, seed.surface);
    let surface_raised = hex("surface_raised", seed.id, seed.surface_raised);
    let text = hex("text", seed.id, seed.text);
    let text_muted = hex("text_muted", seed.id, seed.text_muted);
    let text_faint = hex("text_faint", seed.id, seed.text_faint);
    let accent = hex("accent", seed.id, seed.accent);
    let ok = hex("ok", seed.id, seed.ok);
    let warn = hex("warn", seed.id, seed.warn);
    let error = hex("error", seed.id, seed.error);
    let info = hex("info", seed.id, seed.info);

    // Curated second accent, contrast-guarded (see module docs).
    let accent_alt_seed = hex("accent_alt", seed.id, seed.accent_alt);
    let accent_alt = mix_until_contrast(accent_alt_seed, text, bg, 0.0, 0.04, floors::ACCENT);

    let border = mix_until_contrast(bg, text, bg, 0.12, 0.02, floors::BORDER);
    let selection_bg =
        tint_until_readable(bg, accent, text, 0.30, 0.02, 0.10, floors::SELECTION_TEXT);

    let shadow = if seed.dark {
        Rgba::BLACK.with_alpha(SHADOW_ALPHA_DARK)
    } else {
        text.with_alpha(SHADOW_ALPHA_LIGHT)
    };

    let chart = build_chart(text, [accent, info, ok, warn, error]);
    let syntax = build_syntax(
        surface_raised,
        text,
        text_muted,
        text_faint,
        [accent, ok, warn, accent_alt, info],
    );

    Theme {
        id: seed.id,
        label: seed.label,
        dark: seed.dark,
        tokens: TokenSet {
            bg,
            surface,
            surface_raised,
            overlay: Rgba::BLACK.with_alpha(OVERLAY_ALPHA),
            border,
            border_focus: accent,
            text,
            text_muted,
            text_faint,
            accent,
            accent_alt,
            ok,
            warn,
            error,
            info,
            selection_bg,
            selection_fg: text,
            cursor: accent,
            link: info,
            shadow,
            shadow_ground: shadow.over(bg),
            chart,
            syntax_keyword: syntax[0],
            syntax_string: syntax[1],
            syntax_number: syntax[2],
            syntax_type: syntax[3],
            syntax_func: syntax[4],
            syntax_punct: syntax[5],
            syntax_comment: syntax[6],
        },
    }
}

/// Effective syntax floor on `ground`: the target, capped at what the
/// theme's own body text achieves there (code can never out-read text).
pub(crate) fn syntax_floor(target: f32, text: Rgba, ground: Rgba) -> f32 {
    target.min(crate::theme::contrast::contrast_ratio(text, ground))
}

/// Derive the seven syntax inks (see module docs). Order:
/// [keyword, string, number, type, func, punct, comment].
fn build_syntax(
    ground: Rgba,
    text: Rgba,
    text_muted: Rgba,
    text_faint: Rgba,
    [accent, ok, warn, accent_alt, info]: [Rgba; 5],
) -> [Rgba; 7] {
    let primary = syntax_floor(floors::SYNTAX, text, ground);
    let secondary = syntax_floor(floors::SYNTAX_COMMENT, text, ground);
    let lift = |ink: Rgba, floor: f32| mix_until_contrast(ink, text, ground, 0.0, 0.04, floor);
    let mut inks = [
        lift(accent, primary),       // keyword — the skeleton, strongest ink
        lift(ok, primary),           // string — the near-universal green
        lift(warn, primary),         // number — amber literals
        lift(accent_alt, primary),   // type — the companion accent
        lift(info, primary),         // func — call sites
        lift(text_muted, primary),   // punct — structure, quiet but legible
        lift(text_faint, secondary), // comment — recedes by design
    ];
    // De-clone (green-on-green families): later inks step toward text
    // until they separate; stepping toward text preserves the floor.
    for i in 1..inks.len() {
        let mut tries = 0;
        while tries < 4 && inks[..i].iter().any(|prev| too_close(*prev, inks[i])) {
            inks[i] = mix(inks[i], text, 0.18);
            tries += 1;
        }
    }
    inks
}

/// Build the 8-slot categorical ramp and separate near-clones (several
/// families curate near-identical accent/info pairs; charts must never
/// hand two series the same pen).
fn build_chart(text: Rgba, [accent, info, ok, warn, error]: [Rgba; 5]) -> [Rgba; 8] {
    let mut chart = [
        accent,
        info,
        ok,
        warn,
        error,
        mix(accent, info, 0.5), // violet-family bridge
        mix(ok, info, 0.5),     // teal
        mix(error, warn, 0.5),  // orange
    ];
    for i in 1..chart.len() {
        // Nudge toward text: monotonically improves bg contrast on either
        // polarity, so the 3:1 chart floor survives the separation pass.
        let mut tries = 0;
        while tries < 4 && chart[..i].iter().any(|prev| too_close(*prev, chart[i])) {
            chart[i] = mix(chart[i], text, 0.22);
            tries += 1;
        }
    }
    chart
}

fn too_close(a: Rgba, b: Rgba) -> bool {
    let d = |x: u8, y: u8| x.abs_diff(y);
    d(a.r, b.r).max(d(a.g, b.g)).max(d(a.b, b.b)) < CHART_MIN_CHANNEL_DELTA
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::contrast::{audit, contrast_ratio, decisiveness, DECISIVENESS_MARGIN};

    #[test]
    fn every_theme_passes_the_contrast_audit() {
        use crate::theme::contrast::AUDIT_EXCEPTIONS;
        let mut unexpected: Vec<String> = Vec::new();
        let mut fired: Vec<(&str, &str)> = Vec::new();
        for theme in themes() {
            for v in audit(theme.id, &theme.tokens) {
                if AUDIT_EXCEPTIONS
                    .iter()
                    .any(|(t, r)| v.theme == *t && v.rule == *r)
                {
                    fired.push((theme.id, v.rule));
                } else {
                    unexpected.push(v.to_string());
                }
            }
        }
        assert!(
            unexpected.is_empty(),
            "contrast violations:\n{}",
            unexpected.join("\n")
        );
        // Staleness guard: an exception that no longer fires must be
        // deleted, not grandfathered.
        for e in AUDIT_EXCEPTIONS {
            assert!(fired.contains(e), "stale audit exception {e:?} — remove it");
        }
    }

    #[test]
    fn every_theme_passes_role_hygiene() {
        let mut all: Vec<String> = Vec::new();
        for theme in themes() {
            all.extend(hygiene_violations(theme));
        }
        assert!(all.is_empty(), "hygiene violations:\n{}", all.join("\n"));
    }

    #[test]
    fn ids_unique_and_polarity_decisive() {
        let mut ids: Vec<&str> = themes().iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), themes().len());

        for t in themes() {
            let l = t.tokens.bg.luminance();
            let margin = decisiveness(t.tokens.bg);
            assert!(
                margin >= DECISIVENESS_MARGIN,
                "[{}] indecisive ground: L={l:.3}",
                t.id
            );
            // The declared flag must agree with the measured polarity.
            assert_eq!(
                t.dark,
                l < 0.5,
                "[{}] dark flag disagrees with L={l:.3}",
                t.id
            );
        }
    }

    #[test]
    fn family_coverage_is_complete() {
        for id in [
            "abstract-dark",
            "abstract-light",
            "observer-night",
            "catppuccin-mocha",
            "catppuccin-macchiato",
            "catppuccin-frappe",
            "catppuccin-latte",
            "rose-pine",
            "rose-pine-moon",
            "rose-pine-dawn",
            "tokyo-night",
            "nord",
            "one-dark",
            "one-light",
            "dracula",
            "monokai",
            "gruvbox",
            "solarized-dark",
            "solarized-light",
            "everforest-dark",
            "everforest-light",
            // Original AbstractTUI themes (cycle 5).
            "abstract-aurora",
            "abstract-paper",
            "abstract-ember",
            // Cycle 7 originals.
            "abstract-midnight",
            "abstract-dawn",
        ] {
            assert!(get(id).is_some(), "missing family theme {id}");
        }
        assert_eq!(themes().len(), 26);
    }

    #[test]
    fn faithful_port_spot_checks_against_theme_css() {
        // Byte-level pins on values copied from theme.css — if these move,
        // the port drifted from the source of truth.
        let px = |s: &str| Rgba::from_hex(s).unwrap();
        let mocha = get("catppuccin-mocha").unwrap().tokens;
        assert_eq!(mocha.bg, px("#1e1e2e"));
        assert_eq!(mocha.surface, px("#181825")); // mantle is darker than base: faithful
        assert_eq!(mocha.accent, px("#cba6f7"));
        let gruv = get("gruvbox").unwrap().tokens;
        assert_eq!(gruv.accent, px("#fe8019"));
        assert_eq!(gruv.text, px("#ebdbb2"));
        let sol = get("solarized-light").unwrap().tokens;
        assert_eq!(sol.surface, px("#eee8d5"));
        assert_eq!(sol.warn, px("#916e00"));
        let obs = get("observer-night").unwrap().tokens;
        assert_eq!(obs.accent, px("#e8a54a"));
        assert_eq!(obs.text_faint, px("#748096"));
    }

    #[test]
    fn alpha_discipline_only_overlay_and_shadow_blend() {
        for t in themes() {
            for (id, c) in t.tokens.iter() {
                let expect_alpha = matches!(
                    id,
                    crate::theme::tokens::TokenId::Overlay | crate::theme::tokens::TokenId::Shadow
                );
                assert_eq!(
                    !c.is_opaque(),
                    expect_alpha,
                    "[{}] token {:?} has unexpected alpha {}",
                    t.id,
                    id,
                    c.a
                );
            }
        }
    }

    #[test]
    fn chart_slots_are_pairwise_separated() {
        for t in themes() {
            let c = t.tokens.chart;
            for i in 0..c.len() {
                for j in (i + 1)..c.len() {
                    assert!(
                        !too_close(c[i], c[j]),
                        "[{}] chart{} and chart{} are near-clones ({} vs {})",
                        t.id,
                        i,
                        j,
                        c[i].to_hex(),
                        c[j].to_hex()
                    );
                }
            }
        }
    }

    #[test]
    fn selection_stays_visible_and_readable() {
        for t in themes() {
            let tk = &t.tokens;
            assert!(
                contrast_ratio(tk.selection_fg, tk.selection_bg) >= floors::SELECTION_TEXT,
                "[{}] selected text unreadable",
                t.id
            );
            // The tint must actually tint: some channel moves visibly.
            assert!(
                tk.selection_bg != tk.bg,
                "[{}] selection tint vanished into the ground",
                t.id
            );
        }
    }

    #[test]
    fn lookup_alias_and_fallback_contract() {
        assert_eq!(get("dark").unwrap().id, "abstract-dark");
        assert_eq!(get("light").unwrap().id, "abstract-light");
        assert_eq!(get(" nord ").unwrap().id, "nord");
        assert!(get("no-such-theme").is_none());

        let (t, warn) = resolve("no-such-theme");
        assert_eq!(t.id, DEFAULT_THEME_ID);
        let warn = warn.expect("fallback must carry a warning");
        assert!(
            warn.starts_with("#FALLBACK"),
            "warning must be labeled: {warn}"
        );
        assert!(warn.contains("no-such-theme"));

        let (t, warn) = resolve("gruvbox");
        assert_eq!(t.id, "gruvbox");
        assert!(warn.is_none());
    }

    #[test]
    fn borders_stay_subtle_not_shouting() {
        // The walk must satisfy the floor with the *smallest* passing mix:
        // borders are hairlines, not content. 3:1 is where a border starts
        // competing with text_muted.
        for t in themes() {
            let r = contrast_ratio(t.tokens.border, t.tokens.bg);
            assert!(r >= floors::BORDER, "[{}] border under floor: {r:.2}", t.id);
            assert!(r < 3.2, "[{}] border shouting at {r:.2}:1", t.id);
        }
    }
}
