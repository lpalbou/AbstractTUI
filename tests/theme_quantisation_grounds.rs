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
//!
//! ## Why the pair set is exhaustive, and what it cost to learn
//!
//! The first version of this file measured four pairs I picked by hand:
//! `selection_bg`/`bg`, `surface`/`bg`, `surface_raised`/`surface`,
//! `border`/`bg`. It found 8 collapses, all of them `surface vs bg`, and
//! I wrote up that uniformity as if it were a finding.
//!
//! It was an artefact of the list. Measuring **every pair** of the five
//! opaque grounds instead — 10 pairs per theme, 260 in all — finds 15
//! collapses across SIX different pairs, in 15 of the 26 themes. The
//! hand-picked set missed `shadow_ground` entirely (4 collapses, and its
//! only job is elevation), missed `surface_raised vs selection_bg` (2),
//! and missed `bg vs surface_raised` (1). Nearly half the affected themes
//! were invisible to it.
//!
//! Choosing which pairs to measure is choosing what you are able to find.
//! The pair set here is now generated from the ground list, so adding a
//! ground to `TokenSet` adds its pairs automatically and cannot be
//! forgotten.
//!
//! `border` is deliberately NOT in the ground list: it is a stroke drawn
//! ON a ground, not a ground, and it keeps its own test below.

use abstracttui::base::palette::XTERM_256;
use abstracttui::base::{Rgba, Size};
use abstracttui::render::color::{
    nearest_ansi16, nearest_xterm256, quantize_pair_256, quantize_set_256,
    quantize_set_256_into_with, GroundIntent, PairIntent,
};
use abstracttui::render::{Cell, ColorDepth, FrameDiff, PresentCaps, Presenter, Style, Surface};
use abstracttui::testing::{xterm_256, VtScreen};
use abstracttui::theme::{themes, TokenSet};

/// The OPAQUE grounds — every token a widget can paint a region with, and
/// therefore every token a user can read as "this is a different
/// surface". `overlay` is excluded because it carries alpha and is
/// composited over whatever it covers, so it has no fixed value to
/// quantise.
///
/// The list itself now lives in the engine (`TokenSet::grounds`), because
/// the downlevel path needs it too and two copies would drift: a ground
/// added to `TokenSet` and missed here would simply stop being measured,
/// which is the failure this whole file exists to prevent. This wrapper
/// only re-attaches the snake_case names the pinned sets below are
/// written in — themselves the engine's (`TokenId::name`), not new
/// strings.
fn grounds(t: &TokenSet) -> [(&'static str, Rgba); 5] {
    t.grounds().map(|(id, c)| (id.name(), c))
}

/// Every unordered pair of grounds, named `"a vs b"` in declaration
/// order. Ten per theme. The second member is the one a re-pick would
/// move (see `the_existing_repick_lands_where_the_ideal_seed_edit_lands`).
fn ground_pairs(t: &TokenSet) -> Vec<(String, Rgba, Rgba)> {
    let g = grounds(t);
    let mut out = Vec::new();
    for i in 0..g.len() {
        for j in (i + 1)..g.len() {
            out.push((format!("{} vs {}", g[i].0, g[j].0), g[i].1, g[j].1));
        }
    }
    out
}

/// Every theme+pair that currently collapses at 256 colours, measured
/// exhaustively. Fifteen of the twenty-six themes have one.
///
/// Two of them are the house themes, and `abstract-dark` is the engine
/// default — so out of the box, on a 256-colour terminal, this library
/// renders no visible panel elevation.
///
/// The four `shadow_ground` entries are the ones the hand-picked pair set
/// could not see, and they are not a lesser case: `shadow_ground` exists
/// solely to draw `Block::shadow` elevation strips, so a theme where it
/// collapses into the ground it is drawn on has a shadow that renders as
/// nothing at all.
const KNOWN_256_COLLAPSES: &[(&str, &str)] = &[
    ("abstract-dark", "bg vs surface"),
    ("abstract-light", "bg vs surface"),
    ("observer-night", "bg vs surface"),
    ("catppuccin-mocha", "bg vs surface"),
    ("catppuccin-macchiato", "surface vs shadow_ground"),
    ("catppuccin-frappe", "bg vs surface"),
    ("rose-pine", "bg vs surface"),
    ("rose-pine-moon", "bg vs surface"),
    ("tokyo-night", "surface_raised vs selection_bg"),
    ("solarized-dark", "surface_raised vs selection_bg"),
    ("catppuccin-latte", "surface_raised vs shadow_ground"),
    ("rose-pine-dawn", "bg vs surface_raised"),
    ("one-light", "bg vs surface"),
    ("everforest-light", "surface_raised vs shadow_ground"),
    ("abstract-midnight", "bg vs shadow_ground"),
];

#[test]
fn ground_collapse_at_256_colours_is_exactly_the_known_set() {
    let mut found: Vec<(String, String)> = Vec::new();
    for th in themes() {
        for (name, a, b) in ground_pairs(&th.tokens) {
            if nearest_xterm256(a) == nearest_xterm256(b) {
                found.push((th.id.to_string(), name));
            }
        }
    }
    let actual: Vec<(&str, &str)> = found
        .iter()
        .map(|(t, n)| (t.as_str(), n.as_str()))
        .collect();
    assert_eq!(
        actual, KNOWN_256_COLLAPSES,
        "the set of grounds that collapse at 256 colours changed.\n\
         GREW: a theme lost a surface distinction — fix it or the theme \
         ships with two grounds that render as one on a 256-colour \
         terminal.\n\
         SHRANK: something improved; update KNOWN_256_COLLAPSES and say \
         so on claim:tui-audit-does-not-survive-quantisation."
    );
}

/// **The fact that decides where the fix goes.** No theme has more than
/// one colliding pair, and no theme's five grounds need more than five
/// distinct palette entries out of the 240 available.
///
/// This matters because it rules out the harder shape of the problem. A
/// three-way pileup — three grounds in one cell — would need a joint
/// solve, and a joint solve needs to know which grounds are actually
/// adjacent on screen, which only the compositor knows. A single pairwise
/// collision per theme does not: making the ground set **pairwise
/// distinct** separates every adjacency at once, whatever sits on what.
///
/// That is what makes a per-theme fix sufficient. Widgets choose a ground
/// token unconditionally — `let ground = t.surface;` — with no knowledge
/// of what they are drawn on, so the same token genuinely does appear
/// over different grounds (a `surface_raised` table header sits on
/// `surface`; a `surface_raised` badge sits on `bg`). Pairwise
/// distinctness is placement-independent, so that variety stops
/// mattering.
#[test]
fn no_theme_needs_a_three_way_ground_solve() {
    for th in themes() {
        let g = grounds(&th.tokens);
        let collisions = ground_pairs(&th.tokens)
            .iter()
            .filter(|(_, a, b)| nearest_xterm256(*a) == nearest_xterm256(*b))
            .count();
        assert!(
            collisions <= 1,
            "{}: {collisions} colliding ground pairs. More than one means \
             the pairwise re-pick may not converge on its own and the fix \
             needs a joint solve — which needs adjacency, which only the \
             compositor knows.",
            th.id
        );
        let mut idx: Vec<u8> = g.iter().map(|(_, c)| nearest_xterm256(*c)).collect();
        idx.sort_unstable();
        idx.dedup();
        assert!(
            idx.len() >= g.len() - 1,
            "{}: five grounds occupy only {} palette entries — more than \
             one pair has merged",
            th.id,
            idx.len()
        );
    }
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

/// `selection_bg` vs `bg` never collapses, in any theme — and the
/// reasoning that predicted it would is worth keeping, because it holds:
/// `tint_until_readable` pushes the selection far enough off the ground
/// that the cube separates them, where `surface` — authored, not derived
/// — lands a few units away and does not. The derived token survives
/// quantisation better than the hand-picked one.
///
/// What was WRONG was the scope of the claim, not its content. The
/// earlier version of this test checked `selection_bg` against `bg`
/// alone, passed, and was written up as "selection never collapses into
/// its ground". A selection band sits on more than one ground: select a
/// row inside a popover and it is drawn on `surface_raised`. Against
/// THAT ground it does collapse, in `tokyo-night` and `solarized-dark`.
///
/// So the property is narrower than it read, and the test now says which
/// ground it holds against and pins the exception rather than passing by
/// not looking.
#[test]
fn selection_survives_against_bg_but_not_against_every_ground() {
    for th in themes() {
        assert_ne!(
            nearest_xterm256(th.tokens.selection_bg),
            nearest_xterm256(th.tokens.bg),
            "{}: selection band is invisible on the app ground at 256 \
             colours — this is the one selection property that has always \
             held, and it just stopped",
            th.id
        );
    }
    let on_raised: Vec<&str> = themes()
        .iter()
        .filter(|th| {
            nearest_xterm256(th.tokens.selection_bg) == nearest_xterm256(th.tokens.surface_raised)
        })
        .map(|th| th.id)
        .collect();
    assert_eq!(
        on_raised,
        ["tokyo-night", "solarized-dark"],
        "the set of themes whose selection vanishes on `surface_raised` \
         changed — a selected row inside a popover is the case this \
         covers, and it is in KNOWN_256_COLLAPSES too"
    );
}

/// Ansi16 is measured for the record and deliberately NOT asserted
/// against a floor: 98 of 260 ground pairs collapse there, and the
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
    assert_eq!(total, 260, "26 themes x 10 ground pairs");
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

/// The known-set entries resolved to actual colours: `(theme, pair name,
/// anchor, mover)`. The MOVER is the pair's second member — the one a
/// re-pick nudges and the one an optimal seed edit is measured on first.
fn collapsed_grounds() -> Vec<(&'static str, &'static str, Rgba, Rgba)> {
    KNOWN_256_COLLAPSES
        .iter()
        .map(|(id, pair)| {
            let t = abstracttui::theme::get(id).expect("known-set theme is registered");
            let (anchor_name, mover_name) = pair.split_once(" vs ").expect("pair name is 'a vs b'");
            let of = |want: &str| {
                grounds(&t.tokens)
                    .into_iter()
                    .find(|(n, _)| *n == want)
                    .unwrap_or_else(|| panic!("{id}: no ground named {want}"))
                    .1
            };
            (*id, *pair, of(anchor_name), of(mover_name))
        })
        .collect()
}

/// **The cost of option (b) — and the exhaustive pair set changed the
/// answer.** Fourteen of the fifteen collapsed pairs separate for a move
/// *below* the just-noticeable difference (worst 1.30), provided you may
/// choose which of the two grounds moves. For those, nobody would see
/// the edit at truecolor, and my assumption that (b) meant visibly
/// restyling published themes was wrong.
///
/// The fifteenth is `solarized-dark`, and it is worth more than the
/// fourteen. Its `surface_raised` and `selection_bg` are both dark teals
/// landing in a sparse region of the cube:
///
/// - moving `selection_bg` costs ΔE **6.99** — plainly visible; and
/// - moving `surface_raised` is not possible AT ALL inside the ±14 box,
///   under the constraint that the two keep their luminance order.
///
/// `selection_bg` is also DERIVED, not authored, so "edit the seed" does
/// not even reach it — you would have to retune `tint_until_readable`,
/// which moves every theme to fix one.
///
/// So option (b) is not merely expensive in provenance for this case: for
/// one theme in the set it is **unavailable**. That is the strongest
/// argument against it and it only exists because the pair set stopped
/// being hand-picked.
const B_IS_NOT_FREE_FOR: &[(&str, &str)] = &[("solarized-dark", "surface_raised vs selection_bg")];

#[test]
fn every_collapsed_ground_separates_for_a_sub_jnd_move() {
    let mut expensive = Vec::new();
    for (id, pair, anchor, mover) in collapsed_grounds() {
        let cheapest = [
            nearest_separating(mover, anchor),
            nearest_separating(anchor, mover),
        ]
        .into_iter()
        .flatten()
        .map(|(_, d)| d)
        .fold(f32::INFINITY, f32::min);
        assert!(
            cheapest.is_finite(),
            "{id} ({pair}): NEITHER ground can be separated inside the ±14 \
             box — option (b) cannot fix this theme by any small edit"
        );
        if cheapest >= JND {
            expensive.push((id, pair));
        }
    }
    assert_eq!(
        expensive, B_IS_NOT_FREE_FOR,
        "the set of pairs option (b) cannot fix invisibly changed.\n\
         GREW: another theme now needs a VISIBLE edit to separate — (b) \
         gets worse and the argument for fixing this at render time gets \
         stronger.\n\
         SHRANK: a theme became cheap to fix; update the list and say so \
         on claim:tui-audit-does-not-survive-quantisation."
    );
}

/// The half of the `solarized-dark` finding an equality check cannot
/// carry: its `surface_raised` has NO separating colour in range, so the
/// pair is not merely costly to fix by hand — one side of it is stuck.
#[test]
fn solarized_darks_raised_ground_cannot_be_moved_at_all() {
    let t = abstracttui::theme::get("solarized-dark").expect("house port");
    assert!(
        nearest_separating(t.tokens.surface_raised, t.tokens.selection_bg).is_none(),
        "solarized-dark's surface_raised can now be separated from \
         selection_bg by a small edit. That un-sticks the one case option \
         (b) could not reach, so the claim that (b) is UNAVAILABLE for a \
         theme — not just expensive — no longer holds and the costing on \
         claim:tui-audit-does-not-survive-quantisation needs updating."
    );
}

/// **The cost of option (c), and the reason it wins.** There is no new
/// algorithm to write: `quantize_pair_256` already re-picks a colliding
/// member to the nearest distinct entry preserving light/dark ordering,
/// and handed the two grounds it separates every collapsed pair —
/// landing on *exactly* the palette entry that option (b)'s optimal seed
/// edit arrives at, in every case.
///
/// So (b) and (c) produce the identical rendered result. (b) buys it by
/// editing authored hexes, most of them third-party ports whose entire
/// value is byte-fidelity to upstream, and buys nothing at all for a
/// consumer palette arriving through `theme::Palette`. (c) buys it for
/// every theme, including ones this crate has never seen.
///
/// What this test does NOT say is that the fix is a small one. The policy
/// is settled; the *site* is the open problem, and it is not the emitter
/// — see the note on `resolve_pen` above.
#[test]
fn the_existing_repick_lands_where_the_ideal_seed_edit_lands() {
    for (id, pair, anchor, mover) in collapsed_grounds() {
        let (q_mover, q_anchor) = quantize_pair_256(mover, anchor);
        assert_ne!(
            q_mover, q_anchor,
            "{id} ({pair}): the existing re-pick policy failed to separate \
             two grounds — option (c) needs a new policy after all"
        );
        let (ideal, _) = nearest_separating(mover, anchor)
            .unwrap_or_else(|| panic!("{id} ({pair}): no separating edit"));
        assert_eq!(
            q_mover,
            nearest_xterm256(ideal),
            "{id} ({pair}): the re-pick chose a different entry than the \
             optimal seed edit would reach — (b) and (c) stop being \
             equivalent and the choice between them has to be re-argued"
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
    for (id, _, anchor, mover) in collapsed_grounds() {
        let exact = delta_e(XTERM_256[nearest_xterm256(mover) as usize], mover) == 0.0;
        let (q_mover, _) = quantize_pair_256(mover, anchor);
        if exact && q_mover != nearest_xterm256(mover) {
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

// ---------------------------------------------------------------------
// The fix, proved constructible before it is written.
//
// Slice 3 settled that a PER-THEME decision is enough, because pairwise
// distinctness is placement-independent. Reading the app then changed
// where that decision can be APPLIED, and it is worth writing down
// because the row had specified the wrong thing twice running.
//
// The row said: adjust the ground TOKENS at caps-resolution time. Two
// facts kill that.
//
//   1. Depth is not fixed at startup. `Driver::apply_caps_upgrade`
//      recomputes `present_caps` after the probe answers, and
//      `PresentCaps::color` is exactly the depth this fix keys on. So a
//      depth-derived token set is not set-once; it has to re-derive
//      mid-session. (The good news is that the same branch already
//      poisons the previous frame and damages every layer, so the
//      repaint a re-derivation needs is already paid for.)
//
//   2. `TokenSet` is `Copy` and widgets CAPTURE it by value into state —
//      `select`, `select_multi`, `select_combobox`, `reasoning`,
//      `choice_prompt_view` all hold a `tokens: TokenSet` field. A
//      captured copy would keep pre-adjustment grounds, so mutating the
//      live token set silently splits the theme in two.
//
// And a third that is worse than either: at truecolor there is no defect
// to fix, so an adjusted TokenSet would have to differ BY DEPTH — moving
// authored colours on a terminal where they render exactly.
//
// So: keep the decision per-theme, apply it at emit. Assign each ground
// a distinct palette INDEX up front, and let the pen resolver look a
// cell's ground up in that map instead of computing `nearest`. The
// emitter never has to form the pair — that was the objection to doing
// this at emit, and it holds; it just does not need to, because the pair
// was resolved upstream. Truecolor is untouched, no token moves, nothing
// captured goes stale, and an arbitrary `Block::fill` colour simply
// misses the map and falls back to `nearest` as today.
//
// What follows is not that implementation. It is the precondition the
// implementation rests on, checked so the next slice starts from a fact.
// ---------------------------------------------------------------------

/// Prototype of the assignment the fix would precompute: give every
/// ground a distinct xterm-256 index, moving as little as possible.
///
/// **This is where "no new algorithm needed" stopped being true.** Slices
/// 2 and 3 concluded that `quantize_pair_256`'s existing re-pick was the
/// whole policy. It is — for a PAIR. Applied as a sequence of independent
/// pairwise fixes over a SET it does not converge: `observer-night` has
/// `bg` and `surface` both on 233, and the re-pick moves one of them to
/// the nearest distinct entry preserving luminance order, which is 234 —
/// already held by `surface_raised`. One collapse traded for another.
///
/// The missing piece is small but real: the re-pick excludes exactly ONE
/// index (`nearest_in`'s `exclude: Option<u8>`), and a set assignment has
/// to exclude every index already spoken for. So the fix does add code to
/// `color.rs`, and the earlier claim that it was a pure call-site change
/// was wrong.
///
/// Ownership rule, from `the_repick_sacrifices_an_exactly_representable_
/// ground`: grounds claim their natural entry in order of how exactly the
/// palette already represents them, so a ground rendered perfectly is
/// never the one displaced. That turns the rule from a special case into
/// the traversal order.
///
/// A displaced ground must also avoid the natural entry of any ground not
/// yet placed, or it just moves the collision along — `observer-night`
/// cost two displacements instead of one until that was added, because
/// `surface` took 234 before `surface_raised`, which naturally lives
/// there, had been reached.
fn assign_ground_indices(t: &TokenSet) -> [(&'static str, u8); 5] {
    let g = grounds(t);
    let natural: Vec<u8> = g.iter().map(|(_, c)| nearest_xterm256(*c)).collect();

    // Most-exactly-represented first: they get first claim.
    let mut order: Vec<usize> = (0..g.len()).collect();
    order.sort_by(|&a, &b| {
        let e = |k: usize| delta_e(XTERM_256[natural[k] as usize], g[k].1);
        e(a).total_cmp(&e(b)).then(a.cmp(&b))
    });

    let mut out: [(&'static str, u8); 5] = std::array::from_fn(|i| (g[i].0, 0));
    let mut taken: Vec<(u8, usize)> = Vec::new(); // (index, ground)
    for &k in &order {
        if let Some(&(_, blocker)) = taken.iter().find(|(i, _)| *i == natural[k]) {
            // Nearest entry nobody holds, on the same side of the blocker
            // this ground was on in truecolor.
            let anchor = relative_luminance(g[blocker].1);
            let lighter = relative_luminance(g[k].1) >= anchor;
            let mut best: Option<(u8, u32)> = None;
            for idx in 16u16..=255 {
                let idx = idx as u8;
                if taken.iter().any(|(i, _)| *i == idx) {
                    continue;
                }
                // Also leave alone the natural entry of any ground not
                // yet processed: grabbing it would only move the
                // collision along, which is the cascade `observer-night`
                // produced when this was omitted.
                if order
                    .iter()
                    .skip_while(|&&o| o != k)
                    .skip(1)
                    .any(|&o| natural[o] == idx)
                {
                    continue;
                }
                let e = XTERM_256[idx as usize];
                if (relative_luminance(e)
                    >= relative_luminance(XTERM_256[natural[blocker] as usize]))
                    != lighter
                {
                    continue;
                }
                let d = |x: u8, y: u8| {
                    let d = x as i32 - y as i32;
                    (d * d) as u32
                };
                let dist = d(e.r, g[k].1.r) + d(e.g, g[k].1.g) + d(e.b, g[k].1.b);
                if best.is_none_or(|(_, bd)| dist < bd) {
                    best = Some((idx, dist));
                }
            }
            let chosen = best
                .expect("240 entries cannot all be taken by 5 grounds")
                .0;
            out[k].1 = chosen;
            taken.push((chosen, k));
        } else {
            out[k].1 = natural[k];
            taken.push((natural[k], k));
        }
    }
    out
}

/// The precondition: every theme's five grounds CAN be given five
/// distinct palette entries, and the assignment is a no-op wherever
/// nothing was colliding.
///
/// The second half is the one worth asserting. A fix that separates the
/// 15 broken themes by quietly moving the 11 healthy ones would be a
/// regression wearing a fix's clothes.
#[test]
fn a_distinct_index_per_ground_is_constructible_for_every_theme() {
    for th in themes() {
        let assigned = assign_ground_indices(&th.tokens);
        let mut idx: Vec<u8> = assigned.iter().map(|(_, i)| *i).collect();
        let before: Vec<u8> = grounds(&th.tokens)
            .iter()
            .map(|(_, c)| nearest_xterm256(*c))
            .collect();
        idx.sort_unstable();
        idx.dedup();
        assert_eq!(
            idx.len(),
            5,
            "{}: could not give five grounds five distinct entries — the \
             per-theme fix is not sufficient for this theme and the \
             compositor route is back on the table",
            th.id
        );

        let collided = before
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            < 5;
        let moved: Vec<&str> = assigned
            .iter()
            .zip(&before)
            .filter(|((_, now), was)| now != *was)
            .map(|((n, _), _)| *n)
            .collect();
        if collided {
            assert_eq!(
                moved.len(),
                1,
                "{}: one collision should cost exactly one move, got {:?}",
                th.id,
                moved
            );
        } else {
            assert!(
                moved.is_empty(),
                "{}: nothing collided here, but the assignment moved {:?} \
                 — a fix that perturbs healthy themes is a regression",
                th.id,
                moved
            );
        }
    }
}

/// **The port.** `render::color::quantize_set_256` is the prototype above
/// shipped, and this is the diff that keeps them one thing.
///
/// It is not a redundant test. The prototype computes in CIE76 Lab and
/// float relative luminance; the shipped function computes in the
/// module's own integer metrics — `sq_dist` for "how exactly is this
/// represented" and the integer `luma` proxy for light/dark ordering —
/// because `color.rs` deliberately keeps float gamma out of the emission
/// path. Those are different metrics that happen to agree here, and this
/// asserts the agreement across all 26 themes rather than assuming it. If
/// a future theme lands where they disagree, this fails and says which
/// metric is doing the deciding, instead of the two drifting quietly.
///
/// The shipped function adds ONE rule the prototype has not got:
/// byte-identical grounds share an index. A theme whose `surface` equals
/// its `bg` said those are one surface, and separating them would invent
/// an elevation the author did not draw — `quantize_pair_256` has the same
/// `rgb_eq` guard for the same reason. No built-in theme exercises it, so
/// it is unit-tested in `color.rs` instead.
///
/// **They disagree on exactly one theme, and the disagreement taught
/// something.** In `tokyo-night` neither colliding ground is remotely
/// exact — `surface_raised` and `selection_bg` are blues forced onto the
/// grey ramp at ΔE 20.6 and 21.5 — so the ownership sort is ranking two
/// bad approximations, and the two metrics rank them opposite ways.
/// Measuring what each choice COSTS settles it without appeal to which
/// metric is nicer:
///
/// | move | ΔE before → after | sq before → after |
/// |---|---|---|
/// | shipped: `surface_raised` 238→239 | 20.559 → **20.509** | 1321 → **881** |
/// | prototype: `selection_bg` 238→237 | 21.490 → 21.997 | 1258 → 1958 |
///
/// The shipped assignment moves the ground *toward* its true colour; the
/// prototype moves one away from it. Both separate by the same amount
/// (ΔE 4.32 vs 4.43 between the resulting entries). So the shipped choice
/// is better **in the prototype's own metric**, not merely in its own.
///
/// The finding underneath is that "least exactly represented moves" is a
/// proxy for "cheapest move", and the two come apart when neither ground
/// is exact: which ground is worse-represented does not say which one has
/// somewhere cheap to go, because the direction it must move to preserve
/// the authored elevation order may point away from it. The proxy is
/// still what ships — it is what makes the exactly-represented case
/// (`#ffffff` in the light themes) unconditional — but it is a proxy, and
/// this is where that is visible.
const SET_QUANTISER_DIFFERS_FROM_PROTOTYPE_FOR: &[&str] = &["tokyo-night"];

#[test]
fn the_shipped_set_quantiser_matches_the_reference_prototype() {
    let differ: Vec<&str> = themes()
        .iter()
        .filter(|th| {
            let shipped = quantize_set_256(grounds(&th.tokens).map(|(_, c)| c));
            shipped != assign_ground_indices(&th.tokens).map(|(_, i)| i)
        })
        .map(|th| th.id)
        .collect();
    assert_eq!(
        differ, SET_QUANTISER_DIFFERS_FROM_PROTOTYPE_FOR,
        "the set of themes where the shipped set quantiser and the \
         reference prototype disagree changed.\n\
         GREW: a theme landed where sq_dist/integer-luma and \
         CIE76/relative-luminance part company. Measure what each choice \
         COSTS the moved ground (see the table above) before picking a \
         side — do not assume the shipped one is right because it ships.\n\
         SHRANK: the two metrics converged; say so on \
         claim:tui-audit-does-not-survive-quantisation."
    );
}

/// The two properties the prototype was built to prove, re-asserted
/// against the SHIPPED function — because a fix is only real in the
/// artifact that ships. Distinct entries for every theme's grounds, at one
/// displacement per collision, and not one of the eleven healthy themes
/// perturbed.
#[test]
fn the_shipped_set_quantiser_separates_every_theme_without_perturbing_the_healthy() {
    for th in themes() {
        let g = grounds(&th.tokens);
        let before: Vec<u8> = g.iter().map(|(_, c)| nearest_xterm256(*c)).collect();
        let after = quantize_set_256(g.map(|(_, c)| c));

        let mut distinct = after.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            5,
            "{}: shipped assignment left two grounds on one entry",
            th.id
        );

        let moved: Vec<&str> = g
            .iter()
            .zip(before.iter().zip(after.iter()))
            .filter(|(_, (was, now))| was != now)
            .map(|((n, _), _)| *n)
            .collect();
        let collided = before
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            < 5;
        assert_eq!(
            moved.len(),
            usize::from(collided),
            "{}: expected {} moved ground(s), got {:?}",
            th.id,
            usize::from(collided),
            moved
        );
    }
}

/// Every theme in `KNOWN_256_COLLAPSES` is separated by the shipped
/// function, and separated to the entry option (b)'s optimal seed edit
/// would have reached. That equivalence is the whole argument for fixing
/// this at quantisation time instead of by editing 15 themes' authored
/// hexes; it was measured against `quantize_pair_256` in
/// `the_existing_repick_lands_where_the_ideal_seed_edit_lands`, and it has
/// to survive the move to a set assignment or the argument does not
/// transfer.
///
/// It does not transfer unchanged, and the exceptions are the finding.
/// Two adjustments and one genuine break:
///
/// - Which member moves is not fixed. `nearest_separating` was measured on
///   the pair's second member because that is the one `quantize_pair_256`
///   nudges; the set assignment moves whichever member the ownership rule
///   says, so the ideal is recomputed for the ground that actually moved.
/// - `nearest_separating` is PAIRWISE-BLIND, and so was the costing of
///   option (b) built on it. It finds the smallest seed edit that
///   separates one ground from ONE other, with no idea that the entry it
///   lands on may belong to a third ground. The set assignment cannot use
///   that entry, so it lands elsewhere and this comparison fails — not
///   because the assignment is wrong but because the pairwise ideal was
///   never achievable.
///
/// **That is five of the fifteen collapsed pairs — a third of them — and
/// it is the largest correction this row has had to option (b)'s
/// costing.** `every_collapsed_ground_separates_for_a_sub_jnd_move`
/// reports that fourteen of fifteen separate for a sub-JND edit and only
/// `solarized-dark` is expensive. For these five that number is not the
/// real cost: the entry the cheap edit reaches belongs to a third ground,
/// so an author taking it would separate the reported pair and collapse
/// another. The true edit is a further one, unmeasured, and the
/// sub-JND figure understates it.
///
/// `observer-night` is the worked case, and it is the theme the cascade
/// guard already exists for: `surface` displaced from 233 has 234 as its
/// ideal, which is where `surface_raised` naturally lives, so the
/// assignment takes 235 instead. Option (b) hits the identical wall by
/// hand. So this list is evidence AGAINST (b) — it costs more than it was
/// costed at — and not a divergence of (c) from it.
///
/// It also strengthens what the set assignment buys over a pairwise one:
/// a third of the broken themes need the whole-set view to be fixed
/// correctly, which a per-pair re-pick cannot have by construction.
const IDEAL_SEED_EDIT_IS_UNREACHABLE_FOR: &[(&str, &str)] = &[
    ("observer-night", "bg vs surface"),
    ("catppuccin-macchiato", "surface vs shadow_ground"),
    ("rose-pine", "bg vs surface"),
    ("everforest-light", "surface_raised vs shadow_ground"),
    ("abstract-midnight", "bg vs shadow_ground"),
];

#[test]
fn the_shipped_assignment_lands_where_the_ideal_seed_edit_lands() {
    let mut unreachable = Vec::new();
    for (id, pair, anchor, mover) in collapsed_grounds() {
        let t = abstracttui::theme::get(id).expect("known-set theme");
        let g = grounds(&t.tokens);
        let after = quantize_set_256(g.map(|(_, c)| c));
        let index_of = |c: Rgba| {
            g.iter()
                .position(|(_, x)| *x == c)
                .map(|k| after[k])
                .expect("collapsed pair members are grounds")
        };
        let (q_anchor, q_mover) = (index_of(anchor), index_of(mover));
        assert_ne!(q_anchor, q_mover, "{id} ({pair}): still collapsed");

        // Whichever member the assignment actually moved is the one whose
        // ideal edit we compare against.
        let (moved_now, stayed, landed) = if q_mover != nearest_xterm256(mover) {
            (mover, anchor, q_mover)
        } else {
            (anchor, mover, q_anchor)
        };
        let (ideal, _) = nearest_separating(moved_now, stayed)
            .unwrap_or_else(|| panic!("{id} ({pair}): no separating edit for the moved ground"));
        let ideal = nearest_xterm256(ideal);
        if landed == ideal {
            continue;
        }
        // Only a third ground standing on the ideal entry excuses the
        // miss. Anything else is the equivalence genuinely breaking.
        let taken_by_a_third_ground = g
            .iter()
            .any(|(_, c)| *c != moved_now && *c != stayed && nearest_xterm256(*c) == ideal);
        assert!(
            taken_by_a_third_ground,
            "{id} ({pair}): the set assignment landed on {landed} where the \
             optimal seed edit reaches {ideal}, and no other ground holds \
             {ideal}. Options (b) and (c) stop being equivalent for this \
             theme and the choice between them has to be re-argued on \
             claim:tui-audit-does-not-survive-quantisation."
        );
        unreachable.push((id, pair));
    }
    assert_eq!(
        unreachable, IDEAL_SEED_EDIT_IS_UNREACHABLE_FOR,
        "the set of themes whose pairwise-ideal entry is occupied by a \
         third ground changed. Each entry here is a theme where option \
         (b)'s costing was pairwise-blind and understated the edit it \
         would really need, so the list is evidence about (b), not a \
         waiver for (c)."
    );
}

/// The assignment must obey the ownership rule, not argument order: an
/// exactly-representable ground is never the one that moves. This is the
/// defect `the_repick_sacrifices_an_exactly_representable_ground` pins on
/// the raw `quantize_pair_256` policy, shown to be fixable by the caller
/// rather than by changing that function — which must keep its fg/bg
/// behaviour, where always moving the foreground IS correct.
#[test]
fn the_assignment_never_sacrifices_an_exactly_representable_ground() {
    for th in themes() {
        let g = grounds(&th.tokens);
        for (k, (name, idx)) in assign_ground_indices(&th.tokens).iter().enumerate() {
            let own = nearest_xterm256(g[k].1);
            let exact = delta_e(XTERM_256[own as usize], g[k].1) == 0.0;
            assert!(
                !exact || *idx == own,
                "{}: {name} is represented exactly by the palette and the \
                 assignment moved it anyway — the ownership rule is not \
                 being applied",
                th.id
            );
        }
    }
}

// ---------------------------------------------------------------------
// On the wire.
//
// Everything above measures the POLICY: what index a colour resolves to.
// None of it proves the emitter ever asks. These drive the real
// presenter into the VT model and read the colours back off the screen,
// because a fix that is correct in `color.rs` and unreached by
// `resolve_pen` is not a fix.
// ---------------------------------------------------------------------

/// The assignment for a theme, in the form the presenter takes.
fn assignment_for(t: &TokenSet) -> Vec<(Rgba, u8)> {
    let g = t.grounds();
    let idx = quantize_set_256(g.map(|(_, c)| c));
    g.iter().map(|(_, c)| *c).zip(idx).collect()
}

/// Paint two cells with the given grounds, emit at 256 colours through a
/// real `Presenter`, and read back what the terminal actually shows.
fn painted_grounds(a: Rgba, b: Rgba, assignment: &[(Rgba, u8)]) -> (Option<Rgba>, Option<Rgba>) {
    let caps = PresentCaps {
        color: ColorDepth::Xterm256,
        ..PresentCaps::FULL
    };
    let size = Size::new(4, 1);
    let mut surface = Surface::new(size, Cell::EMPTY);
    let ink = Rgba::rgb(200, 200, 200);
    for (x, ground) in [(0, a), (1, b)] {
        surface.draw_text(
            x,
            0,
            "x",
            Style {
                fg: Some(ink),
                bg: Some(ground),
                ..Style::EMPTY
            },
        );
    }
    let mut presenter = Presenter::new();
    presenter.set_palette_assignment(assignment);
    let mut out = Vec::new();
    presenter.emit(
        FrameDiff::new().compute_full(&Surface::new(size, Cell::EMPTY), &surface),
        &surface,
        &caps,
        &mut out,
    );
    let mut screen = VtScreen::new(size);
    screen.feed(&out);
    assert_eq!(screen.unknown_seq_count(), 0, "unmodeled bytes");
    let paint = |x: i32| screen.cell(x, 0).expect("in bounds").paint.bg;
    (paint(0), paint(1))
}

/// **The defect and the fix, both at the byte level, on the theme that
/// ships by default.** Without an assignment a panel painted in `surface`
/// over the app's `bg` reaches the terminal as ONE colour — there is no
/// panel. With the assignment installed the same two cells arrive
/// distinct.
///
/// The first half is as important as the second: it proves the emitter
/// really does produce the collapse (the measurement tests only prove the
/// lookup would), so if someone fixes this elsewhere and deletes the
/// assignment, this fails rather than passing vacuously.
#[test]
fn the_default_themes_panel_survives_256_colours_only_with_the_assignment() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    let (bg_none, surface_none) = painted_grounds(t.bg, t.surface, &[]);
    assert_eq!(
        bg_none, surface_none,
        "premise: with no assignment the default theme's panel and ground \
         reach the terminal as one colour. If this now differs, the \
         collapse was fixed somewhere else and KNOWN_256_COLLAPSES should \
         have caught it first."
    );

    let assignment = assignment_for(&t);
    let (bg_on, surface_on) = painted_grounds(t.bg, t.surface, &assignment);
    assert_ne!(
        bg_on, surface_on,
        "the assignment is installed and the panel STILL renders as its \
         own ground — resolve_pen is not consulting it"
    );
    // The ground that was not displaced keeps exactly the entry it had.
    assert_eq!(bg_on, bg_none, "an unmoved ground must not shift");
}

/// A colour nobody assigned is untouched: an arbitrary `Block::fill`
/// misses the table and quantises as it always did. This is the property
/// that makes installing an assignment safe for scenes the theme knows
/// nothing about.
#[test]
fn an_unassigned_colour_is_unaffected_by_the_assignment() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    let stranger = Rgba::rgb(180, 20, 90);
    assert!(
        !t.grounds().iter().any(|(_, c)| *c == stranger),
        "premise: not a ground"
    );
    let (plain, _) = painted_grounds(stranger, t.bg, &[]);
    let (mapped, _) = painted_grounds(stranger, t.bg, &assignment_for(&t));
    assert_eq!(plain, mapped);
    assert_eq!(plain, Some(xterm_256(nearest_xterm256(stranger))));
}

/// **Precedence: text beats elevation.** An assignment can push a ground
/// onto the entry a foreground drawn on it wants — a collision the raw
/// lookup did not have. When that happens the foreground still moves,
/// because two surfaces reading as one is a defect and text reading as
/// its own background is erased.
///
/// Constructed rather than found: no built-in theme currently produces
/// the case, and waiting for one to appear is how a precedence rule goes
/// untested until it is wrong in production.
#[test]
fn an_assignment_never_erases_text_it_collides_with() {
    // Ground assigned to 238; ink whose natural entry is also 238.
    let ground = Rgba::rgb(60, 60, 60);
    let ink = XTERM_256[238];
    assert_eq!(nearest_xterm256(ink), 238, "premise: ink lands on 238");
    let assignment = [(ground, 238u8)];

    let caps = PresentCaps {
        color: ColorDepth::Xterm256,
        ..PresentCaps::FULL
    };
    let size = Size::new(2, 1);
    let mut surface = Surface::new(size, Cell::EMPTY);
    surface.draw_text(
        0,
        0,
        "x",
        Style {
            fg: Some(ink),
            bg: Some(ground),
            ..Style::EMPTY
        },
    );
    let mut presenter = Presenter::new();
    presenter.set_palette_assignment(&assignment);
    let mut out = Vec::new();
    presenter.emit(
        FrameDiff::new().compute_full(&Surface::new(size, Cell::EMPTY), &surface),
        &surface,
        &caps,
        &mut out,
    );
    let mut screen = VtScreen::new(size);
    screen.feed(&out);
    let paint = screen.cell(0, 0).expect("in bounds").paint;
    assert_eq!(
        paint.bg,
        Some(xterm_256(238)),
        "the ground keeps its assigned entry — the assignment is a \
         preference the pair rule must not silently override"
    );
    assert_ne!(
        paint.fg, paint.bg,
        "the assignment collided with the ink and the text was emitted \
         invisible: the pair guarantee must outrank the ground preference"
    );
}

// ---------------------------------------------------------------------
// Characterizing an OPEN DEFECT, not a contract.
// claim:tui-separator-invents-distinctions-the-author-did-not-draw
//
// `quantize_set_256` gives every ground its own palette entry. For
// grounds the theme AUTHOR made indistinguishable, that manufactures an
// edge nobody drew, and the 256 rendering ends up MORE separated than
// truecolor. The module's own docstring already forbids this — "inventing
// a distinction the author did not draw would be a worse defect than the
// one this fixes" — but implements the test as `rgb_eq`, byte-identity,
// which cannot see a 1.018 contrast pair.
//
// These two tests pin the SIZE of the defect so it cannot grow unnoticed
// and cannot be quietly declared fixed. They pass on today's defective
// behaviour BY DESIGN. When the fix lands they go red; that is the
// signal to update them to the invariant (`inversions == 0`), not to
// loosen them.
// ---------------------------------------------------------------------

/// Ground pairs where the 256 output separates MORE than truecolor does.
fn ground_inversions() -> Vec<(String, String, String, f32, f32)> {
    use abstracttui::base::palette::XTERM_256;
    use abstracttui::render::color::quantize_set_256;
    use abstracttui::theme::contrast::contrast_ratio;
    let mut out = vec![];
    for t in abstracttui::theme::themes() {
        let g = t.tokens.grounds();
        let a = quantize_set_256([g[0].1, g[1].1, g[2].1, g[3].1, g[4].1]);
        for i in 0..5 {
            for j in (i + 1)..5 {
                let true_c = contrast_ratio(g[i].1, g[j].1);
                let q_c = contrast_ratio(XTERM_256[a[i] as usize], XTERM_256[a[j] as usize]);
                if true_c < 1.05 && q_c > true_c * 1.05 {
                    out.push((
                        t.id.to_string(),
                        g[i].0.name().to_string(),
                        g[j].0.name().to_string(),
                        true_c,
                        q_c,
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn the_separator_invents_edges_in_exactly_seven_known_pairs() {
    let found = ground_inversions();
    let names: Vec<String> = found
        .iter()
        .map(|f| format!("{} {}/{}", f.0, f.1, f.2))
        .collect();
    assert_eq!(
        found.len(),
        7,
        "the invented-edge set CHANGED. Fewer means the fix landed — update \
         this to `assert!(found.is_empty())` and close the row. More means a \
         new theme or a separator change widened a KNOWN defect. Found: {names:?}"
    );
    // The worst instance, named so a regression cannot hide behind the count.
    let worst = found
        .iter()
        .find(|f| f.0 == "solarized-dark")
        .expect("solarized-dark surface_raised/selection_bg is the headline case");
    assert!(
        worst.3 < 1.05 && worst.4 > 1.4,
        "solarized-dark: authored at {:.3} (one colour to any eye), rendered \
         at 256 as {:.3} (a visible edge)",
        worst.3,
        worst.4
    );
}

/// Why the obvious fix does not work, pinned so nobody re-derives it.
///
/// Swapping `rgb_eq` for a colour-distance floor cannot work: the pairs
/// that MUST merge and the pairs that MUST stay apart overlap in
/// `sq_dist`. Any single distance threshold either re-breaks the
/// 15-of-26 elevation defect or leaves the invented edges in place.
#[test]
fn no_single_colour_distance_threshold_can_separate_the_two_populations() {
    use abstracttui::base::Rgba;
    use abstracttui::theme::contrast::contrast_ratio;
    fn sq(a: Rgba, b: Rgba) -> i32 {
        let (dr, dg, db) = (
            a.r as i32 - b.r as i32,
            a.g as i32 - b.g as i32,
            a.b as i32 - b.b as i32,
        );
        dr * dr + dg * dg + db * db
    }
    let (mut merge_max, mut keep_min) = (0i32, i32::MAX);
    for t in abstracttui::theme::themes() {
        let g = t.tokens.grounds();
        for i in 0..g.len() {
            for j in (i + 1)..g.len() {
                let (c, d) = (contrast_ratio(g[i].1, g[j].1), sq(g[i].1, g[j].1));
                if c < 1.05 {
                    merge_max = merge_max.max(d);
                } else if c >= 1.15 {
                    keep_min = keep_min.min(d);
                }
            }
        }
    }
    assert!(
        merge_max > keep_min,
        "the populations SEPARATED (merge_max={merge_max} <= keep_min={keep_min}). \
         A single sq_dist threshold is now viable and this test is obsolete — \
         re-open the row and take the simple fix."
    );
}

// ---------------------------------------------------------------------
// The AUDIT could not ask the ground-vs-ground question at all.
// claim:tui-separator-invents-distinctions-the-author-did-not-draw
//
// Every rule in theme::contrast::audit is ink-on-ground; surface_raised
// appears only as a BACKGROUND for text and syntax checks, never as a
// subject measured against surface or bg. `ground_overlaps` is the
// question, reported and not enforced, because the VERDICT on a
// near-invisible authored pair is DESIGN's and reddening the registry
// on my own initiative is the valid-input-fails defect this module
// already paid for once.
// ---------------------------------------------------------------------

/// The premise: a clean `audit` and an overlapping ground pair coexist.
///
/// This is the whole reason the new function exists. If it ever fails
/// because `audit` grew a ground rule, delete this test — do not weaken
/// it — and move the count below into the audit's own suite.
#[test]
fn a_theme_can_pass_the_audit_with_two_grounds_a_reader_cannot_tell_apart() {
    use abstracttui::theme::contrast::{audit, floors, ground_overlaps};
    let worst = abstracttui::theme::themes()
        .iter()
        .filter_map(|t| {
            ground_overlaps(t.id, &t.tokens, floors::GROUND_SEPARATION_REPORT)
                .into_iter()
                .min_by(|x, y| x.measured.partial_cmp(&y.measured).unwrap())
                .map(|o| (t, o))
        })
        .min_by(|a, b| a.1.measured.partial_cmp(&b.1.measured).unwrap())
        .expect("the registry has at least one overlapping ground pair");
    let (theme, overlap) = worst;
    assert!(
        overlap.measured < 1.01,
        "expected a pair that is effectively one colour, got {overlap}"
    );
    assert!(
        audit(theme.id, &theme.tokens).is_empty(),
        "{}: premise broken — this theme now HAS audit violations, so the \
         clean-audit-plus-invisible-grounds case needs a different witness. \
         Overlap was: {overlap}",
        theme.id
    );
}

/// Pins the size of the reported set, and that the report is not empty.
///
/// A reporting-only check that nobody looks at is decoration, so the
/// numbers live here where `cargo test --all` reads them. These are
/// MEASUREMENTS of the registry as authored, not a contract: if DESIGN
/// rules and themes are edited, update them deliberately.
#[test]
fn the_registry_ground_overlap_report_is_the_measured_size() {
    use abstracttui::theme::contrast::{floors, ground_overlaps};
    let all: Vec<_> = abstracttui::theme::themes()
        .iter()
        .flat_map(|t| ground_overlaps(t.id, &t.tokens, floors::GROUND_SEPARATION_REPORT))
        .collect();
    let themes_hit: std::collections::BTreeSet<&str> =
        all.iter().map(|o| o.theme.as_str()).collect();
    assert_eq!(
        (all.len(), themes_hit.len()),
        (44, 22),
        "the authored ground-overlap set changed: {} pairs across {} of 26 themes",
        all.len(),
        themes_hit.len()
    );
    // Falsifiable floor: a report that cannot fire is decoration.
    assert!(
        ground_overlaps("none", &abstracttui::theme::themes()[0].tokens, 1.0).is_empty(),
        "a floor of 1.0 must report nothing — contrast is never below 1.0, \
         so a hit here means the comparison is inverted"
    );
}

// ---------------------------------------------------------------------
// The ruling: ground separation intent is DECLARED by the theme.
// `commons/decision:ground-separation-intent-is-declared-by-the-theme@2`,
// claim:tui-separator-invents-distinctions-the-author-did-not-draw
//
// (A) a theme declares, PER PAIR, which of its grounds read as one
// surface; (B) a pair it did not name keeps elevation-wins, which is
// every pair of every built-in theme today. The declaration is ADDITIVE:
// an unnamed pair falls to B exactly as if the theme carried no
// declaration at all, so opting in cannot flip the default by omission —
// that reading would silently convert an opting-in theme to the
// merge-everything option the ruling rejected.
//
// The four tests below pin, in order: that the undeclared default moved
// nothing; that opting in without naming a pair ALSO moves nothing (the
// additivity guarantee, and the one an implementation is most likely to
// get wrong); what naming every pair BUYS; and what it COSTS. The third
// also carries a correction to this file's own account of the defect —
// see its docstring.
// ---------------------------------------------------------------------

/// The five grounds of `theme`, assigned under `intent`.
fn assign_grounds(tokens: &TokenSet, intent: GroundIntent) -> [u8; 5] {
    let colors: Vec<Rgba> = tokens.grounds().iter().map(|(_, c)| *c).collect();
    let mut out = [0u8; 5];
    quantize_set_256_into_with(&colors, intent, &mut out);
    out
}

/// All ten ground pairs declared with one intent — the maximal opt-in.
/// With [`PairIntent::Same`] this is the strongest thing a theme can say
/// and therefore the upper bound on what a declaration can do; with
/// [`PairIntent::Distinct`] it is the loudest possible way of saying
/// nothing.
fn every_pair(intent: PairIntent) -> Vec<(usize, usize, PairIntent)> {
    let mut out = Vec::new();
    for i in 0..5 {
        for j in (i + 1)..5 {
            out.push((i, j, intent));
        }
    }
    out
}

/// The ground indices every built-in theme resolved to BEFORE
/// `GroundIntent` existed — a literal baseline, not a re-derivation.
///
/// **Why literal, and it is a correction to this file.** The first
/// version of the guard below compared `quantize_set_256_into_with(...,
/// UNDECLARED)` against `quantize_set_256(...)`. Both are the same
/// implementation, so it pinned *the two entry points agree* while
/// reading as *the bytes did not move*. Falsified by breaking the merge
/// predicate so silence stopped separating: every theme's assignment
/// changed for real, eight other tests went red, and that one stayed
/// GREEN — because both of its sides moved together. A baseline computed
/// from the thing it is a baseline for is not a baseline.
const SHIPPED_GROUND_INDICES: &[(&str, [u8; 5])] = &[
    ("abstract-dark", [234, 235, 23, 236, 233]),
    ("abstract-light", [255, 231, 254, 218, 251]),
    ("observer-night", [233, 235, 234, 237, 232]),
    ("catppuccin-mocha", [235, 234, 236, 239, 233]),
    ("catppuccin-macchiato", [235, 236, 237, 59, 234]),
    ("catppuccin-frappe", [237, 236, 238, 60, 235]),
    ("rose-pine", [234, 236, 235, 238, 233]),
    ("rose-pine-moon", [235, 236, 237, 59, 233]),
    ("tokyo-night", [234, 235, 239, 238, 233]),
    ("nord", [236, 238, 239, 240, 234]),
    ("one-dark", [236, 235, 237, 238, 234]),
    ("dracula", [235, 237, 238, 59, 234]),
    ("monokai", [235, 236, 237, 240, 234]),
    ("gruvbox", [235, 237, 239, 58, 234]),
    ("solarized-dark", [235, 236, 23, 237, 233]),
    ("everforest-dark", [236, 237, 238, 239, 234]),
    ("catppuccin-latte", [255, 254, 188, 183, 252]),
    ("rose-pine-dawn", [255, 231, 254, 188, 252]),
    ("one-light", [255, 231, 254, 153, 252]),
    ("everforest-light", [230, 231, 253, 254, 188]),
    ("solarized-light", [230, 254, 188, 152, 251]),
    ("abstract-aurora", [233, 234, 235, 23, 232]),
    ("abstract-paper", [255, 231, 254, 181, 251]),
    ("abstract-ember", [233, 234, 235, 238, 232]),
    ("abstract-midnight", [235, 233, 234, 238, 232]),
    ("abstract-dawn", [255, 231, 254, 152, 251]),
];

/// **(B), the undeclared default.** Every built-in theme is silent, so
/// every built-in theme must come out of the declaring code path with the
/// bytes it had before that path existed — not "equivalent", identical.
///
/// This is the whole safety argument for shipping `GroundIntent` at all:
/// the mechanism cannot move a theme whose author has not asked it to.
/// So it is asserted against the literal pre-existing indices, and the
/// plain entry point is checked against the same literals rather than
/// against the declaring one — two independent claims, neither of which
/// can be satisfied by both sides drifting together.
///
/// A red here is not a licence to regenerate the table. It means a
/// built-in theme's rendered grounds changed at 256 colours; find out
/// which and why first.
#[test]
fn the_undeclared_default_is_byte_for_byte_the_shipped_assignment() {
    let ids: Vec<&str> = themes().iter().map(|t| t.id).collect();
    let pinned: Vec<&str> = SHIPPED_GROUND_INDICES.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids, pinned,
        "the registry changed shape — the baseline covers every theme or \
         it covers nothing"
    );
    for (t, (id, want)) in themes().iter().zip(SHIPPED_GROUND_INDICES) {
        let g = t.tokens.grounds();
        assert_eq!(t.id, *id);
        assert_eq!(
            quantize_set_256([g[0].1, g[1].1, g[2].1, g[3].1, g[4].1]),
            *want,
            "{id}: the plain assignment moved off the bytes that shipped"
        );
        assert_eq!(
            assign_grounds(&t.tokens, GroundIntent::UNDECLARED),
            *want,
            "{id}: silence must be the behaviour that shipped"
        );
    }
}

/// **Opting in must not flip the default** — the ruling's `@2`
/// correction, across the whole registry.
///
/// A theme that carries a declaration and names no pair, and a theme that
/// declares all ten pairs `Distinct`, must both render byte-for-byte like
/// a theme that never opted in. The failure this forbids is the one a
/// single distinctness list makes almost inevitable: *field present but
/// empty* reading as "no pair must stay distinct", which silently
/// converts an author who opted in without thinking into the
/// merge-everything option the ruling rejected.
///
/// Deliberately over all 26 themes rather than a constructed pair,
/// because the guarantee is about the registry as shipped: no built-in
/// may move the day a declaration field exists to be left empty.
#[test]
fn opting_in_without_naming_a_pair_changes_nothing_in_any_theme() {
    let all_distinct = every_pair(PairIntent::Distinct);
    for t in themes() {
        let silent = assign_grounds(&t.tokens, GroundIntent::UNDECLARED);
        assert_eq!(
            assign_grounds(&t.tokens, GroundIntent::new(&[])),
            silent,
            "{}: an empty declaration moved a ground — 'opted in' must not \
             mean 'merge whatever I did not mention'",
            t.id
        );
        assert_eq!(
            assign_grounds(&t.tokens, GroundIntent::new(&all_distinct)),
            silent,
            "{}: declaring every pair Distinct moved a ground — declaring \
             the default is not supposed to be a change",
            t.id
        );
    }
}

/// **(A), what a declaration buys — and the correction it forced.**
///
/// A theme that declares all ten of its ground pairs `Same` — the
/// maximal opt-in, and the upper bound on what any declaration can do —
/// lets its colliding grounds share an entry. That closes **three** of
/// the seven invented edges. It cannot close the other four, and
/// measuring WHY corrects this file's own account of the defect:
///
/// `gruvbox surface_raised/selection_bg`, `rose-pine-dawn` and
/// `abstract-paper selection_bg/shadow_ground`, and `abstract-dawn
/// selection_bg/shadow_ground` have DIFFERENT NATURAL ENTRIES. Nothing
/// displaced them: `nearest_xterm256` alone lands them on entries further
/// apart than the colours are, and the set assignment leaves both where
/// they fell — under either intent. So `quantize_set_256` invented three
/// of these seven edges; the palette lookup invented the other four, and
/// would still invent them with the whole set policy deleted.
///
/// A declaration cannot reach them because merging only ever happens on a
/// COLLISION. Making `Same` *force* two grounds onto one entry with no
/// collision to resolve is a stronger verb than the one ruled, and it was
/// PUT to DESIGN and REFUSED (`commons#259`): intent releases a merge and
/// never creates one, because forcing a collapse invents the mirror of
/// the defect this mechanism exists to end. The four are re-attributed to
/// the palette lookup and left open, not absorbed here.
#[test]
fn a_declaration_closes_three_of_the_seven_invented_edges_and_the_palette_owns_the_rest() {
    use abstracttui::theme::contrast::contrast_ratio;
    let inverted = |intent: GroundIntent| -> Vec<String> {
        let mut out = vec![];
        for t in themes() {
            let g = t.tokens.grounds();
            let a = assign_grounds(&t.tokens, intent);
            for i in 0..5 {
                for j in (i + 1)..5 {
                    let true_c = contrast_ratio(g[i].1, g[j].1);
                    let q_c = contrast_ratio(XTERM_256[a[i] as usize], XTERM_256[a[j] as usize]);
                    if true_c < 1.05 && q_c > true_c * 1.05 {
                        out.push(format!("{} {}/{}", t.id, g[i].0.name(), g[j].0.name()));
                    }
                }
            }
        }
        out
    };
    let all_same = every_pair(PairIntent::Same);
    let silent = inverted(GroundIntent::UNDECLARED);
    let declared = inverted(GroundIntent::new(&all_same));
    assert_eq!(
        silent.len(),
        7,
        "premise, and it agrees with the row: {silent:?}"
    );
    assert_eq!(
        declared,
        vec![
            "gruvbox surface_raised/selection_bg",
            "rose-pine-dawn selection_bg/shadow_ground",
            "abstract-paper selection_bg/shadow_ground",
            "abstract-dawn selection_bg/shadow_ground",
        ],
        "a declaration closes the three collision-borne edges and only those"
    );

    // And the four survivors are the palette lookup's, not the policy's:
    // distinct natural entries, untouched by either intent.
    for name in &declared {
        let (id, pair) = name.split_once(' ').expect("id and pair");
        let (an, bn) = pair.split_once('/').expect("two ground names");
        let t = themes().iter().find(|t| t.id == id).expect("named theme");
        let g = t.tokens.grounds();
        let at = g
            .iter()
            .position(|(k, _)| k.name() == an)
            .expect("ground a");
        let bt = g
            .iter()
            .position(|(k, _)| k.name() == bn)
            .expect("ground b");
        let (na, nb) = (nearest_xterm256(g[at].1), nearest_xterm256(g[bt].1));
        assert_ne!(
            na, nb,
            "{name}: no collision to merge — the edge is plain nearest's"
        );
        for intent in [GroundIntent::UNDECLARED, GroundIntent::new(&all_same)] {
            let a = assign_grounds(&t.tokens, intent);
            assert_eq!(
                (a[at], a[bt]),
                (na, nb),
                "{name}: the set policy never moved either ground"
            );
        }
    }
}

/// **What the maximal opt-in costs, so the trade is a number and not a
/// hope — and note who now pays it.**
///
/// Declaring all ten pairs `Same` re-collapses 15 ground pairs that the
/// undeclared assignment keeps apart: the elevation defect the set policy
/// was written to fix, handed back deliberately because the author asked
/// for it, pair by pair. One of them (`catppuccin-frappe bg/surface`) is
/// authored above the 1.10 report floor, which is the loudest case an
/// author reaching for the maximal opt-in should expect to see.
///
/// Under the FIRST cut of this mechanism these 15 were the cost of
/// opting in AT ALL — a whole-theme `Declared(&[])` meant "merge
/// everything I did not name", so a theme that added the field and
/// listed nothing paid all 15 by omission. `commons#259` ruled that
/// reading out: the declaration is additive, and reaching this number now
/// takes ten deliberate statements. The measurement is kept because it
/// still bounds what the mechanism can give away — see
/// `opting_in_without_naming_a_pair_changes_nothing_in_any_theme` for the
/// half that says nobody pays it by accident.
///
/// This is a MEASUREMENT of the registry as authored, not a contract: it
/// moves when themes or grounds do, and it is here so it cannot move
/// unnoticed.
#[test]
fn declaring_every_pair_same_re_collapses_fifteen_ground_pairs() {
    use abstracttui::theme::contrast::contrast_ratio;
    let all_same = every_pair(PairIntent::Same);
    let mut lost = vec![];
    for t in themes() {
        let g = t.tokens.grounds();
        let (u, d) = (
            assign_grounds(&t.tokens, GroundIntent::UNDECLARED),
            assign_grounds(&t.tokens, GroundIntent::new(&all_same)),
        );
        for i in 0..5 {
            for j in (i + 1)..5 {
                if u[i] != u[j] && d[i] == d[j] {
                    lost.push((
                        format!("{} {}/{}", t.id, g[i].0.name(), g[j].0.name()),
                        contrast_ratio(g[i].1, g[j].1),
                    ));
                }
            }
        }
    }
    assert_eq!(
        lost.len(),
        15,
        "the cost of the maximal opt-in changed: {lost:?}"
    );
    let loud: Vec<&str> = lost
        .iter()
        .filter(|(_, c)| *c >= 1.10)
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        loud,
        vec!["catppuccin-frappe bg/surface"],
        "the pairs given up above the report floor"
    );
}

// ---------------------------------------------------------------------
// Slice 2: the declaration reaches the DRIVER.
//
// Everything above proves the policy is right when it is handed an
// intent. None of it proves an author can state one. A theme now
// declares in TOKENS (`Theme::ground_intent`), `register` refuses a
// declaration over a non-ground, and `Driver::sync_palette_assignment`
// resolves it against `TokenSet::grounds` and hands it to the separator.
//
// The last test drives the real presenter into the VT model, because a
// declaration that is correct in the type system and unreached by
// `resolve_pen` is not a feature.
// ---------------------------------------------------------------------

/// **Every built-in is silent, and the field cannot quietly stop being
/// silent.** The ruling says elevation wins where a theme has not
/// spoken, and none of these 26 authors has been asked.
///
/// This is the guard that makes `SHIPPED_GROUND_INDICES` mean what it
/// says: those literals are the bytes of a silent registry, so a
/// built-in acquiring a declaration has to change this test first.
#[test]
fn every_built_in_theme_is_silent_about_ground_intent() {
    let speaking: Vec<&str> = themes()
        .iter()
        .filter(|t| !t.ground_intent.is_empty())
        .map(|t| t.id)
        .collect();
    assert!(
        speaking.is_empty(),
        "built-in themes declared ground intent: {speaking:?}. The ruling \
         makes silence the default for every theme this crate ships — if a \
         theme should now speak, that is a DESIGN decision and the shipped \
         byte baseline moves with it."
    );
}

/// `TokenSet::resolve_ground_intent` maps tokens to the positions the
/// separator speaks, and REFUSES a token that is not a ground rather
/// than dropping the pair.
///
/// Dropping is the failure this forbids: a skipped pair leaves the
/// author believing two grounds are declared while nothing carries the
/// declaration, and no call site ever throws.
#[test]
fn resolving_intent_maps_grounds_and_refuses_a_non_ground() {
    use abstracttui::theme::{TokenId, TokenSet};
    assert_eq!(
        TokenSet::resolve_ground_intent(&[(
            TokenId::SelectionBg,
            TokenId::SurfaceRaised,
            PairIntent::Same,
        )]),
        Ok(vec![(3, 2, PairIntent::Same)]),
        "positions must be those of TokenSet::grounds, in its order"
    );
    // `border` is a stroke drawn ON a ground, not a ground — the exact
    // near-miss an author would reach for.
    assert_eq!(
        TokenSet::resolve_ground_intent(&[(TokenId::Bg, TokenId::Border, PairIntent::Same)]),
        Err(TokenId::Border)
    );
    assert_eq!(TokenSet::resolve_ground_intent(&[]), Ok(vec![]));
}

/// `register` refuses a non-ground declaration in BOTH modes.
///
/// `Labeled` exists so a user theme with a contrast miss still renders
/// rather than stranding the user. That reasoning does not transfer: a
/// labelled contrast finding is a theme that works and reads poorly, a
/// labelled non-ground declaration is an author told their grounds are
/// protected when nothing protects them.
#[test]
fn register_refuses_a_declaration_over_a_non_ground_in_both_modes() {
    use abstracttui::theme::{
        default_theme, register, RegisterError, RegisterMode, ThemeCandidate, TokenId,
    };
    for (n, mode) in [RegisterMode::Strict, RegisterMode::Labeled]
        .into_iter()
        .enumerate()
    {
        let base = default_theme();
        let candidate = ThemeCandidate {
            id: format!("gi-non-ground-{n}"),
            label: "Non-ground declaration".into(),
            dark: base.dark,
            tokens: base.tokens,
            ground_intent: vec![(TokenId::Bg, TokenId::Text, PairIntent::Same)],
        };
        match register(candidate, mode) {
            Err(RegisterError::NotAGround(id)) => assert_eq!(id, TokenId::Text),
            other => panic!("{mode:?} accepted a declaration over `text`: {other:?}"),
        }
    }
    // And the same candidate registers once the declaration is clean, so
    // the refusal is about the non-ground and not about the field.
    let base = default_theme();
    let ok = register(
        ThemeCandidate {
            id: "gi-non-ground-ok".into(),
            label: "Clean declaration".into(),
            dark: base.dark,
            tokens: base.tokens,
            ground_intent: vec![(TokenId::Bg, TokenId::Surface, PairIntent::Same)],
        },
        RegisterMode::Strict,
    )
    .expect("a declaration naming two real grounds is fine");
    assert_eq!(ok.theme.ground_intent.len(), 1);
}

/// **The seam, through the REAL driver, on the wire.**
///
/// The tests above prove the parts: the separator honours an intent, the
/// tokens resolve to indices, and `register` guards the field. None of
/// them proves `Driver::sync_palette_assignment` ever ASKS the theme —
/// a first draft of this test rebuilt the driver's three lines itself
/// and would have passed with the driver hard-coded to `UNDECLARED`.
/// That is the same defect this row found in its own baseline guard, so
/// it drives a real `Driver` into a real terminal and reads the colours
/// back off the screen instead.
///
/// The control is the same theme, same colours, registered WITHOUT the
/// declaration: two colours on screen. Without that half this would pass
/// on a theme whose grounds never collided in the first place.
#[test]
fn the_driver_carries_a_themes_declaration_all_the_way_to_the_terminal() {
    use abstracttui::app::{App, Driver, RunConfig};
    use abstracttui::base::Rect;
    use abstracttui::layout::Style as LayoutStyle;
    use abstracttui::term::Capabilities;
    use abstracttui::testing::CaptureTerm;
    use abstracttui::theme::{register, RegisterMode, ThemeCandidate, TokenId};
    use abstracttui::ui::Element;

    // solarized-dark is the headline case: `surface_raised` and
    // `selection_bg` are authored at 1.018 — one colour to any eye — and
    // the separator renders them at 1.518.
    let base = abstracttui::theme::get("solarized-dark").expect("house port");
    let (raised, sel) = (base.tokens.surface_raised, base.tokens.selection_bg);
    assert_eq!(
        nearest_xterm256(raised),
        nearest_xterm256(sel),
        "premise: these two collide, so there is a merge available to release"
    );

    let mk = |id: &str, intent: Vec<(TokenId, TokenId, PairIntent)>| {
        register(
            ThemeCandidate {
                id: id.into(),
                label: id.into(),
                dark: base.dark,
                tokens: base.tokens,
                ground_intent: intent,
            },
            // Labeled: a byte-for-byte copy of a shipped theme, and this
            // test is about intent rather than about its audit.
            RegisterMode::Labeled,
        )
        .expect("registers")
        .theme
    };

    /// Paint the two grounds side by side under `theme`, driven by a
    /// real `Driver` at 256 colours, and read back what the terminal
    /// shows. Nothing here touches `quantize_set_256` or
    /// `resolve_ground_intent`: if the driver does not consult the
    /// theme, this cannot see an intent.
    fn on_screen(theme: &'static abstracttui::theme::Theme) -> (Option<Rgba>, Option<Rgba>) {
        let size = Size::new(4, 1);
        let mut term = CaptureTerm::new(size);
        let mut app = App::new(size);
        let (a, b) = (theme.tokens.surface_raised, theme.tokens.selection_bg);
        app.mount(move |_| {
            Element::new()
                .style(LayoutStyle::fill())
                .draw(move |canvas, rect| {
                    canvas.fill(Rect::new(rect.x, rect.y, 1, 1), ' ', a, a);
                    canvas.fill(Rect::new(rect.x + 1, rect.y, 1, 1), ' ', b, b);
                })
                .build()
        })
        .expect("mount");
        abstracttui::app::set_theme(theme);
        let mut driver = Driver::new(
            &mut app,
            &mut term,
            RunConfig {
                // 256 exactly: truecolor has no defect to fix, so the
                // driver installs an empty assignment there and the
                // declaration would be untestable.
                caps: Some(Capabilities::with(|c| {
                    c.colors_256 = true;
                    c.unicode_ok = true;
                })),
                enter: None,
                probe: false,
                ..RunConfig::default()
            },
        )
        .expect("enter");
        for _ in 0..4 {
            if driver.turn(&mut app, &mut term).expect("turn").idle {
                break;
            }
        }
        let screen = term.screen();
        assert_eq!(screen.unknown_seq_count(), 0, "unmodeled bytes");
        (
            screen.cell(0, 0).expect("in bounds").paint.bg,
            screen.cell(1, 0).expect("in bounds").paint.bg,
        )
    }

    let (a, b) = on_screen(mk("gi-wire-silent", vec![]));
    assert_ne!(
        a, b,
        "control: an undeclared theme still separates them through the \
         driver — if this stops being true the declared case proves nothing"
    );

    let (a, b) = on_screen(mk(
        "gi-wire-declared",
        vec![(
            TokenId::SurfaceRaised,
            TokenId::SelectionBg,
            PairIntent::Same,
        )],
    ));
    assert_eq!(
        a, b,
        "the theme declared these two grounds Same and the terminal still \
         shows an edge — Driver::sync_palette_assignment is not consulting \
         Theme::ground_intent"
    );
    assert_eq!(
        a,
        Some(xterm_256(nearest_xterm256(raised))),
        "and they share the entry the palette gave them, not a third one"
    );
}

/// **What the byte-inert third state actually buys.**
///
/// `Distinct` changes no output: undeclared already keeps two grounds
/// apart. That cost was reported when the three states were ruled and
/// kept deliberately. This is the exchange — a declaration that can be
/// checked against the artifact.
///
/// A theme declaring two grounds `Distinct` and then authoring them at
/// the same hex has contradicted itself. The resolution is determinate
/// (the colours are the artifact, so they share an entry), and before
/// this it happened in silence, leaving the author believing an edge was
/// protected.
#[test]
fn declaring_distinct_over_one_colour_is_reported_as_a_contradiction() {
    use abstracttui::theme::contrast::declaration_contradictions;
    use abstracttui::theme::{default_theme, TokenId};
    let mut t = default_theme().tokens;
    t.surface = t.bg;

    let found = declaration_contradictions(
        "probe",
        &t,
        &[(TokenId::Bg, TokenId::Surface, PairIntent::Distinct)],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("bg") && found[0].contains("surface") && found[0].contains("Distinct"),
        "the finding must name the pair and what was declared: {}",
        found[0]
    );

    // The three near-misses that must NOT be reported.
    assert!(
        declaration_contradictions(
            "probe",
            &t,
            &[(TokenId::Bg, TokenId::Surface, PairIntent::Same)]
        )
        .is_empty(),
        "declaring one colour Same is agreement with the artifact, not a contradiction"
    );
    assert!(
        declaration_contradictions(
            "probe",
            &default_theme().tokens,
            &[(TokenId::Bg, TokenId::Surface, PairIntent::Distinct)]
        )
        .is_empty(),
        "two genuinely different grounds declared Distinct is the normal case"
    );
    assert!(
        declaration_contradictions(
            "probe",
            &default_theme().tokens,
            &[(TokenId::Bg, TokenId::SelectionBg, PairIntent::Same)]
        )
        .is_empty(),
        "Same over two far-apart colours is INERT, not wrong — it asks for a \
         merge that can never happen, and an author may mean it against a \
         future re-tint"
    );
}

/// The contradiction reaches `register` as a hygiene finding — refusing
/// in `Strict`, labelled in `Labeled`.
///
/// That severity is deliberate and differs from `NotAGround`: this theme
/// still renders sensibly and the resolution is determinate, so a
/// user-supplied theme should not be stranded over it. A declaration
/// naming a non-ground has no sensible resolution at all.
#[test]
fn register_treats_a_self_contradicting_declaration_as_hygiene_not_a_hard_error() {
    use abstracttui::theme::{
        default_theme, register, RegisterError, RegisterMode, ThemeCandidate, TokenId,
    };
    let mut tokens = default_theme().tokens;
    tokens.surface = tokens.bg;
    let candidate = |id: &str| ThemeCandidate {
        id: id.into(),
        label: "Contradiction".into(),
        dark: default_theme().dark,
        tokens,
        ground_intent: vec![(TokenId::Bg, TokenId::Surface, PairIntent::Distinct)],
    };

    match register(candidate("gi-contradiction-strict"), RegisterMode::Strict) {
        Err(RegisterError::Rejected { hygiene, .. }) => assert!(
            hygiene.iter().any(|h| h.contains("declared Distinct")),
            "strict refused, but not for the contradiction: {hygiene:?}"
        ),
        other => panic!("strict accepted a self-contradicting declaration: {other:?}"),
    }

    let reg = register(candidate("gi-contradiction-labeled"), RegisterMode::Labeled)
        .expect("labeled registers rather than stranding the author");
    assert!(
        reg.warnings.iter().any(|w| w.contains("declared Distinct")),
        "labeled registered but swallowed the contradiction: {:?}",
        reg.warnings
    );
}

/// **The fact the whole ruling rests on, pinned at last.**
///
/// `no_single_colour_distance_threshold_can_separate_the_two_populations`
/// pins the REFUTATION — that no threshold works. This pins the stronger
/// and simpler thing underneath it: three specific pairs are
/// simultaneously
///
///   (i)  an INVERSION — authored as one surface, rendered with an edge
///        the separator invented; and
///   (ii) a RESCUE — a pair plain nearest collapses and the assignment
///        pulls apart.
///
/// A predicate over `(colour_a, colour_b)` is asked to both merge and
/// separate the SAME two colours. It is not that the right threshold is
/// hard to find; there is no function of the pair that can be right,
/// which is why intent had to be declared rather than measured.
///
/// It is also not a coincidence that these are exactly the three
/// collision-borne inversions: being in both populations REQUIRES a
/// collision, since a rescue is only possible where nearest collapsed.
/// The test asserts that identity rather than just listing three names.
///
/// Owed since before the ruling; the row carried it as prose that
/// nothing checked.
#[test]
fn three_ground_pairs_are_an_inversion_and_a_rescue_at_once() {
    use abstracttui::theme::contrast::contrast_ratio;
    let mut both = Vec::new();
    let mut collision_borne = Vec::new();
    for t in themes() {
        let g = t.tokens.grounds();
        let assigned = quantize_set_256([g[0].1, g[1].1, g[2].1, g[3].1, g[4].1]);
        for i in 0..5 {
            for j in (i + 1)..5 {
                let (na, nb) = (nearest_xterm256(g[i].1), nearest_xterm256(g[j].1));
                let collides = na == nb;

                // (i) inversion: authored as one surface, rendered apart.
                let true_c = contrast_ratio(g[i].1, g[j].1);
                let q_c = contrast_ratio(
                    XTERM_256[assigned[i] as usize],
                    XTERM_256[assigned[j] as usize],
                );
                let inversion = true_c < 1.05 && q_c > true_c * 1.05;

                // (ii) rescue: plain nearest collapsed them, the set
                // assignment pulled them apart.
                let rescue = collides && assigned[i] != assigned[j];

                let name = format!("{} {}/{}", t.id, g[i].0.name(), g[j].0.name());
                if inversion && rescue {
                    both.push(name.clone());
                }
                if inversion && collides {
                    collision_borne.push(name);
                }
            }
        }
    }
    assert_eq!(
        both,
        vec![
            "solarized-dark surface_raised/selection_bg",
            "one-light bg/surface",
            "abstract-midnight bg/shadow_ground",
        ],
        "the set of pairs that are simultaneously an invented edge and a \
         rescued elevation changed. This is the evidence that no predicate \
         f(colour_a, colour_b) can be correct — it would have to merge and \
         separate the same two colours — and it is what makes intent a \
         DECLARATION rather than a measurement. EMPTY means the argument \
         for the ruling no longer has a witness in this registry; say so on \
         claim:tui-separator-invents-distinctions-the-author-did-not-draw \
         before changing anything."
    );
    assert_eq!(
        both, collision_borne,
        "being in both populations must be EXACTLY the collision-borne \
         inversions: a rescue is only possible where plain nearest \
         collapsed, so an inversion that is in both without colliding \
         would mean the rescue test is measuring something else"
    );
}

// ---------------------------------------------------------------------
// The FOUR edges no declaration can reach.
// claim:tui-nearest-inverts-across-the-cube-ramp-boundary
//
// `GroundIntent` closed three of the seven invented edges. The other
// four are not the set policy's: those grounds have different natural
// entries, nothing displaced them, and they survive with the whole set
// policy deleted. Intent cannot reach them because a merge only ever
// happens at a collision and there is none.
//
// The two tests below characterize what they ARE, so the follow-on row
// starts from a mechanism rather than from four theme names.
// ---------------------------------------------------------------------

/// Ground pairs an author drew as one surface that PLAIN
/// `nearest_xterm256` — no set policy anywhere in the picture — renders
/// further apart than they were authored.
fn plain_nearest_inversions() -> Vec<(String, f32, f32, u8, u8)> {
    use abstracttui::theme::contrast::contrast_ratio;
    let mut out = Vec::new();
    for t in themes() {
        let g = t.tokens.grounds();
        for i in 0..5 {
            for j in (i + 1)..5 {
                let true_c = contrast_ratio(g[i].1, g[j].1);
                let (na, nb) = (nearest_xterm256(g[i].1), nearest_xterm256(g[j].1));
                let q_c = contrast_ratio(XTERM_256[na as usize], XTERM_256[nb as usize]);
                if true_c < 1.05 && q_c > true_c * 1.05 {
                    out.push((
                        format!("{} {}/{}", t.id, g[i].0.name(), g[j].0.name()),
                        true_c,
                        q_c,
                        na,
                        nb,
                    ));
                }
            }
        }
    }
    out
}

/// The four, measured from the LOOKUP ALONE.
///
/// The existing `a_declaration_closes_three_of_the_seven...` derives this
/// set as "what survives the maximal opt-in", which is true but states it
/// in terms of the mechanism that cannot fix it. This computes it with
/// `nearest_xterm256` and nothing else, so the claim "these are the
/// palette lookup's, not the separator's" is asserted directly rather
/// than inferred from a survival.
#[test]
fn four_ground_pairs_are_inverted_by_the_palette_lookup_alone() {
    let found = plain_nearest_inversions();
    let names: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "gruvbox surface_raised/selection_bg",
            "rose-pine-dawn selection_bg/shadow_ground",
            "abstract-paper selection_bg/shadow_ground",
            "abstract-dawn selection_bg/shadow_ground",
        ],
        "the set of pairs plain nearest inverts changed. These are the ones \
         no ground declaration can reach — see \
         claim:tui-nearest-inverts-across-the-cube-ramp-boundary."
    );
    // Worst amplification, so the size of the defect is a number and not
    // an adjective: gruvbox renders 1.23x further apart than authored.
    let worst = found.iter().map(|f| f.2 / f.1).fold(0.0f32, f32::max);
    assert!(
        (1.20..1.30).contains(&worst),
        "worst amplification moved to {worst:.3} — it was 1.231 (gruvbox)"
    );
}

/// **The mechanism, and it is narrower than "quantisation is lossy".**
///
/// xterm-256 is two lattices: the 6x6x6 colour CUBE (16..=231) and the
/// 24-step grey RAMP (232..=255). `nearest_xterm256` picks whichever is
/// closer by squared distance, and for a near-neutral colour that
/// decision is a coin toss between two grids with different spacing.
///
/// Every one of the four inversions is a pair of near-identical grounds
/// where one landed on the cube and the other on the ramp. Not one
/// same-side pair inverts, anywhere in the registry. So the defect is
/// not the coarseness of either lattice — it is the BOUNDARY between
/// them: two colours a reader cannot tell apart get quantised against
/// different grids and come out on entries the grids disagree about.
///
/// That matters because it says what a fix would have to be, and what it
/// would NOT have to be. A general pair-aware nearest lookup is not
/// required; keeping two near-identical colours on the SAME lattice
/// would reach all four. Whether that trade is worth it (it moves one of
/// the two off its own nearest entry) is the open question on the row.
///
/// The one split pair that does NOT invert is kept in the count on
/// purpose: `one-light selection_bg/shadow_ground` splits cube/ramp and
/// comes out slightly CLOSER (1.035 -> 1.027). Splitting is necessary
/// for the defect, not sufficient — a fix that treats every split as a
/// bug would move a pair that is fine.
#[test]
fn every_plain_nearest_inversion_straddles_the_cube_ramp_boundary() {
    use abstracttui::theme::contrast::contrast_ratio;
    /// The grey ramp is 232..=255; everything below is the colour cube.
    fn on_ramp(entry: u8) -> bool {
        entry >= 232
    }

    for (name, _, _, na, nb) in plain_nearest_inversions() {
        assert_ne!(
            on_ramp(na),
            on_ramp(nb),
            "{name}: an inversion that does NOT straddle the cube/ramp \
             boundary ({na}/{nb}). The mechanism this row is built on is \
             that near-identical colours quantised against DIFFERENT \
             lattices come out disagreeing — a same-lattice inversion \
             would mean there is a second, unexamined cause."
        );
    }

    // And the converse half, which is the one that keeps the claim
    // honest: no pair that stays on one lattice inverts at all.
    let mut same_lattice_inversions = Vec::new();
    let mut splits = 0;
    for t in themes() {
        let g = t.tokens.grounds();
        for i in 0..5 {
            for j in (i + 1)..5 {
                let true_c = contrast_ratio(g[i].1, g[j].1);
                if true_c >= 1.05 {
                    continue;
                }
                let (na, nb) = (nearest_xterm256(g[i].1), nearest_xterm256(g[j].1));
                let q_c = contrast_ratio(XTERM_256[na as usize], XTERM_256[nb as usize]);
                if on_ramp(na) != on_ramp(nb) {
                    splits += 1;
                } else if q_c > true_c * 1.05 {
                    same_lattice_inversions.push(format!(
                        "{} {}/{}",
                        t.id,
                        g[i].0.name(),
                        g[j].0.name()
                    ));
                }
            }
        }
    }
    assert!(
        same_lattice_inversions.is_empty(),
        "a near-identical pair inverted while staying on ONE lattice: \
         {same_lattice_inversions:?}. The cube/ramp boundary is no longer \
         the whole story and the row needs re-measuring."
    );
    assert_eq!(
        splits, 5,
        "five near-identical ground pairs straddle the boundary and four of \
         them invert. Straddling is NECESSARY but not SUFFICIENT — the fifth \
         (one-light selection_bg/shadow_ground) comes out closer, so a fix \
         that treats every split as a defect would move a pair that is fine."
    );
}

// ---------------------------------------------------------------------
// And the fix that was RULED AVAILABLE and turns out not to exist.
// claim:tui-nearest-inverts-across-the-cube-ramp-boundary
//
// delegate ruled at dm:delegate--tui#13 that a same-lattice snap is mine
// to make: it collapses nothing (both grounds keep distinct entries — it
// changes WHICH entry, not HOW MANY), so the commons#259 precedent
// against forcing a merge does not reach it. The mechanism was left open
// — "same-lattice snapping is one candidate; if measuring turns up a
// better one that also keys on the outcome, take it".
//
// It was built, measured, and REMOVED. The tests below are what is left,
// and they are the useful part: every candidate snap for every one of the
// four pairs, with the reason each one fails. Nothing in `src/` changed.
//
// ## Why it fails, and the reason generalises past this palette
//
// **A ground is in FOUR relations, not one.** The ruling framed the
// defect per PAIR, which is how it is measured and how it reads. But
// moving a ground to satisfy one of its relations moves it against the
// other three, and the four grounds it is not being snapped to do not get
// a vote. `gruvbox` is the whole argument in one theme: the only snap
// that cures `surface_raised`/`selection_bg` (1.238 -> 1.053) makes
// `bg`/`surface_raised` 1.819 -> 2.370, `surface`/`surface_raised`
// 1.367 -> 1.781 and `surface_raised`/`shadow_ground` 2.048 -> 2.669.
// The theme's total ground-rendering error goes from 1.392 to 2.168. It
// fixes one relation by breaking three, each by more than it fixed.
//
// That is the same shape as the error this row already made once and the
// parent row made before it: a remedy that is correct about the thing it
// looks at and wrong about the thing it does not. The guard against it is
// the same too — measure the OUTCOME over the whole artifact, not the
// mechanism over the case that motivated it.
// ---------------------------------------------------------------------

/// How wrong the whole theme's ground rendering is: every ground pair's
/// rendered contrast against what the author drew, as an error >= 0.
///
/// A single number per assignment so a candidate snap can be judged on
/// the set it lands in rather than on the pair that motivated it. Summed
/// rather than maxed on purpose — a snap that halves the worst pair while
/// worsening three others is not an improvement, and a max would call it
/// one.
fn ground_render_error(colors: &[Rgba; 5], idx: &[u8; 5]) -> f32 {
    use abstracttui::theme::contrast::contrast_ratio;
    let mut total = 0.0;
    for i in 0..5 {
        for j in (i + 1)..5 {
            let authored = contrast_ratio(colors[i], colors[j]);
            let rendered = contrast_ratio(XTERM_256[idx[i] as usize], XTERM_256[idx[j] as usize]);
            total += (rendered / authored).max(authored / rendered) - 1.0;
        }
    }
    total
}

/// Nearest entry to `c` on ONE of the two xterm-256 lattices — the cube
/// (16..=231) or the grey ramp (232..=255). What a snap would pick.
fn nearest_on_lattice(c: Rgba, ramp: bool) -> u8 {
    let range = if ramp { 232..256 } else { 16..232 };
    range
        .map(|i| {
            let e = XTERM_256[i];
            let d = |x: u8, y: u8| {
                let d = x as i32 - y as i32;
                d * d
            };
            (d(c.r, e.r) + d(c.g, e.g) + d(c.b, e.b), i as u8)
        })
        .min()
        .expect("both lattices are non-empty")
        .1
}

/// Why one candidate snap was rejected, in the order the rules are
/// applied. `Viable` means every rule passed — the case this row would
/// have shipped.
#[derive(Debug, PartialEq, Eq)]
enum SnapVerdict {
    /// The snapped entry IS the other ground's entry: a forced merge, and
    /// that is the act `commons#259` refused. Not reachable by argument —
    /// the palette simply has one entry for both colours on that lattice.
    WouldMerge,
    /// The pair still renders further apart than authored. A colour is
    /// never taken off its own nearest entry for nothing.
    DoesNotCure,
    /// Right distance, wrong direction: the snapped entry inverts the
    /// authored light/dark ordering, which is elevation upside down. The
    /// same guarantee displacement already gives.
    FlipsOrdering,
    /// Cures the pair and wrecks the set — the theme's total ground
    /// rendering error goes UP.
    WorsensTheSet,
    Viable,
}

/// Every same-lattice snap available for every near-identical ground pair
/// the plain lookup inverts: `(pair, mover, from, to, verdict)`.
///
/// Both directions for each pair, not just the one an implementation
/// would choose. Which ground "should" move is a policy question, and
/// pinning only the preferred candidate would leave the other one able to
/// become viable without anything going red.
fn lattice_snap_candidates() -> Vec<(String, &'static str, u8, u8, SnapVerdict)> {
    use abstracttui::theme::contrast::contrast_ratio;
    let mut out = Vec::new();
    for t in themes() {
        let g = t.tokens.grounds();
        let colors: [Rgba; 5] = core::array::from_fn(|k| g[k].1);
        let natural: [u8; 5] = core::array::from_fn(|k| nearest_xterm256(colors[k]));
        for i in 0..5 {
            for j in (i + 1)..5 {
                let authored = contrast_ratio(colors[i], colors[j]);
                if authored >= 1.05 {
                    continue;
                }
                let rendered = contrast_ratio(
                    XTERM_256[natural[i] as usize],
                    XTERM_256[natural[j] as usize],
                );
                if rendered <= authored * 1.05 {
                    continue;
                }
                let base = ground_render_error(&colors, &natural);
                for (m, a) in [(i, j), (j, i)] {
                    let snapped = nearest_on_lattice(colors[m], natural[a] >= 232);
                    let mut candidate = natural;
                    candidate[m] = snapped;
                    let paired =
                        contrast_ratio(XTERM_256[snapped as usize], XTERM_256[natural[a] as usize]);
                    let luma = |c: Rgba| 2126 * c.r as u32 + 7152 * c.g as u32 + 722 * c.b as u32;
                    let verdict = if snapped == natural[a] {
                        SnapVerdict::WouldMerge
                    } else if paired > authored * 1.05 {
                        SnapVerdict::DoesNotCure
                    } else if (luma(colors[m]) >= luma(colors[a]))
                        != (luma(XTERM_256[snapped as usize])
                            >= luma(XTERM_256[natural[a] as usize]))
                    {
                        SnapVerdict::FlipsOrdering
                    } else if ground_render_error(&colors, &candidate) >= base {
                        SnapVerdict::WorsensTheSet
                    } else {
                        SnapVerdict::Viable
                    };
                    out.push((
                        format!("{} {}/{}", t.id, g[i].0.name(), g[j].0.name()),
                        g[m].0.name(),
                        natural[m],
                        snapped,
                        verdict,
                    ));
                }
            }
        }
    }
    out
}

/// **The ruled fix does not exist, and here is every candidate.**
///
/// Eight candidate snaps — both directions of all four inverted pairs —
/// and not one is viable. Two are barred by the ruling's own rule (the
/// palette has a single entry for both colours on the shared lattice, so
/// the "snap" is the forced merge `commons#259` refused). Three do not
/// cure the pair they were built for. Two flip the authored light/dark
/// ordering. One — `gruvbox` — cures its pair and makes three other
/// relations in the same theme worse.
///
/// Pinned per candidate rather than as a count: a change that turns any
/// one of these viable is a change this row should reopen for, and a
/// count would hide which one moved.
#[test]
fn no_same_lattice_snap_repairs_a_cube_ramp_inversion() {
    let found = lattice_snap_candidates();
    let rendered: Vec<String> = found
        .iter()
        .map(|(pair, mover, from, to, verdict)| format!("{pair}: {mover} {from}->{to} {verdict:?}"))
        .collect();
    let expected = vec![
        // The only candidate that cures its pair without merging or
        // flipping — and it costs three other relations to do it.
        "gruvbox surface_raised/selection_bg: surface_raised 239->59 WorsensTheSet",
        "gruvbox surface_raised/selection_bg: selection_bg 58->238 DoesNotCure",
        // Both of these pairs are so close that the shared lattice holds
        // ONE entry for the two of them. There is no non-merging snap to
        // reject on other grounds — the palette cannot represent the
        // author's near-identity as two distinct entries at all.
        "rose-pine-dawn selection_bg/shadow_ground: selection_bg 188->253 DoesNotCure",
        "rose-pine-dawn selection_bg/shadow_ground: shadow_ground 252->188 WouldMerge",
        "abstract-paper selection_bg/shadow_ground: selection_bg 181->251 WouldMerge",
        "abstract-paper selection_bg/shadow_ground: shadow_ground 251->187 DoesNotCure",
        "abstract-dawn selection_bg/shadow_ground: selection_bg 152->252 DoesNotCure",
        "abstract-dawn selection_bg/shadow_ground: shadow_ground 251->188 FlipsOrdering",
    ];
    assert_eq!(
        rendered, expected,
        "the same-lattice snap candidates moved. Each line is a candidate \
         and the FIRST rule it broke — a new line, a missing line, or a \
         changed verdict all mean the refutation in \
         claim:tui-nearest-inverts-across-the-cube-ramp-boundary needs \
         re-measuring before anything is built on it."
    );
    let viable: Vec<&String> = rendered
        .iter()
        .zip(&found)
        .filter(|(_, (_, _, _, _, v))| *v == SnapVerdict::Viable)
        .map(|(line, _)| line)
        .collect();
    assert!(
        viable.is_empty(),
        "a same-lattice snap became viable: {viable:?}. That is the fix \
         delegate ruled available at dm:delegate--tui#13 and measurement \
         refuted — if one exists now, BUILD IT."
    );
}

/// **The one candidate that cures its pair, priced.**
///
/// `gruvbox` is the case the per-pair framing gets wrong, so it is pinned
/// with numbers rather than described. Snapping `surface_raised` off the
/// grey ramp (239) onto the cube (59) beside `selection_bg` fixes the
/// invented edge between those two and breaks the three other relations
/// `surface_raised` is in.
///
/// This is the whole reason the row closes refuted instead of shipped: a
/// ground is in four relations and a per-pair remedy only ever looks at
/// one. Separate from the candidate table above because that table pins
/// the VERDICT and this pins the PRICE — a future change could keep the
/// verdict while moving the cost by an order of magnitude.
#[test]
fn the_only_curing_snap_breaks_three_relations_to_fix_one() {
    use abstracttui::theme::contrast::contrast_ratio;
    let t = themes()
        .iter()
        .find(|t| t.id == "gruvbox")
        .expect("gruvbox ships");
    let g = t.tokens.grounds();
    let colors: [Rgba; 5] = core::array::from_fn(|k| g[k].1);
    let natural: [u8; 5] = core::array::from_fn(|k| nearest_xterm256(colors[k]));
    let (raised, selection) = (2, 3);
    assert_eq!(
        (g[raised].0.name(), g[selection].0.name()),
        ("surface_raised", "selection_bg"),
        "the ground order moved; the indices below name the wrong pair"
    );
    let mut snapped = natural;
    snapped[raised] = nearest_on_lattice(colors[raised], false);
    assert_eq!(
        (natural[raised], snapped[raised]),
        (239, 59),
        "the snap no longer moves surface_raised from the ramp to cube 59"
    );

    let pair = |idx: &[u8; 5], a: usize, b: usize| {
        contrast_ratio(XTERM_256[idx[a] as usize], XTERM_256[idx[b] as usize])
    };
    let authored = contrast_ratio(colors[raised], colors[selection]);
    // What it buys: the invented edge is gone.
    assert!(
        (1.23..1.25).contains(&pair(&natural, raised, selection))
            && pair(&snapped, raised, selection) < authored * 1.05,
        "the snap no longer cures the pair it was built for: {:.3} -> {:.3} \
         against an authored {authored:.3}",
        pair(&natural, raised, selection),
        pair(&snapped, raised, selection)
    );
    // What it costs: every other relation surface_raised is in.
    for (other, was, now) in [(0, 1.819, 2.370), (1, 1.367, 1.781), (4, 2.048, 2.669)] {
        let before = pair(&natural, raised.min(other), raised.max(other));
        let after = pair(&snapped, raised.min(other), raised.max(other));
        assert!(
            (before - was).abs() < 0.01 && (after - now).abs() < 0.01,
            "surface_raised/{} moved: {before:.3} -> {after:.3}, pinned {was} -> {now}",
            g[other].0.name()
        );
        let authored_other = contrast_ratio(colors[raised], colors[other]);
        assert!(
            (after / authored_other) > (before / authored_other),
            "surface_raised/{} no longer gets worse under the snap — the \
             refutation rests on it doing so",
            g[other].0.name()
        );
    }
    assert!(
        ground_render_error(&colors, &snapped) > ground_render_error(&colors, &natural),
        "the snap stopped making gruvbox worse overall ({:.3} -> {:.3}); \
         the fix may be back on the table",
        ground_render_error(&colors, &natural),
        ground_render_error(&colors, &snapped)
    );
}
