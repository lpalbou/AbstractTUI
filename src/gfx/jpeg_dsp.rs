//! JPEG signal path: dezigzag + dequantization, the inverse DCT, and
//! YCbCr color conversion.
//!
//! IDCT choice (documented per the scope ruling): a NAIVE SEPARABLE
//! floating-point IDCT over a precomputed cosine table — the textbook
//! T.81 A.3.3 definition, transcribed 1:1. Chosen over AAN/integer
//! butterflies deliberately: texture decode is a ONE-TIME cost per
//! model load (measured in the cycle report), and a rasterizer bug
//! hunt must never have to ask "or is it the fancy IDCT?". The
//! separable form is O(8³) per axis instead of O(8⁴) full naive.

use std::sync::OnceLock;

/// Zigzag scan order: `ZIGZAG[i]` = natural (row-major) index of the
/// i-th zigzag coefficient (T.81 figure 5).
#[rustfmt::skip]
pub const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// cos((2x+1)·u·π/16) with the α(u) normalization folded in
/// (α(0) = 1/√2, else 1) and the global 1/2-per-axis factor applied at
/// use. Computed once (OnceLock — std only).
fn cos_table() -> &'static [[f32; 8]; 8] {
    static TABLE: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [[0.0f32; 8]; 8];
        for (u, row) in t.iter_mut().enumerate() {
            let alpha = if u == 0 {
                1.0 / std::f32::consts::SQRT_2
            } else {
                1.0
            };
            for (x, v) in row.iter_mut().enumerate() {
                *v =
                    alpha * ((2.0 * x as f32 + 1.0) * u as f32 * std::f32::consts::PI / 16.0).cos();
            }
        }
        t
    })
}

/// Dezigzag + dequantize into natural order. `zz` and `quant` are both
/// in zigzag order (as they arrive on the wire).
pub fn dequantize(zz: &[i32; 64], quant: &[u16; 64]) -> [f32; 64] {
    let mut natural = [0.0f32; 64];
    for i in 0..64 {
        natural[ZIGZAG[i]] = (zz[i] * quant[i] as i32) as f32;
    }
    natural
}

/// Separable 2D IDCT + level shift (+128) + clamp to u8.
/// `coef` natural order (`coef[r*8 + c]`: r = VERTICAL frequency,
/// c = HORIZONTAL frequency); `out` row-major 8x8 samples.
///
/// Axis discipline (a transposition bug hid here once): pass 1 runs
/// the HORIZONTAL transform (frequency c pairs with spatial x), pass 2
/// the VERTICAL one (r pairs with y). The `single_ac_coefficient` test
/// pins the separable result against the direct 4-loop definition with
/// an asymmetric coefficient pair, so a swapped pairing can never come
/// back quietly.
pub fn idct_8x8(coef: &[f32; 64], out: &mut [u8; 64]) {
    let cos = cos_table();
    // Pass 1 (horizontal): g(r, x) = 1/2 Σc α(c) F(r,c) cos((2x+1)cπ/16).
    let mut tmp = [0.0f32; 64];
    for r in 0..8 {
        for x in 0..8 {
            let mut acc = 0.0;
            for c in 0..8 {
                acc += cos[c][x] * coef[r * 8 + c];
            }
            tmp[r * 8 + x] = acc * 0.5;
        }
    }
    // Pass 2 (vertical): f(x, y) = 1/2 Σr α(r) g(r,x) cos((2y+1)rπ/16).
    for y in 0..8 {
        for x in 0..8 {
            let mut acc = 0.0;
            for r in 0..8 {
                acc += cos[r][y] * tmp[r * 8 + x];
            }
            let v = acc * 0.5 + 128.0;
            out[y * 8 + x] = v.clamp(0.0, 255.0).round() as u8;
        }
    }
}

/// JFIF YCbCr -> RGB (fixed point, <<16 coefficients, rounded).
#[inline]
pub fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y = (y as i32) << 16;
    let cb = cb as i32 - 128;
    let cr = cr as i32 - 128;
    let clamp = |v: i32| (v >> 16).clamp(0, 255) as u8;
    let half = 1 << 15;
    (
        clamp(y + 91_881 * cr + half),
        clamp(y - 22_554 * cb - 46_802 * cr + half),
        clamp(y + 116_130 * cb + half),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_only_block_is_flat() {
        // A DC-only coefficient c produces the constant c/8 across the
        // block (both α(0) factors and both 1/2 scales): with c = 240,
        // every sample is 240/8 + 128 = 158.
        let mut coef = [0.0f32; 64];
        coef[0] = 240.0;
        let mut out = [0u8; 64];
        idct_8x8(&coef, &mut out);
        assert!(out.iter().all(|&s| s == 158), "{out:?}");
    }

    #[test]
    fn single_ac_coefficient_is_a_cosine_wave() {
        // F(1,0) alone: f(x,y) = 1/4 · cos((2x+1)π/16) · c/√2·... —
        // verify against a direct 4-loop reference IDCT.
        let mut coef = [0.0f32; 64];
        coef[1] = 100.0; // u=0, v=1 in natural order (row 0, col 1)
        coef[8] = -60.0; // u=1, v=0
        let mut fast = [0u8; 64];
        idct_8x8(&coef, &mut fast);

        let mut reference = [0u8; 64];
        for y in 0..8 {
            for x in 0..8 {
                let mut acc = 0.0f32;
                for u in 0..8usize {
                    for v in 0..8usize {
                        let au = if u == 0 { 1.0 / 2f32.sqrt() } else { 1.0 };
                        let av = if v == 0 { 1.0 / 2f32.sqrt() } else { 1.0 };
                        acc += 0.25
                            * au
                            * av
                            * coef[u * 8 + v]
                            * (((2 * x + 1) as f32 * v as f32 * std::f32::consts::PI) / 16.0).cos()
                            * (((2 * y + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos();
                    }
                }
                reference[y * 8 + x] = (acc + 128.0).clamp(0.0, 255.0).round() as u8;
            }
        }
        assert_eq!(fast, reference);
    }

    #[test]
    fn zigzag_is_a_permutation() {
        let mut seen = [false; 64];
        for &i in &ZIGZAG {
            assert!(!seen[i]);
            seen[i] = true;
        }
        // Spot pins from the spec figure.
        assert_eq!(ZIGZAG[0], 0);
        assert_eq!(ZIGZAG[1], 1);
        assert_eq!(ZIGZAG[2], 8);
        assert_eq!(ZIGZAG[63], 63);
    }

    #[test]
    fn ycbcr_known_points() {
        assert_eq!(ycbcr_to_rgb(128, 128, 128), (128, 128, 128), "neutral gray");
        assert_eq!(ycbcr_to_rgb(255, 128, 128), (255, 255, 255));
        assert_eq!(ycbcr_to_rgb(0, 128, 128), (0, 0, 0));
        // Pure red in YCbCr: Y=76, Cb=85, Cr=255 (approx).
        let (r, g, b) = ycbcr_to_rgb(76, 85, 255);
        assert!(r > 245 && g < 12 && b < 12, "({r},{g},{b})");
    }
}
