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
    nearest_in(&SYSTEM_16, 0, c, None, None).0
}

/// Joint fg/bg quantization to xterm-256 with contrast preservation.
pub fn quantize_pair_256(fg: Rgba, bg: Rgba) -> (u8, u8) {
    let qbg = nearest_xterm256(bg);
    let qfg = nearest_xterm256(fg);
    if qfg != qbg || rgb_eq(fg, bg) {
        return (qfg, qbg);
    }
    // Collision on originally-distinct colors: re-pick fg among 16..=255
    // (same "never emit system colors" rule as the nearest lookup).
    let (nudged, _) = nearest_in(&XTERM_256[16..], 16, fg, Some(qbg), Some(ordering(fg, bg)));
    (nudged, qbg)
}

/// Joint fg/bg quantization to ANSI-16 with contrast preservation.
pub fn quantize_pair_16(fg: Rgba, bg: Rgba) -> (u8, u8) {
    let qbg = nearest_ansi16(bg);
    let qfg = nearest_ansi16(fg);
    if qfg != qbg || rgb_eq(fg, bg) {
        return (qfg, qbg);
    }
    let (nudged, _) = nearest_in(&SYSTEM_16, 0, fg, Some(qbg), Some(ordering(fg, bg)));
    (nudged, qbg)
}

fn rgb_eq(a: Rgba, b: Rgba) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b
}

/// Whether fg is at-least-as-light (`true`) or darker (`false`) than bg —
/// the ordering a nudge must preserve.
fn ordering(fg: Rgba, bg: Rgba) -> bool {
    luma(fg) >= luma(bg)
}

/// Nearest entry of `table` (indices offset by `base`) to `c`, optionally
/// excluding one index and constraining the light/dark ordering against
/// the excluded entry. Falls back to nearest-distinct when the ordering
/// constraint admits nothing (the background sits at the palette's
/// extreme). Ties pick the lower index — deterministic bytes.
fn nearest_in(
    table: &[Rgba],
    base: u8,
    c: Rgba,
    exclude: Option<u8>,
    fg_not_darker: Option<bool>,
) -> (u8, u32) {
    let anchor_luma = exclude.map(|i| luma(XTERM_256[i as usize]));
    let mut best: Option<(u8, u32)> = None;
    let mut best_unordered: Option<(u8, u32)> = None;
    for (i, &entry) in table.iter().enumerate() {
        let idx = base + i as u8;
        if exclude == Some(idx) {
            continue;
        }
        let d = sq_dist(c, entry);
        if best_unordered.is_none_or(|(_, bd)| d < bd) {
            best_unordered = Some((idx, d));
        }
        if let (Some(lighter), Some(anchor)) = (fg_not_darker, anchor_luma) {
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
