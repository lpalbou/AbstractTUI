//! Rasterizer tests (split file, `#[path]`-included as
//! `raster::tests` — the file-size discipline).
//!
//! OWNER: GFX3D.

use super::*;

const WHITE_LIN: [f32; 3] = [1.0, 1.0, 1.0];

fn vtx(x: f32, y: f32, z: f32, rgb: [f32; 3]) -> RasterVertex {
    RasterVertex::flat(x, y, z, rgb)
}

#[test]
fn fills_a_simple_triangle() {
    let mut fb = Framebuffer::new(16, 16);
    fb.clear(Rgba::TRANSPARENT);
    // Positive-area (visually CW, y-down): right, then down, close.
    fill_triangle(
        &mut fb,
        &[
            vtx(1.0, 1.0, 0.0, WHITE_LIN),
            vtx(14.0, 1.0, 0.0, WHITE_LIN),
            vtx(1.0, 14.0, 0.0, WHITE_LIN),
        ],
        None,
    );
    assert!(fb.coverage() > 0.2, "coverage {}", fb.coverage());
    assert!(fb.depth_at(4, 4).unwrap().is_finite());
    assert!(
        fb.depth_at(15, 15).unwrap().is_infinite(),
        "outside stays empty"
    );
    // Negative-area (back-facing) input is skipped.
    let mut fb2 = Framebuffer::new(16, 16);
    fill_triangle(
        &mut fb2,
        &[
            vtx(1.0, 1.0, 0.0, WHITE_LIN),
            vtx(1.0, 14.0, 0.0, WHITE_LIN),
            vtx(14.0, 1.0, 0.0, WHITE_LIN),
        ],
        None,
    );
    assert_eq!(fb2.coverage(), 0.0);
}

#[test]
fn shared_edge_paints_each_pixel_exactly_once() {
    // The top-left rule's reason to exist: a quad split along its
    // diagonal must cover every interior pixel exactly once —
    // z-equal triangles with LESS z-test would drop double-painted
    // pixels only probabilistically, so count via two passes with
    // different depths: pass 2 at NEARER depth must repaint ALL
    // covered pixels (proving no pixel was left to triangle 1's
    // depth), and total coverage must equal the quad interior.
    let quad = |fb: &mut Framebuffer, z: f32, rgb: [f32; 3]| {
        fill_triangle(
            fb,
            &[
                vtx(2.0, 2.0, z, rgb),
                vtx(12.0, 2.0, z, rgb),
                vtx(2.0, 12.0, z, rgb),
            ],
            None,
        );
        fill_triangle(
            fb,
            &[
                vtx(12.0, 2.0, z, rgb),
                vtx(12.0, 12.0, z, rgb),
                vtx(2.0, 12.0, z, rgb),
            ],
            None,
        );
    };
    let mut fb = Framebuffer::new(16, 16);
    fb.clear(Rgba::TRANSPARENT);
    quad(&mut fb, 0.5, [1.0, 0.0, 0.0]);
    let covered_far: Vec<usize> = (0..256).filter(|&i| fb.depth[i].is_finite()).collect();
    // 10x10 px quad sampled at centers: exactly 100 pixels.
    assert_eq!(covered_far.len(), 100, "quad interior coverage");
    quad(&mut fb, -0.5, [0.0, 1.0, 0.0]);
    for &i in &covered_far {
        // Approximate: barycentric weights sum to `area` exactly in
        // integer math, but inv_area is 1 ulp off in f32.
        assert!(
            (fb.depth[i] + 0.5).abs() < 1e-4,
            "pixel {i} not repainted by nearer quad: {}",
            fb.depth[i]
        );
        assert_eq!(fb.color.pixels()[i].g, 255, "pixel {i} color stale");
    }
    let covered_near = (0..256).filter(|&i| fb.depth[i].is_finite()).count();
    assert_eq!(covered_near, 100, "no seam gaps or double-cover growth");
}

#[test]
fn z_test_orders_triangles() {
    let mut fb = Framebuffer::new(8, 8);
    fb.clear(Rgba::TRANSPARENT);
    // Far red triangle covering everything, then a near green one
    // covering the left half.
    fill_triangle(
        &mut fb,
        &[
            vtx(0.0, 0.0, 0.9, [1.0, 0.0, 0.0]),
            vtx(16.0, 0.0, 0.9, [1.0, 0.0, 0.0]),
            vtx(0.0, 16.0, 0.9, [1.0, 0.0, 0.0]),
        ],
        None,
    );
    fill_triangle(
        &mut fb,
        &[
            vtx(0.0, 0.0, -0.5, [0.0, 1.0, 0.0]),
            vtx(4.0, 0.0, -0.5, [0.0, 1.0, 0.0]),
            vtx(0.0, 16.0, -0.5, [0.0, 1.0, 0.0]),
        ],
        None,
    );
    // And a third triangle BEHIND the red one: must not repaint.
    fill_triangle(
        &mut fb,
        &[
            vtx(0.0, 0.0, 0.99, [0.0, 0.0, 1.0]),
            vtx(16.0, 0.0, 0.99, [0.0, 0.0, 1.0]),
            vtx(0.0, 16.0, 0.99, [0.0, 0.0, 1.0]),
        ],
        None,
    );
    let left = fb.bitmap().get(1, 2).unwrap();
    assert_eq!((left.r, left.g), (0, 255), "near triangle wins: {left:?}");
    let right = fb.bitmap().get(6, 1).unwrap();
    assert_eq!(
        (right.r, right.g, right.b),
        (255, 0, 0),
        "far behind loses: {right:?}"
    );
}

#[test]
fn ndc_depth_outside_unit_range_is_rejected() {
    let mut fb = Framebuffer::new(8, 8);
    fb.clear(Rgba::TRANSPARENT);
    fill_triangle(
        &mut fb,
        &[
            vtx(0.0, 0.0, 1.5, WHITE_LIN),
            vtx(16.0, 0.0, 1.5, WHITE_LIN),
            vtx(0.0, 16.0, 1.5, WHITE_LIN),
        ],
        None,
    );
    assert_eq!(fb.coverage(), 0.0, "beyond-far geometry must not paint");
}

#[test]
fn nan_vertices_are_skipped() {
    let mut fb = Framebuffer::new(8, 8);
    fb.clear(Rgba::TRANSPARENT);
    fill_triangle(
        &mut fb,
        &[
            vtx(f32::NAN, 0.0, 0.0, WHITE_LIN),
            vtx(8.0, 0.0, 0.0, WHITE_LIN),
            vtx(0.0, 8.0, 0.0, WHITE_LIN),
        ],
        None,
    );
    assert_eq!(fb.coverage(), 0.0);
}

#[test]
fn color_interpolates_across_the_face() {
    let mut fb = Framebuffer::new(17, 17);
    fb.clear(Rgba::TRANSPARENT);
    // Black at left corner, red at right: mid-x pixels sit between.
    fill_triangle(
        &mut fb,
        &[
            vtx(0.0, 0.0, 0.0, [0.0, 0.0, 0.0]),
            vtx(17.0, 0.0, 0.0, [1.0, 0.0, 0.0]),
            vtx(0.0, 17.0, 0.0, [0.0, 0.0, 0.0]),
        ],
        None,
    );
    let mid = fb.bitmap().get(8, 1).unwrap();
    assert!(
        mid.r > 130 && mid.r < 210,
        "sqrt(≈0.5)*255 ≈ 180, got {}",
        mid.r
    );
    let near_left = fb.bitmap().get(1, 1).unwrap();
    assert!(near_left.r < 100, "{near_left:?}");
}

#[test]
fn textured_fill_is_perspective_correct() {
    use crate::three::texture::{TextureSampler, Wrap};
    // A 4x1 gradient texture across a triangle whose right edge is
    // 4x farther than the left (inv_w 1.0 vs 0.25): screen-linear
    // UV would put the texture midpoint at screen x≈8; perspective-
    // correct puts it at the harmonic position x≈12.8 of 16. Probe
    // both sides of screen-mid and assert the near half holds most
    // of the texture's left color.
    let tex_bmp =
        crate::gfx::Bitmap::from_fn(4, 1, |x, _| if x < 2 { Rgba::WHITE } else { Rgba::BLACK });
    let tex = TextureSampler::new(&tex_bmp, Wrap::Clamp, Wrap::Clamp).unwrap();
    let mut fb = Framebuffer::new(16, 16);
    fb.clear(Rgba::TRANSPARENT);
    let w = [1.0f32, 0.25, 0.25]; // inv_w per vertex (right side far)
    let mk = |x: f32, y: f32, u: f32, iw: f32| RasterVertex {
        x,
        y,
        ndc_z: 0.0,
        rgb: [1.0, 1.0, 1.0],
        uw: u * iw,
        vw: 0.5 * iw,
        inv_w: iw,
    };
    fill_triangle(
        &mut fb,
        &[
            mk(0.0, 0.0, 0.0, w[0]),
            mk(16.0, 0.0, 1.0, w[1]),
            mk(0.0, 16.0, 0.0, w[2]),
        ],
        Some(&tex),
    );
    // At screen x=8,y=1: screen-linear u would be ≈0.5 (texture
    // black boundary); perspective-correct u = (0.5*0.25)/(0.625)
    // = 0.2 -> white region.
    let mid = fb.bitmap().get(8, 1).unwrap();
    assert!(mid.r > 200, "perspective correction missing: {mid:?}");
    // Near the far corner the texture must reach black.
    let far = fb.bitmap().get(14, 1).unwrap();
    assert!(far.r < 80, "far end should sample the black half: {far:?}");
}

#[test]
fn clip_near_cases() {
    let v = |z: f32| ClipVertex {
        pos: [0.0, 0.0, z],
        rgb: [1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
    };
    let mut out = [v(0.0); 4];

    // Fully in front (z <= -near).
    let n = clip_near(&[v(-2.0), v(-3.0), v(-4.0)], 1.0, &mut out);
    assert_eq!(n, 3);

    // Fully behind.
    let n = clip_near(&[v(-0.1), v(-0.5), v(0.2)], 1.0, &mut out);
    assert_eq!(n, 0);

    // One vertex behind -> quad (4 vertices).
    let tri = [
        ClipVertex {
            pos: [0.0, 0.0, -0.5],
            rgb: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        ClipVertex {
            pos: [1.0, 0.0, -2.0],
            rgb: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        ClipVertex {
            pos: [-1.0, 0.0, -2.0],
            rgb: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
    ];
    let n = clip_near(&tri, 1.0, &mut out);
    assert_eq!(n, 4);
    for cv in &out[..n] {
        assert!(cv.pos[2] <= -1.0 + 1e-6, "{:?}", cv.pos);
    }

    // Two vertices behind -> smaller triangle.
    let tri = [
        ClipVertex {
            pos: [0.0, 0.0, -2.0],
            rgb: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        ClipVertex {
            pos: [1.0, 0.0, -0.2],
            rgb: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        ClipVertex {
            pos: [-1.0, 0.0, -0.2],
            rgb: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
    ];
    let n = clip_near(&tri, 1.0, &mut out);
    assert_eq!(n, 3);
    // Interpolated attribute sanity: crossing points blend colors.
    assert!(out[..n].iter().any(|cv| cv.rgb[0] > 0.0 && cv.rgb[1] > 0.0));
}
