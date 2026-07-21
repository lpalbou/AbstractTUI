//! Accessor extraction: typed vertex/index arrays out of the BIN chunk,
//! validated against hostile input per RT1-8 (tests run REDTEAM's GLB
//! mutator battery — see `three::load`).
//!
//! The rules, each enforced with a NAMED rejection:
//!
//! - all reads are `from_le_bytes` on byte slices (glTF is little-endian
//!   by spec; pointer casts are banned — a future big-endian port must
//!   fail loudly in review, not silently in rendering, and unaligned
//!   `byteOffset` in real files must simply work);
//! - every offset/length/stride computation is checked u64 math — a
//!   `count` of `u32::MAX` must reject, never overflow or allocate;
//! - stride < element size, stride not a multiple of the component
//!   size, and spans past the view/buffer reject by name;
//! - sparse accessors and non-TRIANGLES primitive modes reject by name;
//! - GLB buffer 0 is the BIN chunk; any other buffer index is an
//!   external-file reference the standalone engine refuses.

use crate::base::{Error, Result};
use crate::three::doc::{Accessor, AccessorType, ComponentType, Doc, Primitive};

/// Extracted, validated triangle-mesh data — the rasterizer's input.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals when the primitive has them (same length as
    /// `positions`); the rasterizer falls back to face normals.
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Per-vertex RGBA color (COLOR_0), linear 0..=1.
    pub colors: Option<Vec<[f32; 4]>>,
    /// Triangle list, always indexed (synthesized 0..n when the
    /// primitive was non-indexed). Length is a multiple of 3; every
    /// index is < positions.len() (validated here so the rasterizer
    /// never bounds-checks).
    pub indices: Vec<u32>,
    pub material: Option<usize>,
    /// Skinning: JOINTS_0 (indices into the SKIN's joint list, not
    /// node indices) + WEIGHTS_0, both-or-neither (enforced at
    /// extraction). Range/normalization checks live in `load` where
    /// the skin context exists.
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Generate SMOOTH vertex normals (area-weighted: the unnormalized
    /// cross product's magnitude is 2x the triangle area, so summing
    /// raw crosses weights each face's vote by its size — the standard
    /// artifact-free accumulation). Degenerate triangles contribute a
    /// zero vector and vanish for free. Overwrites nothing when
    /// normals already exist; the flat per-face fallback (rasterizer-
    /// side) remains the default for meshes left without normals.
    pub fn compute_smooth_normals(&mut self) {
        if self.normals.is_some() {
            return;
        }
        let mut acc = vec![[0.0f32; 3]; self.positions.len()];
        for tri in self.indices.chunks_exact(3) {
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let p = |i: usize| self.positions[i];
            let (a, b, c) = (p(i0), p(i1), p(i2));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            if !n.iter().all(|c| c.is_finite()) {
                continue; // NaN-poisoned face must not spread
            }
            for &i in &[i0, i1, i2] {
                for k in 0..3 {
                    acc[i][k] += n[k];
                }
            }
        }
        for n in &mut acc {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-12 {
                for c in n.iter_mut() {
                    *c /= len;
                }
            }
            // Isolated/degenerate vertices keep the zero normal — the
            // rasterizer's zero-safe normalize shades them ambient.
        }
        self.normals = Some(acc);
    }
}

/// Extract one primitive. `bin` is the GLB BIN chunk when present.
pub fn extract_primitive(doc: &Doc, prim: &Primitive, bin: Option<&[u8]>) -> Result<MeshData> {
    if prim.mode != 4 {
        return Err(Error::Parse(format!(
            "gltf: primitive mode {} not supported (only 4 = TRIANGLES)",
            prim.mode
        )));
    }
    let pos_idx = prim
        .position
        .ok_or_else(|| Error::Parse("gltf: primitive has no POSITION attribute".into()))?;

    let positions = read_vec3_f32(doc, pos_idx, bin, "POSITION")?;
    let n = positions.len();

    let normals = match prim.normal {
        None => None,
        Some(i) => {
            let v = read_vec3_f32(doc, i, bin, "NORMAL")?;
            if v.len() != n {
                return Err(Error::Parse(format!(
                    "gltf: NORMAL count {} != POSITION count {n}",
                    v.len()
                )));
            }
            Some(v)
        }
    };
    let uvs = match prim.texcoord0 {
        None => None,
        Some(i) => {
            let v = read_vec2_f32(doc, i, bin, "TEXCOORD_0")?;
            if v.len() != n {
                return Err(Error::Parse(format!(
                    "gltf: TEXCOORD_0 count {} != POSITION count {n}",
                    v.len()
                )));
            }
            Some(v)
        }
    };
    let colors = match prim.color0 {
        None => None,
        Some(i) => {
            let v = read_colors(doc, i, bin)?;
            if v.len() != n {
                return Err(Error::Parse(format!(
                    "gltf: COLOR_0 count {} != POSITION count {n}",
                    v.len()
                )));
            }
            Some(v)
        }
    };

    let (joints, weights) = match (prim.joints0, prim.weights0) {
        (None, None) => (None, None),
        (Some(_), None) | (None, Some(_)) => {
            return Err(Error::Parse(
                "gltf: JOINTS_0 and WEIGHTS_0 must be present together".into(),
            ));
        }
        (Some(j), Some(w)) => {
            let joints = read_joints(doc, j, bin)?;
            let weights = read_weights(doc, w, bin)?;
            if joints.len() != n || weights.len() != n {
                return Err(Error::Parse(format!(
                    "gltf: JOINTS_0 count {} / WEIGHTS_0 count {} != POSITION count {n}",
                    joints.len(),
                    weights.len()
                )));
            }
            (Some(joints), Some(weights))
        }
    };

    let indices = match prim.indices {
        Some(i) => read_indices(doc, i, bin, n)?,
        None => {
            // Non-indexed draw: synthesize. Vertex count must still
            // triangulate.
            (0..n as u32).collect()
        }
    };
    if indices.is_empty() || indices.len() % 3 != 0 {
        return Err(Error::Parse(format!(
            "gltf: triangle index count {} is not a positive multiple of 3",
            indices.len()
        )));
    }

    Ok(MeshData {
        positions,
        normals,
        uvs,
        colors,
        indices,
        material: prim.material,
        joints,
        weights,
    })
}

/// JOINTS_0: VEC4 of u8 or u16 (spec set), widened to u16.
fn read_joints(doc: &Doc, idx: usize, bin: Option<&[u8]>) -> Result<Vec<[u16; 4]>> {
    let what = "JOINTS_0";
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    if acc.ty != AccessorType::Vec4 {
        return Err(Error::Parse(format!(
            "gltf: {what} accessor {idx} must be VEC4, got {:?}",
            acc.ty
        )));
    }
    let per: Box<dyn Fn(usize) -> u16> = match acc.component_type {
        ComponentType::U8 => Box::new(move |o| bytes[o] as u16),
        ComponentType::U16 => Box::new(move |o| u16::from_le_bytes([bytes[o], bytes[o + 1]])),
        ct => {
            return Err(Error::Parse(format!(
                "gltf: {what} accessor {idx} component {ct:?} not in the spec set (u8/u16)"
            )))
        }
    };
    let cs = acc.component_type.byte_size();
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        out.push([per(o), per(o + cs), per(o + 2 * cs), per(o + 3 * cs)]);
    }
    Ok(out)
}

/// WEIGHTS_0: VEC4 of f32, or NORMALIZED u8/u16 (spec set).
fn read_weights(doc: &Doc, idx: usize, bin: Option<&[u8]>) -> Result<Vec<[f32; 4]>> {
    let what = "WEIGHTS_0";
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    if acc.ty != AccessorType::Vec4 {
        return Err(Error::Parse(format!(
            "gltf: {what} accessor {idx} must be VEC4, got {:?}",
            acc.ty
        )));
    }
    let per: Box<dyn Fn(usize) -> f32> = match acc.component_type {
        ComponentType::F32 => Box::new(move |o| f32_at(bytes, o)),
        ComponentType::U8 if acc.normalized => Box::new(move |o| bytes[o] as f32 / 255.0),
        ComponentType::U16 if acc.normalized => {
            Box::new(move |o| u16::from_le_bytes([bytes[o], bytes[o + 1]]) as f32 / 65535.0)
        }
        ct => {
            return Err(Error::Parse(format!(
                "gltf: {what} accessor {idx} component {ct:?} must be f32 or normalized u8/u16"
            )))
        }
    };
    let cs = acc.component_type.byte_size();
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        out.push([per(o), per(o + cs), per(o + 2 * cs), per(o + 3 * cs)]);
    }
    Ok(out)
}

/// Inverse bind matrices: MAT4 f32, column-major (glTF matches our
/// `Mat4::from_cols_array` layout directly).
pub(crate) fn read_mat4_f32(
    doc: &Doc,
    idx: usize,
    bin: Option<&[u8]>,
    what: &str,
) -> Result<Vec<[f32; 16]>> {
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    expect_shape(acc, idx, AccessorType::Mat4, ComponentType::F32, what)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        let mut m = [0.0f32; 16];
        for (k, slot) in m.iter_mut().enumerate() {
            *slot = f32_at(bytes, o + k * 4);
        }
        out.push(m);
    }
    Ok(out)
}

/// Resolve + validate an accessor down to (bytes, stride, element size).
/// Every named rule from the module doc lives here.
fn accessor_bytes<'a, 'd>(
    doc: &'d Doc,
    idx: usize,
    bin: Option<&'a [u8]>,
    what: &str,
) -> Result<(&'a [u8], usize, &'d Accessor)> {
    let acc = doc
        .accessors
        .get(idx)
        .ok_or_else(|| Error::Parse(format!("gltf: {what}: accessor {idx} out of range")))?;
    if acc.sparse {
        return Err(Error::Parse(format!(
            "gltf: {what}: sparse accessor {idx} not supported"
        )));
    }
    if acc.count == 0 {
        return Err(Error::Parse(format!(
            "gltf: {what}: accessor {idx} has count 0"
        )));
    }
    let bv_idx = acc.buffer_view.ok_or_else(|| {
        Error::Parse(format!(
            "gltf: {what}: accessor {idx} has no bufferView (zeros accessor)"
        ))
    })?;
    let view = doc
        .buffer_views
        .get(bv_idx)
        .ok_or_else(|| Error::Parse(format!("gltf: {what}: bufferView {bv_idx} out of range")))?;
    if view.buffer != 0 {
        return Err(Error::Parse(format!(
            "gltf: {what}: buffer {} is an external buffer (only GLB BIN = buffer 0 supported)",
            view.buffer
        )));
    }
    let bin = bin.ok_or_else(|| {
        Error::Parse(format!(
            "gltf: {what}: accessor references BIN but the GLB has no BIN chunk"
        ))
    })?;

    // View bounds against the real BIN chunk, in u64 (a hostile
    // byteOffset near u32::MAX must not wrap 32-bit usize).
    let view_off = view.byte_offset as u64;
    let view_len = view.byte_length as u64;
    let view_end = view_off.checked_add(view_len).ok_or_else(|| {
        Error::Parse(format!("gltf: {what}: bufferView {bv_idx} range overflows"))
    })?;
    if view_end > bin.len() as u64 {
        return Err(Error::Parse(format!(
            "gltf: {what}: bufferView {bv_idx} [{view_off}..{view_end}) runs past BIN ({} bytes)",
            bin.len()
        )));
    }

    // Stride/span arithmetic is SHARED with parse-time validation
    // (three::validate) so the two layers cannot drift; this call
    // re-checks against the REAL view/BIN rather than declared lengths.
    let (stride, elem) = crate::three::validate::accessor_layout(acc, view, what)?;
    let span = crate::three::validate::accessor_span(acc, stride, elem, what)?;
    if span > view_len {
        return Err(Error::Parse(format!(
            "gltf: {what}: accessor {idx} needs {span} bytes, view has {view_len}"
        )));
    }

    let start = (view_off + acc.byte_offset as u64) as usize;
    let end = (view_off + view_len) as usize;
    Ok((&bin[start..end], stride as usize, acc))
}

#[inline]
fn f32_at(bytes: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(bytes[off..off + 4].try_into().expect("span validated"))
}

fn expect_shape(
    acc: &Accessor,
    idx: usize,
    ty: AccessorType,
    ct: ComponentType,
    what: &str,
) -> Result<()> {
    if acc.ty != ty || acc.component_type != ct {
        return Err(Error::Parse(format!(
            "gltf: {what}: accessor {idx} is {:?}/{:?}, expected {ty:?}/{ct:?}",
            acc.ty, acc.component_type
        )));
    }
    Ok(())
}

pub(crate) fn read_vec3_f32(
    doc: &Doc,
    idx: usize,
    bin: Option<&[u8]>,
    what: &str,
) -> Result<Vec<[f32; 3]>> {
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    expect_shape(acc, idx, AccessorType::Vec3, ComponentType::F32, what)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        out.push([f32_at(bytes, o), f32_at(bytes, o + 4), f32_at(bytes, o + 8)]);
    }
    Ok(out)
}

/// Keyframe times / weights: SCALAR f32 accessor.
pub(crate) fn read_scalar_f32(
    doc: &Doc,
    idx: usize,
    bin: Option<&[u8]>,
    what: &str,
) -> Result<Vec<f32>> {
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    expect_shape(acc, idx, AccessorType::Scalar, ComponentType::F32, what)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        out.push(f32_at(bytes, i * stride));
    }
    Ok(out)
}

/// Rotation keyframes: VEC4 f32 accessor (normalized-int rotation
/// outputs are spec-legal but out of scope — named rejection).
pub(crate) fn read_vec4_f32(
    doc: &Doc,
    idx: usize,
    bin: Option<&[u8]>,
    what: &str,
) -> Result<Vec<[f32; 4]>> {
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    expect_shape(acc, idx, AccessorType::Vec4, ComponentType::F32, what)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        out.push([
            f32_at(bytes, o),
            f32_at(bytes, o + 4),
            f32_at(bytes, o + 8),
            f32_at(bytes, o + 12),
        ]);
    }
    Ok(out)
}

fn read_vec2_f32(doc: &Doc, idx: usize, bin: Option<&[u8]>, what: &str) -> Result<Vec<[f32; 2]>> {
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    expect_shape(acc, idx, AccessorType::Vec2, ComponentType::F32, what)?;
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        out.push([f32_at(bytes, o), f32_at(bytes, o + 4)]);
    }
    Ok(out)
}

/// COLOR_0: VEC3 or VEC4 of f32, or normalized u8/u16 (spec set).
/// VEC3 gets alpha 1.
fn read_colors(doc: &Doc, idx: usize, bin: Option<&[u8]>) -> Result<Vec<[f32; 4]>> {
    let what = "COLOR_0";
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    let comps = match acc.ty {
        AccessorType::Vec3 => 3,
        AccessorType::Vec4 => 4,
        other => {
            return Err(Error::Parse(format!(
                "gltf: {what}: accessor {idx} is {other:?}, expected VEC3 or VEC4"
            )))
        }
    };
    let read_comp: Box<dyn Fn(usize) -> f32> = match acc.component_type {
        ComponentType::F32 => Box::new(move |o| f32_at(bytes, o)),
        ComponentType::U8 if acc.normalized => Box::new(move |o| bytes[o] as f32 / 255.0),
        ComponentType::U16 if acc.normalized => Box::new(move |o| {
            u16::from_le_bytes(bytes[o..o + 2].try_into().expect("validated")) as f32 / 65535.0
        }),
        ct => {
            return Err(Error::Parse(format!(
                "gltf: {what}: accessor {idx} componentType {ct:?} (normalized={}) not supported",
                acc.normalized
            )))
        }
    };
    let comp_size = acc.component_type.byte_size();
    let mut out = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let o = i * stride;
        let mut c = [0.0f32, 0.0, 0.0, 1.0];
        for (k, slot) in c.iter_mut().take(comps).enumerate() {
            *slot = read_comp(o + k * comp_size);
        }
        out.push(c);
    }
    Ok(out)
}

/// Indices: SCALAR u8/u16/u32 (u8 tolerated per spec), every value
/// bounds-checked against the vertex count so downstream never has to.
fn read_indices(
    doc: &Doc,
    idx: usize,
    bin: Option<&[u8]>,
    vertex_count: usize,
) -> Result<Vec<u32>> {
    let what = "indices";
    let (bytes, stride, acc) = accessor_bytes(doc, idx, bin, what)?;
    if acc.ty != AccessorType::Scalar {
        return Err(Error::Parse(format!(
            "gltf: {what}: accessor {idx} is {:?}, expected SCALAR",
            acc.ty
        )));
    }
    // Spec: byteStride exists "exclusively for vertex attribute data" —
    // parse rejects it on index views; this is the defense for docs
    // built without going through Doc::parse.
    if stride != acc.component_type.byte_size() {
        return Err(Error::Parse(format!(
            "gltf: {what}: byteStride on an index bufferView (spec forbids)"
        )));
    }
    let mut out = Vec::with_capacity(acc.count);
    match acc.component_type {
        ComponentType::U8 => {
            for i in 0..acc.count {
                out.push(bytes[i * stride] as u32);
            }
        }
        ComponentType::U16 => {
            for i in 0..acc.count {
                let o = i * stride;
                out.push(u16::from_le_bytes(bytes[o..o + 2].try_into().expect("validated")) as u32);
            }
        }
        ComponentType::U32 => {
            for i in 0..acc.count {
                let o = i * stride;
                out.push(u32::from_le_bytes(
                    bytes[o..o + 4].try_into().expect("validated"),
                ));
            }
        }
        ct => {
            return Err(Error::Parse(format!(
                "gltf: {what}: componentType {ct:?} not valid for indices (u8/u16/u32 only)"
            )))
        }
    }
    for (i, &v) in out.iter().enumerate() {
        if v as usize >= vertex_count {
            return Err(Error::Parse(format!(
                "gltf: {what}: index[{i}] = {v} out of range ({vertex_count} vertices)"
            )));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three::doc::{BufferView, Mesh};

    /// Build a Doc + BIN with one strided POSITION accessor to exercise
    /// paths the real assets (tightly packed) do not.
    fn strided_fixture(stride: usize, count: usize, bin_len: usize) -> (Doc, Vec<u8>) {
        let mut doc = Doc::default();
        doc.buffer_views.push(BufferView {
            buffer: 0,
            byte_offset: 0,
            byte_length: bin_len,
            byte_stride: Some(stride),
        });
        doc.accessors.push(Accessor {
            buffer_view: Some(0),
            byte_offset: 0,
            component_type: ComponentType::F32,
            count,
            ty: AccessorType::Vec3,
            normalized: false,
            sparse: false,
        });
        let mut bin = vec![0u8; bin_len];
        for i in 0..count {
            for c in 0..3 {
                let v = (i * 10 + c) as f32;
                let off = i * stride + c * 4;
                if off + 4 <= bin.len() {
                    bin[off..off + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        (doc, bin)
    }

    #[test]
    fn strided_positions_read_correctly() {
        // Stride 16 (12-byte element + 4 padding) x 3 elements.
        let (doc, bin) = strided_fixture(16, 3, 16 * 2 + 12);
        let v = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap();
        assert_eq!(
            v,
            vec![[0.0, 1.0, 2.0], [10.0, 11.0, 12.0], [20.0, 21.0, 22.0]]
        );
    }

    #[test]
    fn stride_rules_reject_by_name() {
        // Stride smaller than element.
        let (doc, bin) = strided_fixture(8, 2, 64);
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("byteStride 8 smaller"), "{err}");

        // Stride not a multiple of component size.
        let (doc, bin) = strided_fixture(13, 2, 64);
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("not a multiple"), "{err}");
    }

    #[test]
    fn span_overflow_and_view_bounds_reject() {
        // count so large stride*count overflows u64? u32::MAX count with
        // stride 12 stays in u64; the SPAN check must catch it.
        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.accessors[0].count = u32::MAX as usize;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("needs"), "{err}");

        // Accessor byteOffset pushing past the view.
        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.accessors[0].byte_offset = u32::MAX as usize;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("needs"), "{err}");

        // View itself past BIN.
        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.buffer_views[0].byte_length = 400;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("runs past BIN"), "{err}");
    }

    #[test]
    fn wrong_shapes_reject_by_name() {
        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.accessors[0].ty = AccessorType::Vec2;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("expected Vec3"), "{err}");

        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.accessors[0].sparse = true;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("sparse"), "{err}");

        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.accessors[0].count = 0;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("count 0"), "{err}");

        // External buffer.
        let (mut doc, bin) = strided_fixture(12, 3, 36);
        doc.buffer_views[0].buffer = 1;
        let err = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap_err();
        assert!(err.to_string().contains("external buffer"), "{err}");

        // Missing BIN.
        let (doc, _) = strided_fixture(12, 3, 36);
        let err = read_vec3_f32(&doc, 0, None, "POSITION").unwrap_err();
        assert!(err.to_string().contains("no BIN chunk"), "{err}");
    }

    #[test]
    fn unaligned_offset_loads_via_from_le_bytes() {
        // RT1-8a: real files violate 4-alignment; byte-slice reads must
        // simply work. Element at byte offset 2.
        let mut doc = Doc::default();
        doc.buffer_views.push(BufferView {
            buffer: 0,
            byte_offset: 0,
            byte_length: 16,
            byte_stride: None,
        });
        doc.accessors.push(Accessor {
            buffer_view: Some(0),
            byte_offset: 2,
            component_type: ComponentType::F32,
            count: 1,
            ty: AccessorType::Vec3,
            normalized: false,
            sparse: false,
        });
        let mut bin = vec![0u8; 16];
        bin[2..6].copy_from_slice(&1.5f32.to_le_bytes());
        bin[6..10].copy_from_slice(&2.5f32.to_le_bytes());
        bin[10..14].copy_from_slice(&(-3.5f32).to_le_bytes());
        let v = read_vec3_f32(&doc, 0, Some(&bin), "POSITION").unwrap();
        assert_eq!(v, vec![[1.5, 2.5, -3.5]]);
    }

    fn tri_fixture() -> (Doc, Vec<u8>) {
        // 3 positions + u16 indices, tightly packed — the extraction
        // happy path plus mutation surface for primitive-level tests.
        let mut doc = Doc::default();
        doc.buffer_views.push(BufferView {
            buffer: 0,
            byte_offset: 0,
            byte_length: 36,
            byte_stride: None,
        });
        doc.buffer_views.push(BufferView {
            buffer: 0,
            byte_offset: 36,
            byte_length: 6,
            byte_stride: None,
        });
        doc.accessors.push(Accessor {
            buffer_view: Some(0),
            byte_offset: 0,
            component_type: ComponentType::F32,
            count: 3,
            ty: AccessorType::Vec3,
            normalized: false,
            sparse: false,
        });
        doc.accessors.push(Accessor {
            buffer_view: Some(1),
            byte_offset: 0,
            component_type: ComponentType::U16,
            count: 3,
            ty: AccessorType::Scalar,
            normalized: false,
            sparse: false,
        });
        doc.meshes.push(Mesh {
            name: None,
            primitives: vec![],
        });
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        (doc, bin)
    }

    fn tri_prim() -> Primitive {
        Primitive {
            position: Some(0),
            normal: None,
            texcoord0: None,
            color0: None,
            joints0: None,
            weights0: None,
            indices: Some(1),
            material: None,
            mode: 4,
        }
    }

    #[test]
    fn primitive_extraction_happy_path() {
        let (doc, bin) = tri_fixture();
        let m = extract_primitive(&doc, &tri_prim(), Some(&bin)).unwrap();
        assert_eq!(m.positions.len(), 3);
        assert_eq!(m.indices, vec![0, 1, 2]);
        assert_eq!(m.triangle_count(), 1);
        assert!(m.normals.is_none());
    }

    #[test]
    fn primitive_rejections_by_name() {
        let (doc, bin) = tri_fixture();

        let mut p = tri_prim();
        p.mode = 1;
        let err = extract_primitive(&doc, &p, Some(&bin)).unwrap_err();
        assert!(err.to_string().contains("mode 1"), "{err}");

        let mut p = tri_prim();
        p.position = None;
        let err = extract_primitive(&doc, &p, Some(&bin)).unwrap_err();
        assert!(err.to_string().contains("no POSITION"), "{err}");

        // Float indices.
        let (mut doc2, bin2) = tri_fixture();
        doc2.accessors[1].component_type = ComponentType::F32;
        doc2.accessors[1].count = 1; // keep span inside the 6-byte view
        let err = extract_primitive(&doc2, &tri_prim(), Some(&bin2)).unwrap_err();
        assert!(err.to_string().contains("not valid for indices"), "{err}");

        // Out-of-range index value.
        let (doc3, mut bin3) = tri_fixture();
        bin3[36..38].copy_from_slice(&9u16.to_le_bytes());
        let err = extract_primitive(&doc3, &tri_prim(), Some(&bin3)).unwrap_err();
        assert!(err.to_string().contains("out of range"), "{err}");

        // Index count not a multiple of 3.
        let (mut doc4, bin4) = tri_fixture();
        doc4.accessors[1].count = 2;
        let err = extract_primitive(&doc4, &tri_prim(), Some(&bin4)).unwrap_err();
        assert!(err.to_string().contains("multiple of 3"), "{err}");
    }

    #[test]
    fn non_indexed_synthesizes_indices() {
        let (doc, bin) = tri_fixture();
        let mut p = tri_prim();
        p.indices = None;
        let m = extract_primitive(&doc, &p, Some(&bin)).unwrap();
        assert_eq!(m.indices, vec![0, 1, 2]);
    }

    #[test]
    fn u8_and_u32_indices_supported() {
        let (mut doc, mut bin) = tri_fixture();
        // Rewrite the index view as u32.
        doc.buffer_views[1].byte_length = 12;
        doc.accessors[1].component_type = ComponentType::U32;
        bin.truncate(36);
        for i in [2u32, 1, 0] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let m = extract_primitive(&doc, &tri_prim(), Some(&bin)).unwrap();
        assert_eq!(m.indices, vec![2, 1, 0]);
    }
}
