//! Parse-time document validation (RT2-2, RT2-3): everything checkable
//! from METADATA ALONE is checked here, at the door, so no consumer
//! between parse and extraction ever handles a dangling index or an
//! impossible layout.
//!
//! The dividing line, applied deliberately:
//!
//! - **Spec-INVALID metadata rejects here**: dangling indices, sparse
//!   accessors (unsupported and loudly so), zero counts, stride games,
//!   spans past the declared view/buffer lengths, wrong attribute
//!   shapes (core glTF mandates POSITION = VEC3/f32 etc.), byteStride
//!   on an index view (spec forbids it — "exclusively with vertex
//!   attributes").
//! - **Spec-LEGAL but unsupported stays extraction-level**: primitive
//!   `mode != TRIANGLES` (a file may carry a LINES primitive we could
//!   legitimately skip one day; today extraction rejects it by name),
//!   node-graph cycles (a WALK concern, rejected during flattening),
//!   and everything that needs the REAL BIN chunk (declared lengths
//!   can lie; extraction re-validates against actual bytes — defense
//!   in depth, not duplication).

use crate::base::{Error, Result};
use crate::three::doc::{Accessor, AccessorType, BufferView, ComponentType, Doc};

/// Resolved element layout of an accessor within its view: (stride,
/// element size), with the stride rules enforced. Shared with the
/// extraction layer so the arithmetic cannot drift between the two.
pub(crate) fn accessor_layout(acc: &Accessor, view: &BufferView, what: &str) -> Result<(u64, u64)> {
    let comp = acc.component_type.byte_size() as u64;
    let elem = comp * acc.ty.components() as u64;
    let stride = match view.byte_stride {
        None => elem,
        Some(s) => {
            let s = s as u64;
            if s < elem {
                return Err(Error::Parse(format!(
                    "gltf: {what}: byteStride {s} smaller than element size {elem}"
                )));
            }
            if !s.is_multiple_of(comp) {
                return Err(Error::Parse(format!(
                    "gltf: {what}: byteStride {s} not a multiple of component size {comp}"
                )));
            }
            s
        }
    };
    Ok((stride, elem))
}

/// Byte span of `count` elements at the accessor offset (checked math;
/// a hostile count must reject, never wrap).
pub(crate) fn accessor_span(acc: &Accessor, stride: u64, elem: u64, what: &str) -> Result<u64> {
    stride
        .checked_mul(acc.count as u64 - 1)
        .and_then(|s| s.checked_add(elem))
        .and_then(|s| s.checked_add(acc.byte_offset as u64))
        .ok_or_else(|| Error::Parse(format!("gltf: {what}: accessor span overflows")))
}

fn check_index(idx: usize, len: usize, what: &str, of: &str) -> Result<()> {
    if idx >= len {
        return Err(Error::Parse(format!(
            "gltf: {what}: index {idx} out of range ({len} {of})"
        )));
    }
    Ok(())
}

pub(crate) fn validate_doc(doc: &Doc) -> Result<()> {
    // Buffer views: buffer index + declared range within the declared
    // buffer length.
    for (i, view) in doc.buffer_views.iter().enumerate() {
        let what = format!("bufferView {i}");
        check_index(view.buffer, doc.buffers.len(), &what, "buffers")?;
        let end = (view.byte_offset as u64)
            .checked_add(view.byte_length as u64)
            .ok_or_else(|| Error::Parse(format!("gltf: {what}: range overflows")))?;
        if end > doc.buffers[view.buffer] as u64 {
            return Err(Error::Parse(format!(
                "gltf: {what}: [{}..{end}) exceeds declared buffer length {}",
                view.byte_offset, doc.buffers[view.buffer]
            )));
        }
    }

    // Accessors: sparse rejection (RT2-3), view index (RT2-2), count,
    // stride rules and span vs the DECLARED view length.
    for (i, acc) in doc.accessors.iter().enumerate() {
        let what = format!("accessor {i}");
        if acc.sparse {
            return Err(Error::Parse(format!(
                "gltf: {what}: sparse accessors not supported (rejected at parse)"
            )));
        }
        if acc.count == 0 {
            return Err(Error::Parse(format!("gltf: {what}: count 0")));
        }
        if let Some(bv) = acc.buffer_view {
            check_index(bv, doc.buffer_views.len(), &what, "bufferViews")?;
            let view = &doc.buffer_views[bv];
            let (stride, elem) = accessor_layout(acc, view, &what)?;
            let span = accessor_span(acc, stride, elem, &what)?;
            if span > view.byte_length as u64 {
                return Err(Error::Parse(format!(
                    "gltf: {what}: needs {span} bytes, bufferView {bv} declares {}",
                    view.byte_length
                )));
            }
        }
    }

    // Meshes/primitives: attribute + indices + material references, and
    // the core-spec attribute SHAPES (a u8 POSITION without the
    // quantization extension is spec-invalid, not merely unsupported —
    // extensionsRequired already rejected that extension upstream).
    for (mi, mesh) in doc.meshes.iter().enumerate() {
        for (pi, prim) in mesh.primitives.iter().enumerate() {
            let what = format!("mesh {mi} primitive {pi}");
            let attr = |name: &str, idx: Option<usize>| -> Result<Option<&Accessor>> {
                match idx {
                    None => Ok(None),
                    Some(a) => {
                        check_index(
                            a,
                            doc.accessors.len(),
                            &format!("{what} {name}"),
                            "accessors",
                        )?;
                        Ok(Some(&doc.accessors[a]))
                    }
                }
            };
            for name in ["POSITION", "NORMAL"] {
                let idx = if name == "POSITION" {
                    prim.position
                } else {
                    prim.normal
                };
                if let Some(acc) = attr(name, idx)? {
                    if acc.ty != AccessorType::Vec3 || acc.component_type != ComponentType::F32 {
                        return Err(Error::Parse(format!(
                            "gltf: {what}: {name} must be VEC3/f32, got {:?}/{:?}",
                            acc.ty, acc.component_type
                        )));
                    }
                }
            }
            if let Some(acc) = attr("TEXCOORD_0", prim.texcoord0)? {
                let ok = acc.ty == AccessorType::Vec2
                    && matches!(
                        (acc.component_type, acc.normalized),
                        (ComponentType::F32, _)
                            | (ComponentType::U8, true)
                            | (ComponentType::U16, true)
                    );
                if !ok {
                    return Err(Error::Parse(format!(
                        "gltf: {what}: TEXCOORD_0 must be VEC2 f32|normalized u8/u16, got {:?}/{:?}",
                        acc.ty, acc.component_type
                    )));
                }
            }
            if let Some(acc) = attr("COLOR_0", prim.color0)? {
                let ok = matches!(acc.ty, AccessorType::Vec3 | AccessorType::Vec4)
                    && matches!(
                        (acc.component_type, acc.normalized),
                        (ComponentType::F32, _)
                            | (ComponentType::U8, true)
                            | (ComponentType::U16, true)
                    );
                if !ok {
                    return Err(Error::Parse(format!(
                        "gltf: {what}: COLOR_0 must be VEC3/VEC4 f32|normalized u8/u16, got {:?}/{:?}",
                        acc.ty, acc.component_type
                    )));
                }
            }
            if let Some(acc) = attr("indices", prim.indices)? {
                if acc.ty != AccessorType::Scalar
                    || !matches!(
                        acc.component_type,
                        ComponentType::U8 | ComponentType::U16 | ComponentType::U32
                    )
                {
                    return Err(Error::Parse(format!(
                        "gltf: {what}: indices must be SCALAR u8/u16/u32, got {:?}/{:?}",
                        acc.ty, acc.component_type
                    )));
                }
                // Spec: byteStride is "defined exclusively for vertex
                // attribute data" — an index view carrying one is
                // malformed, and honoring it would misread interleaved
                // bytes as indices.
                if let Some(bv) = acc.buffer_view {
                    if doc.buffer_views[bv].byte_stride.is_some() {
                        return Err(Error::Parse(format!(
                            "gltf: {what}: byteStride on an index bufferView {bv} (spec forbids)"
                        )));
                    }
                }
            }
            if let Some(m) = prim.material {
                check_index(m, doc.materials.len(), &what, "materials")?;
            }
        }
    }

    // Nodes: mesh + children references (cycle detection stays in the
    // load-time walk — reference validity is parse's job, graph SHAPE
    // is the walker's).
    for (ni, node) in doc.nodes.iter().enumerate() {
        let what = format!("node {ni}");
        if let Some(m) = node.mesh {
            check_index(m, doc.meshes.len(), &what, "meshes")?;
        }
        for &c in &node.children {
            check_index(c, doc.nodes.len(), &what, "nodes")?;
        }
    }
    for &r in &doc.scene_roots {
        check_index(r, doc.nodes.len(), "scene root", "nodes")?;
    }

    // Animations: sampler + accessor + node references resolve.
    for (ai, a) in doc.animations.iter().enumerate() {
        for (si, s) in a.samplers.iter().enumerate() {
            let what = format!("animation {ai} sampler {si}");
            check_index(s.input, doc.accessors.len(), &what, "accessors")?;
            check_index(s.output, doc.accessors.len(), &what, "accessors")?;
        }
        for c in &a.channels {
            let what = format!("animation {ai} channel");
            check_index(c.sampler, a.samplers.len(), &what, "samplers")?;
            check_index(c.target_node, doc.nodes.len(), &what, "nodes")?;
        }
    }

    // Skins: joints/IBM/skeleton references resolve; node.skin points
    // at a real skin (a mesh-less skinned node is legal, ignored).
    for (si, s) in doc.skins.iter().enumerate() {
        let what = format!("skin {si}");
        for &j in &s.joints {
            check_index(j, doc.nodes.len(), &what, "nodes")?;
        }
        if let Some(ibm) = s.inverse_bind_matrices {
            check_index(ibm, doc.accessors.len(), &what, "accessors")?;
        }
        if let Some(sk) = s.skeleton {
            check_index(sk, doc.nodes.len(), &what, "nodes")?;
        }
    }
    for (ni, n) in doc.nodes.iter().enumerate() {
        if let Some(skin) = n.skin {
            check_index(skin, doc.skins.len(), &format!("node {ni}"), "skins")?;
        }
    }

    // Materials/textures/images: the decoration chain resolves.
    for (mi, m) in doc.materials.iter().enumerate() {
        if let Some(t) = m.base_color_texture {
            check_index(t, doc.textures.len(), &format!("material {mi}"), "textures")?;
        }
    }
    for (ti, t) in doc.textures.iter().enumerate() {
        if let Some(s) = t.source {
            check_index(s, doc.images.len(), &format!("texture {ti}"), "images")?;
        }
    }
    for (ii, im) in doc.images.iter().enumerate() {
        if let Some(bv) = im.buffer_view {
            check_index(
                bv,
                doc.buffer_views.len(),
                &format!("image {ii}"),
                "bufferViews",
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::three::doc::Doc;

    fn doc_err(json: &str) -> String {
        Doc::parse(json.as_bytes()).unwrap_err().to_string()
    }

    const HEAD: &str = r#""asset":{"version":"2.0"},"buffers":[{"byteLength":44}]"#;

    #[test]
    fn rt2_2_dangling_indices_reject_at_parse() {
        // accessor -> bufferView out of range.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":7,"componentType":5126,"count":3,"type":"VEC3"}}]}}"#
        ));
        assert!(e.contains("index 7 out of range"), "{e}");

        // bufferView -> buffer out of range.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":3,"byteOffset":0,"byteLength":36}}]}}"#
        ));
        assert!(e.contains("index 3 out of range"), "{e}");

        // primitive -> accessor, material out of range.
        let e = doc_err(&format!(
            r#"{{{HEAD},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":5}}}}]}}]}}"#
        ));
        assert!(e.contains("POSITION") && e.contains("out of range"), "{e}");
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}}],
               "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":9}}]}}]}}"#
        ));
        assert!(e.contains("materials"), "{e}");

        // node -> mesh/children, scene -> node, texture chain.
        let e = doc_err(&format!(r#"{{{HEAD},"nodes":[{{"mesh":4}}]}}"#));
        assert!(e.contains("meshes"), "{e}");
        let e = doc_err(&format!(r#"{{{HEAD},"nodes":[{{"children":[9]}}]}}"#));
        assert!(e.contains("out of range"), "{e}");
        let e = doc_err(&format!(
            r#"{{{HEAD},"scenes":[{{"nodes":[2]}}],"scene":0}}"#
        ));
        assert!(e.contains("scene root"), "{e}");
        let e = doc_err(&format!(
            r#"{{{HEAD},"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":1}}}}}}]}}"#
        ));
        assert!(e.contains("textures"), "{e}");
        let e = doc_err(&format!(r#"{{{HEAD},"textures":[{{"source":3}}]}}"#));
        assert!(e.contains("images"), "{e}");
        let e = doc_err(&format!(r#"{{{HEAD},"images":[{{"bufferView":8}}]}}"#));
        assert!(e.contains("bufferViews"), "{e}");
    }

    #[test]
    fn rt2_3_sparse_rejects_at_parse() {
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
                              "sparse":{{"count":1}}}}]}}"#
        ));
        assert!(e.contains("sparse"), "{e}");
    }

    #[test]
    fn declared_layout_rules_reject_at_parse() {
        // Span past the declared view.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":99,"type":"VEC3"}}]}}"#
        ));
        assert!(e.contains("needs"), "{e}");

        // View past the declared buffer.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":36,"byteLength":800}}]}}"#
        ));
        assert!(e.contains("exceeds declared buffer length"), "{e}");

        // Stride rules.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36,"byteStride":4}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":2,"type":"VEC3"}}]}}"#
        ));
        assert!(e.contains("smaller than element"), "{e}");

        // Zero count.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":0,"type":"VEC3"}}]}}"#
        ));
        assert!(e.contains("count 0"), "{e}");
    }

    #[test]
    fn attribute_shape_rules_reject_at_parse() {
        // u8 POSITION (spec-invalid without the quantization extension).
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5121,"count":3,"type":"VEC3"}}],
               "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}]}}"#
        ));
        assert!(e.contains("POSITION must be VEC3/f32"), "{e}");

        // Float indices.
        let e = doc_err(&format!(
            r#"{{{HEAD},"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                            {{"bufferView":0,"componentType":5126,"count":3,"type":"SCALAR"}}],
               "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}]}}"#
        ));
        assert!(e.contains("indices must be SCALAR"), "{e}");

        // byteStride on an index view (severity call: REJECT — honoring
        // a spec-forbidden stride risks misreading interleaved bytes as
        // indices; no legitimate producer emits it). Fixture note: the
        // view must be long enough that the ACCESSOR-level span check
        // (stride 4 x 3 u16 = 10 bytes) passes — otherwise the span
        // rejection fires first and this test pins the wrong rule
        // (exactly the interrupted-wave failure: 8 declared < 10).
        let e = doc_err(&format!(
            r#"{{{HEAD},
               "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},
                              {{"buffer":0,"byteOffset":32,"byteLength":12,"byteStride":4}}],
               "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                            {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],
               "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}]}}"#
        ));
        assert!(e.contains("byteStride on an index bufferView"), "{e}");
    }

    #[test]
    fn valid_documents_still_parse() {
        // The minimal mutator GLB's JSON must survive all new rules.
        let glb = crate::testing::glb_mutate::minimal_glb();
        let chunks = crate::three::glb::split(&glb).unwrap();
        assert!(Doc::parse(chunks.json).is_ok());
    }
}
