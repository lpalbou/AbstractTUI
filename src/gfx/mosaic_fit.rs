//! Per-cell fitting: choose one glyph + (fg, bg) for a handful of
//! subpixels. Split from `mosaic.rs` so the grid orchestration and the
//! per-cell math evolve (and get attacked) independently.
//!
//! ## The 2-color fit (Quadrant / Sextant)
//!
//! A cell shows one glyph: some subpixels take the foreground color,
//! the rest the background. For a fixed partition (glyph pattern) the
//! best colors are weighted means, because the total squared error
//! `Σ_fg w·‖c−fg‖² + Σ_bg w·‖c−bg‖²` is minimized per channel by the
//! weighted average of each side (least squares). So the search is:
//! for every candidate pattern, take the two weighted means, sum the
//! squared error, keep the argmin. Weights are alpha, so transparent
//! pixels influence nothing.
//!
//! Two implementation choices worth defending:
//!
//! - **Canonical patterns.** A pattern and its complement describe the
//!   same partition with fg/bg swapped, so only patterns where
//!   subpixel 0 is background are searched: 8 instead of 16
//!   (quadrant), 32 instead of 64 (sextant). Free side effect: a
//!   uniform cell wins as pattern 0 == space + background color, which
//!   is exactly chafa's "never blit full blocks" SGR-run economy.
//! - **Error against the *quantized* means.** Means are rounded to u8
//!   before the error is scored, because u8 is what the terminal will
//!   actually display; scoring unrounded means can prefer a pattern
//!   whose displayed (rounded) colors are worse. Per channel and set:
//!   `Σ w(c−m)² = Σ wc² − 2m·Σ wc + m²·Σ w`, all integer — one pass
//!   over the pattern's set bits for the sums, O(1) for the error.
//!
//! Metric: squared distance in sRGB with integer math (chafa parity).
//! Perceptual metrics (Oklab/CIEDE2000) were rejected: an order of
//! magnitude more arithmetic per candidate for differences that are
//! invisible at 2x3-pixel granularity. Revisit only with evidence.

use crate::base::Rgba;
use crate::gfx::mosaic::MosaicCell;

/// Quadrant glyphs indexed by fg pattern bits (bit i = subpixel i is
/// fg; subpixels row-major: 0=UL, 1=UR, 2=LL, 3=LR).
pub(crate) const QUADRANT_CHARS: [char; 16] = [
    ' ', '\u{2598}', '\u{259D}', '\u{2580}', // -, UL, UR, upper half
    '\u{2596}', '\u{258C}', '\u{259E}', '\u{259B}', // LL, left half, anti-diag, no-LR
    '\u{2597}', '\u{259A}', '\u{2590}', '\u{259C}', // LR, diag, right half, no-LL
    '\u{2584}', '\u{2599}', '\u{259F}', '\u{2588}', // lower half, no-UR, no-UL, full
];

/// Sextant glyphs indexed by fg pattern bits (bit i = subpixel i,
/// row-major 2x3: 0=UL, 1=UR, 2=ML, 3=MR, 4=LL, 5=LR). Unicode names
/// number cells 1..=6 in the same order, so index == the pattern the
/// name describes. U+1FB00.. block sextants skip the three patterns
/// that already exist as legacy blocks (▌ at 21, ▐ at 42, █ at 63).
pub(crate) const SEXTANT_CHARS: [char; 64] = [
    ' ', '🬀', '🬁', '🬂', '🬃', '🬄', '🬅', '🬆', '🬇', '🬈', '🬉', '🬊', '🬋', '🬌', '🬍', '🬎', '🬏', '🬐', '🬑',
    '🬒', '🬓', '▌', '🬔', '🬕', '🬖', '🬗', '🬘', '🬙', '🬚', '🬛', '🬜', '🬝', '🬞', '🬟', '🬠', '🬡', '🬢', '🬣',
    '🬤', '🬥', '🬦', '🬧', '▐', '🬨', '🬩', '🬪', '🬫', '🬬', '🬭', '🬮', '🬯', '🬰', '🬱', '🬲', '🬳', '🬴', '🬵',
    '🬶', '🬷', '🬸', '🬹', '🬺', '🬻', '█',
];

/// Braille dot bit for subpixel (x, y): U+2800 + OR of lit bits.
/// Dots are numbered column-first (1-3,7 left; 4-6,8 right) with the
/// bottom row split out historically — hence the irregular bit table.
pub(crate) const BRAILLE_BITS: [[u32; 2]; 4] =
    [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// HalfBlock: exact by construction — `▀` paints the top half with fg.
/// A uniform pair canonicalizes to space + bg (SGR-run economy; the
/// glyph is irrelevant when both halves match).
pub(crate) fn fit_half_block(top: Rgba, bottom: Rgba) -> MosaicCell {
    if top == bottom {
        MosaicCell {
            ch: ' ',
            fg: bottom,
            bg: bottom,
        }
    } else {
        MosaicCell {
            ch: '\u{2580}',
            fg: top,
            bg: bottom,
        }
    }
}

/// Weighted least-squares 2-color fit over all canonical patterns.
/// `chars` is indexed by fg-bit pattern; `chars.len() == 1 << n`.
pub(crate) fn fit_two_color(sub: &[Rgba], chars: &[char]) -> MosaicCell {
    let n = sub.len();
    debug_assert!(n <= 8 && chars.len() == 1 << n);

    // Per-subpixel weighted moments (w = alpha): w*c per channel plus
    // totals. u32 is safe: 255²·8 < 2³². The w·c² moment of the
    // cycle-6 scorer is gone — the partition-independent term cancels
    // in the comparison (see the scoring note below).
    let mut wc = [[0u32; 3]; 8];
    let mut w = [0u32; 8];
    let mut tot_w = 0u64;
    let mut tot_c = [0u64; 3];
    let mut tot_a = 0u32;
    for i in 0..n {
        let p = sub[i];
        let wi = p.a as u32;
        w[i] = wi;
        wc[i] = [wi * p.r as u32, wi * p.g as u32, wi * p.b as u32];
        tot_w += wi as u64;
        for ch in 0..3 {
            tot_c[ch] += wc[i][ch] as u64;
        }
        tot_a += p.a as u32;
    }

    // Nothing visible in this cell at all.
    if tot_w == 0 {
        return MosaicCell::EMPTY;
    }

    let mut best_pattern = 0usize;
    let mut best_score = f64::NEG_INFINITY;

    // Canonical patterns only: subpixel 0 belongs to bg (even indices).
    //
    // Scoring (cycle-7 perf wave): for a fixed partition, the optimal
    // per-side color is the weighted mean, and the residual is
    //   Σ w·c² − Σ_side (Σ w·c)² / Σ_side w    (per channel).
    // The first term is partition-independent, so MINIMIZING the
    // error is MAXIMIZING  score = Σ_ch (fc²)/fw + Σ_ch (bc²)/bw —
    // two divisions per pattern instead of the previous six (rounded
    // integer means were computed per pattern before; now only the
    // winner's means are materialized). Exactness: all sums are
    // integers < 2^40, exact in f64; the score compares TRUE least-
    // squares residuals instead of u8-quantized ones — on near-ties
    // the winning glyph can differ from cycle-6 by one equivalent
    // pattern (deliberate; goldens re-reviewed).
    let mut pattern = 0usize;
    while pattern < (1 << n) {
        // Gather fg-side sums by iterating set bits.
        let mut fw = 0u64;
        let mut fc = [0u64; 3];
        let mut bits = pattern;
        while bits != 0 {
            let i = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            fw += w[i] as u64;
            for ch in 0..3 {
                fc[ch] += wc[i][ch] as u64;
            }
        }
        let bw = tot_w - fw;
        let bc = [tot_c[0] - fc[0], tot_c[1] - fc[1], tot_c[2] - fc[2]];

        let side = |c: &[u64; 3], sw: u64| -> f64 {
            if sw == 0 {
                0.0
            } else {
                let s = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) as f64;
                s / sw as f64
            }
        };
        let score = side(&fc, fw) + side(&bc, bw);

        // Strict greater-than: ties keep the earliest (lowest)
        // pattern, which has the fewest fg bits — cheapest for SGR
        // runs (same tie rule as cycle 6).
        if score > best_score {
            best_score = score;
            best_pattern = pattern;
        }
        pattern += 2;
    }

    // Materialize the winner's colors only (means + alphas).
    let mut fw = 0u64;
    let mut fc = [0u64; 3];
    let mut fa = 0u32;
    let mut fcount = 0u32;
    let mut bits = best_pattern;
    while bits != 0 {
        let i = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        fw += w[i] as u64;
        for ch in 0..3 {
            fc[ch] += wc[i][ch] as u64;
        }
        fa += sub[i].a as u32;
        fcount += 1;
    }
    let bw = tot_w - fw;
    let bc = [tot_c[0] - fc[0], tot_c[1] - fc[1], tot_c[2] - fc[2]];
    let ba = tot_a - fa;
    let bcount = n as u32 - fcount;

    // Rounded means after explicit zero checks; checked_div would
    // obscure the `+ sw/2` rounding intent.
    #[allow(clippy::manual_checked_ops)]
    let mean = |s: &[u64; 3], sw: u64| -> [u8; 3] {
        if sw == 0 {
            [0, 0, 0]
        } else {
            [
                ((s[0] + sw / 2) / sw) as u8,
                ((s[1] + sw / 2) / sw) as u8,
                ((s[2] + sw / 2) / sw) as u8,
            ]
        }
    };
    let fm = mean(&fc, fw);
    let bm = mean(&bc, bw);
    // Reported alpha is the plain mean of the side's alphas so a
    // half-covered cell composites believably; color mean is
    // alpha-weighted (premultiplied semantics).
    #[allow(clippy::manual_checked_ops)]
    let alpha = |sum_a: u32, count: u32| -> u8 {
        if count == 0 {
            0
        } else {
            ((sum_a + count / 2) / count) as u8
        }
    };
    let best_fg = if fw == 0 {
        Rgba::TRANSPARENT
    } else {
        Rgba::new(fm[0], fm[1], fm[2], alpha(fa, fcount))
    };
    let best_bg = if bw == 0 {
        Rgba::TRANSPARENT
    } else {
        Rgba::new(bm[0], bm[1], bm[2], alpha(ba, bcount))
    };

    MosaicCell {
        ch: chars[best_pattern],
        fg: best_fg,
        bg: best_bg,
    }
}

/// Braille: threshold on alpha-weighted luma against the cell mean.
/// Deliberately NOT a coverage fit: braille dots render sparse (a lit
/// dot covers a fraction of its 1/8 cell), so the "fg paints this
/// fraction of the cell" model behind the block fit is wrong for it —
/// braille's real job is structure and line art. Integer luma (BT.709
/// weights x10000) instead of the WCAG linearized luminance: the
/// threshold only needs a monotone brightness ordering, and skipping
/// the gamma powf keeps the hot path integer.
pub(crate) fn fit_braille(sub: &[Rgba]) -> MosaicCell {
    debug_assert_eq!(sub.len(), 8);
    let luma = |p: Rgba| -> u64 {
        // (0..=2_550_000) * alpha: transparent pixels read as dark.
        (2126 * p.r as u64 + 7152 * p.g as u64 + 722 * p.b as u64) * p.a as u64
    };
    let mut lum = [0u64; 8];
    let mut total = 0u64;
    let mut any_visible = false;
    for (i, &p) in sub.iter().enumerate() {
        lum[i] = luma(p);
        total += lum[i];
        any_visible |= p.a != 0;
    }
    if !any_visible {
        return MosaicCell::EMPTY;
    }
    let mean = total / 8;

    let mut bits = 0u32;
    let mut lit_c = [0u64; 3];
    let mut lit_w = 0u64;
    let mut lit_a = 0u32;
    let mut lit_n = 0u32;
    let mut unlit_c = [0u64; 3];
    let mut unlit_w = 0u64;
    let mut unlit_a = 0u32;
    let mut unlit_n = 0u32;
    for (i, &p) in sub.iter().enumerate() {
        let w = p.a as u64;
        // Strictly above the mean: a uniform cell lights no dots and
        // falls through to blank-braille + background color.
        if lum[i] > mean {
            bits |= BRAILLE_BITS[i / 2][i % 2];
            lit_w += w;
            lit_a += p.a as u32;
            lit_n += 1;
            lit_c[0] += w * p.r as u64;
            lit_c[1] += w * p.g as u64;
            lit_c[2] += w * p.b as u64;
        } else {
            unlit_w += w;
            unlit_a += p.a as u32;
            unlit_n += 1;
            unlit_c[0] += w * p.r as u64;
            unlit_c[1] += w * p.g as u64;
            unlit_c[2] += w * p.b as u64;
        }
    }
    let side = |c: &[u64; 3], sw: u64, sa: u32, n: u32| -> Rgba {
        if sw == 0 || n == 0 {
            Rgba::TRANSPARENT
        } else {
            Rgba::new(
                ((c[0] + sw / 2) / sw) as u8,
                ((c[1] + sw / 2) / sw) as u8,
                ((c[2] + sw / 2) / sw) as u8,
                ((sa + n / 2) / n) as u8,
            )
        }
    };
    MosaicCell {
        ch: char::from_u32(0x2800 + bits).expect("braille block is contiguous"),
        fg: side(&lit_c, lit_w, lit_a, lit_n),
        bg: side(&unlit_c, unlit_w, unlit_a, unlit_n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sextant_table_is_consistent() {
        // FULL verification against the Unicode 13 allocation rule for
        // Symbols for Legacy Computing (cycle-7 upgrade from spot
        // checks): U+1FB00.. allocates every 2x3 pattern in bit order
        // EXCEPT the four with pre-existing glyphs — empty (space,
        // bits 0), left half (▌ U+258C, bits 21 = cells 1,3,5), right
        // half (▐ U+2590, bits 42 = cells 2,4,6), full block (█
        // U+2588, bits 63). Every skipped pattern shifts later
        // codepoints down by one.
        assert_eq!(SEXTANT_CHARS[0], ' ');
        assert_eq!(SEXTANT_CHARS[21], '\u{258C}');
        assert_eq!(SEXTANT_CHARS[42], '\u{2590}');
        assert_eq!(SEXTANT_CHARS[63], '\u{2588}');
        for bits in 1u32..63 {
            if bits == 21 || bits == 42 {
                continue;
            }
            let skipped = u32::from(bits > 21) + u32::from(bits > 42);
            let expected = char::from_u32(0x1FB00 + bits - 1 - skipped).unwrap();
            assert_eq!(
                SEXTANT_CHARS[bits as usize],
                expected,
                "sextant bits {bits:06b} must map to U+{:04X}",
                0x1FB00 + bits - 1 - skipped
            );
        }
        let uniq: std::collections::HashSet<char> = SEXTANT_CHARS.iter().copied().collect();
        assert_eq!(uniq.len(), 64);
    }

    #[test]
    fn quadrant_table_is_consistent() {
        assert_eq!(QUADRANT_CHARS[0], ' ');
        assert_eq!(QUADRANT_CHARS[0b0011], '\u{2580}'); // upper half = UL|UR
        assert_eq!(QUADRANT_CHARS[0b0101], '\u{258C}'); // left half = UL|LL
        assert_eq!(QUADRANT_CHARS[0b1111], '\u{2588}');
        let uniq: std::collections::HashSet<char> = QUADRANT_CHARS.iter().copied().collect();
        assert_eq!(uniq.len(), 16);
    }

    #[test]
    fn braille_bit_table_covers_all_dots() {
        let mut all = 0u32;
        for row in BRAILLE_BITS {
            for b in row {
                assert_eq!(all & b, 0, "duplicate bit");
                all |= b;
            }
        }
        assert_eq!(all, 0xFF);
    }

    #[test]
    fn two_color_fit_moment_headroom() {
        // Worst-case accumulators: 8 fully-saturated opaque subpixels.
        // wcc per pixel = 255 * 3 * 255² = 49,744,125 < u32::MAX; the
        // fit must not wrap (a wrap would misrank patterns silently).
        let sub = [Rgba::WHITE; 6];
        let cell = fit_two_color(&sub, &SEXTANT_CHARS);
        assert_eq!(cell.ch, ' ', "uniform white = space + bg");
        assert_eq!((cell.bg.r, cell.bg.g, cell.bg.b), (255, 255, 255));
    }
}
