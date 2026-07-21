//! RGBA pixel buffer: the interchange type of the whole gfx/three
//! pipeline. PNG decodes into it, the 3D rasterizer draws into it, the
//! mosaic renderer and (cycle 2) the protocol emitters consume it.
//!
//! Convention: row-major, straight (non-premultiplied) alpha — the form
//! PNG, kitty `f=32` and iTerm2 payloads want. Filtering math converts
//! to premultiplied internally (see `resize_bilinear_into` for why).

use crate::base::Rgba;

/// Owned RGBA image. Width/height are `u32` (pixel spaces are never
/// negative; cell spaces use `i32` geometry from `base`).
///
/// ```
/// use abstracttui::base::Rgba;
/// use abstracttui::gfx::Bitmap;
///
/// let mut img = Bitmap::new(4, 3, Rgba::TRANSPARENT);
/// img.set(1, 1, Rgba::rgb(255, 0, 0));
/// assert_eq!(img.get(1, 1).unwrap().r, 255);
/// assert_eq!(img.get(9, 9), None); // out of bounds is None, not a panic
///
/// // Minification chain (texture mips, thumbnails):
/// let half = img.box_halved();
/// assert_eq!((half.width(), half.height()), (2, 2));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    w: u32,
    h: u32,
    px: Vec<Rgba>,
}

impl Default for Bitmap {
    /// Empty (0x0) bitmap — the "no allocation yet" state used by
    /// buffer-reusing consumers like `MosaicRenderer`.
    fn default() -> Bitmap {
        Bitmap {
            w: 0,
            h: 0,
            px: Vec::new(),
        }
    }
}

impl Bitmap {
    /// Solid-color bitmap. `w * h == 0` yields a legal empty bitmap.
    pub fn new(w: u32, h: u32, fill: Rgba) -> Bitmap {
        Bitmap {
            w,
            h,
            px: vec![fill; (w as usize) * (h as usize)],
        }
    }

    /// Build from a per-pixel closure — the test workhorse.
    pub fn from_fn(w: u32, h: u32, mut f: impl FnMut(u32, u32) -> Rgba) -> Bitmap {
        let mut px = Vec::with_capacity((w as usize) * (h as usize));
        for y in 0..h {
            for x in 0..w {
                px.push(f(x, y));
            }
        }
        Bitmap { w, h, px }
    }

    /// Adopt an existing pixel buffer (length must match).
    pub fn from_pixels(w: u32, h: u32, px: Vec<Rgba>) -> Option<Bitmap> {
        if px.len() == (w as usize) * (h as usize) {
            Some(Bitmap { w, h, px })
        } else {
            None
        }
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn height(&self) -> u32 {
        self.h
    }

    pub fn is_empty(&self) -> bool {
        self.px.is_empty()
    }

    /// One mip step: box-filter 2x2 average, dimensions halved
    /// (rounding up; odd edges average the pixels that exist). Plain
    /// per-channel means including alpha — the standard chain for
    /// minification (a gamma-aware chain would need linear-space
    /// textures end to end; documented approximation, invisible at
    /// cell resolution).
    pub fn box_halved(&self) -> Bitmap {
        let w = (self.w.max(1)).div_ceil(2);
        let h = (self.h.max(1)).div_ceil(2);
        Bitmap::from_fn(w, h, |x, y| {
            let (x0, y0) = (x * 2, y * 2);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    if let Some(p) = self.get(x0 + dx, y0 + dy) {
                        acc[0] += p.r as u32;
                        acc[1] += p.g as u32;
                        acc[2] += p.b as u32;
                        acc[3] += p.a as u32;
                        n += 1;
                    }
                }
            }
            debug_assert!(n > 0);
            Rgba::new(
                (acc[0] / n) as u8,
                (acc[1] / n) as u8,
                (acc[2] / n) as u8,
                (acc[3] / n) as u8,
            )
        })
    }

    /// Mip chain BELOW this bitmap: level 1 (half), level 2 (quarter),
    /// ... down to 1x1. Memory cost is ~1/3 of the base image.
    pub fn mip_chain(&self) -> Vec<Bitmap> {
        let mut out = Vec::new();
        if self.w == 0 || self.h == 0 {
            return out;
        }
        let mut cur = self.box_halved();
        while cur.width() > 1 || cur.height() > 1 {
            let next = cur.box_halved();
            out.push(cur);
            cur = next;
        }
        out.push(cur); // the 1x1 tail
        out
    }

    pub fn pixels(&self) -> &[Rgba] {
        &self.px
    }

    pub fn pixels_mut(&mut self) -> &mut [Rgba] {
        &mut self.px
    }

    pub fn fill(&mut self, color: Rgba) {
        self.px.fill(color);
    }

    /// Bounds-checked read; `None` outside the image.
    pub fn get(&self, x: u32, y: u32) -> Option<Rgba> {
        if x < self.w && y < self.h {
            Some(self.px[self.idx(x, y)])
        } else {
            None
        }
    }

    /// Bounds-checked write; out-of-range writes are ignored (rasterizer
    /// clipping relies on this being cheap and non-panicking).
    pub fn set(&mut self, x: u32, y: u32, c: Rgba) {
        if x < self.w && y < self.h {
            let i = self.idx(x, y);
            self.px[i] = c;
        }
    }

    #[inline]
    fn idx(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.w as usize) + (x as usize)
    }

    /// One row as a slice (panics on out-of-range row: internal use is
    /// always loop-bounded by `height()`).
    pub fn row(&self, y: u32) -> &[Rgba] {
        let start = (y as usize) * (self.w as usize);
        &self.px[start..start + self.w as usize]
    }

    /// Convert to premultiplied alpha in place. Rounding: `c*a/255` with
    /// +127 bias (round-half-up) so premultiply→unpremultiply of an
    /// opaque image is the identity.
    pub fn premultiply(&mut self) {
        for p in &mut self.px {
            if p.a != 255 {
                let a = p.a as u32;
                p.r = ((p.r as u32 * a + 127) / 255) as u8;
                p.g = ((p.g as u32 * a + 127) / 255) as u8;
                p.b = ((p.b as u32 * a + 127) / 255) as u8;
            }
        }
    }

    /// Inverse of `premultiply` (lossy where alpha is small: information
    /// genuinely does not survive c*0). Fully transparent stays black.
    pub fn unpremultiply(&mut self) {
        for p in &mut self.px {
            if p.a != 255 && p.a != 0 {
                let a = p.a as u32;
                p.r = (((p.r as u32) * 255 + a / 2) / a).min(255) as u8;
                p.g = (((p.g as u32) * 255 + a / 2) / a).min(255) as u8;
                p.b = (((p.b as u32) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }

    /// Copy a sub-rectangle (clamped to the image; empty when the
    /// clamped region is empty). Consumers: cover/none image fits crop
    /// the source before mosaic resampling.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Bitmap {
        let x = x.min(self.w);
        let y = y.min(self.h);
        let w = w.min(self.w - x);
        let h = h.min(self.h - y);
        let mut px = Vec::with_capacity((w as usize) * (h as usize));
        for row in y..y + h {
            let start = (row as usize) * (self.w as usize) + x as usize;
            px.extend_from_slice(&self.px[start..start + w as usize]);
        }
        Bitmap { w, h, px }
    }

    /// Nearest-neighbor resize into a fresh bitmap.
    pub fn resize_nearest(&self, nw: u32, nh: u32) -> Bitmap {
        let mut out = Bitmap::new(nw, nh, Rgba::TRANSPARENT);
        self.resize_nearest_into(&mut out);
        out
    }

    /// Nearest-neighbor resize reusing `dst`'s allocation (dst keeps its
    /// dimensions). Sample points at pixel centers: src = (i+0.5)*sw/dw,
    /// computed in integer math as (2i+1)*sw/(2dw).
    pub fn resize_nearest_into(&self, dst: &mut Bitmap) {
        if dst.is_empty() || self.is_empty() {
            return;
        }
        let (sw, sh) = (self.w as u64, self.h as u64);
        let (dw, dh) = (dst.w as u64, dst.h as u64);
        for y in 0..dst.h {
            let sy = (((2 * y as u64 + 1) * sh) / (2 * dh)).min(sh - 1) as u32;
            let srow = self.row(sy);
            let drow_start = (y as usize) * (dst.w as usize);
            for x in 0..dst.w {
                let sx = (((2 * x as u64 + 1) * sw) / (2 * dw)).min(sw - 1) as usize;
                dst.px[drow_start + x as usize] = srow[sx];
            }
        }
    }

    /// Bilinear resize into a fresh bitmap.
    pub fn resize_bilinear(&self, nw: u32, nh: u32) -> Bitmap {
        let mut out = Bitmap::new(nw, nh, Rgba::TRANSPARENT);
        self.resize_bilinear_into(&mut out);
        out
    }

    /// Bilinear resize reusing `dst`'s allocation.
    ///
    /// WHY the premultiplied detour: filtering straight RGBA averages the
    /// RGB of fully-transparent pixels (whose color is meaningless) into
    /// visible neighbors, producing dark/dirty halos at every alpha edge.
    /// Weighting each sample's color by its alpha (= premultiplying) makes
    /// invisible pixels contribute nothing. Opaque images skip nothing —
    /// premultiplied == straight at a=255, so the cost is two u32 muls
    /// per tap only when alpha < 255.
    pub fn resize_bilinear_into(&self, dst: &mut Bitmap) {
        if dst.is_empty() || self.is_empty() {
            return;
        }
        // 8.8 fixed point is plenty: terminal-sized targets are < 2^12 px,
        // and 1/256 sub-pixel precision is below one quantization step.
        const FP: u32 = 8;
        const ONE: u32 = 1 << FP;
        let sw = self.w;
        let sh = self.h;
        let step_x = ((sw as u64) << FP) / dst.w as u64;
        let step_y = ((sh as u64) << FP) / dst.h as u64;
        for y in 0..dst.h {
            // Center-aligned sampling: src_y = (y+0.5)*step - 0.5.
            let fy =
                ((y as u64 * step_y + step_y / 2).max((ONE / 2) as u64) - (ONE / 2) as u64) as u32;
            let y0 = (fy >> FP).min(sh - 1);
            let y1 = (y0 + 1).min(sh - 1);
            let wy = fy & (ONE - 1);
            let r0 = self.row(y0);
            let r1 = self.row(y1);
            let drow_start = (y as usize) * (dst.w as usize);
            for x in 0..dst.w {
                let fx = ((x as u64 * step_x + step_x / 2).max((ONE / 2) as u64) - (ONE / 2) as u64)
                    as u32;
                let x0 = ((fx >> FP).min(sw - 1)) as usize;
                let x1 = (x0 + 1).min(sw as usize - 1);
                let wx = fx & (ONE - 1);

                // Premultiplied taps, weights in 8.8.
                let tap = |p: Rgba| -> [u32; 4] {
                    let a = p.a as u32;
                    [p.r as u32 * a, p.g as u32 * a, p.b as u32 * a, a * 255]
                };
                let t00 = tap(r0[x0]);
                let t10 = tap(r0[x1]);
                let t01 = tap(r1[x0]);
                let t11 = tap(r1[x1]);
                let mut acc = [0u32; 4];
                for c in 0..4 {
                    let top = t00[c] * (ONE - wx) + t10[c] * wx; // 16.8
                    let bot = t01[c] * (ONE - wx) + t11[c] * wx;
                    acc[c] = ((top as u64 * (ONE - wy) as u64 + bot as u64 * wy as u64) >> (2 * FP))
                        as u32;
                }
                // acc = [r*a, g*a, b*a, a*255]; un-premultiply.
                // Rounded divide after an explicit zero check reads
                // clearer than checked_div contortions around `+ d/2`.
                let a255 = acc[3];
                #[allow(clippy::manual_checked_ops)]
                let out = if a255 == 0 {
                    Rgba::TRANSPARENT
                } else {
                    Rgba::new(
                        ((acc[0] * 255 + a255 / 2) / a255).min(255) as u8,
                        ((acc[1] * 255 + a255 / 2) / a255).min(255) as u8,
                        ((acc[2] * 255 + a255 / 2) / a255).min(255) as u8,
                        ((a255 + 127) / 255).min(255) as u8,
                    )
                };
                dst.px[drow_start + x as usize] = out;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_fn_and_get_set() {
        let mut b = Bitmap::from_fn(3, 2, |x, y| Rgba::new(x as u8, y as u8, 0, 255));
        assert_eq!(b.get(2, 1), Some(Rgba::new(2, 1, 0, 255)));
        assert_eq!(b.get(3, 0), None);
        b.set(0, 0, Rgba::WHITE);
        assert_eq!(b.get(0, 0), Some(Rgba::WHITE));
        b.set(99, 99, Rgba::WHITE); // silently ignored
        assert_eq!(b.pixels().len(), 6);
    }

    #[test]
    fn from_pixels_checks_len() {
        assert!(Bitmap::from_pixels(2, 2, vec![Rgba::BLACK; 4]).is_some());
        assert!(Bitmap::from_pixels(2, 2, vec![Rgba::BLACK; 3]).is_none());
    }

    #[test]
    fn nearest_upscale_exact_blocks() {
        // 2x1 red|blue doubled to 4x2 must be perfectly block-replicated.
        let src = Bitmap::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba::rgb(255, 0, 0)
            } else {
                Rgba::rgb(0, 0, 255)
            }
        });
        let dst = src.resize_nearest(4, 2);
        for y in 0..2 {
            assert_eq!(dst.get(0, y).unwrap(), Rgba::rgb(255, 0, 0));
            assert_eq!(dst.get(1, y).unwrap(), Rgba::rgb(255, 0, 0));
            assert_eq!(dst.get(2, y).unwrap(), Rgba::rgb(0, 0, 255));
            assert_eq!(dst.get(3, y).unwrap(), Rgba::rgb(0, 0, 255));
        }
    }

    #[test]
    fn nearest_identity() {
        let src = Bitmap::from_fn(5, 4, |x, y| {
            Rgba::new((x * 50) as u8, (y * 60) as u8, 7, 255)
        });
        assert_eq!(src.resize_nearest(5, 4), src);
    }

    #[test]
    fn bilinear_identity_and_midpoint() {
        let src = Bitmap::from_fn(4, 4, |x, y| {
            Rgba::new((x * 60) as u8, (y * 60) as u8, 0, 255)
        });
        // Identity resize reproduces the image exactly (weights hit 0).
        assert_eq!(src.resize_bilinear(4, 4), src);
        // Downscale 2x1 black|white to 1x1: the center sample sits exactly
        // between the two pixels -> mid gray.
        let bw = Bitmap::from_fn(2, 1, |x, _| if x == 0 { Rgba::BLACK } else { Rgba::WHITE });
        let mid = bw.resize_bilinear(1, 1).get(0, 0).unwrap();
        assert!((mid.r as i32 - 127).abs() <= 1, "got {}", mid.r);
        assert_eq!(mid.a, 255);
    }

    #[test]
    fn bilinear_ignores_transparent_color() {
        // Transparent-green next to opaque-red: the halo bug would tint
        // the result toward green; premultiplied filtering must not.
        let src = Bitmap::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba::rgb(255, 0, 0)
            } else {
                Rgba::new(0, 255, 0, 0)
            }
        });
        let out = src.resize_bilinear(1, 1).get(0, 0).unwrap();
        assert_eq!(out.g, 0, "transparent pixel's color leaked: {out:?}");
        assert_eq!(out.r, 255, "opaque color must survive un-premultiply");
        assert!((out.a as i32 - 128).abs() <= 1);
    }

    #[test]
    fn premultiply_round_trip_opaque() {
        let mut b = Bitmap::from_fn(3, 3, |x, y| {
            Rgba::new((x * 80) as u8, (y * 80) as u8, 123, 255)
        });
        let orig = b.clone();
        b.premultiply();
        assert_eq!(b, orig, "opaque premultiply is identity");
        b.unpremultiply();
        assert_eq!(b, orig);
    }

    #[test]
    fn premultiply_half_alpha() {
        let mut b = Bitmap::new(1, 1, Rgba::new(200, 100, 50, 128));
        b.premultiply();
        let p = b.get(0, 0).unwrap();
        assert_eq!((p.r, p.g, p.b), (100, 50, 25));
        assert_eq!(p.a, 128);
    }

    #[test]
    fn crop_clamps_and_copies() {
        let src = Bitmap::from_fn(4, 3, |x, y| Rgba::new(x as u8, y as u8, 0, 255));
        let c = src.crop(1, 1, 2, 2);
        assert_eq!((c.width(), c.height()), (2, 2));
        assert_eq!(c.get(0, 0), Some(Rgba::new(1, 1, 0, 255)));
        assert_eq!(c.get(1, 1), Some(Rgba::new(2, 2, 0, 255)));
        // Clamped past the edge.
        let c = src.crop(3, 2, 10, 10);
        assert_eq!((c.width(), c.height()), (1, 1));
        let c = src.crop(9, 9, 1, 1);
        assert!(c.is_empty());
    }

    #[test]
    fn empty_bitmaps_are_safe() {
        let e = Bitmap::new(0, 0, Rgba::BLACK);
        assert!(e.is_empty());
        let mut d = Bitmap::new(2, 2, Rgba::BLACK);
        e.resize_nearest_into(&mut d); // no-op, no panic
        e.resize_bilinear_into(&mut d);
        assert_eq!(e.resize_nearest(3, 3).width(), 3); // stays transparent
    }
}
