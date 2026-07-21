//! REDTEAM cycle-5 independent JPEG pass. GFX3D shipped a truncation
//! ladder, a 600-case marker-soup, and dimension-bomb guards; this suite
//! attacks the half those cannot reach — STRUCTURALLY VALID but
//! pathological entropy tables and header lies that walk deep into the
//! build + MCU paths — plus a seeded malformed corpus verified under the
//! counting-allocator budget in `alloc_budget.rs`.
//!
//! Contract under attack (jpeg.rs header): decode-or-reject, NEVER panic,
//! NEVER absurd allocation. Deep single-code Huffman trees are the named
//! soft spot; component-count/selector lies and restart edges are the
//! header-lie surface.

use abstracttui::gfx::jpeg;
use abstracttui::testing::jpeg_build::{CompSpec, FlatJpeg, HuffSpec};
use abstracttui::testing::Rng;

/// Sanity: the flat builder must produce a REAL decodable baseline JPEG,
/// or every "it decoded" assertion below is vacuous.
#[test]
fn flat_builder_produces_decodable_jpeg() {
    let gray = FlatJpeg::grayscale(16, 16).build();
    let img = jpeg::decode(&gray).expect("flat grayscale must decode");
    assert_eq!((img.width(), img.height()), (16, 16));

    let color = FlatJpeg::color444(24, 16).build();
    let img = jpeg::decode(&color).expect("flat 4:4:4 must decode");
    assert_eq!((img.width(), img.height()), (24, 16));
}

/// The named soft spot: a valid table whose only symbol sits at a DEEP
/// canonical code length. Every length 1..=16 must decode (the entropy
/// stream is generated to match) — no length may wedge or misdecode.
#[test]
fn deep_single_code_huffman_trees_decode_at_every_length() {
    for len in 1..=16usize {
        let bytes = FlatJpeg::grayscale(16, 16).with_flat_code_len(len).build();
        let img = jpeg::decode(&bytes).unwrap_or_else(|e| panic!("flat_code_len {len}: {e}"));
        assert_eq!((img.width(), img.height()), (16, 16), "len {len}");
        // Flat DC=0 over an identity quantizer is a mid-gray field; the
        // exact value is the IDCT of a zero block (level shift => 128).
        let p = img.get(3, 3).unwrap();
        assert!(
            (p.r as i32 - 128).abs() <= 2,
            "len {len}: flat field should be ~128, got {p:?}"
        );
    }
}

/// A code length of 16 with the WRONG number of entropy bits (a decoder
/// that miscounts deep codes would read into the next block or past the
/// stream). We assert graceful handling of a deep tree whose stream is
/// deliberately one bit short: reject, never panic.
#[test]
fn deep_tree_truncated_entropy_rejects_cleanly() {
    let mut bytes = FlatJpeg::grayscale(16, 16).with_flat_code_len(16).build();
    // Chop the final entropy byte + EOI, leaving the MCU loop starved.
    // Find the SOS marker and truncate a few bytes into its data.
    let sos = bytes.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    let cut = (sos + 20).min(bytes.len());
    bytes.truncate(cut);
    // Must be an error (truncated entropy), and must not panic.
    assert!(
        jpeg::decode(&bytes).is_err(),
        "starved deep-tree stream decoded"
    );
}

/// Oversubscribed Huffman: more codes than a length can hold. The build
/// must reject by name (codes overflow their bit length), never accept a
/// table that would decode ambiguously.
#[test]
fn oversubscribed_huffman_table_rejected() {
    let mut j = FlatJpeg::grayscale(16, 16);
    // Three 1-bit codes is impossible (only 0 and 1 exist at length 1).
    j.huff[0] = HuffSpec {
        class: 0,
        id: 0,
        counts: [3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        values: vec![0, 1, 2],
    };
    let err = jpeg::decode(&j.build()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("overflow") || msg.contains("DHT"),
        "expected overflow/DHT rejection, got: {msg}"
    );
}

/// DHT count/value mismatch (declares N symbols, carries M): rejected.
#[test]
fn dht_symbol_count_lie_rejected() {
    let mut j = FlatJpeg::grayscale(16, 16);
    // Declare one 2-bit code but supply zero values.
    j.huff[0] = HuffSpec {
        class: 0,
        id: 0,
        counts: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        values: vec![],
    };
    assert!(jpeg::decode(&j.build()).is_err(), "DHT length lie accepted");
}

/// Component-count lie: SOF says 3 components, SOS says 1 (or vice
/// versa). The decoder must reject the multi-scan / mismatch, not index
/// out of bounds.
#[test]
fn component_count_mismatch_rejected() {
    let mut j = FlatJpeg::color444(16, 16);
    // SOS claims a single component while SOF has three.
    j.sos_selectors = Some(vec![1]);
    // Build manually-inconsistent SOS by dropping to 1 selector: the
    // builder writes ns from components.len(), so instead lie the other
    // way — give a 4th phantom selector.
    let bytes = j.build();
    // The builder keeps SOS ns == components.len(); to force a real
    // mismatch, patch the SOS ns byte to 1.
    let mut patched = bytes.clone();
    let sos = patched.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    // SOS: FF DA len_hi len_lo ns ... ; ns is at sos+4.
    patched[sos + 4] = 1;
    assert!(
        jpeg::decode(&patched).is_err(),
        "component-count mismatch accepted"
    );
}

/// Missing quant/entropy table references: SOF/SOS bind ids that were
/// never defined. Must be a named error, never a panic on the None.
#[test]
fn dangling_table_references_rejected() {
    let mut j = FlatJpeg::grayscale(16, 16);
    // Reference a quant id nobody defined.
    j.components[0] = CompSpec {
        id: 1,
        h: 1,
        v: 1,
        quant_id: 3,
        dc_id: 0,
        ac_id: 0,
    };
    assert!(
        jpeg::decode(&j.build()).is_err(),
        "dangling quant ref accepted"
    );

    let mut j = FlatJpeg::grayscale(16, 16);
    j.components[0] = CompSpec {
        id: 1,
        h: 1,
        v: 1,
        quant_id: 0,
        dc_id: 2,
        ac_id: 0,
    };
    assert!(
        jpeg::decode(&j.build()).is_err(),
        "dangling DC ref accepted"
    );
}

/// Restart interval edge values: 0 (none), 1 (every MCU), and larger
/// than the MCU count (never fires). All three must decode a flat image,
/// with the entropy generator inserting matching RSTn markers.
#[test]
fn restart_interval_edges_decode() {
    for dri in [0u16, 1, 2, 3, 255] {
        let mut j = FlatJpeg::grayscale(24, 24); // 3x3 = 9 MCUs
        j.dri = dri;
        let img = jpeg::decode(&j.build()).unwrap_or_else(|e| panic!("dri {dri}: {e}"));
        assert_eq!((img.width(), img.height()), (24, 24), "dri {dri}");
    }
}

/// A restart marker with the WRONG RSTn index in the stream: the decoder
/// expects RST0,RST1,... in sequence and must reject a mismatch by name.
#[test]
fn wrong_restart_marker_index_rejected() {
    let mut j = FlatJpeg::grayscale(24, 24);
    j.dri = 1;
    let mut bytes = j.build();
    // Find the first RST marker (FF D0) in the entropy and bump it to a
    // wrong index (FF D5).
    if let Some(p) = bytes
        .windows(2)
        .position(|w| w[0] == 0xFF && (0xD0..=0xD7).contains(&w[1]))
    {
        bytes[p + 1] = 0xD5;
        assert!(jpeg::decode(&bytes).is_err(), "wrong RST index accepted");
    } else {
        panic!("no restart marker found in a DRI=1 stream");
    }
}

/// Sampling-factor lies beyond 1..=2 are rejected (the general MCU walk
/// only supports up to 2x2); a 3x sampling factor must be a named error.
#[test]
fn oversized_sampling_factor_rejected() {
    let mut j = FlatJpeg::color444(16, 16);
    j.components[0] = CompSpec {
        id: 1,
        h: 3,
        v: 1,
        quant_id: 0,
        dc_id: 0,
        ac_id: 0,
    };
    let err = jpeg::decode(&j.build()).unwrap_err();
    assert!(err.to_string().contains("sampling factor"), "{err}");
}

/// The property net: 500+ seeded mutations of the flat builder's output —
/// header field stomps, table-length lies, entropy corruption, segment
/// splices — every one decode-or-reject, never panic. Runs both the
/// grayscale and color444 bases and a deep-tree base (the build path most
/// likely to mishandle a mutated count).
#[test]
fn seeded_pathological_corpus_never_panics() {
    let bases: Vec<Vec<u8>> = vec![
        FlatJpeg::grayscale(16, 16).build(),
        FlatJpeg::color444(24, 16).build(),
        FlatJpeg::grayscale(16, 16).with_flat_code_len(16).build(),
        FlatJpeg::color444(16, 16).with_flat_code_len(12).build(),
    ];
    let mut rng = Rng::new(0x0000_FEE1_DEAD_BEEF);
    let mut decoded_ok = 0usize;
    let mut rejected = 0usize;
    for _ in 0..600 {
        let base = &bases[rng.below(bases.len())];
        let mut b = base.clone();
        match rng.below(6) {
            0 => {
                // Stomp a run of bytes anywhere.
                for _ in 0..1 + rng.below(6) {
                    let off = rng.below(b.len());
                    b[off] ^= (rng.below(255) as u8) | 1;
                }
            }
            1 => {
                // Truncate.
                let cut = rng.below(b.len());
                b.truncate(cut);
            }
            2 => {
                // Target a segment length field: find a FF marker with a
                // length and lie about it.
                if let Some(p) = find_length_marker(&b, &mut rng) {
                    b[p + 2] = rng.below(255) as u8;
                    b[p + 3] = rng.below(255) as u8;
                }
            }
            3 => {
                // Corrupt the SOF dimensions.
                if let Some(sof) = b
                    .windows(2)
                    .position(|w| w[0] == 0xFF && (w[1] == 0xC0 || w[1] == 0xC1))
                {
                    for i in 0..4 {
                        if sof + 5 + i < b.len() {
                            b[sof + 5 + i] = rng.below(255) as u8;
                        }
                    }
                }
            }
            4 => {
                // Splice garbage into the entropy stream.
                let at = rng.below(b.len());
                let g: Vec<u8> = (0..rng.below(24)).map(|_| rng.below(255) as u8).collect();
                b.splice(at..at, g);
            }
            _ => {
                // Flip a marker byte into another marker class.
                if let Some(p) = b.windows(2).position(|w| w[0] == 0xFF && w[1] >= 0xC0) {
                    b[p + 1] = 0xC0 + (rng.below(0x30) as u8);
                }
            }
        }
        match jpeg::decode(&b) {
            Ok(img) => {
                // A decode that claims success must produce a sane bitmap
                // (dimensions within the builder's originals, non-empty).
                assert!(img.width() > 0 && img.height() > 0, "zero-dim success");
                assert!(
                    img.width() <= 64 && img.height() <= 64,
                    "absurd dims from mutation"
                );
                decoded_ok += 1;
            }
            Err(_) => rejected += 1,
        }
    }
    eprintln!("jpeg corpus: {decoded_ok} decoded, {rejected} rejected, 0 panics (600 cases)");
    assert_eq!(decoded_ok + rejected, 600);
}

/// Find a length-carrying marker (skips SOI/EOI/RSTn/standalone) and
/// return its `FF` offset, or None.
fn find_length_marker(b: &[u8], rng: &mut Rng) -> Option<usize> {
    let mut positions = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == 0xFF {
            let m = b[i + 1];
            let has_len = !matches!(m, 0x00 | 0xFF | 0xD8 | 0xD9 | 0x01 | 0xD0..=0xD7);
            if has_len && i + 3 < b.len() {
                positions.push(i);
            }
        }
        i += 1;
    }
    if positions.is_empty() {
        None
    } else {
        Some(positions[rng.below(positions.len())])
    }
}
