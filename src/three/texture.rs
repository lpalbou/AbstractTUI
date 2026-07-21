//! Texture sampling for the rasterizer: bilinear filtering with
//! repeat/clamp wrap modes over a decoded `gfx::Bitmap`.
//!
//! sRGB policy (the ONE place it lives): glTF base-color texels are
//! sRGB-encoded; lighting math runs in linear space. The engine's
//! gamma pair is the cheap gamma-2 approximation — `srgb8_to_linear`
//! here squares, the rasterizer's output `sqrt`s (see
//! `raster::fill_triangle`) — chosen over the exact 2.4 curve because
//! two `powf` per texel per pixel is the wrong place to spend the
//! frame budget and the terminal-cell quantization downstream hides
//! the residue. `baseColorFactor` and vertex colors are DECLARED
//! linear by glTF and are never converted.

use crate::gfx::bitmap::Bitmap;

/// UV wrap mode (glTF sampler subset; default REPEAT per spec).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    Clamp,
}

/// Magnification filter. Bilinear is the default (terminal-cell
/// output oversamples the texture and linear filtering hides the
/// stairs); Nearest exists for pixel-art sources and for the
/// cheapest-possible path (1 fetch vs 4).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    Bilinear,
    Nearest,
}

impl Wrap {
    /// Map a texel coordinate into [0, n).
    #[inline]
    fn apply(self, i: i32, n: i32) -> i32 {
        match self {
            Wrap::Repeat => i.rem_euclid(n),
            Wrap::Clamp => i.clamp(0, n - 1),
        }
    }
}

/// Borrowed sampler: texture data + wrap modes, sampling into LINEAR
/// RGB (the rasterizer's color space).
pub struct TextureSampler<'a> {
    texels: &'a Bitmap,
    wrap_u: Wrap,
    wrap_v: Wrap,
    filter: Filter,
}

/// sRGB u8 -> linear, gamma-2 approximation (see module doc).
#[inline]
pub fn srgb8_to_linear(v: u8) -> f32 {
    let f = v as f32 / 255.0;
    f * f
}

impl<'a> TextureSampler<'a> {
    /// `None` for empty bitmaps — a 0-sized texture cannot be sampled
    /// and callers degrade to untextured (labeled by the scene layer).
    pub fn new(texels: &'a Bitmap, wrap_u: Wrap, wrap_v: Wrap) -> Option<TextureSampler<'a>> {
        if texels.is_empty() {
            None
        } else {
            Some(TextureSampler {
                texels,
                wrap_u,
                wrap_v,
                filter: Filter::Bilinear,
            })
        }
    }

    /// Builder: switch the magnification filter.
    pub fn with_filter(mut self, filter: Filter) -> TextureSampler<'a> {
        self.filter = filter;
        self
    }

    /// Sample at (u, v) in texture space (v = 0 is the TOP row —
    /// glTF's UV origin matches image row order, no flip). Returns
    /// linear RGB; texel alpha is ignored in v1 (opaque rendering).
    pub fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        match self.filter {
            Filter::Bilinear => self.sample_bilinear(u, v),
            Filter::Nearest => self.sample_nearest(u, v),
        }
    }

    /// Nearest texel: one fetch, hard edges.
    fn sample_nearest(&self, u: f32, v: f32) -> [f32; 3] {
        let (w, h) = (self.texels.width() as i32, self.texels.height() as i32);
        // Floor of u*w picks the texel whose [k/w, (k+1)/w) span holds u.
        let x = self.wrap_u.apply((u * w as f32).floor() as i32, w);
        let y = self.wrap_v.apply((v * h as f32).floor() as i32, h);
        let p = self.texels.row(y as u32)[x as usize];
        [
            srgb8_to_linear(p.r),
            srgb8_to_linear(p.g),
            srgb8_to_linear(p.b),
        ]
    }

    fn sample_bilinear(&self, u: f32, v: f32) -> [f32; 3] {
        let (w, h) = (self.texels.width() as i32, self.texels.height() as i32);
        // Texel-center convention: u*w - 0.5 puts u=0.5/w exactly on
        // texel 0's center, so a 1:1 mapping does not blur.
        let x = u * w as f32 - 0.5;
        let y = v * h as f32 - 0.5;
        let (x0f, y0f) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0f, y - y0f);
        // NaN guard: hostile UVs (from NaN-poisoned but finite-position
        // vertices) must not index out of range — rem_euclid/clamp on a
        // huge cast is safe, NaN casts to 0 in Rust (saturating), so
        // the math below cannot panic either way.
        let (x0, y0) = (x0f as i32, y0f as i32);
        let tx = |i: i32| self.wrap_u.apply(i, w);
        let ty = |i: i32| self.wrap_v.apply(i, h);
        let p00 = self.texels.row(ty(y0) as u32)[tx(x0) as usize];
        let p10 = self.texels.row(ty(y0) as u32)[tx(x0 + 1) as usize];
        let p01 = self.texels.row(ty(y0 + 1) as u32)[tx(x0) as usize];
        let p11 = self.texels.row(ty(y0 + 1) as u32)[tx(x0 + 1) as usize];
        let mut out = [0.0f32; 3];
        let ch = |p: crate::base::Rgba, k: usize| match k {
            0 => srgb8_to_linear(p.r),
            1 => srgb8_to_linear(p.g),
            _ => srgb8_to_linear(p.b),
        };
        for (k, o) in out.iter_mut().enumerate() {
            let top = ch(p00, k) * (1.0 - fx) + ch(p10, k) * fx;
            let bot = ch(p01, k) * (1.0 - fx) + ch(p11, k) * fx;
            *o = top * (1.0 - fy) + bot * fy;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;

    fn checker() -> Bitmap {
        // 2x2: white / black on the top row, black / white below.
        Bitmap::from_fn(2, 2, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        })
    }

    #[test]
    fn texel_centers_sample_exactly() {
        let t = checker();
        let s = TextureSampler::new(&t, Wrap::Repeat, Wrap::Repeat).unwrap();
        // Center of texel (0,0) = uv (0.25, 0.25) on a 2x2.
        assert_eq!(s.sample(0.25, 0.25), [1.0, 1.0, 1.0]);
        assert_eq!(s.sample(0.75, 0.25), [0.0, 0.0, 0.0]);
        assert_eq!(s.sample(0.75, 0.75), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn midpoints_blend_bilinearly() {
        let t = checker();
        let s = TextureSampler::new(&t, Wrap::Repeat, Wrap::Repeat).unwrap();
        // Halfway between a white and a black texel center: linear
        // mean of (1.0, 0.0) = 0.5 per channel.
        let m = s.sample(0.5, 0.25);
        assert!((m[0] - 0.5).abs() < 1e-6, "{m:?}");
    }

    #[test]
    fn wrap_modes() {
        let t = checker();
        let rep = TextureSampler::new(&t, Wrap::Repeat, Wrap::Repeat).unwrap();
        // u = 1.25 wraps to 0.25.
        assert_eq!(rep.sample(1.25, 0.25), rep.sample(0.25, 0.25));
        assert_eq!(rep.sample(-0.75, 0.25), rep.sample(0.25, 0.25));

        let cl = TextureSampler::new(&t, Wrap::Clamp, Wrap::Clamp).unwrap();
        // Far outside clamps to the edge texel.
        assert_eq!(cl.sample(9.0, 0.25), cl.sample(0.75, 0.25));
        assert_eq!(cl.sample(-9.0, -9.0), cl.sample(0.25, 0.25));
    }

    #[test]
    fn srgb_gamma_pair_round_trips() {
        // linear -> sqrt output (raster) is the inverse of the square
        // here: u8 -> linear -> sqrt*255 must round-trip within 1.
        for v in [0u8, 1, 7, 128, 200, 255] {
            let lin = srgb8_to_linear(v);
            let back = (lin.sqrt() * 255.0 + 0.5) as u8;
            assert!((back as i32 - v as i32).abs() <= 1, "{v} -> {back}");
        }
    }

    #[test]
    fn empty_texture_refused() {
        let t = Bitmap::default();
        assert!(TextureSampler::new(&t, Wrap::Repeat, Wrap::Repeat).is_none());
    }

    #[test]
    fn nearest_filter_has_hard_edges() {
        let t = checker();
        let s = TextureSampler::new(&t, Wrap::Repeat, Wrap::Repeat)
            .unwrap()
            .with_filter(Filter::Nearest);
        // Just left/right of the texel boundary at u = 0.5: nearest
        // snaps to pure white / pure black — no blend anywhere.
        assert_eq!(s.sample(0.49, 0.25), [1.0, 1.0, 1.0]);
        assert_eq!(s.sample(0.51, 0.25), [0.0, 0.0, 0.0]);
        // Wrap still applies.
        assert_eq!(s.sample(1.49, 0.25), s.sample(0.49, 0.25));
    }
}
