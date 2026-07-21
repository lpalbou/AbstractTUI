//! Hostile-GLB generator: a byte-exact minimal valid GLB plus seeded and
//! deterministic mutations of every structural boundary — the fixture
//! pack GFX3D validates accessor extraction against (tests-first, per
//! RT1-8).
//!
//! OWNER: REDTEAM.
//!
//! Every mutant carries an [`Expect`]: the campaign asserts loaders
//! NEVER panic, reject `MustReject` mutants with an error (naming the
//! problem), and still accept `MustLoad` ones (spec-legal oddities —
//! trailing garbage after the declared length, unknown chunk types,
//! missing BIN when nothing references it).

use super::fuzzish::Rng;

/// What a well-behaved loader must do with a mutant.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Spec-valid: must parse (split + doc + extraction when it exists).
    MustLoad,
    /// Malformed: must return an error. Panics and absurd allocations
    /// are failures.
    MustReject,
    /// Byte soup: any `Result` is acceptable; panics are failures.
    NoPanic,
}

pub struct GlbMutant {
    pub name: String,
    pub bytes: Vec<u8>,
    pub expect: Expect,
}

// The JSON document for the minimal GLB. One buffer (the BIN chunk), one
// bufferView over it, POSITION accessor (3 vertices, VEC3/f32), an index
// accessor (SCALAR/u16), one triangle primitive, node, scene, material.
// Kept as a single const so mutations can string-replace exact spans.
const MINIMAL_JSON: &str = concat!(
    r#"{"asset":{"version":"2.0"},"#,
    r#""buffers":[{"byteLength":44}],"#,
    r#""bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},"#,
    r#"{"buffer":0,"byteOffset":36,"byteLength":8}],"#,
    r#""accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},"#,
    r#"{"bufferView":1,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}],"#,
    r#""materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0.5,0.25,1]}}],"#,
    r#""meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],"#,
    r#""nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#
);

/// BIN payload: 3 x VEC3 f32 positions (36 bytes) + 3 x u16 indices
/// (6 bytes) + 2 bytes padding the buffer to the declared 44
/// (spec allows BIN up to 3 bytes longer than buffer.byteLength; here
/// buffer.byteLength covers it exactly).
fn minimal_bin() -> Vec<u8> {
    let mut bin = Vec::with_capacity(44);
    let verts: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    for v in verts {
        bin.extend_from_slice(&v.to_le_bytes());
    }
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin.extend_from_slice(&[0u8, 0]); // pad to 44 = buffer.byteLength
    bin
}

fn pad4(len: usize) -> usize {
    (len + 3) & !3
}

/// Assemble a GLB from parts, honestly (correct lengths, padding).
pub fn assemble(json: &[u8], bin: Option<&[u8]>) -> Vec<u8> {
    let json_padded = pad4(json.len());
    let bin_padded = bin.map(|b| pad4(b.len())).unwrap_or(0);
    let total = 12 + 8 + json_padded + bin.map(|_| 8 + bin_padded).unwrap_or(0);
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes()); // 'glTF'
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_padded as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // 'JSON'
    out.extend_from_slice(json);
    out.resize(12 + 8 + json_padded, b' '); // JSON pads with spaces
    if let Some(b) = bin {
        out.extend_from_slice(&(bin_padded as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // 'BIN\0'
        out.extend_from_slice(b);
        out.resize(total, 0); // BIN pads with zeros
    }
    out
}

/// The canonical minimal valid GLB (one triangle). Loaders must accept it.
pub fn minimal_glb() -> Vec<u8> {
    assemble(MINIMAL_JSON.as_bytes(), Some(&minimal_bin()))
}

fn patch_u32(bytes: &mut [u8], off: usize, value: u32) {
    bytes[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

/// A mutant whose JSON differs by one exact string replacement. Panics if
/// the needle is absent — a mutation that no longer bites must be
/// noticed, not silently skipped.
fn json_mutant(name: &str, needle: &str, replacement: &str, expect: Expect) -> GlbMutant {
    assert!(
        MINIMAL_JSON.contains(needle),
        "mutation needle {needle:?} no longer matches the base JSON"
    );
    let json = MINIMAL_JSON.replacen(needle, replacement, 1);
    GlbMutant {
        name: format!("json_{name}"),
        bytes: assemble(json.as_bytes(), Some(&minimal_bin())),
        expect,
    }
}

/// The deterministic mutation battery + `random_count` seeded byte-soup
/// mutants. Deterministic mutants carry intent in their names; seeded
/// ones reproduce by (seed, index).
pub fn mutants(seed: u64, random_count: usize) -> Vec<GlbMutant> {
    let base = minimal_glb();
    let mut out: Vec<GlbMutant> = Vec::new();
    let mut push = |name: &str, bytes: Vec<u8>, expect: Expect| {
        out.push(GlbMutant {
            name: name.to_string(),
            bytes,
            expect,
        });
    };

    // -- container-level -----------------------------------------------------

    push("valid_minimal", base.clone(), Expect::MustLoad);
    {
        // Trailing garbage past declared length is spec-legal.
        let mut b = base.clone();
        b.extend_from_slice(b"GARBAGE-AFTER-DECLARED-LENGTH");
        push("trailing_garbage", b, Expect::MustLoad);
    }
    {
        // Unknown chunk type appended (skipped per spec).
        let json = MINIMAL_JSON.as_bytes();
        let bin = minimal_bin();
        let mut b = assemble(json, Some(&bin));
        let extra = b"who knows";
        let padded = pad4(extra.len());
        b.extend_from_slice(&(padded as u32).to_le_bytes());
        b.extend_from_slice(&0x5453_5552u32.to_le_bytes()); // 'RUST'
        b.extend_from_slice(extra);
        b.resize(b.len() + (padded - extra.len()), 0);
        let total = b.len() as u32;
        patch_u32(&mut b, 8, total);
        push("unknown_chunk_appended", b, Expect::MustLoad);
    }
    push(
        "bad_magic",
        {
            let mut b = base.clone();
            patch_u32(&mut b, 0, 0x0BAD_F00D);
            b
        },
        Expect::MustReject,
    );
    for version in [0u32, 1, 3, u32::MAX] {
        push(
            &format!("version_{version}"),
            {
                let mut b = base.clone();
                patch_u32(&mut b, 4, version);
                b
            },
            Expect::MustReject,
        );
    }
    push(
        "declared_len_past_buffer",
        {
            let mut b = base.clone();
            let real = b.len() as u32;
            patch_u32(&mut b, 8, real + 1);
            b
        },
        Expect::MustReject,
    );
    push(
        "declared_len_u32max",
        {
            let mut b = base.clone();
            patch_u32(&mut b, 8, u32::MAX);
            b
        },
        Expect::MustReject,
    );
    push(
        "declared_len_zero",
        {
            let mut b = base.clone();
            patch_u32(&mut b, 8, 0);
            b
        },
        Expect::MustReject,
    ); // no JSON chunk inside the declared region
    push(
        "declared_len_header_only",
        {
            let mut b = base.clone();
            patch_u32(&mut b, 8, 12);
            b
        },
        Expect::MustReject,
    );

    // -- chunk-header lies ----------------------------------------------------

    // JSON chunk length claims to reach exactly the end (swallowing BIN):
    // structurally valid GLB (one big JSON chunk) whose JSON tail is
    // space padding + BIN garbage -> must reject at JSON or reference
    // level, never panic.
    push(
        "json_len_swallows_bin",
        {
            let mut b = base.clone();
            let total = b.len() as u32;
            patch_u32(&mut b, 12, total - 12 - 8);
            b
        },
        Expect::MustReject,
    );
    push(
        "json_len_u32max",
        {
            let mut b = base.clone();
            patch_u32(&mut b, 12, u32::MAX);
            b
        },
        Expect::MustReject,
    );
    push(
        "chunk_len_overflow_bait",
        {
            // Chunk length such that off + 8 + len wraps u32 space: on 64-bit
            // usize it will not wrap, so the guard must be the buffer bound;
            // this mutant proves no add-then-compare UB either way.
            let mut b = base.clone();
            patch_u32(&mut b, 12, u32::MAX - 4);
            b
        },
        Expect::MustReject,
    );
    {
        // BIN before JSON (order violation).
        let json = MINIMAL_JSON.as_bytes();
        let bin = minimal_bin();
        let json_padded = pad4(json.len());
        let bin_padded = pad4(bin.len());
        let total = 12 + 8 + bin_padded + 8 + json_padded;
        let mut b = Vec::with_capacity(total);
        b.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&(bin_padded as u32).to_le_bytes());
        b.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        b.extend_from_slice(&bin);
        b.resize(12 + 8 + bin_padded, 0);
        b.extend_from_slice(&(json_padded as u32).to_le_bytes());
        b.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        b.extend_from_slice(json);
        b.resize(total, b' ');
        push("bin_before_json", b, Expect::MustReject);
    }
    {
        // Duplicate JSON chunk.
        let json = MINIMAL_JSON.as_bytes();
        let single = assemble(json, None);
        let json_padded = pad4(json.len());
        let mut b = single.clone();
        b.extend_from_slice(&(json_padded as u32).to_le_bytes());
        b.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        b.extend_from_slice(json);
        b.resize(single.len() + 8 + json_padded, b' ');
        let total = b.len() as u32;
        patch_u32(&mut b, 8, total);
        push("duplicate_json_chunk", b, Expect::MustReject);
    }
    push(
        "missing_bin_with_buffer_refs",
        assemble(MINIMAL_JSON.as_bytes(), None),
        Expect::MustReject,
    );

    // -- truncation ladder ------------------------------------------------------
    // Every structurally interesting boundary, plus each byte of the header.
    let json_padded = pad4(MINIMAL_JSON.len());
    let boundaries: Vec<usize> = (0..=12)
        .chain([13, 15, 19, 20 + json_padded / 2, 20 + json_padded])
        .chain([20 + json_padded + 4, 20 + json_padded + 8, base.len() - 1])
        .collect();
    for cut in boundaries {
        if cut >= base.len() {
            continue;
        }
        // A GLB cut anywhere is corrupt EXCEPT cut=0..12 which is just
        // "too short" — all MustReject either way.
        push(
            &format!("truncate_at_{cut}"),
            base[..cut].to_vec(),
            Expect::MustReject,
        );
    }

    // -- JSON/document-level (accessor extraction attack surface) -------------

    out.push(json_mutant(
        "accessor_offset_huge",
        r#""bufferView":0,"byteOffset":0,"componentType":5126"#,
        r#""bufferView":0,"byteOffset":4294967295,"componentType":5126"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "accessor_count_overflow_bait",
        r#""componentType":5126,"count":3"#,
        r#""componentType":5126,"count":4294967295"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "accessor_count_zero",
        r#""componentType":5126,"count":3"#,
        r#""componentType":5126,"count":0"#,
        Expect::MustReject, // a POSITION accessor with no vertices is degenerate
    ));
    out.push(json_mutant(
        "component_type_confusion_u8_positions",
        r#""componentType":5126,"count":3,"type":"VEC3""#,
        r#""componentType":5121,"count":3,"type":"VEC3""#,
        Expect::MustReject, // POSITION must be f32 in our subset
    ));
    out.push(json_mutant(
        "component_type_unknown",
        r#""componentType":5126"#,
        r#""componentType":9999"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "type_confusion_mat4",
        r#""count":3,"type":"VEC3""#,
        r#""count":3,"type":"MAT4""#,
        Expect::MustReject, // 3 MAT4s = 192 bytes > 36-byte view
    ));
    out.push(json_mutant(
        "bufferview_index_oob",
        r#""accessors":[{"bufferView":0"#,
        r#""accessors":[{"bufferView":7"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "buffer_index_oob",
        r#""bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}"#,
        r#""bufferViews":[{"buffer":3,"byteOffset":0,"byteLength":36}"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "view_past_buffer_end",
        r#"{"buffer":0,"byteOffset":36,"byteLength":8}"#,
        r#"{"buffer":0,"byteOffset":36,"byteLength":800}"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "view_offset_overflow_bait",
        r#"{"buffer":0,"byteOffset":36,"byteLength":8}"#,
        r#"{"buffer":0,"byteOffset":4294967290,"byteLength":8}"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "stride_smaller_than_element",
        r#"{"buffer":0,"byteOffset":0,"byteLength":36}"#,
        r#"{"buffer":0,"byteOffset":0,"byteLength":36,"byteStride":4}"#,
        Expect::MustReject, // VEC3/f32 needs 12; stride 4 interleaves nonsense
    ));
    out.push(json_mutant(
        "stride_unaligned",
        r#"{"buffer":0,"byteOffset":0,"byteLength":36}"#,
        r#"{"buffer":0,"byteOffset":0,"byteLength":36,"byteStride":13}"#,
        Expect::MustReject, // stride must be a multiple of component size
    ));
    out.push(json_mutant(
        "accessor_offset_unaligned_but_inside",
        r#""bufferView":0,"byteOffset":0,"componentType":5126,"count":3"#,
        r#""bufferView":0,"byteOffset":2,"componentType":5126,"count":2"#,
        Expect::NoPanic, // spec says 4-align; real files violate it. Load
                         // (from_le_bytes tolerates) or reject — never panic.
    ));
    out.push(json_mutant(
        "indices_type_float",
        r#"{"bufferView":1,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}"#,
        r#"{"bufferView":1,"byteOffset":0,"componentType":5126,"count":2,"type":"SCALAR"}"#,
        Expect::MustReject, // float indices are not in the subset
    ));
    out.push(json_mutant(
        "primitive_mode_lines",
        r#""attributes":{"POSITION":0},"indices":1,"material":0"#,
        r#""attributes":{"POSITION":0},"indices":1,"material":0,"mode":1"#,
        Expect::MustReject, // only TRIANGLES(4) in v1, reject loudly
    ));
    out.push(json_mutant(
        "sparse_accessor",
        r#""componentType":5126,"count":3,"type":"VEC3""#,
        r#""componentType":5126,"count":3,"type":"VEC3","sparse":{"count":1}"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "extensions_required",
        r#""asset":{"version":"2.0"}"#,
        r#""asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "asset_version_1",
        r#""asset":{"version":"2.0"}"#,
        r#""asset":{"version":"1.0"}"#,
        Expect::MustReject,
    ));
    out.push(json_mutant(
        "node_self_cycle",
        r#""nodes":[{"mesh":0}]"#,
        r#""nodes":[{"mesh":0,"children":[0]}]"#,
        Expect::MustReject, // cyclic hierarchy must not hang/overflow
    ));
    out.push(json_mutant(
        "json_not_object",
        r#"{"asset":{"version":"2.0"},"#,
        r#"[{"asset":{"version":"2.0"},"#,
        Expect::MustReject, // broken JSON grammar too — reject, no panic
    ));

    // -- seeded byte soup -------------------------------------------------------

    let mut rng = Rng::new(seed);
    for i in 0..random_count {
        let mut b = base.clone();
        match rng.below(4) {
            0 => {
                // Random cut.
                let cut = rng.below(b.len());
                b.truncate(cut);
            }
            1 => {
                // Random 4-byte integer stomp at a 4-aligned offset.
                let off = rng.below(b.len().saturating_sub(4)) & !3;
                let v = rng.next_u32();
                patch_u32(&mut b, off, v);
            }
            2 => {
                // Random byte flips (1..=8).
                for _ in 0..rng.range(1, 8) {
                    let off = rng.below(b.len());
                    b[off] ^= rng.byte() | 1;
                }
            }
            _ => {
                // Splice random garbage into the middle.
                let at = rng.below(b.len());
                let garbage: Vec<u8> = (0..rng.range(1, 32)).map(|_| rng.byte()).collect();
                b.splice(at..at, garbage);
            }
        }
        out.push(GlbMutant {
            name: format!("soup_{seed}_{i}"),
            bytes: b,
            expect: Expect::NoPanic,
        });
    }

    out
}

/// Cycle-3 additions: FLOAT-PAYLOAD mutations — the JSON stays valid,
/// the BIN vertex data turns hostile (NaN/Inf/extreme positions). The
/// pinned contract (reviews/cycle3): the loader MAY accept non-finite
/// vertex data (glTF does not forbid it at the container level), but
/// the render pipeline must SURVIVE it — non-finite triangles drop,
/// the framebuffer stays finite, nothing panics.
pub fn float_payload_mutants() -> Vec<GlbMutant> {
    let bin = minimal_bin();
    let mut out = Vec::new();
    let mut push = |name: &str, patch: &dyn Fn(&mut Vec<u8>)| {
        let mut b = bin.clone();
        patch(&mut b);
        out.push(GlbMutant {
            name: format!("float_{name}"),
            bytes: assemble(MINIMAL_JSON.as_bytes(), Some(&b)),
            expect: Expect::NoPanic,
        });
    };
    let put = |b: &mut Vec<u8>, float_idx: usize, v: f32| {
        let off = float_idx * 4;
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    };
    push("nan_one_position", &|b| put(b, 1, f32::NAN));
    push("nan_all_positions", &|b| {
        for i in 0..9 {
            put(b, i, f32::NAN);
        }
    });
    push("inf_position", &|b| put(b, 3, f32::INFINITY));
    push("neg_inf_position", &|b| put(b, 6, f32::NEG_INFINITY));
    push("huge_position", &|b| put(b, 0, 3.0e38));
    push("tiny_denormal_position", &|b| put(b, 4, 1.0e-42));
    push("mixed_nan_inf", &|b| {
        put(b, 0, f32::NAN);
        put(b, 5, f32::INFINITY);
        put(b, 8, f32::NEG_INFINITY);
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_mutants_reproducible_and_named() {
        let a = float_payload_mutants();
        let b = float_payload_mutants();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.bytes, y.bytes, "{} not reproducible", x.name);
            assert_eq!(x.expect, Expect::NoPanic);
        }
    }

    #[test]
    fn minimal_glb_is_well_formed() {
        let b = minimal_glb();
        assert_eq!(&b[0..4], b"glTF");
        assert_eq!(
            u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,
            b.len()
        );
        // Total length 4-aligned, chunks aligned.
        assert_eq!(b.len() % 4, 0);
    }

    #[test]
    fn battery_is_deterministic_and_names_unique() {
        let a = mutants(7, 50);
        let b = mutants(7, 50);
        assert_eq!(a.len(), b.len());
        let mut names = std::collections::BTreeSet::new();
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.name, y.name);
            assert_eq!(x.bytes, y.bytes, "mutant {} not reproducible", x.name);
            assert!(
                names.insert(x.name.clone()),
                "duplicate mutant name {}",
                x.name
            );
        }
        assert!(a.iter().filter(|m| m.expect == Expect::MustLoad).count() >= 3);
        assert!(a.iter().filter(|m| m.expect == Expect::MustReject).count() >= 30);
    }
}
