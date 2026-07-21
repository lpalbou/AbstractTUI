//! Typed, metadata-level views over a glTF 2.0 JSON chunk: accessors,
//! bufferViews, meshes/primitives, nodes, materials — everything the
//! rasterizer setup needs except the raw vertex bytes. Reading actual
//! data out of the BIN chunk (strides, component conversion) lands in
//! cycle 2 together with the rasterizer; the types here are shaped for
//! that consumer.
//!
//! Unsupported features fail loudly with named errors — most
//! importantly `extensionsRequired` (Draco / meshopt compressed
//! variants of our own test assets exist precisely to pin this).

use crate::base::{Error, Result};
use crate::three::gltf_json::{self, Value};

/// glTF accessor componentType constants (GL enums).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ComponentType {
    I8,
    U8,
    I16,
    U16,
    U32,
    F32,
}

impl ComponentType {
    pub fn from_gl(v: u32) -> Result<ComponentType> {
        match v {
            5120 => Ok(ComponentType::I8),
            5121 => Ok(ComponentType::U8),
            5122 => Ok(ComponentType::I16),
            5123 => Ok(ComponentType::U16),
            5125 => Ok(ComponentType::U32),
            5126 => Ok(ComponentType::F32),
            _ => Err(Error::Parse(format!("gltf: unknown componentType {v}"))),
        }
    }

    pub fn byte_size(self) -> usize {
        match self {
            ComponentType::I8 | ComponentType::U8 => 1,
            ComponentType::I16 | ComponentType::U16 => 2,
            ComponentType::U32 | ComponentType::F32 => 4,
        }
    }
}

/// glTF accessor type (component count per element).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AccessorType {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
    Mat4,
}

impl AccessorType {
    /// Parse the glTF `type` string (named to avoid colliding with
    /// `std::str::FromStr::from_str` — this is not that trait).
    pub fn from_gltf(s: &str) -> Result<AccessorType> {
        match s {
            "SCALAR" => Ok(AccessorType::Scalar),
            "VEC2" => Ok(AccessorType::Vec2),
            "VEC3" => Ok(AccessorType::Vec3),
            "VEC4" => Ok(AccessorType::Vec4),
            "MAT4" => Ok(AccessorType::Mat4),
            _ => Err(Error::Parse(format!(
                "gltf: unsupported accessor type {s:?}"
            ))),
        }
    }

    pub fn components(self) -> usize {
        match self {
            AccessorType::Scalar => 1,
            AccessorType::Vec2 => 2,
            AccessorType::Vec3 => 3,
            AccessorType::Vec4 => 4,
            AccessorType::Mat4 => 16,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferView {
    pub buffer: usize,
    pub byte_offset: usize,
    pub byte_length: usize,
    /// Distance between consecutive elements for vertex data; `None`
    /// means tightly packed.
    pub byte_stride: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Accessor {
    /// `None` is spec-legal (all-zeros accessor, used by sparse) — the
    /// extractor rejects it until sparse support exists.
    pub buffer_view: Option<usize>,
    pub byte_offset: usize,
    pub component_type: ComponentType,
    pub count: usize,
    pub ty: AccessorType,
    pub normalized: bool,
    /// The accessor carries a `sparse` substitution block. Recorded at
    /// parse, rejected BY NAME at extraction (RT1-8d).
    pub sparse: bool,
}

#[derive(Debug, Clone)]
pub struct Primitive {
    pub position: Option<usize>,
    pub normal: Option<usize>,
    pub texcoord0: Option<usize>,
    pub color0: Option<usize>,
    /// Skinning attributes: JOINTS_0 (VEC4 u8/u16 joint indices into
    /// the skin's joint list) + WEIGHTS_0 (VEC4 blend weights).
    pub joints0: Option<usize>,
    pub weights0: Option<usize>,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    /// glTF `mode`; only 4 (TRIANGLES, the default) renders in v1.
    pub mode: u32,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: Option<String>,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: Option<String>,
    pub mesh: Option<usize>,
    /// Index into `Doc::skins`; the mesh on this node is skinned.
    pub skin: Option<usize>,
    pub children: Vec<usize>,
    /// Column-major matrix if the node uses one...
    pub matrix: Option<[f32; 16]>,
    /// ...otherwise TRS (glTF forbids mixing; we keep both raw and let
    /// the cycle-2 flattener compose via `Mat4::from_trs`).
    pub translation: [f32; 3],
    /// Quaternion x, y, z, w (glTF order).
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct Material {
    pub name: Option<String>,
    /// pbrMetallicRoughness.baseColorFactor, default `[1,1,1,1]`.
    pub base_color: [f32; 4],
    /// pbrMetallicRoughness.baseColorTexture.index into `Doc::textures`.
    pub base_color_texture: Option<usize>,
    /// emissiveFactor (core glTF), default `[0,0,0]` — adds to lit color.
    pub emissive: [f32; 3],
    /// The material declares a normalTexture. Unsupported (no tangent
    /// pipeline) — the loader labels it, never silently ignores it.
    pub has_normal_texture: bool,
}

/// skins[] — joint list + inverse bind matrices for vertex skinning.
#[derive(Debug, Clone)]
pub struct Skin {
    pub name: Option<String>,
    /// Accessor of MAT4 f32 inverse bind matrices (one per joint).
    /// Absent = identity per spec.
    pub inverse_bind_matrices: Option<usize>,
    /// Node indices acting as joints; JOINTS_0 values index THIS list.
    pub joints: Vec<usize>,
    /// Optional skeleton root node (unused by the math; kept for
    /// diagnostics).
    pub skeleton: Option<usize>,
}

/// animations[].samplers[] — keyframe curve description.
#[derive(Debug, Clone)]
pub struct AnimSampler {
    /// Accessor of keyframe TIMES (SCALAR f32, seconds).
    pub input: usize,
    /// Accessor of keyframe VALUES (VEC3/VEC4 f32).
    pub output: usize,
    /// "LINEAR" | "STEP" | "CUBICSPLINE" (validated at extraction).
    pub interpolation: String,
}

/// animations[].channels[] — a curve bound to one node property.
#[derive(Debug, Clone)]
pub struct AnimChannel {
    pub sampler: usize,
    pub target_node: usize,
    /// "translation" | "rotation" | "scale" | "weights".
    pub target_path: String,
}

#[derive(Debug, Clone)]
pub struct AnimationDef {
    pub name: Option<String>,
    pub samplers: Vec<AnimSampler>,
    pub channels: Vec<AnimChannel>,
}

/// texture entry: indirection to an image (sampler ignored in v1 —
/// we sample nearest; wrap modes come with a real material system).
#[derive(Debug, Clone)]
pub struct Texture {
    pub source: Option<usize>,
}

/// images[] entry. GLB-embedded images live in a bufferView; external
/// `uri` images are recorded but not fetched (standalone engine).
#[derive(Debug, Clone)]
pub struct Image {
    pub buffer_view: Option<usize>,
    pub mime_type: Option<String>,
    pub uri: Option<String>,
}

/// Metadata-level glTF document.
#[derive(Debug, Default)]
pub struct Doc {
    /// Declared `buffers[].byteLength` values (buffer 0 = the BIN chunk;
    /// the DECLARED length is validated against the real chunk at load).
    pub buffers: Vec<usize>,
    pub buffer_views: Vec<BufferView>,
    pub accessors: Vec<Accessor>,
    pub meshes: Vec<Mesh>,
    pub nodes: Vec<Node>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub images: Vec<Image>,
    pub animations: Vec<AnimationDef>,
    pub skins: Vec<Skin>,
    /// Root node indices of the default scene (empty if no scenes).
    pub scene_roots: Vec<usize>,
}

impl Doc {
    /// Parse the JSON chunk of a GLB into typed views, then validate
    /// referential integrity + declared data layout (RT2-2/RT2-3: every
    /// check that needs no binary data happens HERE, so no consumer
    /// between parse and extraction ever holds a dangling index).
    pub fn parse(json_chunk: &[u8]) -> Result<Doc> {
        let doc = Self::parse_unvalidated(json_chunk)?;
        crate::three::validate::validate_doc(&doc)?;
        Ok(doc)
    }

    fn parse_unvalidated(json_chunk: &[u8]) -> Result<Doc> {
        let root = gltf_json::parse_bytes(json_chunk)?;

        // asset.version major 2 is mandatory; refuse anything else.
        let version = root
            .get("asset")
            .and_then(|a| a.get("version"))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Parse("gltf: missing asset.version".into()))?;
        if !version.starts_with("2.") {
            return Err(Error::Parse(format!(
                "gltf: unsupported version {version:?}"
            )));
        }

        // Required extensions we do not implement = the asset cannot
        // render faithfully. Name them (Draco/meshopt are real cases).
        if let Some(reqs) = root.get("extensionsRequired") {
            let names: Vec<&str> = reqs.elements().filter_map(Value::as_str).collect();
            if !names.is_empty() {
                return Err(Error::Parse(format!(
                    "gltf: required extensions not supported: {}",
                    names.join(", ")
                )));
            }
        }

        let mut doc = Doc::default();

        for (i, b) in root
            .get("buffers")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            doc.buffers.push(req_usize(b, "byteLength", i, "buffer")?);
        }

        for (i, bv) in root
            .get("bufferViews")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            doc.buffer_views.push(BufferView {
                buffer: req_usize(bv, "buffer", i, "bufferView")?,
                byte_offset: opt_usize(bv, "byteOffset")?.unwrap_or(0),
                byte_length: req_usize(bv, "byteLength", i, "bufferView")?,
                byte_stride: opt_usize(bv, "byteStride")?,
            });
        }

        for (i, a) in root
            .get("accessors")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            let ct = a
                .get("componentType")
                .and_then(Value::as_u32)
                .ok_or_else(|| Error::Parse(format!("gltf: accessor {i} missing componentType")))?;
            let ty = a
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Parse(format!("gltf: accessor {i} missing type")))?;
            doc.accessors.push(Accessor {
                buffer_view: opt_usize(a, "bufferView")?,
                byte_offset: opt_usize(a, "byteOffset")?.unwrap_or(0),
                component_type: ComponentType::from_gl(ct)?,
                count: req_usize(a, "count", i, "accessor")?,
                ty: AccessorType::from_gltf(ty)?,
                normalized: a
                    .get("normalized")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                sparse: a.get("sparse").is_some(),
            });
        }

        for (mi, m) in root
            .get("meshes")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            let mut prims = Vec::new();
            for p in m.get("primitives").into_iter().flat_map(Value::elements) {
                let attrs = p.get("attributes");
                let attr = |name: &str| -> Result<Option<usize>> {
                    match attrs.and_then(|a| a.get(name)) {
                        None => Ok(None),
                        Some(v) => v.as_usize().map(Some).ok_or_else(|| {
                            Error::Parse(format!("gltf: mesh {mi} attribute {name} not an index"))
                        }),
                    }
                };
                prims.push(Primitive {
                    position: attr("POSITION")?,
                    normal: attr("NORMAL")?,
                    texcoord0: attr("TEXCOORD_0")?,
                    color0: attr("COLOR_0")?,
                    joints0: attr("JOINTS_0")?,
                    weights0: attr("WEIGHTS_0")?,
                    indices: opt_usize(p, "indices")?,
                    material: opt_usize(p, "material")?,
                    mode: p.get("mode").and_then(Value::as_u32).unwrap_or(4),
                });
            }
            doc.meshes.push(Mesh {
                name: name_of(m),
                primitives: prims,
            });
        }

        for (ni, n) in root
            .get("nodes")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            let matrix = match n.get("matrix") {
                None => None,
                Some(v) => Some(f32_array::<16>(v, ni, "matrix")?),
            };
            doc.nodes.push(Node {
                name: name_of(n),
                mesh: opt_usize(n, "mesh")?,
                skin: opt_usize(n, "skin")?,
                children: n
                    .get("children")
                    .into_iter()
                    .flat_map(Value::elements)
                    .filter_map(Value::as_usize)
                    .collect(),
                matrix,
                translation: match n.get("translation") {
                    None => [0.0; 3],
                    Some(v) => f32_array::<3>(v, ni, "translation")?,
                },
                rotation: match n.get("rotation") {
                    None => [0.0, 0.0, 0.0, 1.0],
                    Some(v) => f32_array::<4>(v, ni, "rotation")?,
                },
                scale: match n.get("scale") {
                    None => [1.0; 3],
                    Some(v) => f32_array::<3>(v, ni, "scale")?,
                },
            });
        }

        for m in root.get("materials").into_iter().flat_map(Value::elements) {
            let pbr = m.get("pbrMetallicRoughness");
            let base_color = match pbr.and_then(|p| p.get("baseColorFactor")) {
                None => [1.0; 4],
                Some(v) => f32_array::<4>(v, doc.materials.len(), "baseColorFactor")?,
            };
            let base_color_texture = pbr
                .and_then(|p| p.get("baseColorTexture"))
                .and_then(|t| t.get("index"))
                .and_then(Value::as_usize);
            let emissive = match m.get("emissiveFactor") {
                None => [0.0; 3],
                Some(v) => f32_array::<3>(v, doc.materials.len(), "emissiveFactor")?,
            };
            doc.materials.push(Material {
                name: name_of(m),
                base_color,
                base_color_texture,
                emissive,
                has_normal_texture: m.get("normalTexture").is_some(),
            });
        }

        for t in root.get("textures").into_iter().flat_map(Value::elements) {
            doc.textures.push(Texture {
                source: opt_usize(t, "source")?,
            });
        }

        for im in root.get("images").into_iter().flat_map(Value::elements) {
            doc.images.push(Image {
                buffer_view: opt_usize(im, "bufferView")?,
                mime_type: im
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                uri: im.get("uri").and_then(Value::as_str).map(str::to_owned),
            });
        }

        for (si, s) in root
            .get("skins")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            let joints: Vec<usize> = s
                .get("joints")
                .into_iter()
                .flat_map(Value::elements)
                .filter_map(Value::as_usize)
                .collect();
            if joints.is_empty() {
                return Err(Error::Parse(format!("gltf: skin {si} has no joints")));
            }
            doc.skins.push(Skin {
                name: name_of(s),
                inverse_bind_matrices: opt_usize(s, "inverseBindMatrices")?,
                joints,
                skeleton: opt_usize(s, "skeleton")?,
            });
        }

        for (ai, a) in root
            .get("animations")
            .into_iter()
            .flat_map(Value::elements)
            .enumerate()
        {
            let mut samplers = Vec::new();
            for (si, s) in a
                .get("samplers")
                .into_iter()
                .flat_map(Value::elements)
                .enumerate()
            {
                samplers.push(AnimSampler {
                    input: req_usize(s, "input", si, "animation sampler")?,
                    output: req_usize(s, "output", si, "animation sampler")?,
                    interpolation: s
                        .get("interpolation")
                        .and_then(Value::as_str)
                        .unwrap_or("LINEAR")
                        .to_owned(),
                });
            }
            let mut channels = Vec::new();
            for c in a.get("channels").into_iter().flat_map(Value::elements) {
                let target = c.get("target").ok_or_else(|| {
                    Error::Parse(format!("gltf: animation {ai} channel without target"))
                })?;
                // Channels targeting nothing local (spec-legal for
                // extensions) are skipped; node-targeting ones bind.
                let Some(node) = target.get("node").and_then(Value::as_usize) else {
                    continue;
                };
                channels.push(AnimChannel {
                    sampler: req_usize(c, "sampler", ai, "animation channel")?,
                    target_node: node,
                    target_path: target
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            Error::Parse(format!("gltf: animation {ai} channel without path"))
                        })?
                        .to_owned(),
                });
            }
            doc.animations.push(AnimationDef {
                name: name_of(a),
                samplers,
                channels,
            });
        }

        // Default scene: `scene` index into `scenes`, else scene 0 if
        // present. A GLB with no scenes still parses (meshes are usable
        // directly); roots stay empty.
        let scenes = root.get("scenes");
        let scene_idx = root.get("scene").and_then(Value::as_usize).unwrap_or(0);
        if let Some(scene) = scenes.and_then(|s| s.idx(scene_idx)) {
            doc.scene_roots = scene
                .get("nodes")
                .into_iter()
                .flat_map(Value::elements)
                .filter_map(Value::as_usize)
                .collect();
        }

        Ok(doc)
    }
}

fn name_of(v: &Value) -> Option<String> {
    v.get("name").and_then(Value::as_str).map(str::to_owned)
}

fn req_usize(v: &Value, key: &str, idx: usize, what: &str) -> Result<usize> {
    v.get(key)
        .and_then(Value::as_usize)
        .ok_or_else(|| Error::Parse(format!("gltf: {what} {idx} missing {key}")))
}

fn opt_usize(v: &Value, key: &str) -> Result<Option<usize>> {
    match v.get(key) {
        None => Ok(None),
        Some(x) => x
            .as_usize()
            .map(Some)
            .ok_or_else(|| Error::Parse(format!("gltf: {key} is not a non-negative integer"))),
    }
}

fn f32_array<const N: usize>(v: &Value, idx: usize, what: &str) -> Result<[f32; N]> {
    let arr = v
        .as_array()
        .ok_or_else(|| Error::Parse(format!("gltf: node {idx} {what} is not an array")))?;
    if arr.len() != N {
        return Err(Error::Parse(format!(
            "gltf: node {idx} {what} has {} elements (want {N})",
            arr.len()
        )));
    }
    let mut out = [0.0f32; N];
    for (i, e) in arr.iter().enumerate() {
        out[i] = e
            .as_f64()
            .ok_or_else(|| Error::Parse(format!("gltf: node {idx} {what}[{i}] is not a number")))?
            as f32;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
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
            ("/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_draco.glb", "draco"),
            ("/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_meshopt.glb", "meshopt"),
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
}
