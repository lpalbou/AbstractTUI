//! Accessor-extraction tests (split file, `#[path]`-included as
//! `extract::tests` — the file-size discipline).
//!
//! OWNER: GFX3D.

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
