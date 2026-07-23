//! Scene/renderer tests (split file, `#[path]`-included as
//! `scene::tests` — synthetic scenes, clipping, lighting, skinning,
//! mip selection, and the overlay/compose contracts).
//!
//! OWNER: GFX3D.

use super::*;
use crate::three::extract::MeshData;
use crate::three::load::{MaterialData, MeshInstance};

/// Hand-built model: helper for synthetic scenes.
fn model_of(tris: Vec<([f32; 9], [f32; 4])>) -> Model {
    // Each entry: 3 positions (xyz xyz xyz) + a base color.
    let mut model = Model::default();
    for (pos, color) in tris {
        let positions = vec![
            [pos[0], pos[1], pos[2]],
            [pos[3], pos[4], pos[5]],
            [pos[6], pos[7], pos[8]],
        ];
        let mat_idx = model.materials.len();
        model.materials.push(MaterialData {
            base_color: color,
            ..MaterialData::default()
        });
        model.instances.push(MeshInstance {
            data: MeshData {
                positions,
                normals: None,
                uvs: None,
                colors: None,
                indices: vec![0, 1, 2],
                material: Some(mat_idx),
                ..MeshData::default()
            },
            world: Mat4::IDENTITY,
            source_node: None,
        });
    }
    model
}

/// CCW-from-camera triangle helper (camera on +Z looking at −Z):
/// counter-clockwise in y-up right-handed space.
fn tri_at(z: f32, half: f32) -> [f32; 9] {
    [-half, -half, z, half, -half, z, 0.0, half, z]
}

#[test]
fn mip_level_picks_by_texel_density() {
    let uv = ([0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
    // 256x256 texels squeezed onto a 16px triangle: tpp = 65536/2
    // over 128... concretely: screen2 = 16*16 = 256 (2x area of an
    // 8x16 right triangle... keep it simple: uv_area2 = 1 * 65536,
    // screen2 = 256 -> tpp = 256 -> level = floor(8/2) = 4.
    let lvl = mip_level(
        (0.0, 0.0),
        (16.0, 0.0),
        (0.0, 16.0),
        uv.0,
        uv.1,
        uv.2,
        65536.0,
        8,
    );
    assert_eq!(lvl, 4);
    // Magnification (few texels over many pixels): level 0.
    let lvl = mip_level(
        (0.0, 0.0),
        (100.0, 0.0),
        (0.0, 100.0),
        uv.0,
        uv.1,
        uv.2,
        64.0,
        8,
    );
    assert_eq!(lvl, 0);
    // Degenerate screen triangle: cheapest (last) level.
    let lvl = mip_level(
        (5.0, 5.0),
        (5.0, 5.0),
        (5.0, 5.0),
        uv.0,
        uv.1,
        uv.2,
        65536.0,
        8,
    );
    assert_eq!(lvl, 8);
    // Clamp to the chain length.
    let lvl = mip_level(
        (0.0, 0.0),
        (2.0, 0.0),
        (0.0, 2.0),
        uv.0,
        uv.1,
        uv.2,
        16_777_216.0,
        3,
    );
    assert_eq!(lvl, 3);
    // NaN UVs: level 0, no panic.
    let lvl = mip_level(
        (0.0, 0.0),
        (16.0, 0.0),
        (0.0, 16.0),
        [f32::NAN, 0.0],
        uv.1,
        uv.2,
        65536.0,
        8,
    );
    assert_eq!(lvl, 0);
}

#[test]
fn mips_average_minified_checkerboards() {
    use crate::gfx::bitmap::Bitmap;
    use crate::three::extract::MeshData;
    use crate::three::load::{MaterialData, MeshInstance, Model};

    // A 1-texel checker (128x128) on a quad rendered FAR AWAY
    // (~12px): without mips, bilinear reads isolated texels —
    // extreme blacks/whites survive; with mips the selected level
    // is a box average — mid-gray. This is the shimmer mechanism:
    // per-frame extremes flip with sub-texel camera motion.
    let checker = Bitmap::from_fn(128, 128, |x, y| {
        if (x + y) % 2 == 0 {
            Rgba::rgb(255, 255, 255)
        } else {
            Rgba::rgb(0, 0, 0)
        }
    });
    let quad = MeshData {
        positions: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
        normals: None,
        uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        colors: None,
        indices: vec![0, 1, 2, 0, 2, 3],
        material: Some(0),
        ..MeshData::default()
    };
    let build = |mips: bool| {
        let mut mat = MaterialData {
            texture: Some(checker.clone()),
            // Ambient-only white light response: isolate sampling.
            base_color: [1.0; 4],
            ..MaterialData::default()
        };
        if mips {
            mat.mips = checker.mip_chain();
        }
        Model {
            instances: vec![MeshInstance {
                data: quad.clone(),
                world: Mat4::IDENTITY,
                source_node: None,
            }],
            materials: vec![mat],
            rig: None,
            warnings: Vec::new(),
        }
    };
    let render_spread = |model: &Model| -> (u8, u8) {
        let mut fb = Framebuffer::new(48, 48);
        let mut scene = Scene::new(model, Camera::orbit(Vec3::ZERO, 12.0, 0.0, 0.0));
        scene.double_sided = true;
        scene.light = Light {
            direction: Vec3::new(0.0, 0.0, -1.0),
            ambient: 1.0,
            diffuse: 0.0,
        };
        render(&scene, &mut fb);
        let mut min = 255u8;
        let mut max = 0u8;
        for p in fb.bitmap().pixels() {
            if p.a > 0 {
                min = min.min(p.r);
                max = max.max(p.r);
            }
        }
        (min, max)
    };
    let (min_raw, max_raw) = render_spread(&build(false));
    let (min_mip, max_mip) = render_spread(&build(true));
    let spread_raw = max_raw - min_raw;
    let spread_mip = max_mip - min_mip;
    assert!(
        spread_mip < spread_raw / 2,
        "mips must collapse the minified checker toward its mean \
         (raw spread {spread_raw}, mip spread {spread_mip})"
    );
}

#[test]
fn camera_is_total_over_hostile_bounds() {
    // Overflow radius: per-axis finite bounds whose span is inf
    // (the exact shape the mutator render pass caught panicking
    // inside Mat4::perspective's near/far assertion).
    let cam = Camera::framing(Vec3::splat(f32::MIN), Vec3::splat(f32::MAX), 0.3, 0.2);
    assert!(cam.near > 0.0 && cam.far > cam.near, "{cam:?}");
    let _ = cam.projection(1.0); // must not assert
                                 // Point bounds (radius 0) and a non-finite orbit distance.
    let cam = Camera::framing(Vec3::splat(2.0), Vec3::splat(2.0), 0.0, 0.0);
    assert!(cam.near > 0.0 && cam.far > cam.near);
    let _ = cam.projection(1.0);
    let cam = Camera::orbit(Vec3::ZERO, f32::INFINITY, 0.0, 0.0);
    assert!(cam.near > 0.0 && cam.far > cam.near && cam.distance.is_finite());
    let _ = cam.projection(1.0);
}

#[test]
fn degenerate_geometry_renders_nothing_and_never_panics() {
    use crate::three::extract::MeshData;
    // NaN vertices, zero-area triangles (collinear + repeated
    // index), an all-NaN triangle, and an empty-normal mesh in one
    // model: the renderer must skip them all quietly.
    let mesh = MeshData {
        positions: vec![
            [f32::NAN, 0.0, 0.0], // NaN vertex
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0], // collinear with 1 and 3
            [3.0, 0.0, 0.0],
            [0.0, 1.0, -0.5],
        ],
        normals: None,
        uvs: None,
        colors: None,
        indices: vec![
            0, 1, 2, // NaN corner
            1, 2, 3, // zero area (collinear)
            4, 4, 4, // repeated index (degenerate)
            0, 0, 0, // repeated NaN
        ],
        material: None,
        ..MeshData::default()
    };
    let model = crate::three::primitives::model_of(mesh, [1.0; 4]);
    let mut fb = Framebuffer::new(32, 32);
    let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 3.0, 0.3, 0.2));
    scene.double_sided = true;
    render(&scene, &mut fb);
    // The collinear and repeated-index triangles have zero area,
    // the NaN ones are skipped: nothing may paint.
    assert_eq!(fb.coverage(), 0.0, "degenerate geometry painted pixels");

    // Same mesh through smooth-normal generation: NaN faces must
    // not poison the accumulation, and rendering stays safe.
    let mut model = model;
    model.ensure_smooth_normals();
    render(
        &Scene::new(&model, Camera::orbit(Vec3::ZERO, 3.0, 0.3, 0.2)),
        &mut fb,
    );
    assert_eq!(fb.coverage(), 0.0);
}

#[test]
fn camera_orbit_and_framing() {
    let cam = Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0);
    let eye = cam.eye();
    assert!((eye.z - 5.0).abs() < 1e-5 && eye.x.abs() < 1e-6);
    // Framing puts the whole box inside the frustum: distance must
    // exceed the bounding radius.
    let cam = Camera::framing(Vec3::splat(-1.0), Vec3::splat(1.0), 0.3, 0.2);
    assert!(cam.distance > (Vec3::splat(1.0) - Vec3::ZERO).length());
    // Extreme pitch stays finite (up-vector guard).
    let cam = Camera::orbit(Vec3::ZERO, 3.0, 0.0, 10.0);
    let v = cam.view();
    assert!(v.m.iter().all(|f| f.is_finite()));
}

#[test]
fn scene_depth_ordering_through_full_pipeline() {
    // Near green triangle at z=1, far red at z=-1 (camera at +5Z
    // looking toward origin): green must win the overlap.
    let model = model_of(vec![
        (tri_at(-1.0, 2.0), [1.0, 0.0, 0.0, 1.0]),
        (tri_at(1.0, 1.0), [0.0, 1.0, 0.0, 1.0]),
    ]);
    let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
    let mut fb = Framebuffer::new(64, 64);
    render(&scene, &mut fb);
    assert!(fb.coverage() > 0.05, "coverage {}", fb.coverage());
    // Center: both triangles overlap; green is nearer.
    let center = fb.bitmap().get(32, 36).unwrap();
    assert!(
        center.g > center.r,
        "near triangle must occlude: {center:?}"
    );
    // Outside the small green tri but inside the big red one.
    let outer = fb.bitmap().get(10, 50).unwrap();
    assert!(
        outer.r > outer.g,
        "far triangle visible at edges: {outer:?}"
    );
}

#[test]
fn backface_cull_and_double_sided() {
    // Same triangle wound to face AWAY from the camera.
    let mut back = tri_at(0.0, 1.0);
    back.swap(0, 3); // swap first two vertices' x
    back.swap(1, 4);
    back.swap(2, 5);
    let model = model_of(vec![(back, [1.0, 1.0, 1.0, 1.0])]);
    let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
    let mut fb = Framebuffer::new(32, 32);
    render(&scene, &mut fb);
    assert_eq!(fb.coverage(), 0.0, "backface must cull");
    scene.double_sided = true;
    render(&scene, &mut fb);
    assert!(fb.coverage() > 0.05, "double_sided renders it");
}

#[test]
fn camera_inside_geometry_clips_instead_of_exploding() {
    // A triangle BEHIND the near plane straddling the camera: near
    // clip must produce stable output (no NaN, no full-screen
    // garbage from a w<=0 projection).
    let model = model_of(vec![(tri_at(4.99, 50.0), [1.0, 1.0, 1.0, 1.0])]);
    let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
    let mut fb = Framebuffer::new(32, 32);
    render(&scene, &mut fb); // camera at z=5, near ~0.05: triangle at z=4.99 is 0.01 in front
                             // The triangle is huge and hugs the near plane: it either
                             // clips away or fills sanely — the assert is "no NaN depths".
    for y in 0..32 {
        for x in 0..32 {
            let d = fb.depth_at(x, y).unwrap();
            assert!(!d.is_nan(), "NaN depth at {x},{y}");
        }
    }
}

#[test]
fn gouraud_uses_vertex_normals() {
    // One triangle with normals tilted toward/away from the light:
    // the lit corner must be brighter than the unlit one.
    let mut model = model_of(vec![(tri_at(0.0, 2.0), [1.0, 1.0, 1.0, 1.0])]);
    model.instances[0].data.normals = Some(vec![
        [0.0, 0.0, 1.0], // toward camera/light
        [1.0, 0.0, 0.0], // sideways
        [0.0, 0.0, 1.0],
    ]);
    let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
    scene.light = Light {
        direction: Vec3::new(0.0, 0.0, -1.0),
        ambient: 0.2,
        diffuse: 0.8,
    };
    let mut fb = Framebuffer::new(48, 48);
    render(&scene, &mut fb);
    // Corner near vertex 0 (bottom-left) vs corner near vertex 1
    // (bottom-right): v0 normal faces the light, v1 is sideways.
    let lit = fb.bitmap().get(12, 40).unwrap();
    let dim = fb.bitmap().get(36, 40).unwrap();
    assert!(
        lit.r as i32 > dim.r as i32 + 30,
        "gouraud gradient missing: lit {lit:?} dim {dim:?}"
    );
}

#[test]
fn render_is_deterministic() {
    let model = model_of(vec![(tri_at(0.0, 1.5), [0.9, 0.5, 0.2, 1.0])]);
    let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 4.0, 0.4, 0.3));
    let mut a = Framebuffer::new(40, 30);
    let mut b = Framebuffer::new(40, 30);
    render(&scene, &mut a);
    render(&scene, &mut b);
    assert_eq!(a.bitmap(), b.bitmap());
}
