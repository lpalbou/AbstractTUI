//! End-to-end proof: real GLB -> Model -> rasterizer -> gfx mosaic
//! cells, plus the charter perf budget as an `#[ignore]`d bench
//! (`cargo test --release -- --ignored perf_` per testing doctrine).
//!
//! Real-asset tests are guarded by `Path::exists` and skip silently on
//! machines without the sibling repos; the synthetic pipeline test
//! runs everywhere.

use crate::base::{Point, Rgba};
use crate::gfx::mosaic::{self, MosaicMode};
use crate::three::load::Model;
use crate::three::raster::Framebuffer;
use crate::three::scene::{render, Camera, Scene};

const HELMET: &str = "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb";
const XWING: &str = "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb";

fn load_if_present(path: &str) -> Option<Model> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return None;
    }
    Some(Model::load(&std::fs::read(p).expect("readable")).expect("valid asset"))
}

fn framed_scene(model: &Model) -> Scene<'_> {
    let (min, max) = model.bounds().expect("finite bounds");
    let camera = Camera::framing(min, max, 0.6, 0.35);
    let mut scene = Scene::new(model, camera);
    // Real-world assets (trimesh exports) are not guaranteed
    // consistently wound; render both sides for the proof test.
    scene.double_sided = true;
    scene.background = Rgba::TRANSPARENT;
    scene
}

/// The synthetic end-to-end: minimal GLB (REDTEAM's) through the whole
/// pipe into mosaic cells. Runs on every machine.
#[test]
fn minimal_glb_to_mosaic_cells() {
    let model = Model::load(&crate::testing::glb_mutate::minimal_glb()).unwrap();
    let scene = framed_scene(&model);
    let mut fb = Framebuffer::new(80, 48);
    render(&scene, &mut fb);
    assert!(fb.coverage() > 0.05, "triangle visible: {}", fb.coverage());

    let grid = mosaic::render(fb.bitmap(), 40, 24, MosaicMode::HalfBlock);
    let non_empty = grid
        .cells()
        .iter()
        .filter(|c| !(c.fg.is_transparent() && c.bg.is_transparent()))
        .count();
    assert!(
        non_empty > 20,
        "mosaic carries the image: {non_empty} cells"
    );
    // Iterator shape for RENDER's blit_mosaic bridges positions.
    let patches: Vec<_> = grid.cell_patches(Point::new(2, 3)).collect();
    assert_eq!(patches.len(), 40 * 24);
    assert_eq!(patches[0].0, Point::new(2, 3));
}

/// Real-asset proof: render at 160x96, mosaic to 80x48, deterministic
/// snapshot + coverage + depth invariants.
#[test]
fn e2e_real_asset_render_and_mosaic() {
    // Helmet first: a convex, centered object makes the center-cell
    // probe meaningful; the x-wing (long + thin, ~2.5% pixel coverage
    // from an oblique framing) is the fallback for partial checkouts.
    let Some(model) = load_if_present(HELMET).or_else(|| load_if_present(XWING)) else {
        return;
    };
    let scene = framed_scene(&model);
    let mut fb = Framebuffer::new(160, 96);
    render(&scene, &mut fb);

    // Nonzero, sane coverage. Lower bound is deliberately modest: the
    // framing camera fits the BOUNDING SPHERE, and a long-thin model
    // (the x-wing) fills only a few percent of the frame from an
    // oblique angle; the invariant is "clearly visible", not "big".
    let cov = fb.coverage();
    assert!(cov > 0.015 && cov < 0.95, "coverage {cov}");

    // Depth ordering invariant: the model is 3D — covered depths span
    // a real range, all within NDC, and the nearest covered pixel is
    // strictly nearer than the mean (a flat billboard would fail).
    let mut min_d = f32::INFINITY;
    let mut max_d = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for y in 0..96 {
        for x in 0..160 {
            let d = fb.depth_at(x, y).unwrap();
            if d.is_finite() {
                min_d = min_d.min(d);
                max_d = max_d.max(d);
                sum += d as f64;
                n += 1;
            }
        }
    }
    let mean = (sum / n as f64) as f32;
    assert!((-1.0..=1.0).contains(&min_d) && (-1.0..=1.0).contains(&max_d));
    assert!(max_d - min_d > 1e-4, "no depth relief: {min_d}..{max_d}");
    assert!(
        min_d < mean,
        "nearest pixel {min_d} not in front of mean {mean}"
    );

    // Mosaic to cells and sample deterministically: a re-render + re-
    // mosaic must produce IDENTICAL cells (f32 + integer math, no
    // randomness anywhere in the pipe).
    let grid_a = mosaic::render(fb.bitmap(), 80, 48, MosaicMode::HalfBlock);
    let mut fb2 = Framebuffer::new(160, 96);
    render(&scene, &mut fb2);
    let grid_b = mosaic::render(fb2.bitmap(), 80, 48, MosaicMode::HalfBlock);
    assert_eq!(
        grid_a.cells(),
        grid_b.cells(),
        "pipeline must be deterministic"
    );

    let non_empty = grid_a
        .cells()
        .iter()
        .filter(|c| !(c.fg.is_transparent() && c.bg.is_transparent()))
        .count();
    assert!(
        non_empty > 60,
        "{non_empty} non-empty cells of {}",
        grid_a.cells().len()
    );

    // Sampled probe cells: the framing camera centers the model, so
    // SOME cell in the 5x5 block around the frame center carries it
    // (exact-center misses are legal for concave/thin silhouettes);
    // the corner stays background.
    let center_hit = (38..=42).any(|cx| {
        (22..=26).any(|cy| {
            let c = grid_a.get(cx, cy).unwrap();
            !(c.fg.is_transparent() && c.bg.is_transparent())
        })
    });
    assert!(center_hit, "no model content near frame center");
    let corner = grid_a.get(0, 0).unwrap();
    assert!(
        corner.fg.is_transparent() && corner.bg.is_transparent(),
        "corner should be background: {corner:?}"
    );
}

/// Turntable sanity: two yaw angles produce different frames (the
/// renderer actually consumes the camera, not a cached view).
#[test]
fn e2e_orbit_changes_the_frame() {
    let Some(model) = load_if_present(XWING).or_else(|| load_if_present(HELMET)) else {
        return;
    };
    let (min, max) = model.bounds().unwrap();
    let mut fb_a = Framebuffer::new(80, 48);
    let mut fb_b = Framebuffer::new(80, 48);
    let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
    scene.double_sided = true;
    render(&scene, &mut fb_a);
    scene.camera.yaw = 1.2;
    render(&scene, &mut fb_b);
    assert_ne!(fb_a.bitmap(), fb_b.bitmap(), "yaw must change the image");
}

/// Charter budget: 80x24-cell viewport = 160x96 px half-block, shaded
/// mesh ≥ 30 fps single-thread release (docs/design/00-vision.md).
/// Run: `cargo test --release -- --ignored perf_three_helmet`.
#[test]
#[ignore = "perf budget; run explicitly in release"]
fn perf_three_helmet_160x96() {
    let Some(model) = load_if_present(HELMET).or_else(|| load_if_present(XWING)) else {
        eprintln!("perf_three: no local asset, skipped");
        return;
    };
    let (min, max) = model.bounds().unwrap();
    let mut fb = Framebuffer::new(160, 96);
    let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
    scene.double_sided = true;

    let m = crate::testing::bench::time_median("three_render_160x96", 3, 5, 10, |i| {
        // Vary yaw so no frame can be cached away.
        scene.camera.yaw = i as f32 * 0.13;
        render(&scene, &mut fb);
        crate::testing::bench::sink(fb.coverage());
    });
    eprintln!("{} ({} triangles)", m.report(), model.triangle_count());
    // Charter: ≥ 30 fps -> 33.3 ms; assert with slack (CI machines).
    m.assert_under(std::time::Duration::from_millis(33));
}

/// Helmet textured e2e (cycle 5): its JPEG baseColorTexture now
/// DECODES — the flagship asset renders with real texture sampling.
/// The pin: the textured render differs from a baseColorFactor-only
/// render of the same geometry (texture actually applied), and no
/// jpeg fallback label remains.
#[test]
fn e2e_helmet_renders_textured() {
    let Some(model) = load_if_present(HELMET) else {
        return;
    };
    assert!(
        !model.warnings.iter().any(|w| w.contains("jpeg")),
        "jpeg fallback must be gone: {:?}",
        model.warnings
    );
    let tex = model
        .materials
        .iter()
        .find_map(|m| m.texture.as_ref())
        .expect("helmet baseColorTexture decodes");
    assert!(
        tex.width() >= 256 && tex.height() >= 256,
        "{}x{}",
        tex.width(),
        tex.height()
    );

    let mut stripped = Model {
        instances: model.instances.clone(),
        materials: model.materials.clone(),
        rig: None,
        warnings: Vec::new(),
    };
    for m in &mut stripped.materials {
        m.texture = None;
    }
    let (min, max) = model.bounds().unwrap();
    let cam = Camera::framing(min, max, 0.6, 0.35);
    let mut fb_tex = Framebuffer::new(160, 96);
    let mut fb_flat = Framebuffer::new(160, 96);
    let mut scene = Scene::new(&model, cam);
    scene.double_sided = true;
    render(&scene, &mut fb_tex);
    let mut scene = Scene::new(&stripped, cam);
    scene.double_sided = true;
    render(&scene, &mut fb_flat);
    assert!(fb_tex.coverage() > 0.05);
    assert_eq!(fb_tex.coverage(), fb_flat.coverage(), "geometry identical");
    assert_ne!(
        fb_tex.bitmap(),
        fb_flat.bitmap(),
        "texture must actually sample"
    );
}

/// Textured e2e: the x-wing carries a PNG baseColorTexture + UVs, so
/// the textured path engages automatically. Sanity: texturing changes
/// the pixels vs a texture-stripped clone of the same model.
#[test]
fn e2e_textured_render_differs_from_untextured() {
    let Some(model) = load_if_present(XWING) else {
        return;
    };
    assert!(
        model.materials.iter().any(|m| m.texture.is_some()),
        "x-wing texture should decode"
    );
    let mut stripped_model = Model {
        instances: model.instances.clone(),
        materials: model.materials.clone(),
        rig: None,
        warnings: Vec::new(),
    };
    for m in &mut stripped_model.materials {
        m.texture = None;
    }
    let (min, max) = model.bounds().unwrap();
    let cam = Camera::framing(min, max, 0.6, 0.35);
    let mut fb_tex = Framebuffer::new(160, 96);
    let mut fb_flat = Framebuffer::new(160, 96);
    let mut scene = Scene::new(&model, cam);
    scene.double_sided = true;
    render(&scene, &mut fb_tex);
    let mut scene = Scene::new(&stripped_model, cam);
    scene.double_sided = true;
    render(&scene, &mut fb_flat);
    assert_eq!(fb_tex.coverage(), fb_flat.coverage(), "geometry identical");
    assert_ne!(fb_tex.bitmap(), fb_flat.bitmap(), "texture must show");
}

/// Textured perf. TWO measurements, one pin:
///
/// - The BUDGET PIN runs on the charter-class model (helmet, 15k tris —
///   the 30 fps promise's asset class) with a synthetic 256² texture
///   injected (its real textures are JPEG and stay undecoded), so
///   textured-vs-plain is apples to apples at the promised scale.
/// - The x-wing (120k tris, real PNG texture) is REPORT-ONLY with a
///   generous sanity ceiling: it is 8x past the charter class and
///   vertex-bound — it exceeds 33 ms with texturing OFF too, so
///   pinning it to the charter budget would fail on geometry, not
///   texturing (measured live in cycle 3: ~68 ms textured vs ~80 ms
///   untextured medians on a noisy shared box — texture sampling is
///   NOT the driver at that scale).
///
/// Synthetic two-bone skinned sphere at bench scale: every vertex is a
/// non-trivial 2-joint blend (worst-case skinning arithmetic), driven
/// by one LINEAR rotation channel. No sibling-repo asset animates, so
/// the scale proof is built here (the correctness proof lives in
/// `skin_tests` on a hand-checked bar).
fn skinned_sphere_model(stacks: u32, slices: u32) -> Model {
    use crate::three::animation::{Animation, Interpolation, NodePose, Track, TrackValues};
    use crate::three::load::{MaterialData, MeshInstance, Rig, RigNode, SkinData};
    use crate::three::math::{Mat4, Vec3};

    let mut mesh = crate::three::primitives::uv_sphere(1.0, stacks, slices);
    let n = mesh.positions.len();
    let mut joints = Vec::with_capacity(n);
    let mut weights = Vec::with_capacity(n);
    for p in &mesh.positions {
        // Blend by height: y in [-1, 1] -> w1 in [0, 1].
        let w1 = ((p[1] + 1.0) * 0.5).clamp(0.0, 1.0);
        joints.push([0u16, 1, 0, 0]);
        weights.push([1.0 - w1, w1, 0.0, 0.0]);
    }
    mesh.joints = Some(joints);
    mesh.weights = Some(weights);
    mesh.material = Some(0);

    let rest = |t: Vec3| NodePose {
        translation: t,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: Vec3::new(1.0, 1.0, 1.0),
    };
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let track = Track {
        node: 1,
        times: vec![0.0, 1.0],
        values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]]),
        interpolation: Interpolation::Linear,
    };
    let t_down = {
        let mut m = Mat4::IDENTITY;
        m.m[13] = -1.0;
        m
    };
    Model {
        instances: vec![MeshInstance {
            data: mesh,
            world: Mat4::IDENTITY,
            source_node: Some(2),
        }],
        materials: vec![MaterialData::default()],
        rig: Some(Rig {
            nodes: vec![
                RigNode {
                    rest: rest(Vec3::ZERO),
                    matrix: None,
                    children: vec![1],
                },
                RigNode {
                    rest: rest(Vec3::new(0.0, 1.0, 0.0)),
                    matrix: None,
                    children: vec![],
                },
                RigNode {
                    rest: rest(Vec3::ZERO),
                    matrix: None,
                    children: vec![],
                },
            ],
            roots: vec![0, 2],
            animations: vec![Animation::new(Some("bend".into()), vec![track])],
            skins: vec![SkinData {
                joints: vec![0, 1],
                inverse_bind: vec![Mat4::IDENTITY, t_down],
            }],
            instance_skins: vec![Some(0)],
        }),
        warnings: Vec::new(),
    }
}

/// Animated-model frame cost: pose sampling alone, skinned render, and
/// the rigid render of the same mesh for the skinning delta.
/// `cargo test --release -- --ignored perf_three_animated`
#[test]
#[ignore = "perf budget; run explicitly in release"]
fn perf_three_animated_160x96() {
    use crate::three::load::Pose;

    // 128x256 sphere: ~32k verts / ~65k tris — x-wing-class vertex
    // load with every vertex on the 2-joint blend path.
    let model = skinned_sphere_model(128, 256);
    let tris = model.triangle_count();
    let mut pose = Pose::default();
    let sample = crate::testing::bench::time_median("skinned_pose_sample", 3, 20, 50, |i| {
        let t = (i as f32 * 0.01) % 1.0;
        assert!(model.sample_pose_full(0, t, &mut pose));
        crate::testing::bench::sink(pose.skin_joints[0][1].m[0]);
    });

    let (min, max) = model.bounds().unwrap();
    let mut fb = Framebuffer::new(160, 96);
    let mut renderer = crate::three::scene::SceneRenderer::new();
    let skinned = crate::testing::bench::time_median("skinned_render_160x96", 3, 5, 12, |i| {
        let t = (i as f32 * 0.037) % 1.0;
        assert!(model.sample_pose_full(0, t, &mut pose));
        let mut scene = Scene::new(&model, Camera::framing(min, max, 0.4, 0.3));
        scene.double_sided = true;
        scene.pose = Some(&pose);
        renderer.render(&scene, &mut fb);
        crate::testing::bench::sink(fb.coverage());
    });

    // Rigid baseline: same mesh, no skin binding (rest pose render).
    let mut rigid_model = skinned_sphere_model(128, 256);
    rigid_model.rig = None;
    let rigid = crate::testing::bench::time_median("rigid_render_160x96", 3, 5, 12, |i| {
        let mut scene = Scene::new(&rigid_model, Camera::framing(min, max, 0.4, 0.3));
        scene.camera.yaw = 0.4 + i as f32 * 0.01;
        scene.double_sided = true;
        renderer.render(&scene, &mut fb);
        crate::testing::bench::sink(fb.coverage());
    });

    eprintln!(
        "perf_three_animated: {tris} tris | pose sample {:.3}ms | skinned {:.2}ms | rigid {:.2}ms",
        sample.median.as_secs_f64() * 1e3,
        skinned.median.as_secs_f64() * 1e3,
        rigid.median.as_secs_f64() * 1e3,
    );
    // Generous ceilings: catch order-of-magnitude regressions, not
    // machine noise (charter doctrine).
    sample.assert_under(std::time::Duration::from_millis(10));
    skinned.assert_under(std::time::Duration::from_millis(120));
}

/// PERF ENVELOPE (report-only): the app-author budget table for
/// docs/design/gfx-three.md — every asset class at two standard
/// viewport sizes (80x48 cells half-block = 160x96 px; 160x96 cells =
/// 320x192 px), textured where the asset has textures.
/// `cargo test --release -- --ignored perf_three_envelope -- --nocapture`
#[test]
#[ignore = "report-only envelope; run explicitly in release"]
fn perf_three_envelope() {
    let mut rows: Vec<String> = Vec::new();
    let mut renderer = crate::three::scene::SceneRenderer::new();
    let mut measure = |label: &str, model: &Model, w: u32, h: u32| {
        let (min, max) = model.bounds().unwrap();
        let mut fb = Framebuffer::new(w, h);
        let name = format!("{label}_{w}x{h}");
        let m = crate::testing::bench::time_median(&name, 3, 5, 8, |i| {
            let mut scene = Scene::new(model, Camera::framing(min, max, 0.0, 0.3));
            scene.camera.yaw = i as f32 * 0.13;
            scene.double_sided = true;
            renderer.render(&scene, &mut fb);
            crate::testing::bench::sink(fb.coverage());
        });
        rows.push(format!(
            "| {label} | {} | {w}x{h} | {:.2} ms |",
            model.triangle_count(),
            m.median.as_secs_f64() * 1e3
        ));
    };

    let sphere = {
        let mut m = crate::three::primitives::model_of(
            crate::three::primitives::uv_sphere(1.0, 64, 128),
            [0.8, 0.8, 0.9, 1.0],
        );
        m.instances[0].data.compute_smooth_normals();
        m
    };
    for (w, h) in [(160, 96), (320, 192)] {
        measure("synthetic sphere (untextured, gouraud)", &sphere, w, h);
    }
    if let Some(model) = load_if_present(HELMET) {
        for (w, h) in [(160, 96), (320, 192)] {
            measure("helmet (JPEG textured)", &model, w, h);
        }
        let plain = Model {
            instances: model.instances.clone(),
            materials: model
                .materials
                .iter()
                .map(|m| crate::three::load::MaterialData {
                    base_color: m.base_color,
                    emissive: m.emissive,
                    ..Default::default()
                })
                .collect(),
            rig: None,
            warnings: Vec::new(),
        };
        for (w, h) in [(160, 96), (320, 192)] {
            measure("helmet (untextured)", &plain, w, h);
        }
    }
    if let Some(model) = load_if_present(XWING) {
        for (w, h) in [(160, 96), (320, 192)] {
            measure("x-wing (PNG textured)", &model, w, h);
        }
    }
    let skinned = skinned_sphere_model(128, 256);
    let mut pose = crate::three::load::Pose::default();
    for (w, h) in [(160u32, 96u32), (320, 192)] {
        let (min, max) = skinned.bounds().unwrap();
        let mut fb = Framebuffer::new(w, h);
        let name = format!("skinned_sphere_{w}x{h}");
        let m = crate::testing::bench::time_median(&name, 3, 5, 8, |i| {
            let t = (i as f32 * 0.037) % 1.0;
            assert!(skinned.sample_pose_full(0, t, &mut pose));
            let mut scene = Scene::new(&skinned, Camera::framing(min, max, 0.4, 0.3));
            scene.double_sided = true;
            scene.pose = Some(&pose);
            renderer.render(&scene, &mut fb);
            crate::testing::bench::sink(fb.coverage());
        });
        rows.push(format!(
            "| skinned sphere (animated, 2-joint blend) | {} | {w}x{h} | {:.2} ms |",
            skinned.triangle_count(),
            m.median.as_secs_f64() * 1e3
        ));
    }

    eprintln!("| asset | triangles | framebuffer | median ms/frame |");
    eprintln!("|---|---|---|---|");
    for r in &rows {
        eprintln!("{r}");
    }
}

/// `cargo test --release -- --ignored perf_three_textured`
#[test]
#[ignore = "perf budget; run explicitly in release"]
fn perf_three_textured_160x96() {
    use crate::base::Rgba;
    // --- charter-class pin: helmet plain vs helmet + synthetic texture.
    if let Some(mut model) = load_if_present(HELMET) {
        let (min, max) = model.bounds().unwrap();
        let mut fb = Framebuffer::new(160, 96);
        let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
        scene.double_sided = true;
        let plain = crate::testing::bench::time_median("helmet_plain_160x96", 3, 5, 10, |i| {
            scene.camera.yaw = i as f32 * 0.13;
            render(&scene, &mut fb);
            crate::testing::bench::sink(fb.coverage());
        });
        let checker = crate::gfx::Bitmap::from_fn(256, 256, |x, y| {
            if ((x / 16) + (y / 16)) % 2 == 0 {
                Rgba::rgb(220, 60, 60)
            } else {
                Rgba::rgb(40, 40, 200)
            }
        });
        for m in &mut model.materials {
            m.texture = Some(checker.clone());
        }
        let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
        scene.double_sided = true;
        let tex = crate::testing::bench::time_median("helmet_textured_160x96", 3, 5, 10, |i| {
            scene.camera.yaw = i as f32 * 0.13;
            render(&scene, &mut fb);
            crate::testing::bench::sink(fb.coverage());
        });
        eprintln!("{} ({} triangles)", plain.report(), model.triangle_count());
        eprintln!("{}", tex.report());
        plain.assert_under(std::time::Duration::from_millis(33));
        tex.assert_under(std::time::Duration::from_millis(33));
    } else {
        eprintln!("perf_three_textured: no helmet asset, charter pin skipped");
    }

    // --- x-wing report-only (real PNG texture, off-charter scale).
    if let Some(model) = load_if_present(XWING) {
        let (min, max) = model.bounds().unwrap();
        let mut fb = Framebuffer::new(160, 96);
        let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
        scene.double_sided = true;
        let tex = crate::testing::bench::time_median("xwing_textured_160x96", 2, 3, 5, |i| {
            scene.camera.yaw = i as f32 * 0.13;
            render(&scene, &mut fb);
            crate::testing::bench::sink(fb.coverage());
        });
        eprintln!("{} ({} triangles)", tex.report(), model.triangle_count());
        // PINNED asset-class budget (cycle 7; ceiling widened cycle 8
        // after a contention flake): idle-box median is 7.5–11.7 ms
        // post-vertex-wave, but this box builds six agents' release
        // trees in parallel and medians were observed at ~27 ms (worst
        // run 34.5 ms) under full load. 60 ms catches any real
        // order-of-magnitude regression without flaking on contention;
        // the REGRESSION BAR for humans is the idle median — if that
        // exceeds ~20 ms, investigate regardless of this pin.
        tex.assert_under(std::time::Duration::from_millis(60));
    }
}

/// Where does x-wing time go? Renders the same scene into a 1x1
/// framebuffer (vertex stage + triangle setup dominate; almost no
/// pixels fill) vs the full 160x96 target — the difference attributes
/// raster+shade cost. Uses the reusable `SceneRenderer` (the cycle-4
/// scratch), so numbers reflect the steady-state path.
/// `cargo test --release -- --ignored perf_profile_xwing`
#[test]
#[ignore = "perf profile; run explicitly in release"]
fn perf_profile_xwing() {
    let Some(model) = load_if_present(XWING) else {
        eprintln!("perf_profile_xwing: no asset, skipped");
        return;
    };
    let (min, max) = model.bounds().unwrap();
    let mut scene = Scene::new(&model, Camera::framing(min, max, 0.0, 0.3));
    scene.double_sided = true;
    let mut renderer = crate::three::scene::SceneRenderer::new();

    let mut fb_tiny = Framebuffer::new(1, 1);
    let tiny = crate::testing::bench::time_median("xwing_vertex_setup_1x1", 2, 5, 5, |i| {
        scene.camera.yaw = i as f32 * 0.13;
        renderer.render(&scene, &mut fb_tiny);
        crate::testing::bench::sink(fb_tiny.coverage());
    });
    let mut fb_full = Framebuffer::new(160, 96);
    let full = crate::testing::bench::time_median("xwing_full_160x96", 2, 5, 5, |i| {
        scene.camera.yaw = i as f32 * 0.13;
        renderer.render(&scene, &mut fb_full);
        crate::testing::bench::sink(fb_full.coverage());
    });
    eprintln!("{}", tiny.report());
    eprintln!("{}", full.report());
    eprintln!(
        "attribution: vertex+setup ≈ {:?}, raster+shade ≈ {:?} ({} tris)",
        tiny.median,
        full.median.saturating_sub(tiny.median),
        model.triangle_count()
    );
}

/// One-time texture decode cost (cycle-5 JPEG decoder): times the
/// helmet's baseColorTexture jpeg alone and the whole Model::load.
/// `cargo test --release -- --ignored perf_jpeg_decode`
#[test]
#[ignore = "perf report; run explicitly in release"]
fn perf_jpeg_decode_helmet() {
    let path = std::path::Path::new(HELMET);
    if !path.exists() {
        eprintln!("perf_jpeg_decode: no helmet asset, skipped");
        return;
    }
    let bytes = std::fs::read(path).unwrap();

    // Extract the baseColorTexture's raw jpeg bytes.
    let chunks = crate::three::glb::split(&bytes).unwrap();
    let doc = crate::three::Doc::parse(chunks.json).unwrap();
    let bin = chunks.bin.unwrap();
    let tex = doc.materials[0].base_color_texture.unwrap();
    let img = doc.textures[tex].source.unwrap();
    let view = &doc.buffer_views[doc.images[img].buffer_view.unwrap()];
    let jpeg = &bin[view.byte_offset..view.byte_offset + view.byte_length];

    let t0 = std::time::Instant::now();
    let decoded = crate::gfx::jpeg::decode(jpeg).unwrap();
    let jpeg_time = t0.elapsed();
    let t1 = std::time::Instant::now();
    let model = Model::load(&bytes).unwrap();
    let load_time = t1.elapsed();
    eprintln!(
        "helmet baseColorTexture: {} bytes -> {}x{} in {:?}; full Model::load {:?} ({} tris)",
        jpeg.len(),
        decoded.width(),
        decoded.height(),
        jpeg_time,
        load_time,
        model.triangle_count()
    );
}

/// The mosaic leg of the budget: full-frame 160x96 -> 80x48 half-block
/// conversion must be a small fraction of the frame budget.
#[test]
#[ignore = "perf budget; run explicitly in release"]
fn perf_mosaic_of_render_target() {
    let src = crate::gfx::Bitmap::from_fn(160, 96, |x, y| {
        Rgba::rgb((x * 3) as u8, (y * 5) as u8, ((x ^ y) * 2) as u8)
    });
    let mut r = mosaic::MosaicRenderer::new();
    let m = crate::testing::bench::time_median("mosaic_160x96_halfblock", 3, 5, 50, |_| {
        let g = r.render(&src, 80, 48, MosaicMode::HalfBlock);
        crate::testing::bench::sink(g.cells().len());
    });
    eprintln!("{}", m.report());
    m.assert_under(std::time::Duration::from_millis(3));
}
