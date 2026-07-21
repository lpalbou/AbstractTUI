//! Floyd–Steinberg error-diffusion dithering onto an arbitrary target
//! palette. Consumers: the sixel encoder (cycle 2 — sixel is limited to
//! ~256 color registers, and we plan 16–64 for emission economy) and
//! low-color mosaic targets. The mosaic 2-color fit itself stays
//! truecolor; dithering is a *pre-quantization* pass.
//!
//! Serpentine scanning (left→right on even rows, right→left on odd) is
//! used because unidirectional diffusion drags error in one direction
//! and produces visible diagonal "worms"; alternating direction breaks
//! them up for the cost of an index flip. Error is carried per channel
//! in i16 workspace rows with saturating math — diffusion can push
//! intermediate values out of 0..=255 legitimately, and clamping only
//! at the palette lookup would double-count the clamp error.

use crate::base::Rgba;
use crate::gfx::bitmap::Bitmap;

/// Nearest palette entry by squared sRGB distance (the same metric the
/// mosaic fit uses — consistency keeps quantization decisions aligned).
/// Alpha does not participate: palettes are opaque display colors.
fn nearest(palette: &[Rgba], r: i32, g: i32, b: i32) -> usize {
    let mut best = 0usize;
    let mut best_d = i64::MAX;
    for (i, p) in palette.iter().enumerate() {
        let dr = (r - p.r as i32) as i64;
        let dg = (g - p.g as i32) as i64;
        let db = (b - p.b as i32) as i64;
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

/// Dither `src` onto `palette` in place. Returns the per-pixel palette
/// indices (the sixel encoder wants indices, not colors). Pixels with
/// alpha 0 are left untouched and get index `usize::MAX` — transparency
/// is handled by the emitter (sixel P2=1), not by the palette.
///
/// Floyd–Steinberg kernel (x = current pixel, error e distributed):
///
/// ```text
///           x    7/16
///   3/16  5/16   1/16      (mirrored on right-to-left rows)
/// ```
pub fn floyd_steinberg(src: &mut Bitmap, palette: &[Rgba]) -> Vec<usize> {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let mut indices = vec![usize::MAX; w * h];
    if palette.is_empty() || w == 0 || h == 0 {
        return indices;
    }

    // Two rows of running error per channel: current and next.
    let mut err_cur = vec![[0i32; 3]; w];
    let mut err_next = vec![[0i32; 3]; w];

    for y in 0..h {
        let ltr = y % 2 == 0;
        for i in 0..w {
            let x = if ltr { i } else { w - 1 - i };
            let p = src.get(x as u32, y as u32).expect("in bounds");
            if p.a == 0 {
                continue; // fully transparent: no color to quantize
            }
            let e = err_cur[x];
            // Apply carried error, clamp to displayable range for the
            // lookup but keep the *unclamped* value for error math so
            // saturation is not silently forgotten... clamping here and
            // computing residual against the clamped value is the
            // standard choice: it prevents runaway accumulation at
            // image borders (error cannot exceed 255/channel).
            let r = (p.r as i32 + e[0]).clamp(0, 255);
            let g = (p.g as i32 + e[1]).clamp(0, 255);
            let b = (p.b as i32 + e[2]).clamp(0, 255);
            let idx = nearest(palette, r, g, b);
            let q = palette[idx];
            indices[y * w + x] = idx;
            src.set(x as u32, y as u32, Rgba::new(q.r, q.g, q.b, p.a));

            let dr = r - q.r as i32;
            let dg = g - q.g as i32;
            let db = b - q.b as i32;
            // Neighbor offsets follow scan direction: "forward" is +1
            // on LTR rows, -1 on RTL rows; the diagonal pair mirrors.
            let fwd: i32 = if ltr { 1 } else { -1 };
            let spread = |row: &mut [[i32; 3]], xi: i32, num: i32| {
                if xi >= 0 && (xi as usize) < w {
                    let cell = &mut row[xi as usize];
                    cell[0] += dr * num / 16;
                    cell[1] += dg * num / 16;
                    cell[2] += db * num / 16;
                }
            };
            spread(&mut err_cur, x as i32 + fwd, 7);
            spread(&mut err_next, x as i32 - fwd, 3);
            spread(&mut err_next, x as i32, 5);
            spread(&mut err_next, x as i32 + fwd, 1);
        }
        std::mem::swap(&mut err_cur, &mut err_next);
        for e in err_next.iter_mut() {
            *e = [0; 3];
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_palette_colors_pass_through() {
        let pal = [Rgba::BLACK, Rgba::WHITE, Rgba::rgb(255, 0, 0)];
        let mut b = Bitmap::from_fn(3, 1, |x, _| pal[x as usize]);
        let idx = floyd_steinberg(&mut b, &pal);
        assert_eq!(idx, vec![0, 1, 2]);
        assert_eq!(b.get(2, 0).unwrap(), Rgba::rgb(255, 0, 0));
    }

    #[test]
    fn mid_gray_on_bw_palette_averages_out() {
        // 50% gray dithered to black/white must approximate 50% cover:
        // conservation of brightness is the whole point of dithering.
        let pal = [Rgba::BLACK, Rgba::WHITE];
        let mut b = Bitmap::new(16, 16, Rgba::rgb(128, 128, 128));
        let idx = floyd_steinberg(&mut b, &pal);
        let whites = idx.iter().filter(|&&i| i == 1).count();
        let ratio = whites as f32 / 256.0;
        assert!((ratio - 0.5).abs() < 0.1, "white ratio {ratio}");
    }

    #[test]
    fn transparent_pixels_skipped() {
        let pal = [Rgba::BLACK, Rgba::WHITE];
        let mut b = Bitmap::new(2, 1, Rgba::TRANSPARENT);
        b.set(1, 0, Rgba::WHITE);
        let idx = floyd_steinberg(&mut b, &pal);
        assert_eq!(idx[0], usize::MAX);
        assert_eq!(idx[1], 1);
        assert_eq!(b.get(0, 0).unwrap(), Rgba::TRANSPARENT, "untouched");
    }

    #[test]
    fn empty_palette_is_safe() {
        let mut b = Bitmap::new(2, 2, Rgba::WHITE);
        let idx = floyd_steinberg(&mut b, &[]);
        assert!(idx.iter().all(|&i| i == usize::MAX));
    }

    #[test]
    fn alpha_preserved_through_quantization() {
        let pal = [Rgba::BLACK, Rgba::WHITE];
        let mut b = Bitmap::new(1, 1, Rgba::new(200, 200, 200, 90));
        floyd_steinberg(&mut b, &pal);
        assert_eq!(b.get(0, 0).unwrap().a, 90);
    }
}
