//! Do a theme's distinct GROUNDS survive colour-depth quantisation?
//!
//! The contrast audit (`theme::audit`) runs on truecolor values, and
//! quantisation to `Ansi256`/`Ansi16` happens downstream at emit. So the
//! audit's guarantees are not carried across a depth downgrade — and
//! nothing else was checking what happens when they are not.
//!
//! `quantize_pair_256` already protects the case it was designed for: a
//! foreground and its own background collapsing to one entry, which
//! would erase text. What it cannot protect is two different GROUNDS
//! collapsing into each other, because they are never handed to it as a
//! pair. A panel is `surface` on `bg`; each cell's own fg/bg pair
//! survives, and the panel still vanishes.
//!
//! ## This file pins a DEFECT, not a guarantee
//!
//! The list below is what currently collapses. It is here so the set
//! cannot grow silently and so the fix has a baseline to move, NOT
//! because the behaviour is acceptable. `claim:tui-audit-does-not-
//! survive-quantisation` in `#commons` holds the open question of what
//! the fix is.

use abstracttui::base::palette::XTERM_256;
use abstracttui::base::Rgba;
use abstracttui::render::color::{nearest_ansi16, nearest_xterm256};
use abstracttui::theme::{themes, TokenSet};

/// The ground pairs a user reads as "these are different surfaces".
fn ground_pairs(t: &TokenSet) -> [(&'static str, Rgba, Rgba); 4] {
    [
        ("selection_bg vs bg", t.selection_bg, t.bg),
        ("surface vs bg", t.surface, t.bg),
        ("surface_raised vs surface", t.surface_raised, t.surface),
        ("border vs bg", t.border, t.bg),
    ]
}

/// Every theme+pair that currently collapses at 256 colours, measured.
/// All eight are `surface vs bg`, and two of them are the engine's own
/// house themes — `abstract-dark` is the default, so the out-of-the-box
/// experience on a 256-colour terminal has no visible panel elevation.
const KNOWN_256_COLLAPSES: &[(&str, &str)] = &[
    ("abstract-dark", "surface vs bg"),
    ("abstract-light", "surface vs bg"),
    ("observer-night", "surface vs bg"),
    ("catppuccin-mocha", "surface vs bg"),
    ("catppuccin-frappe", "surface vs bg"),
    ("rose-pine", "surface vs bg"),
    ("rose-pine-moon", "surface vs bg"),
    ("one-light", "surface vs bg"),
];

#[test]
fn ground_collapse_at_256_colours_is_exactly_the_known_set() {
    let mut found: Vec<(String, &'static str)> = Vec::new();
    for th in themes() {
        for (name, a, b) in ground_pairs(&th.tokens) {
            let (qa, qb) = (nearest_xterm256(a), nearest_xterm256(b));
            if qa == qb {
                found.push((th.id.to_string(), name));
            }
        }
    }
    let actual: Vec<(&str, &str)> = found.iter().map(|(t, n)| (t.as_str(), *n)).collect();
    assert_eq!(
        actual, KNOWN_256_COLLAPSES,
        "the set of grounds that collapse at 256 colours changed.\n\
         GREW: a new theme lost a surface distinction — fix it or the \
         theme ships with invisible panels on a 256-colour terminal.\n\
         SHRANK: something improved; update KNOWN_256_COLLAPSES and say \
         so on claim:tui-audit-does-not-survive-quantisation."
    );
}

/// The distinction that DOES survive, and it is the mitigation worth
/// knowing: a bordered panel still reads at 256 colours even when its
/// fill has collapsed into the ground. Elevation-by-border survives;
/// elevation-by-fill does not.
#[test]
fn borders_survive_256_quantisation_even_where_the_fill_does_not() {
    for th in themes() {
        let t = &th.tokens;
        assert_ne!(
            nearest_xterm256(t.border),
            nearest_xterm256(t.bg),
            "{}: border collapses into bg at 256 colours — the last cue \
             that a panel exists would be gone",
            th.id
        );
    }
}

/// `selection_bg` was the pair I expected to fail and it does not, in
/// any theme. Recorded as a passing property rather than dropped,
/// because the reasoning that predicted a failure was wrong in a
/// specific way worth keeping: `tint_until_readable` pushes the
/// selection far enough off the ground that the cube separates them,
/// even when `surface` — which is authored, not derived — does not.
/// The derived token survives quantisation better than the hand-picked
/// one.
#[test]
fn selection_never_collapses_into_its_ground_at_256_colours() {
    for th in themes() {
        let t = &th.tokens;
        assert_ne!(
            nearest_xterm256(t.selection_bg),
            nearest_xterm256(t.bg),
            "{}: selection band is invisible at 256 colours",
            th.id
        );
    }
}

/// Ansi16 is measured for the record and deliberately NOT asserted
/// against a floor: 36 of 104 ground pairs collapse there, and the
/// number is not actionable, because the 16 system registers are
/// user-themable. A terminal's `color4` is whatever the user set it to,
/// so no build-time check can know the rendered colour. `color.rs` says
/// as much — 16-colour is "best-effort by construction".
///
/// What this test does pin is that the measurement still RUNS, so the
/// number in the module docs stays honest.
#[test]
fn ansi16_ground_collapse_is_measured_and_unknowable() {
    let mut collapsed = 0;
    let mut total = 0;
    for th in themes() {
        for (_, a, b) in ground_pairs(&th.tokens) {
            total += 1;
            if nearest_ansi16(a) == nearest_ansi16(b) {
                collapsed += 1;
            }
        }
    }
    assert_eq!(total, 104, "26 themes x 4 ground pairs");
    assert!(
        collapsed > 0 && collapsed < total,
        "expected SOME 16-colour collapse and not total collapse, got \
         {collapsed}/{total} — a 0 or a {total} means the measurement \
         stopped measuring"
    );
}

/// The worked example that started this, kept executable so the number
/// in the write-up cannot rot: a consumer reported a selected card
/// header painting `rgba(48,48,48)` where the token is `rgba(88,39,61)`.
/// That is not a stray grey — it is the token, quantised.
#[test]
fn the_reported_grey_is_the_selection_token_quantised() {
    let t = abstracttui::theme::get("abstract-dark").expect("house theme");
    let sel = t.tokens.selection_bg;
    assert_eq!((sel.r, sel.g, sel.b), (88, 39, 61));
    let idx = nearest_xterm256(sel);
    assert_eq!(idx, 236, "xterm-256 index");
    let q = XTERM_256[idx as usize];
    assert_eq!(
        (q.r, q.g, q.b),
        (48, 48, 48),
        "index 236 is the grey ramp at 8 + 10*4 — the reported colour"
    );
}
