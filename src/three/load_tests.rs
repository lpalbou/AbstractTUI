//! Model-loading tests (split file, `#[path]`-included as
//! `load::tests` — the GLB mutator campaign, the synthetic animated
//! fixture, budgets, and the guarded real-asset end-to-end loads).
//!
//! OWNER: GFX3D.

use super::*;
use crate::testing::glb_mutate::{self, Expect};
use crate::three::extract::MeshData;

/// REDTEAM's battery, driven through the single load entry point
/// (RT1-8 tests-first contract). MustLoad => full pipeline Ok;
/// MustReject => Err (the panic hook proves "named error, no
/// panic"); NoPanic => any Result.
#[test]
fn glb_mutator_campaign() {
    let battery = glb_mutate::mutants(0xC0FFEE, 300);
    let mut rejected = 0usize;
    // Cycle-7 hostile pass: anything that LOADS also RENDERS —
    // load tolerance without render tolerance is half a defense
    // (degenerate geometry must draw nothing, never panic).
    let mut fb = crate::three::raster::Framebuffer::new(24, 24);
    let mut renderer = crate::three::scene::SceneRenderer::new();
    let mut render_survivor = |model: &Model, name: &str| {
        let camera = model.fit_camera(0.4, 0.3);
        let mut scene = crate::three::scene::Scene::new(model, camera);
        scene.double_sided = true;
        renderer.render(&scene, &mut fb);
        // No assertion on coverage: hostile geometry may honestly
        // paint nothing. Reaching here without panic is the test.
        crate::testing::bench::sink(fb.coverage());
        let _ = name;
    };
    for m in &battery {
        let result = Model::load(&m.bytes);
        match m.expect {
            Expect::MustLoad => {
                let model = result.unwrap_or_else(|e| panic!("{} must load: {e}", m.name));
                assert!(model.triangle_count() > 0, "{}: no triangles", m.name);
                render_survivor(&model, &m.name);
            }
            Expect::MustReject => {
                let err = match result {
                    Err(e) => e,
                    Ok(_) => panic!("{} must reject", m.name),
                };
                // Named rejection: the message must say something
                // beyond a bare word (all our errors are prefixed).
                let msg = err.to_string();
                assert!(msg.len() > 12, "{}: unnamed rejection {msg:?}", m.name);
                rejected += 1;
            }
            Expect::NoPanic => {
                // Reaching here without panic is the assert; if the
                // soup happened to load, it must render safely too.
                if let Ok(model) = result {
                    render_survivor(&model, &m.name);
                }
            }
        }
    }
    assert!(rejected >= 30, "battery shrank? {rejected} rejects");
}

/// Synthetic animated GLB (no asset in the sibling repos animates —
/// verified by scanning every *.glb JSON chunk): a two-node
/// hierarchy where the ROOT translates (LINEAR, 3 keys) and the
/// mesh-bearing CHILD rotates 90° about Z (STEP, 2 keys). Exercises
/// parse -> validate -> track build -> pose sample -> hierarchy
/// propagation in one fixture.
fn animated_glb() -> (String, Vec<u8>) {
    let mut bin = Vec::new();
    // positions: unit triangle @0 (36 bytes)
    for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    // indices u16 @36 (6 bytes) + 2 pad -> 44
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin.extend_from_slice(&[0, 0]);
    // times @44 (12)
    for t in [0.0f32, 1.0, 2.0] {
        bin.extend_from_slice(&t.to_le_bytes());
    }
    // translations @56 (36)
    for p in [[0.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 4.0, 0.0]] {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    // rotation times @92 (8)
    for t in [0.0f32, 1.0] {
        bin.extend_from_slice(&t.to_le_bytes());
    }
    // rotation quats @100 (32): identity, then 90° about Z
    let s = std::f32::consts::FRAC_1_SQRT_2;
    for q in [[0.0f32, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]] {
        for c in q {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    assert_eq!(bin.len(), 132);
    let json = r#"{
      "asset": {"version": "2.0"},
      "buffers": [{"byteLength": 132}],
      "bufferViews": [
        {"buffer":0,"byteOffset":0,"byteLength":36},
        {"buffer":0,"byteOffset":36,"byteLength":6},
        {"buffer":0,"byteOffset":44,"byteLength":12},
        {"buffer":0,"byteOffset":56,"byteLength":36},
        {"buffer":0,"byteOffset":92,"byteLength":8},
        {"buffer":0,"byteOffset":100,"byteLength":32}
      ],
      "accessors": [
        {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
        {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"},
        {"bufferView":2,"componentType":5126,"count":3,"type":"SCALAR"},
        {"bufferView":3,"componentType":5126,"count":3,"type":"VEC3"},
        {"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR"},
        {"bufferView":5,"componentType":5126,"count":2,"type":"VEC4"}
      ],
      "meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
      "nodes": [
        {"children":[1],"name":"root"},
        {"mesh":0,"translation":[0,1,0],"name":"child"}
      ],
      "scenes": [{"nodes":[0]}],
      "scene": 0,
      "animations": [{
        "name": "move",
        "samplers": [
          {"input":2,"output":3,"interpolation":"LINEAR"},
          {"input":4,"output":5,"interpolation":"STEP"}
        ],
        "channels": [
          {"sampler":0,"target":{"node":0,"path":"translation"}},
          {"sampler":1,"target":{"node":1,"path":"rotation"}}
        ]
      }]
    }"#;
    (json.to_string(), bin)
}

#[test]
fn animated_glb_samples_through_the_hierarchy() {
    let (json, bin) = animated_glb();
    let model = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap();
    assert_eq!(model.animations().len(), 1);
    let anim = &model.animations()[0];
    assert_eq!(anim.name.as_deref(), Some("move"));
    assert_eq!(anim.duration(), 2.0);
    assert_eq!(anim.tracks.len(), 2);

    // Rest pose: instance world = child local translate(0,1,0).
    let inst = &model.instances[0];
    assert_eq!(inst.source_node, Some(1));
    let rest = inst.world.transform_point(Vec3::ZERO);
    assert_eq!((rest.x, rest.y, rest.z), (0.0, 1.0, 0.0));

    let mut pose = Vec::new();
    // t=0.5: root translation lerps to (1,0,0); STEP rotation still
    // identity. Origin -> (1,1,0).
    assert!(model.sample_pose(0, 0.5, &mut pose));
    assert_eq!(pose.len(), model.instances.len());
    let p = pose[0].transform_point(Vec3::ZERO);
    assert!(
        (p.x - 1.0).abs() < 1e-5 && (p.y - 1.0).abs() < 1e-5,
        "{p:?}"
    );

    // t=2.0: root at (2,4,0); child = T(0,1,0)·R90z, so child-local
    // (1,0,0) -> R90z -> (0,1,0) -> +(0,1,0) -> +(2,4,0) = (2,6,0).
    assert!(model.sample_pose(0, 2.0, &mut pose));
    let px = pose[0].transform_point(Vec3::new(1.0, 0.0, 0.0));
    assert!(
        (px.x - 2.0).abs() < 1e-4 && (px.y - 6.0).abs() < 1e-4 && px.z.abs() < 1e-4,
        "rotated+translated: {px:?}"
    );

    // Out-of-range animation index: refused, out untouched.
    assert!(!model.sample_pose(7, 0.0, &mut pose));

    // Static models refuse pose sampling.
    let cube = crate::three::primitives::model_of(crate::three::primitives::cube(1.0), [1.0; 4]);
    assert!(!cube.sample_pose(0, 0.0, &mut Vec::new()));
}

#[test]
fn cubicspline_skips_with_label_and_weights_labeled() {
    // CUBICSPLINE: the CHANNEL drops loudly, the file still loads
    // and the remaining channels play (cycle-6 ruling: label, not
    // whole-file rejection).
    let (json, bin) = animated_glb();
    let cubic = json.replace(
        "\"interpolation\":\"LINEAR\"",
        "\"interpolation\":\"CUBICSPLINE\"",
    );
    let model = Model::load(&glb_mutate::assemble(cubic.as_bytes(), Some(&bin))).unwrap();
    assert_eq!(
        model.animations()[0].tracks.len(),
        1,
        "rotation channel survives"
    );
    assert!(
        model
            .warnings
            .iter()
            .any(|w| w.contains("#FALLBACK") && w.contains("CUBICSPLINE")),
        "{:?}",
        model.warnings
    );

    // weights channels skip with a label (path checked before the
    // output accessor shape, so the VEC4 output is never read).
    let weights = json.replace("\"path\":\"rotation\"", "\"path\":\"weights\"");
    let model = Model::load(&glb_mutate::assemble(weights.as_bytes(), Some(&bin))).unwrap();
    assert_eq!(
        model.animations()[0].tracks.len(),
        1,
        "weights track skipped"
    );
    assert!(
        model
            .warnings
            .iter()
            .any(|w| w.contains("#FALLBACK") && w.contains("weights")),
        "{:?}",
        model.warnings
    );

    // Decreasing keyframe times: named rejection.
    let (json, mut bin) = animated_glb();
    bin[44..48].copy_from_slice(&9.0f32.to_le_bytes()); // times[0] = 9 > times[1]
    let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("decrease"), "{err}");

    // Animation channel pointing at a missing node: parse-level
    // named rejection (validate.rs).
    let (json, bin) = animated_glb();
    let bad = json.replace(
        "{\"node\":0,\"path\":\"translation\"}",
        "{\"node\":9,\"path\":\"translation\"}",
    );
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("node"), "{err}");
}

#[test]
fn triangle_budget_rejects_on_declaration_alone() {
    // Declares 2M+ triangles via the index accessor count while
    // shipping a 4-byte BIN. The DECLARED metadata is internally
    // consistent (accessor fits its view, view fits the declared
    // buffer length) so parse-time validation passes — only the
    // real BIN is a lie, and the budget must fire BEFORE
    // extraction ever compares against it (bounded memory on
    // hostile declarations).
    let index_count = (MAX_TRIANGLES + 1) * 3;
    let index_bytes = index_count * 4;
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "buffers": [{{"byteLength": {total}}}],
          "bufferViews": [
            {{"buffer":0,"byteOffset":0,"byteLength":36}},
            {{"buffer":0,"byteOffset":36,"byteLength":{index_bytes}}}
          ],
          "accessors": [
            {{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
            {{"bufferView":1,"componentType":5125,"count":{index_count},"type":"SCALAR"}}
          ],
          "meshes": [{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
          "nodes": [{{"mesh":0}}],
          "scenes": [{{"nodes":[0]}}],
          "scene": 0
        }}"#,
        total = 36 + index_bytes,
    );
    let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&[0, 0, 0, 0]))).unwrap_err();
    assert!(err.to_string().contains("triangle count exceeds"), "{err}");
}

#[test]
fn load_stats_report_decode_cost() {
    let (model, stats) = Model::load_with_stats(&glb_mutate::minimal_glb()).unwrap();
    assert_eq!(stats.triangles, model.triangle_count());
    assert!(stats.total > std::time::Duration::ZERO);
    assert_eq!(stats.textures_decoded, 0);
}

#[test]
fn smooth_normal_generation_is_area_weighted_and_optional() {
    // Two coplanar triangles sharing an edge: every generated
    // normal must be the plane normal exactly (coplanar faces
    // cannot disagree).
    let mut mesh = MeshData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0, -1.0],
        ],
        normals: None,
        uvs: None,
        colors: None,
        indices: vec![0, 1, 2, 2, 1, 3],
        material: None,
        ..MeshData::default()
    };
    mesh.compute_smooth_normals();
    let normals = mesh.normals.as_ref().unwrap();
    for n in normals {
        assert!(
            (n[1] - 1.0).abs() < 1e-6,
            "flat ground plane normal +Y: {n:?}"
        );
    }
    // Existing normals are never overwritten.
    let sentinel = vec![[0.0, 0.0, 1.0]; 4];
    mesh.normals = Some(sentinel.clone());
    mesh.compute_smooth_normals();
    assert_eq!(mesh.normals.as_ref().unwrap(), &sentinel);

    // Degenerate triangle (repeated index) contributes nothing and
    // does not poison neighbors.
    let mut degen = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: None,
        uvs: None,
        colors: None,
        indices: vec![0, 1, 2, 0, 0, 1],
        material: None,
        ..MeshData::default()
    };
    degen.compute_smooth_normals();
    let n = degen.normals.as_ref().unwrap()[0];
    assert!((n[2] - 1.0).abs() < 1e-6, "{n:?}");
}

#[test]
fn emissive_and_normal_map_metadata_load() {
    // Patch the animated fixture's mesh with a material carrying
    // emissive + normalTexture: emissive lands in MaterialData, the
    // normal map degrades with a label (no tangent pipeline).
    let (mut anim_json, bin) = animated_glb();
    anim_json = anim_json.replace(
        r#""meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"#,
        r#""meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
      "materials": [{"emissiveFactor":[0.5,0.25,0.125],"normalTexture":{"index":0}}],"#,
    );
    let model = Model::load(&glb_mutate::assemble(anim_json.as_bytes(), Some(&bin))).unwrap();
    assert_eq!(model.materials[0].emissive, [0.5, 0.25, 0.125]);
    assert!(
        model
            .warnings
            .iter()
            .any(|w| w.contains("#FALLBACK") && w.contains("normal map")),
        "{:?}",
        model.warnings
    );
}

#[test]
fn load_glb_convenience_and_fit_camera() {
    // Round-trip through a temp file: the 3-line app path.
    let dir = std::env::temp_dir().join("abstracttui_load_glb_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("minimal.glb");
    std::fs::write(&path, glb_mutate::minimal_glb()).unwrap();
    let (model, stats) = load_glb_with_stats(&path).unwrap();
    assert!(model.triangle_count() > 0);
    assert!(stats.total > std::time::Duration::ZERO);
    let model2 = load_glb(&path).unwrap();
    assert_eq!(model2.triangle_count(), model.triangle_count());
    std::fs::remove_file(&path).ok();

    // Missing file: named error naming the path.
    let err = load_glb(dir.join("nope.glb")).unwrap_err();
    assert!(err.to_string().contains("nope.glb"), "{err}");

    // fit_camera frames the bounds; center is the AABB midpoint.
    let c = model.center().unwrap();
    assert_eq!((c.x, c.y), (0.5, 0.5));
    let cam = model.fit_camera(0.3, 0.2);
    assert!(cam.distance.is_finite() && cam.distance > 0.0);
    // Empty model: visible no-op camera, no NaN.
    let empty = Model::default();
    let cam = empty.fit_camera(0.0, 0.0);
    assert!(cam.distance == 1.0 && cam.eye().z.is_finite());
}

#[test]
fn minimal_glb_loads_with_geometry() {
    let model = Model::load(&glb_mutate::minimal_glb()).unwrap();
    assert_eq!(model.triangle_count(), 1);
    assert_eq!(model.instances.len(), 1);
    let (min, max) = model.bounds().unwrap();
    assert_eq!((min.x, min.y, min.z), (0.0, 0.0, 0.0));
    assert_eq!((max.x, max.y, max.z), (1.0, 1.0, 0.0));
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
}

/// Real sibling-repo assets (guarded; skip silently elsewhere).
#[test]
fn real_assets_load_end_to_end() {
    let cases: [(&str, usize); 3] = [
        // (path, expected minimum triangle count)
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb",
            10_000,
        ),
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/machine.glb",
            10,
        ),
        (
            "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb",
            50_000,
        ),
    ];
    for (path, min_tris) in cases {
        let p = std::path::Path::new(path);
        if !p.exists() {
            continue;
        }
        let bytes = std::fs::read(p).unwrap();
        let model = Model::load(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            model.triangle_count() >= min_tris,
            "{path}: {} triangles",
            model.triangle_count()
        );
        // Transforms finite, bounds sane (non-degenerate, not absurd).
        let (min, max) = model
            .bounds()
            .unwrap_or_else(|| panic!("{path}: no finite bounds"));
        for v in [min, max] {
            assert!(
                v.x.is_finite() && v.y.is_finite() && v.z.is_finite(),
                "{path}"
            );
        }
        let extent = max - min;
        assert!(
            extent.x > 0.0 && extent.y > 0.0,
            "{path}: flat bounds {min:?}..{max:?}"
        );
        assert!(extent.length() < 1e6, "{path}: absurd extent {extent:?}");

        // Texture expectations (cycle 5): helmet's baseColorTexture
        // is JPEG and now DECODES (the labeled fallback is gone);
        // x-wing's is PNG.
        if path.contains("helmet") || path.contains("x-wing") {
            assert!(
                !model.warnings.iter().any(|w| w.contains("jpeg")),
                "{path}: jpeg fallback should be gone: {:?}",
                model.warnings
            );
            let tex = model.materials.iter().find_map(|m| m.texture.as_ref());
            let t = tex.unwrap_or_else(|| panic!("{path}: baseColorTexture should decode"));
            assert!(t.width() > 0 && t.height() > 0);
        }
    }
}

#[test]
fn compressed_assets_still_reject_via_load() {
    for path in [
        "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_draco.glb",
        "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_meshopt.glb",
    ] {
        let p = std::path::Path::new(path);
        if !p.exists() {
            continue;
        }
        let bytes = std::fs::read(p).unwrap();
        assert!(Model::load(&bytes).is_err(), "{path} must reject");
    }
}
