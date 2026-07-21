//! Skinned-animation proof + hostility suite over a synthetic GLB.
//!
//! No asset in the sibling repos carries animations or skins (verified
//! by scanning every *.glb JSON chunk — all static exports), so the
//! correctness proof is a hand-built 2-bone BENDING BAR: a 6-vertex
//! strip from y=0..2, bottom rung welded to joint 0 (root, origin),
//! top rung to joint 1 (child at y=1), middle rung blended 50/50. One
//! LINEAR rotation channel swings joint 1 by 90° about Z. Every stage
//! runs for real: parse -> validate -> extract (JOINTS_0/WEIGHTS_0/
//! IBM) -> load sanitation -> pose sample -> joint matrices ->
//! vertex blend -> raster.
//!
//! OWNER: GFX3D.

use crate::testing::glb_mutate;
use crate::three::load::{Model, Pose};
use crate::three::math::Vec3;

/// (json, bin) for the bending bar. Byte offsets are load-bearing for
/// the hostile mutations below; keep the layout comment in sync.
///
/// BIN layout: pos@0(72) idx@72(24) joints@96(24) weights@120(96)
/// ibm@216(128) times@344(8) rots@352(32) = 384 bytes.
pub(crate) fn skinned_bar_glb() -> (String, Vec<u8>) {
    let mut bin = Vec::with_capacity(384);
    // positions: rungs at y=0, 1, 2 (x = ±0.2)
    for p in [
        [-0.2f32, 0.0, 0.0],
        [0.2, 0.0, 0.0],
        [-0.2, 1.0, 0.0],
        [0.2, 1.0, 0.0],
        [-0.2, 2.0, 0.0],
        [0.2, 2.0, 0.0],
    ] {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    // indices: 4 CCW triangles
    for i in [0u16, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    // JOINTS_0 (u8 VEC4): bottom -> joint 0, middle -> 0+1, top -> 1
    for j in [
        [0u8, 0, 0, 0],
        [0, 0, 0, 0],
        [0, 1, 0, 0],
        [0, 1, 0, 0],
        [1, 0, 0, 0],
        [1, 0, 0, 0],
    ] {
        bin.extend_from_slice(&j);
    }
    // WEIGHTS_0 (f32 VEC4)
    for w in [
        [1.0f32, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.5, 0.5, 0.0, 0.0],
        [0.5, 0.5, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
    ] {
        for c in w {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    // IBM (MAT4 f32, column-major): joint0 identity; joint1 T(0,-1,0)
    let ident: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let t_down: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0,
    ];
    for m in [ident, t_down] {
        for c in m {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    // times [0, 1]
    for t in [0.0f32, 1.0] {
        bin.extend_from_slice(&t.to_le_bytes());
    }
    // rotations: identity -> 90° about Z
    let s = std::f32::consts::FRAC_1_SQRT_2;
    for q in [[0.0f32, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]] {
        for c in q {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    assert_eq!(bin.len(), 384);

    let json = r#"{
      "asset": {"version": "2.0"},
      "buffers": [{"byteLength": 384}],
      "bufferViews": [
        {"buffer":0,"byteOffset":0,"byteLength":72},
        {"buffer":0,"byteOffset":72,"byteLength":24},
        {"buffer":0,"byteOffset":96,"byteLength":24},
        {"buffer":0,"byteOffset":120,"byteLength":96},
        {"buffer":0,"byteOffset":216,"byteLength":128},
        {"buffer":0,"byteOffset":344,"byteLength":8},
        {"buffer":0,"byteOffset":352,"byteLength":32}
      ],
      "accessors": [
        {"bufferView":0,"componentType":5126,"count":6,"type":"VEC3","min":[-0.2,0,0],"max":[0.2,2,0]},
        {"bufferView":1,"componentType":5123,"count":12,"type":"SCALAR"},
        {"bufferView":2,"componentType":5121,"count":6,"type":"VEC4"},
        {"bufferView":3,"componentType":5126,"count":6,"type":"VEC4"},
        {"bufferView":4,"componentType":5126,"count":2,"type":"MAT4"},
        {"bufferView":5,"componentType":5126,"count":2,"type":"SCALAR"},
        {"bufferView":6,"componentType":5126,"count":2,"type":"VEC4"}
      ],
      "meshes": [{"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":2,"WEIGHTS_0":3},"indices":1}]}],
      "skins": [{"inverseBindMatrices":4,"joints":[0,1]}],
      "nodes": [
        {"children":[1],"name":"root_joint"},
        {"translation":[0,1,0],"name":"tip_joint"},
        {"mesh":0,"skin":0,"name":"bar"}
      ],
      "scenes": [{"nodes":[0,2]}],
      "scene": 0,
      "animations": [{
        "name": "bend",
        "samplers": [{"input":5,"output":6,"interpolation":"LINEAR"}],
        "channels": [{"sampler":0,"target":{"node":1,"path":"rotation"}}]
      }]
    }"#;
    (json.to_string(), bin)
}

fn load_bar() -> Model {
    let (json, bin) = skinned_bar_glb();
    Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap()
}

/// RT6-2 closure: the animated+skinned bar also exists as an ON-DISK
/// asset (`src/three/fixtures/animated_bar.glb`, generated from the
/// same layout this file documents), so the full load → rig → pose
/// path has a live in-repo subject a disk-scanning suite can point at
/// — not only in-memory synthetic bytes.
#[test]
fn animated_bar_fixture_file_loads_and_plays() {
    let bytes = include_bytes!("fixtures/animated_bar.glb");
    let model = Model::load(bytes).unwrap();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    assert_eq!(model.animations().len(), 1);
    assert_eq!(model.animations()[0].name.as_deref(), Some("bend"));
    let mut pose = Pose::default();
    assert!(model.sample_pose_full(0, 1.0, &mut pose));
    // Same hand-checked expectation as the in-memory twin: the tip
    // vertex swings to (-1, 1.2, 0) at t=1.
    let p = pose.skin_joints[0][1].transform_point(Vec3::new(0.2, 2.0, 0.0));
    assert!(
        (p.x + 1.0).abs() < 1e-4 && (p.y - 1.2).abs() < 1e-4,
        "fixture file must play identically: {p:?}"
    );
}

#[test]
fn skinned_bar_loads_with_rig_and_skin() {
    let model = load_bar();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    let rig = model.rig.as_ref().expect("skin implies rig");
    assert_eq!(rig.skins.len(), 1);
    assert_eq!(rig.skins[0].joints, vec![0, 1]);
    assert_eq!(rig.instance_skins, vec![Some(0)]);
    assert_eq!(model.instance_skin(0), Some(0));
    let data = &model.instances[0].data;
    assert_eq!(data.joints.as_ref().unwrap().len(), 6);
    assert_eq!(data.weights.as_ref().unwrap().len(), 6);
    // IBM decoded: joint1's inverse bind is T(0,-1,0) — column-major
    // translation lands in m[12..15].
    let ibm1 = &rig.skins[0].inverse_bind[1];
    assert_eq!((ibm1.m[12], ibm1.m[13], ibm1.m[14]), (0.0, -1.0, 0.0));
}

#[test]
fn skinned_pose_bends_the_bar_exactly() {
    let model = load_bar();
    let mut pose = Pose::default();
    assert!(model.sample_pose_full(0, 1.0, &mut pose));
    assert_eq!(pose.skin_joints.len(), 1);
    let mats = &pose.skin_joints[0];
    assert_eq!(mats.len(), 2);

    // Joint 0 at rest: world = I, IBM = I -> exact identity.
    let p = mats[0].transform_point(Vec3::new(0.2, 0.0, 0.0));
    assert!((p.x - 0.2).abs() < 1e-5 && p.y.abs() < 1e-5, "{p:?}");

    // Joint 1 at t=1: M = T(0,1,0)·R90z·T(0,-1,0). Top-rung vertex
    // (0.2, 2, 0) swings to (-1.0, 1.2, 0).
    let p = mats[1].transform_point(Vec3::new(0.2, 2.0, 0.0));
    assert!(
        (p.x + 1.0).abs() < 1e-4 && (p.y - 1.2).abs() < 1e-4 && p.z.abs() < 1e-5,
        "tip vertex: {p:?}"
    );

    // Middle rung blends 50/50: 0.5·(0.2,1,0) + 0.5·(0,1.2,0) =
    // (0.1, 1.1, 0) — the linear-blend-skinning fold, by hand.
    let rigid = Vec3::new(0.2, 1.0, 0.0);
    let a = mats[0].transform_point(rigid);
    let b = mats[1].transform_point(rigid);
    let blended = (a + b) * 0.5;
    assert!(
        (blended.x - 0.1).abs() < 1e-4 && (blended.y - 1.1).abs() < 1e-4,
        "{blended:?}"
    );

    // t=0 is the bind pose: every joint matrix is identity, so the
    // skinned vertices reproduce the authored positions exactly.
    assert!(model.sample_pose_full(0, 0.0, &mut pose));
    for (i, expect) in [
        (0usize, Vec3::new(-0.2, 0.0, 0.0)),
        (1, Vec3::new(0.2, 0.0, 0.0)),
    ] {
        let p = model.instances[0].data.positions[i];
        let m = &pose.skin_joints[0][0];
        let out = m.transform_point(Vec3::new(p[0], p[1], p[2]));
        assert!(
            (out - expect).length() < 1e-5,
            "bind pose vertex {i}: {out:?}"
        );
    }
}

#[test]
fn skinned_render_moves_pixels() {
    use crate::base::Rgba;
    use crate::three::raster::Framebuffer;
    use crate::three::scene::{Scene, SceneRenderer};

    let model = load_bar();
    let camera = model.fit_camera(0.0, 0.15);
    let mut fb = Framebuffer::new(80, 48);
    let mut renderer = SceneRenderer::new();

    let snapshot =
        |fb: &Framebuffer| -> Vec<u8> { fb.bitmap().pixels().iter().map(|p| p.a).collect() };

    // Rest render (no pose): the authored bind pose.
    let mut scene = Scene::new(&model, camera);
    scene.background = Rgba::TRANSPARENT;
    scene.double_sided = true;
    renderer.render(&scene, &mut fb);
    let rest = snapshot(&fb);
    assert!(rest.iter().any(|&a| a > 0), "bar must cover pixels");

    // Bent render: pose at t=1. Same camera, same everything else.
    let mut pose = Pose::default();
    assert!(model.sample_pose_full(0, 1.0, &mut pose));
    let mut scene = Scene::new(&model, camera);
    scene.background = Rgba::TRANSPARENT;
    scene.double_sided = true;
    scene.pose = Some(&pose);
    renderer.render(&scene, &mut fb);
    let bent = snapshot(&fb);
    assert!(bent.iter().any(|&a| a > 0), "bent bar must cover pixels");
    assert_ne!(rest, bent, "skinned pose must move pixels");

    // Determinism: the same t samples the same pose and paints the
    // same frame.
    let mut pose2 = Pose::default();
    assert!(model.sample_pose_full(0, 1.0, &mut pose2));
    let mut scene = Scene::new(&model, camera);
    scene.background = Rgba::TRANSPARENT;
    scene.double_sided = true;
    scene.pose = Some(&pose2);
    renderer.render(&scene, &mut fb);
    assert_eq!(bent, snapshot(&fb), "sample(t) is pure");
}

// ---- hostility (REDTEAM surface) ----------------------------------------

#[test]
fn hostile_joint_index_out_of_range_rejects_by_name() {
    let (json, mut bin) = skinned_bar_glb();
    // Top rung vertex 4, slot 0 (offset 96 + 4*4) carries weight 1.0:
    // point it at joint 7 of a 2-joint skin.
    bin[96 + 16] = 7;
    let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("joint index 7"), "{msg}");
}

#[test]
fn hostile_garbage_joint_with_zero_weight_is_ignored() {
    let (json, mut bin) = skinned_bar_glb();
    // Vertex 0 slot 3 has weight 0.0 — exporters pad these with
    // garbage; a zero-weight index must not reject.
    bin[96 + 3] = 255;
    let model = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
}

#[test]
fn hostile_zero_sum_weights_reject_by_name() {
    let (json, mut bin) = skinned_bar_glb();
    // Zero out vertex 0's whole weight row (offset 120 + 0).
    for k in 0..16 {
        bin[120 + k] = 0;
    }
    let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("sum to zero"), "{err}");
}

#[test]
fn hostile_nan_weight_rejects_by_name() {
    let (json, mut bin) = skinned_bar_glb();
    bin[120..124].copy_from_slice(&f32::NAN.to_le_bytes());
    let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("non-finite"), "{err}");
}

#[test]
fn drifted_weights_renormalize_with_label() {
    let (json, mut bin) = skinned_bar_glb();
    // Vertex 0: [0.6, 0.6, 0, 0] — sums to 1.2 (way past the 1% gate).
    bin[120..124].copy_from_slice(&0.6f32.to_le_bytes());
    bin[124..128].copy_from_slice(&0.6f32.to_le_bytes());
    let model = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap();
    assert!(
        model
            .warnings
            .iter()
            .any(|w| w.contains("#FALLBACK") && w.contains("renormalized")),
        "{:?}",
        model.warnings
    );
    let w = model.instances[0].data.weights.as_ref().unwrap()[0];
    let sum: f32 = w.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "{w:?}");
}

#[test]
fn hostile_ibm_shortfall_rejects_by_name() {
    let (json, bin) = skinned_bar_glb();
    // Declare only 1 IBM for 2 joints.
    let bad = json.replace(
        r#"{"bufferView":4,"componentType":5126,"count":2,"type":"MAT4"}"#,
        r#"{"bufferView":4,"componentType":5126,"count":1,"type":"MAT4"}"#,
    );
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("inverse bind"), "{err}");
}

#[test]
fn missing_ibm_field_defaults_to_identity_per_spec() {
    let (json, bin) = skinned_bar_glb();
    let no_ibm = json.replace(r#""inverseBindMatrices":4,"#, "");
    let model = Model::load(&glb_mutate::assemble(no_ibm.as_bytes(), Some(&bin))).unwrap();
    let rig = model.rig.as_ref().unwrap();
    // Identity IBMs: joint matrices at rest equal the joint worlds.
    let ibm = &rig.skins[0].inverse_bind;
    assert_eq!(ibm.len(), 2);
    assert_eq!(ibm[0].m, crate::three::math::Mat4::IDENTITY.m);
}

#[test]
fn hostile_joints_without_weights_reject_by_name() {
    let (json, bin) = skinned_bar_glb();
    let bad = json.replace(r#","WEIGHTS_0":3"#, "");
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("JOINTS_0 and WEIGHTS_0"), "{err}");
}

#[test]
fn hostile_skin_and_node_indices_reject_at_parse() {
    let (json, bin) = skinned_bar_glb();
    // Skin joints referencing a nonexistent node.
    let bad = json.replace(r#""joints":[0,1]"#, r#""joints":[0,9]"#);
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("nodes"), "{err}");

    // node.skin out of range.
    let bad = json.replace(r#""mesh":0,"skin":0"#, r#""mesh":0,"skin":5"#);
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("skins"), "{err}");

    // Empty joint list.
    let bad = json.replace(r#""joints":[0,1]"#, r#""joints":[]"#);
    let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
    assert!(err.to_string().contains("no joints"), "{err}");
}

#[test]
fn pose_sampling_is_allocation_shaped_for_reuse() {
    // Not a true allocator hook (no custom allocator in a std-only
    // crate): assert the OBSERVABLE contract instead — capacities
    // stabilize after the first sample, so steady-state playback
    // cannot be reallocating.
    let model = load_bar();
    let mut pose = Pose::default();
    assert!(model.sample_pose_full(0, 0.3, &mut pose));
    let caps = (
        pose.instance_worlds.capacity(),
        pose.skin_joints.capacity(),
        pose.skin_joints[0].capacity(),
    );
    for i in 0..50 {
        assert!(model.sample_pose_full(0, i as f32 * 0.02, &mut pose));
    }
    assert_eq!(
        caps,
        (
            pose.instance_worlds.capacity(),
            pose.skin_joints.capacity(),
            pose.skin_joints[0].capacity(),
        ),
        "steady-state sampling must not grow"
    );
}
