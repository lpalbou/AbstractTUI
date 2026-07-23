//! Software triangle rasterizer: framebuffer (RGBA bitmap + f32
//! z-buffer), near-plane polygon clipping, and an integer edge-function
//! fill with the D3D-style top-left rule.
//!
//! Coordinate conventions (derived and test-pinned, not folklore):
//!
//! - Screen space is y-DOWN, sampled at pixel centers, snapped to
//!   1/16-subpixel integer coordinates (4 bits) so the fill rule is
//!   exact integer math — floats wobble at shared edges.
//! - `orient2d(a,b,c) = (b.x−a.x)(c.y−a.y) − (b.y−a.y)(c.x−a.x)`;
//!   with y down, a POSITIVE area triangle winds visually clockwise.
//!   glTF front faces are CCW in y-UP right-handed space, which lands
//!   as NEGATIVE area here — the scene stage swaps two vertices to
//!   canonicalize front faces to positive area before filling.
//! - Top-left fill rule for positive-area triangles in y-down space:
//!   a TOP edge is horizontal pointing right (dy == 0, dx > 0), a
//!   LEFT edge points up (dy < 0). Pixels on non-top-left edges are
//!   excluded via a −1 bias so a shared edge paints exactly once (the
//!   two-triangle quad test pins this).
//! - Depth is NDC z interpolated LINEARLY in screen space — that is
//!   perspective-correct for depth (z_ndc = z_clip/w_clip is an affine
//!   function of screen x,y; only attributes like color/uv would need
//!   the 1/w treatment). Color uses screen-linear interpolation, a
//!   documented v1 approximation that is invisible at 160x96-px
//!   triangle sizes.
//! - z-test: LESS passes (camera looks −Z; near maps to NDC −1).

use crate::base::Rgba;
use crate::gfx::bitmap::Bitmap;

/// Render target: color bitmap + z-buffer.
pub struct Framebuffer {
    color: Bitmap,
    depth: Vec<f32>,
    w: u32,
    h: u32,
}

impl Framebuffer {
    pub fn new(w: u32, h: u32) -> Framebuffer {
        Framebuffer {
            color: Bitmap::new(w, h, Rgba::TRANSPARENT),
            depth: vec![f32::INFINITY; (w as usize) * (h as usize)],
            w,
            h,
        }
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn height(&self) -> u32 {
        self.h
    }

    /// Reset color + depth for a new frame (no reallocation).
    pub fn clear(&mut self, background: Rgba) {
        self.color.fill(background);
        self.depth.fill(f32::INFINITY);
    }

    pub fn bitmap(&self) -> &Bitmap {
        &self.color
    }

    /// Depth at a pixel (INFINITY = never covered). Test/diagnostic
    /// surface — the render loop uses the raw buffer.
    pub fn depth_at(&self, x: u32, y: u32) -> Option<f32> {
        if x < self.w && y < self.h {
            Some(self.depth[(y * self.w + x) as usize])
        } else {
            None
        }
    }

    /// Whole depth buffer, row-major (post-processing effects — depth
    /// fog — read it in bulk).
    pub fn depths(&self) -> &[f32] {
        &self.depth
    }

    /// Mutable color access for POST-processing passes (fog, trails).
    /// Depth stays consistent — post passes recolor, they never move
    /// geometry.
    pub fn bitmap_mut(&mut self) -> &mut Bitmap {
        &mut self.color
    }

    /// Depth fog: mix covered pixels toward `ground` by NDC depth
    /// (near 0%, far `max_mix`). The standard atmosphere cue for cell-
    /// scale 3D — consumers: the brandmark's storyboard fog, the
    /// viewer widget's `.fog()` option.
    pub fn depth_fog(&mut self, ground: Rgba, max_mix: f32) {
        let max_mix = max_mix.clamp(0.0, 1.0);
        if max_mix == 0.0 {
            return;
        }
        for i in 0..self.depth.len() {
            let d = self.depth[i];
            if !d.is_finite() {
                continue;
            }
            // NDC z spans [-1, 1]; normalize to [0, 1] fog progress.
            let k = ((d + 1.0) * 0.5).clamp(0.0, 1.0) * max_mix;
            if k > 0.0 {
                let p = self.color.pixels()[i];
                self.color.pixels_mut()[i] = p.lerp(ground, k);
            }
        }
    }

    /// Fraction of pixels covered by geometry this frame.
    pub fn coverage(&self) -> f32 {
        if self.depth.is_empty() {
            return 0.0;
        }
        let covered = self.depth.iter().filter(|d| d.is_finite()).count();
        covered as f32 / self.depth.len() as f32
    }
}

/// A vertex ready for rasterization: subpixel screen position, NDC
/// depth, linear-space RGB, and perspective-correct UV carriers.
#[derive(Copy, Clone, Debug)]
pub struct RasterVertex {
    pub x: f32,
    pub y: f32,
    pub ndc_z: f32,
    /// LINEAR-space color 0..=1 (sRGB conversion happens at write).
    pub rgb: [f32; 3],
    /// u/w, v/w, 1/w — the quantities that ARE affine in screen space
    /// (raw u,v are not; that is the whole perspective-correction
    /// story). Untextured callers leave them 0/0/1.
    pub uw: f32,
    pub vw: f32,
    pub inv_w: f32,
}

impl RasterVertex {
    /// Untextured vertex (uv carriers neutral).
    pub fn flat(x: f32, y: f32, ndc_z: f32, rgb: [f32; 3]) -> RasterVertex {
        RasterVertex {
            x,
            y,
            ndc_z,
            rgb,
            uw: 0.0,
            vw: 0.0,
            inv_w: 1.0,
        }
    }
}

/// A view-space vertex entering near-clipping.
#[derive(Copy, Clone, Debug)]
pub struct ClipVertex {
    pub pos: [f32; 3],
    pub rgb: [f32; 3],
    pub uv: [f32; 2],
}

/// Sutherland–Hodgman against the near plane `z <= -near` (camera
/// looks down −Z in view space; geometry in front has negative z).
/// A triangle in, up to 4 vertices out (0 = fully behind). Attributes
/// lerp linearly — exact in view space (it is affine).
pub fn clip_near(tri: &[ClipVertex; 3], near: f32, out: &mut [ClipVertex; 4]) -> usize {
    let inside = |v: &ClipVertex| v.pos[2] <= -near;
    let mut n = 0usize;
    for i in 0..3 {
        let cur = &tri[i];
        let next = &tri[(i + 1) % 3];
        let (ci, ni) = (inside(cur), inside(next));
        if ci {
            out[n] = *cur;
            n += 1;
        }
        if ci != ni {
            // Edge crosses the plane: t where z = -near.
            let dz = next.pos[2] - cur.pos[2];
            // dz cannot be 0 when exactly one endpoint is inside.
            let t = (-near - cur.pos[2]) / dz;
            let lerp3 = |a: [f32; 3], b: [f32; 3]| {
                [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ]
            };
            let lerp2 =
                |a: [f32; 2], b: [f32; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            out[n] = ClipVertex {
                pos: lerp3(cur.pos, next.pos),
                rgb: lerp3(cur.rgb, next.rgb),
                uv: lerp2(cur.uv, next.uv),
            };
            n += 1;
        }
        debug_assert!(n <= 4);
    }
    n
}

/// Clip a screen-space convex polygon to the axis-aligned rect
/// [-band, w+band] x [-band, h+band] (Sutherland–Hodgman, one plane at
/// a time). ALL RasterVertex attributes lerp linearly here — exactly
/// correct for this pipeline's interpolation model: ndc_z, u/w, v/w
/// and 1/w are screen-affine, and color is screen-linear by documented
/// convention. Returns the vertex count in `out` (0 = fully outside).
///
/// This is the RT3-1 exactness half: the scene stage bounds every
/// coordinate near the framebuffer so the rasterizer's defensive snap
/// clamp (COORD_CLAMP) never distorts real geometry.
pub fn clip_screen_rect(
    poly: &[RasterVertex],
    fb_w: f32,
    fb_h: f32,
    band: f32,
    out: &mut [RasterVertex; 12],
) -> usize {
    // Ping-pong buffers: 4 planes, each pass can add at most one vertex
    // (convex input), so 4 (near-clip max) + 4 planes ≤ 8 ≤ 12.
    let mut a = [poly[0]; 12];
    let mut b = [poly[0]; 12];
    let mut n = poly.len().min(12);
    a[..n].copy_from_slice(&poly[..n]);

    // Plane test as (select coordinate, keep-if, boundary value).
    let planes: [(bool, bool, f32); 4] = [
        (true, true, -band),        // x >= -band
        (true, false, fb_w + band), // x <= w+band
        (false, true, -band),       // y >= -band
        (false, false, fb_h + band),
    ];
    let (mut src, mut dst) = (&mut a, &mut b);
    for (is_x, keep_ge, bound) in planes {
        let coord = |v: &RasterVertex| if is_x { v.x } else { v.y };
        let inside = |v: &RasterVertex| {
            if keep_ge {
                coord(v) >= bound
            } else {
                coord(v) <= bound
            }
        };
        let mut m = 0usize;
        for i in 0..n {
            let cur = src[i];
            let nxt = src[(i + 1) % n];
            let (ci, ni) = (inside(&cur), inside(&nxt));
            if ci {
                dst[m] = cur;
                m += 1;
            }
            if ci != ni {
                let denom = coord(&nxt) - coord(&cur);
                // denom != 0 when exactly one side is inside.
                let t = (bound - coord(&cur)) / denom;
                let l = |a: f32, b: f32| a + (b - a) * t;
                dst[m] = RasterVertex {
                    x: l(cur.x, nxt.x),
                    y: l(cur.y, nxt.y),
                    ndc_z: l(cur.ndc_z, nxt.ndc_z),
                    rgb: [
                        l(cur.rgb[0], nxt.rgb[0]),
                        l(cur.rgb[1], nxt.rgb[1]),
                        l(cur.rgb[2], nxt.rgb[2]),
                    ],
                    uw: l(cur.uw, nxt.uw),
                    vw: l(cur.vw, nxt.vw),
                    inv_w: l(cur.inv_w, nxt.inv_w),
                };
                m += 1;
            }
            debug_assert!(m <= 12);
        }
        n = m;
        if n == 0 {
            return 0;
        }
        std::mem::swap(&mut src, &mut dst);
    }
    out[..n].copy_from_slice(&src[..n]);
    n
}

/// Subpixel resolution: 4 fractional bits.
const SUB_BITS: i64 = 4;
const SUB: f32 = (1 << SUB_BITS) as f32;
/// Pixel center offset in subpixel units.
const HALF: i64 = 1 << (SUB_BITS - 1);

/// Coordinate safety bound in SUBPIXELS (RT3-1). orient2d multiplies
/// two coordinate deltas: with |coord| ≤ 2^29, deltas ≤ 2^30, products
/// ≤ 2^60 and their difference ≤ 2^61 — comfortably inside i64. The
/// f32 -> i64 snap CLAMPS to this bound, so `orient2d` can never
/// overflow no matter what a caller feeds in (3e38 included; f32→i64
/// casts saturate in Rust and the clamp tightens that to the safe
/// range). Geometry beyond ±2^25 px is DISTORTED by the clamp — the
/// scene stage never produces such coordinates (its screen-space
/// guard-band clip bounds every vertex near the framebuffer); the
/// clamp is the defense line for direct `fill_triangle` callers.
const COORD_CLAMP: i64 = 1 << 29;

#[inline]
fn orient2d(ax: i64, ay: i64, bx: i64, by: i64, cx: i64, cy: i64) -> i64 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

/// Top-left classification for POSITIVE-area (y-down) triangles; see
/// the module doc derivation.
#[inline]
fn is_top_left(ax: i64, ay: i64, bx: i64, by: i64) -> bool {
    let dy = by - ay;
    let dx = bx - ax;
    dy < 0 || (dy == 0 && dx > 0)
}

/// Fill one triangle, optionally textured. Vertices must already be
/// canonicalized to positive screen area (the scene stage culls or
/// swaps); zero-area and NaN triangles are skipped here as a second
/// line of defense.
///
/// Texturing is perspective-correct: u/w, v/w, 1/w interpolate
/// linearly in screen space (they are affine there) and each pixel
/// divides back. Vertex COLOR stays screen-linear — at cell-scale
/// triangle sizes the color error is invisible, while UV error would
/// swim visibly across large textured faces, which is why UV gets the
/// full treatment and color does not (documented v1 asymmetry).
pub fn fill_triangle(
    fb: &mut Framebuffer,
    v: &[RasterVertex; 3],
    texture: Option<&crate::three::texture::TextureSampler<'_>>,
) {
    for p in v {
        if !(p.x.is_finite() && p.y.is_finite() && p.ndc_z.is_finite()) {
            return;
        }
    }
    // Snap to subpixel integers, clamped to the overflow-safe range
    // (RT3-1; see COORD_CLAMP).
    let snap = |c: f32| ((c * SUB).round() as i64).clamp(-COORD_CLAMP, COORD_CLAMP);
    let xs: [i64; 3] = [snap(v[0].x), snap(v[1].x), snap(v[2].x)];
    let ys: [i64; 3] = [snap(v[0].y), snap(v[1].y), snap(v[2].y)];
    let area = orient2d(xs[0], ys[0], xs[1], ys[1], xs[2], ys[2]);
    if area <= 0 {
        return; // degenerate or wrong-winding (culled upstream)
    }

    // Pixel-space bounding box, clamped.
    let min_x = ((xs.iter().min().unwrap() - HALF) >> SUB_BITS).max(0);
    let max_x = ((xs.iter().max().unwrap() + HALF) >> SUB_BITS).min(fb.w as i64 - 1);
    let min_y = ((ys.iter().min().unwrap() - HALF) >> SUB_BITS).max(0);
    let max_y = ((ys.iter().max().unwrap() + HALF) >> SUB_BITS).min(fb.h as i64 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }

    // Edge functions w_i(p) = orient2d(v_j, v_k, p), cyclic. Steps per
    // +1 pixel in x/y (in subpixel units: 1 px = SUB).
    let edge = |j: usize, k: usize, px: i64, py: i64| orient2d(xs[j], ys[j], xs[k], ys[k], px, py);
    let step_x = [
        (ys[1] - ys[2]) << SUB_BITS,
        (ys[2] - ys[0]) << SUB_BITS,
        (ys[0] - ys[1]) << SUB_BITS,
    ];
    let step_y = [
        (xs[2] - xs[1]) << SUB_BITS,
        (xs[0] - xs[2]) << SUB_BITS,
        (xs[1] - xs[0]) << SUB_BITS,
    ];
    let bias = [
        if is_top_left(xs[1], ys[1], xs[2], ys[2]) {
            0
        } else {
            -1
        },
        if is_top_left(xs[2], ys[2], xs[0], ys[0]) {
            0
        } else {
            -1
        },
        if is_top_left(xs[0], ys[0], xs[1], ys[1]) {
            0
        } else {
            -1
        },
    ];

    // Row-start edge values at the first pixel center.
    let px0 = (min_x << SUB_BITS) + HALF;
    let py0 = (min_y << SUB_BITS) + HALF;
    let mut row_w = [
        edge(1, 2, px0, py0) + bias[0],
        edge(2, 0, px0, py0) + bias[1],
        edge(0, 1, px0, py0) + bias[2],
    ];

    let inv_area = 1.0 / area as f32;
    let z = [v[0].ndc_z, v[1].ndc_z, v[2].ndc_z];
    let c = [v[0].rgb, v[1].rgb, v[2].rgb];

    for y in min_y..=max_y {
        let mut w = row_w;
        let row_base = (y as usize) * (fb.w as usize);
        for x in min_x..=max_x {
            if w[0] >= 0 && w[1] >= 0 && w[2] >= 0 {
                // Barycentrics from the UNBIASED edge values: the bias
                // exists only to gate coverage on shared edges; using
                // it in interpolation would skew weights so that
                // w0+w1+w2 != area and constant attributes drift.
                let l0 = (w[0] - bias[0]) as f32 * inv_area;
                let l1 = (w[1] - bias[1]) as f32 * inv_area;
                let l2 = (w[2] - bias[2]) as f32 * inv_area;
                let depth = l0 * z[0] + l1 * z[1] + l2 * z[2];
                let idx = row_base + x as usize;
                // LESS z-test; NDC beyond the far plane (> 1) is
                // rejected so geometry past `far` cannot smear.
                if depth < fb.depth[idx] && (-1.0..=1.0).contains(&depth) {
                    fb.depth[idx] = depth;
                    let lin = |k: usize| l0 * c[0][k] + l1 * c[1][k] + l2 * c[2][k];
                    let mut rgb = [lin(0), lin(1), lin(2)];
                    if let Some(tex) = texture {
                        // Perspective divide per pixel: uv = (u/w)/(1/w).
                        let iw = l0 * v[0].inv_w + l1 * v[1].inv_w + l2 * v[2].inv_w;
                        if iw > 1e-12 {
                            let u = (l0 * v[0].uw + l1 * v[1].uw + l2 * v[2].uw) / iw;
                            let tv = (l0 * v[0].vw + l1 * v[1].vw + l2 * v[2].vw) / iw;
                            let t = tex.sample(u, tv);
                            rgb = [rgb[0] * t[0], rgb[1] * t[1], rgb[2] * t[2]];
                        }
                    }
                    // Linear -> sRGB-ish (gamma-2 sqrt approximation:
                    // one sqrt per channel beats powf 2.4 at 30 fps,
                    // and the mosaic quantization hides the residue;
                    // texture.rs::srgb8_to_linear is the inverse half).
                    let to_srgb = |l: f32| (l.clamp(0.0, 1.0).sqrt() * 255.0 + 0.5) as u8;
                    fb.color.pixels_mut()[idx] =
                        Rgba::rgb(to_srgb(rgb[0]), to_srgb(rgb[1]), to_srgb(rgb[2]));
                }
            }
            w[0] += step_x[0];
            w[1] += step_x[1];
            w[2] += step_x[2];
        }
        row_w[0] += step_y[0];
        row_w[1] += step_y[1];
        row_w[2] += step_y[2];
    }
}

#[cfg(test)]
#[path = "raster_tests.rs"]
mod tests;
