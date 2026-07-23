//! glTF JSON document-parse tests (split file, `#[path]`-included as
//! `doc::tests` — the file-size discipline).
//!
//! OWNER: GFX3D.

use super::*;
use crate::three::glb;

#[test]
fn doc_parses_minimal_mesh() {
    let json = br#"{
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 100}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 72, "byteStride": 12},
            {"buffer": 0, "byteOffset": 72, "byteLength": 6}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 6, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ],
        "meshes": [{"name": "tri", "primitives": [
            {"attributes": {"POSITION": 0}, "indices": 1, "material": 0}
        ]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1, 0.5, 0.25, 1]}}],
        "nodes": [
            {"mesh": 0, "translation": [1, 2, 3], "children": [1]},
            {"matrix": [1,0,0,0, 0,1,0,0, 0,0,1,0, 5,6,7,1]}
        ],
        "scene": 0,
        "scenes": [{"nodes": [0]}]
    }"#;
    let doc = Doc::parse(json).unwrap();
    assert_eq!(doc.buffer_views.len(), 2);
    assert_eq!(doc.buffer_views[0].byte_stride, Some(12));
    assert_eq!(doc.accessors[0].component_type, ComponentType::F32);
    assert_eq!(doc.accessors[0].ty, AccessorType::Vec3);
    assert_eq!(doc.accessors[1].component_type, ComponentType::U16);
    let prim = &doc.meshes[0].primitives[0];
    assert_eq!(prim.position, Some(0));
    assert_eq!(prim.indices, Some(1));
    assert_eq!(prim.mode, 4, "TRIANGLES is the default");
    assert_eq!(doc.materials[0].base_color, [1.0, 0.5, 0.25, 1.0]);
    assert!(!doc.accessors[0].sparse);
    assert_eq!(doc.nodes[0].translation, [1.0, 2.0, 3.0]);
    assert_eq!(
        doc.nodes[0].rotation,
        [0.0, 0.0, 0.0, 1.0],
        "identity default"
    );
    assert_eq!(doc.nodes[0].scale, [1.0; 3]);
    assert_eq!(doc.nodes[0].children, vec![1]);
    assert!(doc.nodes[1].matrix.is_some());
    assert_eq!(doc.scene_roots, vec![0]);
}

#[test]
fn doc_rejects_required_extensions_and_bad_versions() {
    let draco =
        br#"{"asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]}"#;
    let err = Doc::parse(draco).unwrap_err();
    assert!(
        err.to_string().contains("KHR_draco_mesh_compression"),
        "{err}"
    );

    let v1 = br#"{"asset":{"version":"1.0"}}"#;
    assert!(Doc::parse(v1).is_err());
    let none = br#"{}"#;
    assert!(Doc::parse(none).is_err());
}

#[test]
fn doc_rejects_malformed_fields() {
    let bad_count = br#"{"asset":{"version":"2.0"},"accessors":[{"componentType":5126,"count":1.5,"type":"VEC3"}]}"#;
    assert!(Doc::parse(bad_count).is_err(), "fractional count");
    let bad_type = br#"{"asset":{"version":"2.0"},"accessors":[{"componentType":5126,"count":1,"type":"MAT3"}]}"#;
    assert!(Doc::parse(bad_type).is_err(), "unsupported accessor type");
    let bad_ct = br#"{"asset":{"version":"2.0"},"accessors":[{"componentType":9999,"count":1,"type":"VEC3"}]}"#;
    assert!(Doc::parse(bad_ct).is_err(), "unknown componentType");
    let bad_trs = br#"{"asset":{"version":"2.0"},"nodes":[{"translation":[1,2]}]}"#;
    assert!(Doc::parse(bad_trs).is_err(), "translation arity");
}

/// Header + JSON-chunk reads of the real sibling-repo assets. Reads
/// only what exists; skips silently on machines without the repos.
#[test]
fn real_assets_split_and_parse() {
    let cases: [(&str, usize, usize); 3] = [
        // (path, expected meshes, expected accessors)
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb",
            1,
            4,
        ),
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/machine.glb",
            2,
            3,
        ),
        (
            "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb",
            1,
            3,
        ),
    ];
    for (path, meshes, accessors) in cases {
        let p = std::path::Path::new(path);
        if !p.exists() {
            continue;
        }
        let bytes = std::fs::read(p).unwrap();
        let chunks = glb::split(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(chunks.bin.is_some(), "{path}: BIN chunk expected");
        let doc = Doc::parse(chunks.json).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(doc.meshes.len(), meshes, "{path}");
        assert_eq!(doc.accessors.len(), accessors, "{path}");
        assert!(!doc.scene_roots.is_empty(), "{path}: no scene roots");
        // Every referenced accessor index must resolve.
        for mesh in &doc.meshes {
            for prim in &mesh.primitives {
                for idx in [prim.position, prim.normal, prim.texcoord0, prim.indices]
                    .into_iter()
                    .flatten()
                {
                    assert!(
                        idx < doc.accessors.len(),
                        "{path}: accessor {idx} out of range"
                    );
                }
            }
        }
    }
}

/// The compressed helmet variants must fail *by name*, proving the
/// extensionsRequired gate works on real files.
#[test]
fn real_compressed_assets_rejected_loudly() {
    let cases = [
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_draco.glb",
            "draco",
        ),
        (
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_meshopt.glb",
            "meshopt",
        ),
    ];
    for (path, needle) in cases {
        let p = std::path::Path::new(path);
        if !p.exists() {
            continue;
        }
        let bytes = std::fs::read(p).unwrap();
        let chunks = glb::split(&bytes).unwrap();
        let err = Doc::parse(chunks.json).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains(needle),
            "{path}: error should name the extension, got: {msg}"
        );
    }
}
