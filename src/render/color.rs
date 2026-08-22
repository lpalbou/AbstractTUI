//! Color downlevel quantization for terminals without truecolor.
//!
//! RT1-7: the palette data is `base::palette` — the ONE xterm table shared
//! with the testing VT model. This module owns only the *policy* (nearest
//! lookup, gray-vs-cube choice, pair contrast preservation); it holds no
//! color values of its own.
//!
//! Formulas (docs/design/render.md §2.4):
//!
//! - xterm-256 cube (16..=231): channel -> level index via the midpoint
//!   thresholds `[48, 115, 155, 195, 235]` over levels
//!   `[0, 95, 135, 175, 215, 255]`; index = `16 + 36r + 6g + b`.
//! - Gray ramp (232..=255): candidate step from integer luma
//!   `(2126*R + 7152*G + 722*B) / 10000`, value `8 + 10*step`.
//! - Winner: smaller squared RGB distance; cube wins ties (chroma beats a
//!   marginally closer gray).
//! - 16-color: nearest `SYSTEM_16` entry by squared distance, lowest index
//!   winning ties. Real terminals theme these registers, so 16-color mode
//!   is best-effort by construction.
//!
//! Pair quantization (DESIGN request 3): quantizing fg and bg separately
//! can collapse a deliberately-subtle theme pair (dark-theme faint text)
//! into ONE palette entry — text vanishes. `quantize_pair_*` re-picks the
//! foreground when a collision would erase originally-distinct colors:
//! nearest *distinct* palette entry whose luma keeps the original
//! light/dark ordering relative to the background. Luma ordering uses the
//! integer luma proxy above — deterministic, no float gamma in the
//! emission path.
//!
//! Set quantization (`quantize_set_256`): the pair policy sees the two
//! colors inside ONE cell, so it cannot protect two GROUNDS that meet
//! across a cell boundary — a panel fill collapsing into the field behind
//! it. That pair is resolvable once per theme and depth instead, upstream
//! of emit; `quantize_set_256` is that policy, and it is the same metrics
//! (`sq_dist`, `luma`) applied to a set rather than a pair.

use crate::base::palette::{SYSTEM_16, XTERM_256};
use crate::base::Rgba;

/// Midpoints between adjacent `CUBE_LEVELS` — the nearest-level decision
/// thresholds. Kept next to the policy (the levels themselves live in
/// base; a drift test pins the derivation).
const CUBE_THRESHOLDS: [u8; 5] = [48, 115, 155, 195, 235];

fn cube_index(v: u8) -> usize {
    CUBE_THRESHOLDS.iter().position(|&t| v < t).unwrap_or(5)
}

fn sq_dist(a: Rgba, b: Rgba) -> u32 {
    let d = |x: u8, y: u8| {
        let d = x as i32 - y as i32;
        (d * d) as u32
    };
    d(a.r, b.r) + d(a.g, b.g) + d(a.b, b.b)
}

/// Integer luma proxy for light/dark ORDERING decisions (not perceptual
/// truth): monotone per channel, deterministic, no gamma math.
fn luma(c: Rgba) -> u32 {
    (2126 * c.r as u32 + 7152 * c.g as u32 + 722 * c.b as u32) / 10000
}

/// Nearest xterm-256 index (16..=255; the themable system colors 0..=15
/// are deliberately never produced).
pub fn nearest_xterm256(c: Rgba) -> u8 {
    let ci = (cube_index(c.r), cube_index(c.g), cube_index(c.b));
    let cube_idx = (16 + 36 * ci.0 + 6 * ci.1 + ci.2) as u8;
    let cube_dist = sq_dist(c, XTERM_256[cube_idx as usize]);

    let gray_step = (luma(c) as i32 - 8 + 5).div_euclid(10).clamp(0, 23);
    let gray_idx = (232 + gray_step) as u8;
    let gray_dist = sq_dist(c, XTERM_256[gray_idx as usize]);

    if gray_dist < cube_dist {
        gray_idx
    } else {
        cube_idx
    }
}

/// Nearest ANSI-16 index (0..=15) against the shared xterm default table.
pub fn nearest_ansi16(c: Rgba) -> u8 {
    nearest_in(&SYSTEM_16, 0, c, &[], None).0
}

/// Joint fg/bg quantization to xterm-256 with contrast preservation.
pub fn quantize_pair_256(fg: Rgba, bg: Rgba) -> (u8, u8) {
    quantize_pair_256_assigned(fg, bg, &[])
}

/// An upstream decision about which palette entry specific colors get,
/// as `(color, index)` pairs — what [`quantize_set_256`] produced, zipped
/// back onto the colors it was given.
///
/// The emitter cannot make this decision itself: it sees one cell at a
/// time, so two GROUNDS that collapse into each other are never in front
/// of it together. Resolved once per theme and depth instead, it arrives
/// here as a lookup. An empty assignment is exactly today's behavior, by
/// construction rather than by promise — [`quantize_pair_256`] IS the
/// empty case.
pub type PaletteAssignment<'a> = &'a [(Rgba, u8)];

/// The assigned index for `c`, or the nearest entry when it has none.
///
/// Matching is on RGB, ignoring alpha — the same thing the nearest lookup
/// ignores. An arbitrary color (a `Block::fill` nobody themed) misses the
/// table and quantises as it always did.
pub fn nearest_xterm256_assigned(c: Rgba, assignment: PaletteAssignment) -> u8 {
    assignment
        .iter()
        .find(|(a, _)| rgb_eq(*a, c))
        .map_or_else(|| nearest_xterm256(c), |(_, i)| *i)
}

/// [`quantize_pair_256`] over an assignment: each color takes its
/// assigned entry if it has one, and the pair guarantee is then applied
/// ON TOP.
///
/// The order matters and it is a real precedence decision. An assignment
/// can CREATE a collision the raw lookup did not have (a ground displaced
/// onto the entry this cell's foreground naturally wants) as easily as it
/// removes one. When that happens the foreground still moves, exactly as
/// it does today: two grounds rendering as one surface is a defect, but a
/// foreground rendering as its own background is erased text, and text
/// wins. The ground assignment is a preference; the pair separation is a
/// guarantee.
pub fn quantize_pair_256_assigned(fg: Rgba, bg: Rgba, assignment: PaletteAssignment) -> (u8, u8) {
    let qbg = nearest_xterm256_assigned(bg, assignment);
    let qfg = nearest_xterm256_assigned(fg, assignment);
    if qfg != qbg || rgb_eq(fg, bg) {
        return (qfg, qbg);
    }
    // Collision on originally-distinct colors: re-pick fg among 16..=255
    // (same "never emit system colors" rule as the nearest lookup).
    let (nudged, _) = nearest_in(
        &XTERM_256[16..],
        16,
        fg,
        &[qbg],
        Some((qbg, ordering(fg, bg))),
    );
    (nudged, qbg)
}

/// Joint fg/bg quantization to ANSI-16 with contrast preservation.
pub fn quantize_pair_16(fg: Rgba, bg: Rgba) -> (u8, u8) {
    let qbg = nearest_ansi16(bg);
    let qfg = nearest_ansi16(fg);
    if qfg != qbg || rgb_eq(fg, bg) {
        return (qfg, qbg);
    }
    let (nudged, _) = nearest_in(&SYSTEM_16, 0, fg, &[qbg], Some((qbg, ordering(fg, bg))));
    (nudged, qbg)
}

/// The number of xterm-256 entries this module will ever emit: 16..=255.
/// The system registers 0..=15 are user-themable, so no build-time
/// decision can know what they render as.
const ASSIGNABLE_256: usize = 240;

/// Joint quantization of a SET of colors to xterm-256, giving each
/// originally-distinct color its own palette entry.
///
/// [`quantize_pair_256`] protects the two colors *inside one cell* — a
/// foreground and the background it sits on. It cannot protect two
/// GROUNDS (a panel fill and the field behind it) because those are never
/// handed to it together: they live in different cells, so the emitter
/// never sees the pair. Elevation drawn by fill therefore disappears at
/// 256 colors even though every cell's own fg/bg survives. Fifteen of the
/// twenty-six built-in themes lose a ground distinction that way,
/// `abstract-dark` — the default — among them
/// (`tests/theme_quantisation_grounds.rs` pins the set).
///
/// The pair is resolvable UPSTREAM, once per theme and depth, and that is
/// what this is for: hand it the grounds, get back one distinct index per
/// ground, and let the emitter look a color up instead of computing
/// `nearest`. Truecolor is untouched and no authored color moves; only
/// the palette entry a ground is drawn with changes, and only on a
/// terminal that could not tell the two apart anyway.
///
/// Guarantees, in the order they are applied:
///
/// - **Byte-identical colors share an index.** A theme whose `surface`
///   equals its `bg` said those are one surface; inventing a distinction
///   the author did not draw would be a worse defect than the one this
///   fixes.
/// - **The most exactly-represented color claims its entry first.** Where
///   two colors want one entry, the one the palette already renders
///   *perfectly* keeps it and the approximated one moves. Applying this as
///   the traversal order rather than as a special case is what stops
///   argument order from deciding (the defect
///   `quantize_pair_256` has by design: for fg/bg, always moving the
///   foreground IS correct — never move a background out from under the
///   other cells sharing it).
/// - **A displaced color keeps its light/dark ordering** against the color
///   that displaced it, so authored elevation direction survives.
/// - **A displaced color also avoids the natural entry of any color not
///   yet placed**, or the collision merely moves along. (`observer-night`
///   costs two displacements instead of one without this: `surface`
///   displaced from 233 lands on 234, which is where `surface_raised`
///   naturally lives.)
///
/// Ties pick the lower index — deterministic bytes.
///
/// `N` above `ASSIGNABLE_256 / 2` is not rejected but is not guaranteed
/// either: a color that finds every candidate spoken for keeps its
/// nearest entry, exactly as today. Five grounds are the intended scale.
pub fn quantize_set_256<const N: usize>(colors: [Rgba; N]) -> [u8; N] {
    let natural: [u8; N] = core::array::from_fn(|i| nearest_xterm256(colors[i]));

    // Most-exactly-represented first: they get first claim on their own
    // entry. `sq_dist` is the same metric the nearest lookup uses, so
    // "exactly represented" here means the same thing it means there.
    let mut order: [usize; N] = core::array::from_fn(|i| i);
    order.sort_by_key(|&k| (sq_dist(XTERM_256[natural[k] as usize], colors[k]), k));

    let mut assigned: [Option<u8>; N] = [None; N];
    for &k in order.iter() {
        if let Some(same) = (0..N).find(|&j| assigned[j].is_some() && rgb_eq(colors[j], colors[k]))
        {
            assigned[k] = assigned[same];
            continue;
        }
        let Some(blocker) = (0..N).find(|&j| assigned[j] == Some(natural[k])) else {
            assigned[k] = Some(natural[k]);
            continue;
        };
        // Every entry already spoken for: the ones handed out, plus the
        // natural entry of every color still to be placed.
        let mut blocked: Vec<u8> = (0..N).filter_map(|j| assigned[j]).collect();
        blocked.extend(
            (0..N)
                .filter(|&j| j != k && assigned[j].is_none())
                .map(|j| natural[j]),
        );
        if blocked.len() >= ASSIGNABLE_256 {
            assigned[k] = Some(natural[k]);
            continue;
        }
        let anchor = assigned[blocker].expect("the blocker holds an index by construction");
        let lighter = luma(colors[k]) >= luma(colors[blocker]);
        let (idx, _) = nearest_in(
            &XTERM_256[16..],
            16,
            colors[k],
            &blocked,
            Some((anchor, lighter)),
        );
        assigned[k] = Some(idx);
    }
    core::array::from_fn(|i| assigned[i].expect("every color is placed exactly once"))
}

fn rgb_eq(a: Rgba, b: Rgba) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b
}

/// Whether fg is at-least-as-light (`true`) or darker (`false`) than bg —
/// the ordering a nudge must preserve.
fn ordering(fg: Rgba, bg: Rgba) -> bool {
    luma(fg) >= luma(bg)
}

/// Nearest entry of `table` (indices offset by `base`) to `c`, excluding
/// every index in `blocked` and optionally constraining the light/dark
/// ordering against an anchor entry (`Some((anchor, c_not_darker))`).
/// Falls back to nearest-unblocked when the ordering constraint admits
/// nothing (the anchor sits at the palette's extreme). Ties pick the
/// lower index — deterministic bytes.
///
/// `blocked` is a slice rather than a single index because the set
/// assignment ([`quantize_set_256`]) has to exclude every entry already
/// spoken for; the pair path passes the one background index and scans a
/// one-element slice.
fn nearest_in(
    table: &[Rgba],
    base: u8,
    c: Rgba,
    blocked: &[u8],
    ordered_against: Option<(u8, bool)>,
) -> (u8, u32) {
    let anchor_luma = ordered_against.map(|(i, _)| luma(XTERM_256[i as usize]));
    let c_not_darker = ordered_against.map(|(_, o)| o);
    let mut best: Option<(u8, u32)> = None;
    let mut best_unordered: Option<(u8, u32)> = None;
    for (i, &entry) in table.iter().enumerate() {
        let idx = base + i as u8;
        if blocked.contains(&idx) {
            continue;
        }
        let d = sq_dist(c, entry);
        if best_unordered.is_none_or(|(_, bd)| d < bd) {
            best_unordered = Some((idx, d));
        }
        if let (Some(lighter), Some(anchor)) = (c_not_darker, anchor_luma) {
            let ok = if lighter {
                luma(entry) >= anchor
            } else {
                luma(entry) <= anchor
            };
            if !ok {
                continue;
            }
        }
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((idx, d));
        }
    }
    best.or(best_unordered)
        .expect("palette tables are non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::palette;

    /// RT1-7 drift pin: the thresholds kept here must be the midpoints of
    /// the base cube levels — if base ever changes, this fails instead of
    /// the two sides diverging silently.
    #[test]
    fn thresholds_are_base_level_midpoints() {
        for (i, &t) in CUBE_THRESHOLDS.iter().enumerate() {
            let lo = palette::CUBE_LEVELS[i] as u16;
            let hi = palette::CUBE_LEVELS[i + 1] as u16;
            assert_eq!(t as u16, (lo + hi).div_ceil(2), "midpoint {i}");
        }
        // And the levels this module was derived from are the base's.
        assert_eq!(palette::CUBE_LEVELS, [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff]);
    }

    #[test]
    fn cube_corners_map_exactly() {
        assert_eq!(nearest_xterm256(Rgba::rgb(0, 0, 0)), 16);
        assert_eq!(nearest_xterm256(Rgba::rgb(255, 255, 255)), 231);
        assert_eq!(nearest_xterm256(Rgba::rgb(255, 0, 0)), 196);
        assert_eq!(nearest_xterm256(Rgba::rgb(0, 255, 0)), 46);
        assert_eq!(nearest_xterm256(Rgba::rgb(0, 0, 255)), 21);
        assert_eq!(nearest_xterm256(Rgba::rgb(95, 135, 175)), 67); // 16+36+12+3
                                                                   // Every produced index resolves through the shared table.
        for c in [Rgba::rgb(3, 7, 250), Rgba::rgb(130, 128, 126)] {
            let idx = nearest_xterm256(c);
            assert!(idx >= 16);
            let _ = palette::xterm_256(idx); // total for all inputs
        }
    }

    #[test]
    fn grays_prefer_the_ramp() {
        assert_eq!(nearest_xterm256(Rgba::rgb(128, 128, 128)), 244);
        assert_eq!(nearest_xterm256(Rgba::rgb(8, 8, 8)), 232);
        assert_eq!(nearest_xterm256(Rgba::rgb(238, 238, 238)), 255);
    }

    #[test]
    fn ansi16_primaries_against_shared_table() {
        assert_eq!(nearest_ansi16(Rgba::rgb(0, 0, 0)), 0);
        assert_eq!(nearest_ansi16(Rgba::rgb(255, 0, 0)), 9);
        assert_eq!(nearest_ansi16(Rgba::rgb(130, 10, 10)), 1); // near 0x800000
        assert_eq!(nearest_ansi16(Rgba::rgb(255, 255, 255)), 15);
        assert_eq!(nearest_ansi16(Rgba::rgb(0, 190, 190)), 6);
        assert_eq!(nearest_ansi16(Rgba::rgb(192, 192, 192)), 7);
    }

    #[test]
    fn pair_preserves_dark_theme_faint_text() {
        // Dark theme: near-black bg, slightly lighter faint text. Both
        // quantize to gray 234 alone — the pair must not collapse.
        let bg = Rgba::rgb(26, 27, 38);
        let fg = Rgba::rgb(30, 30, 40);
        assert_eq!(
            nearest_xterm256(bg),
            nearest_xterm256(fg),
            "premise: collision"
        );
        let (qfg, qbg) = quantize_pair_256(fg, bg);
        assert_ne!(qfg, qbg, "distinct colors stay distinct");
        // fg was lighter; it must stay at-least-as-light.
        assert!(
            luma(XTERM_256[qfg as usize]) >= luma(XTERM_256[qbg as usize]),
            "ordering preserved: fg {qfg} vs bg {qbg}"
        );
    }

    #[test]
    fn pair_without_collision_is_plain_nearest() {
        let fg = Rgba::rgb(255, 0, 0);
        let bg = Rgba::rgb(0, 0, 0);
        assert_eq!(quantize_pair_256(fg, bg), (196, 16));
        assert_eq!(quantize_pair_16(fg, bg), (9, 0));
    }

    #[test]
    fn pair_identical_colors_stay_identical() {
        let c = Rgba::rgb(30, 30, 40);
        let (qfg, qbg) = quantize_pair_256(c, c);
        assert_eq!(qfg, qbg, "genuinely identical colors may collapse");
    }

    /// The guard the pair path has (`pair_identical_colors_stay_identical`)
    /// carried over to the set: a theme that gives two grounds the same
    /// hex said they are ONE surface. Separating them would invent an
    /// elevation nobody authored, which is a worse defect than the
    /// collapse this whole policy exists to fix. No built-in theme does
    /// this, so this is the only place it is covered.
    #[test]
    fn set_identical_colors_share_one_index() {
        let a = Rgba::rgb(30, 30, 40);
        let far = Rgba::rgb(200, 30, 30);
        let out = quantize_set_256([a, a, far]);
        assert_eq!(out[0], out[1], "identical colors may share an entry");
        assert_eq!(out[0], nearest_xterm256(a), "and it is the natural one");
        assert_ne!(out[2], out[0]);
    }

    /// Nothing colliding, nothing moved — the property that keeps this
    /// safe to apply to every theme rather than only the broken ones.
    #[test]
    fn set_without_collision_is_plain_nearest() {
        let colors = [
            Rgba::rgb(255, 0, 0),
            Rgba::rgb(0, 0, 0),
            Rgba::rgb(255, 255, 255),
        ];
        assert_eq!(quantize_set_256(colors), [196, 16, 231]);
    }

    /// Three grounds landing on one entry: all three come out distinct,
    /// in the authored light/dark order. This is the case
    /// `quantize_pair_256` cannot express at all — it separates two and
    /// has nowhere to put the third.
    #[test]
    fn set_separates_a_three_way_pileup_keeping_order() {
        let dark = Rgba::rgb(26, 27, 38);
        let mid = Rgba::rgb(28, 29, 36);
        let light = Rgba::rgb(30, 30, 40);
        let n = nearest_xterm256(dark);
        assert!(
            nearest_xterm256(mid) == n && nearest_xterm256(light) == n,
            "premise: all three collide on {n}"
        );
        let [qd, qm, ql] = quantize_set_256([dark, mid, light]);
        assert!(qd != qm && qm != ql && qd != ql, "{qd} {qm} {ql}");
        let l = |i: u8| luma(XTERM_256[i as usize]);
        assert!(l(qd) <= l(qm) && l(qm) <= l(ql), "{qd} {qm} {ql}");
    }

    /// The ownership rule, on the case that motivated it: pure white is
    /// represented EXACTLY (entry 231), an off-white next to it is not, so
    /// the off-white is the one that moves.
    #[test]
    fn set_never_moves_an_exactly_representable_color() {
        let white = Rgba::rgb(255, 255, 255);
        let off = Rgba::rgb(250, 250, 250);
        assert_eq!(
            nearest_xterm256(white),
            nearest_xterm256(off),
            "premise: collision"
        );
        let [qw, qo] = quantize_set_256([white, off]);
        assert_eq!(qw, 231, "the exact one keeps its entry");
        assert_ne!(qo, 231);
        // Argument order must not decide it: swapped, same outcome.
        let [qo2, qw2] = quantize_set_256([off, white]);
        assert_eq!((qw2, qo2), (qw, qo));
    }

    /// A one-element set is the nearest lookup, and an empty one is not a
    /// panic — both are reachable from a caller that maps over a token
    /// list.
    #[test]
    fn set_degenerate_sizes_are_total() {
        assert_eq!(quantize_set_256([Rgba::rgb(255, 0, 0)]), [196]);
        assert_eq!(quantize_set_256::<0>([]), [0u8; 0]);
    }

    #[test]
    fn pair_16_collision_nudges_with_ordering() {
        // Both quantize to black in 16-color space.
        let bg = Rgba::rgb(10, 10, 10);
        let fg = Rgba::rgb(40, 40, 40);
        assert_eq!(nearest_ansi16(bg), nearest_ansi16(fg), "premise: collision");
        let (qfg, qbg) = quantize_pair_16(fg, bg);
        assert_ne!(qfg, qbg);
        assert!(luma(SYSTEM_16[qfg as usize]) >= luma(SYSTEM_16[qbg as usize]));
    }

    #[test]
    fn pair_darker_fg_ordering() {
        // Light bg, slightly darker fg colliding on white 231.
        let bg = Rgba::rgb(255, 255, 255);
        let fg = Rgba::rgb(246, 246, 248);
        let (qfg, qbg) = quantize_pair_256(fg, bg);
        assert_ne!(qfg, qbg);
        assert!(luma(XTERM_256[qfg as usize]) <= luma(XTERM_256[qbg as usize]));
    }
}
