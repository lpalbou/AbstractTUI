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
use abstracttui::render::color::{nearest_ansi16, nearest_xterm256, quantize_pair_256};
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

// ---------------------------------------------------------------------
// Costing the fix.
//
// The measurement above says WHAT is broken. The three tests below say
// what the candidate fixes would cost, because that is the fact that
// chooses between them and it should not have to be re-derived by hand
// (or trusted from a write-up) next time someone opens this file.
//
//   (b) move the eight seeds so the cube separates them
//   (c) separate the two grounds at render time, leaving the seeds alone
//
// One structural fact frames both, and it is not measurable so it is
// written down here instead. (c) was first sketched as "do it at emit,
// the way `quantize_pair_256` already does for fg/bg". That site cannot
// work. `sgr::resolve_pen` takes ONE cell: `cell.fg` and `cell.bg` are
// the two colours *inside* it, which is exactly why the fg/bg pair is
// available there. Two GROUNDS are never in one cell — a panel's fill and
// the field behind it are different cells — so the emitter does not have
// the pair and cannot be given it without handing it the scene. The
// policy below is right; the site has to be somewhere the two grounds are
// still known together (the token set at caps-resolution time, or the
// compositor), and choosing between those is the open part of the row.
//
// Perceptual distance here is CIE76 ΔE, with CIE76's own just-noticeable
// difference of 2.3. Crude next to ΔE2000, and deliberately paired with
// its own JND constant rather than a borrowed one — the two are only
// meaningful together. It is used for small nudges of near-neutral
// grounds, which is the region CIE76 handles least badly.
// ---------------------------------------------------------------------

/// CIE76's just-noticeable difference: below this, two colours side by
/// side are not reliably distinguishable.
const JND: f32 = 2.3;

fn srgb_to_linear(u: u8) -> f32 {
    let c = u as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lab(c: Rgba) -> (f32, f32, f32) {
    let (r, g, b) = (
        srgb_to_linear(c.r),
        srgb_to_linear(c.g),
        srgb_to_linear(c.b),
    );
    // D65 white point.
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

fn delta_e(a: Rgba, b: Rgba) -> f32 {
    let (l1, a1, b1) = lab(a);
    let (l2, a2, b2) = lab(b);
    ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
}

fn relative_luminance(c: Rgba) -> f32 {
    0.2126 * srgb_to_linear(c.r) + 0.7152 * srgb_to_linear(c.g) + 0.0722 * srgb_to_linear(c.b)
}

/// The colour closest to `orig` (CIE76) that quantises to a *different*
/// xterm-256 entry than `other`, searched over a ±14 box and required to
/// keep the author's elevation direction (whichever of the two grounds
/// was the lighter stays the lighter). This is option (b) performed
/// optimally: the smallest edit to one authored hex that buys back the
/// distinction.
fn nearest_separating(orig: Rgba, other: Rgba) -> Option<(Rgba, f32)> {
    const RADIUS: i32 = 14;
    let stays_lighter = relative_luminance(orig) >= relative_luminance(other);
    let q_other = nearest_xterm256(other);
    let mut best: Option<(Rgba, f32)> = None;
    for dr in -RADIUS..=RADIUS {
        for dg in -RADIUS..=RADIUS {
            for db in -RADIUS..=RADIUS {
                let (r, g, b) = (orig.r as i32 + dr, orig.g as i32 + dg, orig.b as i32 + db);
                if !(0..=255).contains(&r) || !(0..=255).contains(&g) || !(0..=255).contains(&b) {
                    continue;
                }
                let c = Rgba::rgb(r as u8, g as u8, b as u8);
                if nearest_xterm256(c) == q_other {
                    continue;
                }
                if (relative_luminance(c) >= relative_luminance(other)) != stays_lighter {
                    continue;
                }
                let d = delta_e(orig, c);
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((c, d));
                }
            }
        }
    }
    best
}

/// Every theme in the collapsed set, as `(bg, surface)`.
fn collapsed_grounds() -> Vec<(&'static str, Rgba, Rgba)> {
    KNOWN_256_COLLAPSES
        .iter()
        .map(|(id, _)| {
            let t = abstracttui::theme::get(id).expect("known-set theme is registered");
            (*id, t.tokens.bg, t.tokens.surface)
        })
        .collect()
}

/// **The cost of option (b), and it is not what I expected.** Every one
/// of the eight separates for a move that is *below* the just-noticeable
/// difference — provided you may choose which of the two grounds moves.
/// Nobody would see the edit at truecolor.
///
/// I opened this row assuming (b) meant visibly restyling published
/// themes and half-rejected it on that basis. It does not. Whatever
/// argues against (b) — and something does, see the note on the
/// following test — it is not the look.
#[test]
fn every_collapsed_ground_separates_for_a_sub_jnd_move() {
    for (id, bg, surface) in collapsed_grounds() {
        let by_surface = nearest_separating(surface, bg);
        let by_bg = nearest_separating(bg, surface);
        let cheapest = [by_surface, by_bg]
            .into_iter()
            .flatten()
            .map(|(_, d)| d)
            .fold(f32::INFINITY, f32::min);
        assert!(
            cheapest < JND,
            "{id}: cheapest separating edit is ΔE {cheapest:.2}, at or over \
             the {JND} JND — option (b) would be a visible restyle for this \
             theme and the costing that rejected it on other grounds needs \
             re-reading"
        );
    }
}

/// **The cost of option (c), and the reason it wins.** There is no new
/// algorithm to write: `quantize_pair_256` already re-picks a colliding
/// member to the nearest distinct entry preserving light/dark ordering,
/// and handed the two grounds it separates all eight — landing on
/// *exactly* the palette entry that option (b)'s optimal seed edit
/// arrives at, in every case.
///
/// So (b) and (c) produce the identical rendered result. (b) buys it by
/// editing eight authored hexes, six of which are third-party ports whose
/// entire value is byte-fidelity to upstream, and buys nothing at all for
/// a consumer palette arriving through `theme::Palette`. (c) buys it for
/// every theme, including ones this crate has never seen.
///
/// What this test does NOT say is that the fix is a small one. The policy
/// is settled; the *site* is the open problem, and it is not the emitter
/// — see the note on `resolve_pen` above.
#[test]
fn the_existing_repick_lands_where_the_ideal_seed_edit_lands() {
    for (id, bg, surface) in collapsed_grounds() {
        let (q_surface, q_bg) = quantize_pair_256(surface, bg);
        assert_ne!(
            q_surface, q_bg,
            "{id}: the existing re-pick policy failed to separate two \
             grounds — option (c) needs a new policy after all"
        );
        let (ideal, _) = nearest_separating(surface, bg)
            .unwrap_or_else(|| panic!("{id}: no separating surface edit"));
        assert_eq!(
            q_surface,
            nearest_xterm256(ideal),
            "{id}: the re-pick chose a different entry than the optimal \
             seed edit would reach — (b) and (c) stop being equivalent and \
             the choice between them has to be re-argued"
        );
    }
}

/// A DEFECT in the policy above, found by costing it, pinned so the fix
/// cannot forget it.
///
/// For fg/bg the question "which member moves?" has an obvious answer:
/// the foreground. Never move a background out from under the other cells
/// that share it. `quantize_pair_256` therefore always nudges its first
/// argument — and for two GROUNDS neither member is privileged, so that
/// rule picks by argument order instead of by merit.
///
/// It picks wrong here. In both light themes `surface` is `#ffffff`,
/// which the palette represents *exactly* (ΔE 0.00 — index 231 is pure
/// white), while `bg` is an off-white the palette can only approximate.
/// The policy sacrifices the one that was perfect. The optimal edit moves
/// `bg` instead, for ΔE 0.69 and 1.30 respectively.
///
/// Whoever writes the ground-aware separator: choose the member to nudge
/// by which is already further from its own palette entry, and delete
/// this test.
#[test]
fn the_repick_sacrifices_an_exactly_representable_ground() {
    let mut sacrificed = Vec::new();
    for (id, bg, surface) in collapsed_grounds() {
        let exact = delta_e(XTERM_256[nearest_xterm256(surface) as usize], surface) == 0.0;
        let (q_surface, _) = quantize_pair_256(surface, bg);
        if exact && q_surface != nearest_xterm256(surface) {
            sacrificed.push(id);
        }
    }
    assert_eq!(
        sacrificed,
        ["abstract-light", "one-light"],
        "the set of themes whose exactly-representable ground gets moved \
         changed. EMPTY: the ground-aware separator landed — good, delete \
         this test. OTHERWISE: a theme joined or left the case, and the \
         rule 'nudge whichever member is already inexact' needs re-checking \
         against it."
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
