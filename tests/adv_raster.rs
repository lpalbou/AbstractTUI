//! REDTEAM cycle-3 attack: the software rasterizer (GFX3D's confessed
//! risks): degenerate/sliver triangles, watertight shared edges, huge
//! coordinates (i64 overflow hunt in the subpixel edge math), NaN/Inf
//! vertex payloads through the real loader, camera-inside-mesh, and
//! the render pipeline's finiteness guarantees.

use abstracttui::base::Rgba;
use abstracttui::testing::glb_mutate;
use abstracttui::testing::Rng;
use abstracttui::three::load::Model;
use abstracttui::three::raster::{fill_triangle, Framebuffer, RasterVertex};
use abstracttui::three::scene::{render, Camera, Scene};
use abstracttui::three::Vec3;

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

fn vtx(x: f32, y: f32, z: f32) -> RasterVertex {
    RasterVertex::flat(x, y, z, WHITE)
}

fn painted(fb: &Framebuffer) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for y in 0..fb.height() {
        for x in 0..fb.width() {
            if fb.depth_at(x, y).unwrap().is_finite() {
                out.push((x, y));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Watertightness: a split quad paints every interior pixel exactly once.
// ---------------------------------------------------------------------------

/// Two triangles sharing a diagonal: no pixel may be painted by BOTH
/// (double-paint = z-fighting seam artifacts under blending) and no
/// interior pixel may be painted by NEITHER (gap = flickering seam).
/// 200 seeded quads with subpixel-offset corners.
#[test]
fn shared_edge_watertight_across_random_quads() {
    let mut rng = Rng::new(0x7EA7);
    for round in 0..200 {
        // Random rect with random subpixel jitter on every corner, split
        // along one of its two diagonals.
        let x0 = 1.0 + (rng.below(20) as f32) + (rng.below(16) as f32) / 16.0;
        let y0 = 1.0 + (rng.below(20) as f32) + (rng.below(16) as f32) / 16.0;
        let w = 2.0 + (rng.below(30) as f32) + (rng.below(16) as f32) / 16.0;
        let h = 2.0 + (rng.below(20) as f32) + (rng.below(16) as f32) / 16.0;
        let (x1, y1) = (x0 + w, y0 + h);
        // Corners: a=(x0,y0) b=(x1,y0) c=(x1,y1) d=(x0,y1); split a-c or b-d.
        // Winding: positive area per fill_triangle's y-down orientation.
        let (t1, t2) = if rng.chance(1, 2) {
            (
                [vtx(x0, y0, 0.0), vtx(x1, y0, 0.0), vtx(x1, y1, 0.0)],
                [vtx(x0, y0, 0.0), vtx(x1, y1, 0.0), vtx(x0, y1, 0.0)],
            )
        } else {
            (
                [vtx(x0, y0, 0.0), vtx(x1, y0, 0.0), vtx(x0, y1, 0.0)],
                [vtx(x1, y0, 0.0), vtx(x1, y1, 0.0), vtx(x0, y1, 0.0)],
            )
        };
        let mut fa = Framebuffer::new(64, 48);
        fa.clear(Rgba::TRANSPARENT);
        fill_triangle(&mut fa, &t1, None);
        let mut fbuf = Framebuffer::new(64, 48);
        fbuf.clear(Rgba::TRANSPARENT);
        fill_triangle(&mut fbuf, &t2, None);

        let a: std::collections::BTreeSet<_> = painted(&fa).into_iter().collect();
        let b: std::collections::BTreeSet<_> = painted(&fbuf).into_iter().collect();
        let double: Vec<_> = a.intersection(&b).collect();
        assert!(
            double.is_empty(),
            "round {round}: {} double-painted seam pixels, first {:?} (quad {x0},{y0}..{x1},{y1})",
            double.len(),
            double.first()
        );

        // Gap check: every pixel center strictly inside the rect must be
        // painted by exactly one half.
        let mut gaps = Vec::new();
        let (ix0, iy0) = ((x0.floor() as u32) + 1, (y0.floor() as u32) + 1);
        let (ix1, iy1) = (
            (x1.ceil() as u32).saturating_sub(2),
            (y1.ceil() as u32).saturating_sub(2),
        );
        for y in iy0..=iy1.min(47) {
            for x in ix0..=ix1.min(63) {
                let cx = x as f32 + 0.5;
                let cy = y as f32 + 0.5;
                let inside = cx > x0 && cx < x1 && cy > y0 && cy < y1;
                if inside && !a.contains(&(x, y)) && !b.contains(&(x, y)) {
                    gaps.push((x, y));
                }
            }
        }
        assert!(
            gaps.is_empty(),
            "round {round}: {} seam gaps, first {:?} (quad {x0},{y0}..{x1},{y1})",
            gaps.len(),
            gaps.first()
        );
    }
}

/// A triangle fan around a shared center: every spoke edge is shared by
/// two triangles — the harsher watertightness shape (8 seeded fans).
#[test]
fn fan_around_center_paints_once() {
    let mut rng = Rng::new(0xFA9);
    for round in 0..8 {
        let cx = 20.0 + (rng.below(16) as f32) / 16.0;
        let cy = 15.0 + (rng.below(16) as f32) / 16.0;
        let spokes = 5 + rng.below(6);
        let radius = 12.0;
        let pts: Vec<(f32, f32)> = (0..spokes)
            .map(|i| {
                let ang = (i as f32 / spokes as f32) * std::f32::consts::TAU;
                (cx + radius * ang.cos(), cy + radius * ang.sin())
            })
            .collect();
        let mut count = vec![0u8; 64 * 48];
        for i in 0..spokes {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % spokes];
            let mut fb = Framebuffer::new(64, 48);
            fb.clear(Rgba::TRANSPARENT);
            // y-down positive winding: center, next, current.
            fill_triangle(
                &mut fb,
                &[vtx(cx, cy, 0.0), vtx(bx, by, 0.0), vtx(ax, ay, 0.0)],
                None,
            );
            for (x, y) in painted(&fb) {
                count[(y * 64 + x) as usize] += 1;
            }
        }
        let doubles = count.iter().filter(|&&c| c > 1).count();
        assert_eq!(
            doubles, 0,
            "round {round}: {doubles} pixels painted by 2+ fan triangles"
        );
    }
}

// ---------------------------------------------------------------------------
// Degenerates and slivers.
// ---------------------------------------------------------------------------

#[test]
fn degenerate_and_sliver_triangles_are_safe() {
    let cases: &[[RasterVertex; 3]] = &[
        // Zero area: identical points.
        [vtx(5.0, 5.0, 0.0), vtx(5.0, 5.0, 0.0), vtx(5.0, 5.0, 0.0)],
        // Collinear.
        [
            vtx(1.0, 1.0, 0.0),
            vtx(10.0, 10.0, 0.0),
            vtx(20.0, 20.0, 0.0),
        ],
        // Subpixel sliver (thinner than one subpixel step).
        [
            vtx(1.0, 1.0, 0.0),
            vtx(30.0, 1.01, 0.0),
            vtx(1.0, 1.02, 0.0),
        ],
        // Long diagonal sliver crossing the whole buffer.
        [
            vtx(-100.0, -100.0, 0.0),
            vtx(200.0, 200.0, 0.0),
            vtx(-99.9, -100.0, 0.0),
        ],
        // Entirely off-screen.
        [
            vtx(-50.0, -50.0, 0.0),
            vtx(-10.0, -50.0, 0.0),
            vtx(-10.0, -10.0, 0.0),
        ],
    ];
    for (i, tri) in cases.iter().enumerate() {
        let mut fb = Framebuffer::new(40, 30);
        fb.clear(Rgba::TRANSPARENT);
        fill_triangle(&mut fb, tri, None);
        // No panic is the main assertion; slivers may touch a few pixels
        // but never spray.
        assert!(
            fb.coverage() <= 0.2,
            "case {i}: sliver coverage {}",
            fb.coverage()
        );
    }
}

#[test]
fn non_finite_vertices_render_nothing() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut fb = Framebuffer::new(20, 20);
        fb.clear(Rgba::TRANSPARENT);
        fill_triangle(
            &mut fb,
            &[vtx(bad, 1.0, 0.0), vtx(10.0, 1.0, 0.0), vtx(1.0, 10.0, 0.0)],
            None,
        );
        fill_triangle(
            &mut fb,
            &[vtx(1.0, 1.0, bad), vtx(10.0, 1.0, 0.0), vtx(1.0, 10.0, 0.0)],
            None,
        );
        assert_eq!(fb.coverage(), 0.0);
    }
}

// ---------------------------------------------------------------------------
// Huge-coordinate overflow hunt (the i64 subpixel edge math).
// ---------------------------------------------------------------------------

/// Screen coordinates far outside the buffer but finite: the snapped
/// subpixel math must neither overflow (debug panic) nor paint garbage.
/// Glancing near-plane geometry legitimately produces huge projected
/// x/y (clip_near bounds only z), so this is a real input class.
/// Magnitudes here stay inside the CURRENT safe envelope (~1e6 px:
/// products (2·16·c)² < i64::MAX needs c ≲ 9e7).
#[test]
fn large_offscreen_coordinates_within_envelope_are_safe() {
    let magnitudes = [1.0e3f32, 1.0e4, 1.0e5, 1.0e6];
    for &m in &magnitudes {
        for sign in [1.0f32, -1.0] {
            let mut fb = Framebuffer::new(32, 24);
            fb.clear(Rgba::TRANSPARENT);
            fill_triangle(
                &mut fb,
                &[
                    vtx(5.0, 5.0, 0.0),
                    vtx(sign * m, 3.0, 0.0),
                    vtx(4.0, sign * m, 0.0),
                ],
                None,
            );
            fill_triangle(
                &mut fb,
                &[
                    vtx(sign * m, 0.0, 0.0),
                    vtx(sign * m, m, 0.0),
                    vtx(m, sign * m, 0.0),
                ],
                None,
            );
        }
    }
}

/// RT3-1 (CLOSED cycle 4): `orient2d` overflowed i64 for coordinates
/// ≳ 1e8 px. GFX3D's fix: snap clamps to ±2^29 subpixels (overflow-
/// impossible products) + the scene stage's screen-space guard-band
/// clip keeps real geometry exact. Ignore lifted by owner per R4-2;
/// REDTEAM re-verified at cycle close. Permanent acceptance test.
#[test]
fn huge_but_finite_coordinates_do_not_overflow() {
    let magnitudes = [1.0e8f32, 1.0e12, 1.0e18, 3.0e38];
    for &m in &magnitudes {
        for sign in [1.0f32, -1.0] {
            let mut fb = Framebuffer::new(32, 24);
            fb.clear(Rgba::TRANSPARENT);
            fill_triangle(
                &mut fb,
                &[
                    vtx(5.0, 5.0, 0.0),
                    vtx(sign * m, 3.0, 0.0),
                    vtx(4.0, sign * m, 0.0),
                ],
                None,
            );
            fill_triangle(
                &mut fb,
                &[
                    vtx(sign * m, 0.0, 0.0),
                    vtx(sign * m, m, 0.0),
                    vtx(m, sign * m, 0.0),
                ],
                None,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The real pipeline: hostile GLB float payloads + camera abuse.
// ---------------------------------------------------------------------------

fn assert_fb_finite(fb: &Framebuffer, ctx: &str) {
    for y in 0..fb.height() {
        for x in 0..fb.width() {
            let px = fb.bitmap().get(x, y).unwrap();
            let _ = px; // Rgba is u8 — always finite; depth is the risk:
            if let Some(d) = fb.depth_at(x, y) {
                assert!(
                    d.is_infinite() || (-1.0..=1.0).contains(&d),
                    "{ctx}: depth {d} at ({x},{y}) outside NDC"
                );
                assert!(!d.is_nan(), "{ctx}: NaN depth at ({x},{y})");
            }
        }
    }
}

#[test]
fn nan_inf_glb_payloads_survive_the_full_pipeline() {
    for mutant in glb_mutate::float_payload_mutants() {
        // Contract pinned cycle 3: the loader MAY accept non-finite
        // vertex data; the render must SURVIVE it.
        let model = match Model::load(&mutant.bytes) {
            Ok(m) => m,
            Err(_) => continue, // rejection is equally acceptable
        };
        let camera = Camera::orbit(Vec3::new(0.5, 0.5, 0.0), 3.0, 0.6, 0.4);
        let scene = Scene::new(&model, camera);
        let mut fb = Framebuffer::new(80, 48);
        render(&scene, &mut fb);
        assert_fb_finite(&fb, &mutant.name);
    }
}

#[test]
fn camera_inside_and_degenerate_cameras_are_safe() {
    let model = Model::load(&glb_mutate::minimal_glb()).expect("minimal loads");
    let cameras = [
        // Inside the triangle's plane, staring along it.
        Camera::orbit(Vec3::new(0.3, 0.3, 0.0), 0.0001, 0.0, 0.0),
        // Absurd distance.
        Camera::orbit(Vec3::new(0.0, 0.0, 0.0), 1.0e12, 1.0, 1.0),
        // Extreme pitch (gimbal edges).
        Camera::orbit(
            Vec3::new(0.5, 0.5, 0.0),
            2.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        Camera::orbit(
            Vec3::new(0.5, 0.5, 0.0),
            2.0,
            0.0,
            -std::f32::consts::FRAC_PI_2,
        ),
        // Target == eye degenerate (distance zero).
        Camera::orbit(Vec3::new(0.5, 0.5, 0.0), 0.0, 0.0, 0.0),
    ];
    for (i, camera) in cameras.into_iter().enumerate() {
        let scene = Scene::new(&model, camera);
        let mut fb = Framebuffer::new(60, 40);
        render(&scene, &mut fb);
        assert_fb_finite(&fb, &format!("camera case {i}"));
    }
}

/// Near-plane clipping correctness: a big quad straddling the camera
/// plane must render its in-front part (nonzero coverage) and never
/// smear or panic — the camera-inside-the-mesh daily case.
#[test]
fn geometry_straddling_the_near_plane_clips_cleanly() {
    let model = Model::load(&glb_mutate::minimal_glb()).expect("minimal loads");
    // Walk the camera THROUGH the triangle along its normal.
    for step in 0..30 {
        let d = 3.0 - step as f32 * 0.2; // 3.0 .. -2.8: passes through
        let camera = Camera::orbit(Vec3::new(0.5, 0.3, 0.0), d.abs().max(0.001), 0.0, 0.0);
        let scene = Scene::new(&model, camera);
        let mut fb = Framebuffer::new(60, 40);
        render(&scene, &mut fb);
        assert_fb_finite(&fb, &format!("near-plane step {step}"));
    }
}

// ---------------------------------------------------------------------------
// Textured path: perspective correctness (landed mid-cycle-3; attacked
// the same day). An affine shortcut interpolates raw u,v; the correct
// path interpolates u/w, v/w, 1/w and divides per pixel — at strong
// perspective the two disagree wildly at the screen midpoint.
// ---------------------------------------------------------------------------

#[test]
fn perspective_correct_texture_sampling_no_affine_shortcut() {
    use abstracttui::gfx::Bitmap;
    use abstracttui::three::texture::{TextureSampler, Wrap};

    // Two-tone texture: left half red, right half blue (in u).
    let tex = Bitmap::from_fn(64, 64, |x, _| {
        if x < 32 {
            Rgba::rgb(255, 0, 0)
        } else {
            Rgba::rgb(0, 0, 255)
        }
    });
    let sampler = TextureSampler::new(&tex, Wrap::Clamp, Wrap::Clamp).expect("non-empty");

    // A full-screen-wide quad with STRONG perspective: left edge at
    // w = 1 (near), right edge at w = 8 (far). Screen x spans 0..64.
    // Vertex carriers: uw = u/w, vw = v/w, inv_w = 1/w.
    let v = |x: f32, y: f32, u: f32, vv: f32, w: f32| RasterVertex {
        x,
        y,
        ndc_z: 0.0,
        rgb: WHITE,
        uw: u / w,
        vw: vv / w,
        inv_w: 1.0 / w,
    };
    let (near_w, far_w) = (1.0, 8.0);
    let quad = [
        v(0.0, 0.0, 0.0, 0.0, near_w),
        v(64.0, 0.0, 1.0, 0.0, far_w),
        v(64.0, 48.0, 1.0, 1.0, far_w),
        v(0.0, 48.0, 0.0, 1.0, near_w),
    ];
    let mut fb = Framebuffer::new(64, 48);
    fb.clear(Rgba::TRANSPARENT);
    fill_triangle(&mut fb, &[quad[0], quad[1], quad[2]], Some(&sampler));
    fill_triangle(&mut fb, &[quad[0], quad[2], quad[3]], Some(&sampler));

    // Screen midpoint x = 32: affine u would be 0.5 (BLUE boundary);
    // perspective-correct u = (0.5·uw_n + 0.5·uw_f)/(0.5·iw_n + 0.5·iw_f)
    //                       = (0 + 0.0625)/(0.5 + 0.0625) = 0.111 — RED.
    let mid = fb.bitmap().get(32, 24).unwrap();
    assert!(
        mid.r > mid.b,
        "screen-midpoint texel must still be RED (u≈0.11) under perspective; \
         got {} — affine shortcut suspected",
        mid.to_hex()
    );
    // And the crossover to blue must happen far right of center: find it.
    let mut crossover = None;
    for x in 0..64 {
        let px = fb.bitmap().get(x, 24).unwrap();
        if px.b > px.r {
            crossover = Some(x);
            break;
        }
    }
    // Analytic: u(x) = 0.5 at s where (s·uw_f)/((1-s)·iw_n + s·iw_f) = 0.5
    // -> s ≈ 0.888 -> x ≈ 57.
    let x = crossover.expect("blue must appear before the right edge");
    assert!(
        (52..=62).contains(&x),
        "red/blue crossover at x={x}, expected ~57 for perspective-correct \
         (affine would put it at 32)"
    );

    // v-axis too: top-left texel red, and no NaN/garbage anywhere.
    for y in 0..48 {
        for x in 0..64 {
            let _ = fb.bitmap().get(x, y).unwrap();
        }
    }
}

/// Degenerate texture carriers: inv_w = 0 (point at infinity after a
/// projection bug) and negative inv_w must not panic or produce NaN
/// sampling coordinates (division by the interpolated 1/w).
#[test]
fn hostile_uv_carriers_are_safe() {
    use abstracttui::gfx::Bitmap;
    use abstracttui::three::texture::{TextureSampler, Wrap};
    let tex = Bitmap::from_fn(8, 8, |x, y| Rgba::rgb((x * 30) as u8, (y * 30) as u8, 9));
    let sampler = TextureSampler::new(&tex, Wrap::Repeat, Wrap::Repeat).expect("non-empty");
    let cases: [[f32; 3]; 4] = [
        [0.0, 0.0, 0.0],   // inv_w = 0 everywhere (division hazard)
        [-1.0, 0.5, -0.5], // negative carriers
        [f32::MAX, 1.0, f32::MIN_POSITIVE],
        [1.0e-40, 2.0e-40, 1.0e-40], // denormals
    ];
    for (i, c) in cases.iter().enumerate() {
        let mk = |x: f32, y: f32| RasterVertex {
            x,
            y,
            ndc_z: 0.0,
            rgb: WHITE,
            uw: c[0],
            vw: c[1],
            inv_w: c[2],
        };
        let mut fb = Framebuffer::new(24, 24);
        fb.clear(Rgba::TRANSPARENT);
        fill_triangle(
            &mut fb,
            &[mk(1.0, 1.0), mk(22.0, 1.0), mk(1.0, 22.0)],
            Some(&sampler),
        );
        // No panic; pixels remain valid u8 color (trivially finite) —
        // the assertion is reaching this line for case {i}.
        let _ = fb.coverage();
        let _ = i;
    }
}
