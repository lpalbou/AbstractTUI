//! Per-triangle shading + winding helpers of the vertex stage: mip
//! selection, linear-blend skinning, flat lambert, and the winding
//! canonicalization in front of `raster::fill_triangle`. `#[path]`
//! sibling of scene.rs (file-size split); private to the scene world.
//!
//! OWNER: GFX3D.

use crate::three::math::{Mat4, Vec3};
use crate::three::raster::{fill_triangle, Framebuffer, RasterVertex};

use super::Light;

#[inline]
pub(super) fn minmax3(a: f32, b: f32, c: f32) -> (f32, f32) {
    (a.min(b).min(c), a.max(b).max(c))
}

/// A skinned primitive's vertex attributes: (JOINTS_0, WEIGHTS_0).
pub(super) type SkinAttrs<'a> = (&'a [[u16; 4]], &'a [[f32; 4]]);

/// Per-triangle mip level from the texels-per-pixel ratio: UV area
/// (in LEVEL-0 texels) over screen area, both doubled (the /2 cancels
/// in the ratio). Level k halves resolution per step, so texel density
/// shrinks 4x per level: level = floor(log2(tpp) / 2). tpp <= 1 means
/// magnification — always level 0 (bilinear handles it). Degenerate
/// screen triangles take the smallest mip (they cover ~no pixels; the
/// cheapest read is the right read).
#[allow(clippy::too_many_arguments)]
pub(super) fn mip_level(
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    texels: f32,
    max_level: usize,
) -> usize {
    let screen2 = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs();
    if screen2 <= 1e-6 {
        return max_level;
    }
    let uv_area2 = ((uv1[0] - uv0[0]) * (uv2[1] - uv0[1]) - (uv1[1] - uv0[1]) * (uv2[0] - uv0[0]))
        .abs()
        * texels;
    // Negated on purpose: NaN UV area must land here too.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(uv_area2 > 0.0) {
        return 0; // zero/NaN UV area: nothing to minify
    }
    let tpp = uv_area2 / screen2;
    if tpp <= 1.0 {
        return 0;
    }
    ((tpp.log2() * 0.5) as usize).min(max_level)
}

/// Weighted blend of up to 4 joint matrices (linear blend skinning).
/// Zero-weight slots skip entirely — exporters pad unused slots with
/// arbitrary joint indices, so the index is only trusted where the
/// weight is nonzero (load sanitizes exactly that set; the `get` is
/// belt for hand-built models).
pub(super) fn blend4(mats: &[Mat4], joints: &[u16; 4], weights: &[f32; 4]) -> Mat4 {
    let mut out = [0.0f32; 16];
    for k in 0..4 {
        let w = weights[k];
        if w == 0.0 {
            continue;
        }
        let Some(m) = mats.get(joints[k] as usize) else {
            continue;
        };
        for (o, s) in out.iter_mut().zip(m.m.iter()) {
            *o += s * w;
        }
    }
    Mat4::from_cols_array(out)
}

/// Face lambert term from view-space positions (flat-shading path).
#[inline]
pub(super) fn flat_intensity(
    view_pos: &[Vec3],
    i0: usize,
    i1: usize,
    i2: usize,
    light: Light,
    to_light: Vec3,
) -> f32 {
    let (p0, p1, p2) = (view_pos[i0], view_pos[i1], view_pos[i2]);
    let n = (p1 - p0).cross(p2 - p0).normalize();
    light.ambient + light.diffuse * n.dot(to_light).max(0.0)
}

/// Winding canonicalization + fill: glTF front faces (CCW in y-up)
/// land NEGATIVE in y-down screen space — swap to the rasterizer's
/// positive-area convention; positive input is a back face (filled
/// only when double-sided).
#[inline]
pub(super) fn emit_winding(
    fb: &mut Framebuffer,
    a: RasterVertex,
    b: RasterVertex,
    c: RasterVertex,
    double_sided: bool,
    tex: Option<&crate::three::texture::TextureSampler<'_>>,
) {
    let signed = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if signed < 0.0 {
        fill_triangle(fb, &[a, c, b], tex);
    } else if signed > 0.0 && double_sided {
        fill_triangle(fb, &[a, b, c], tex);
    }
    // signed == 0: degenerate, skip.
}
