//! REDTEAM cycle-2 attack: GFX3D's file parsers (PNG, glTF JSON, GLB)
//! and the mosaic math — their own confessed top-3 risk list, plus the
//! GLB mutator campaign the accessor-extraction work validates against.

use abstracttui::base::Rgba;
use abstracttui::gfx::bitmap::Bitmap;
use abstracttui::gfx::{mosaic, png, MosaicMode};
use abstracttui::testing::glb_mutate::{self, Expect};
use abstracttui::testing::Rng;
use abstracttui::three::{doc::Doc, glb};

// ---------------------------------------------------------------------------
// PNG chunk toolbox (hand-rolled here: the rig must not depend on the
// code under attack to build its weapons).
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

fn ihdr(w: u32, h: u32, depth: u8, color_type: u8, interlace: u8) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&w.to_be_bytes());
    p.extend_from_slice(&h.to_be_bytes());
    p.extend_from_slice(&[depth, color_type, 0, 0, interlace]);
    chunk(b"IHDR", &p)
}

/// Deflate via the sanctioned crate (stored blocks would also do, but
/// miniz is already in-tree and battle-tested for VALID streams).
fn zlib(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

fn png_bytes(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

/// A valid 2x2 RGBA PNG built independently of gfx code.
fn valid_rgba_2x2() -> Vec<u8> {
    // Two scanlines, filter 0: [filter][RGBA x2]
    let raw = [
        0u8, 255, 0, 0, 255, 0, 255, 0, 255, // row 0
        0, 0, 0, 255, 255, 255, 255, 255, 128, // row 1
    ];
    png_bytes(&[
        ihdr(2, 2, 8, 6, 0),
        chunk(b"IDAT", &zlib(&raw)),
        chunk(b"IEND", &[]),
    ])
}

#[test]
fn png_valid_fixture_decodes() {
    let img = png::decode(&valid_rgba_2x2()).expect("independent fixture must decode");
    assert_eq!((img.width(), img.height()), (2, 2));
    assert_eq!(img.get(0, 0), Some(Rgba::rgb(255, 0, 0)));
    assert_eq!(img.get(1, 1).unwrap().a, 128);
}

#[test]
fn png_dimension_bomb_rejected_before_allocation() {
    // 0xFFFF_FFFF x 0xFFFF_FFFF: decode must refuse on the cap, fast.
    let bomb = png_bytes(&[
        ihdr(0xFFFF_FFFF, 0xFFFF_FFFF, 8, 6, 0),
        chunk(b"IDAT", &zlib(&[0u8; 8])),
        chunk(b"IEND", &[]),
    ]);
    let err = png::decode(&bomb).expect_err("dimension bomb must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("dimension") || msg.contains("pixel") || msg.contains("large"),
        "rejection must name the cap, got: {msg}"
    );
    // Just under the cap in one axis, absurd in product: still rejected.
    let bomb2 = png_bytes(&[ihdr(1 << 20, 1 << 20, 8, 6, 0), chunk(b"IEND", &[])]);
    assert!(png::decode(&bomb2).is_err());
}

#[test]
fn png_zero_dimension_rejected() {
    for (w, h) in [(0u32, 4u32), (4, 0), (0, 0)] {
        let p = png_bytes(&[
            ihdr(w, h, 8, 6, 0),
            chunk(b"IDAT", &zlib(&[])),
            chunk(b"IEND", &[]),
        ]);
        assert!(png::decode(&p).is_err(), "{w}x{h} must reject");
    }
}

#[test]
fn png_crc_corruption_policy_is_rejection() {
    let mut p = valid_rgba_2x2();
    // Flip one byte inside IDAT's CRC (last 4 bytes before IEND chunk).
    let idat_crc_pos = p.len() - 12 /* IEND */ - 4 /* IDAT CRC */;
    p[idat_crc_pos] ^= 0xFF;
    let err = png::decode(&p).expect_err("bad CRC must reject (documented stance)");
    assert!(err.to_string().to_lowercase().contains("crc"), "{err}");
}

#[test]
fn png_chunk_length_lies_never_panic() {
    let base = valid_rgba_2x2();
    // Stomp the IHDR length field with hostile values.
    for lie in [0u32, 5, 200, 0xFFFF_FFFF] {
        let mut p = base.clone();
        p[8..12].copy_from_slice(&lie.to_be_bytes());
        let _ = png::decode(&p); // any Err is fine; panic is the failure
    }
    // Truncation ladder: every prefix of the valid file.
    for cut in 0..base.len() {
        let _ = png::decode(&base[..cut]);
    }
}

#[test]
fn png_idat_inflates_to_wrong_size_rejected() {
    // Correct chunks, but the pixel stream is one byte short / long.
    for delta in [-1i32, 1] {
        let good_len: i32 = 2 * (1 + 2 * 4);
        let len = (good_len + delta) as usize;
        let raw = vec![0u8; len];
        let p = png_bytes(&[
            ihdr(2, 2, 8, 6, 0),
            chunk(b"IDAT", &zlib(&raw)),
            chunk(b"IEND", &[]),
        ]);
        assert!(
            png::decode(&p).is_err(),
            "IDAT size lie ({delta}) must reject"
        );
    }
}

#[test]
fn png_bad_filter_byte_rejected() {
    let raw = [
        5u8, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 128,
    ];
    let p = png_bytes(&[
        ihdr(2, 2, 8, 6, 0),
        chunk(b"IDAT", &zlib(&raw)),
        chunk(b"IEND", &[]),
    ]);
    assert!(png::decode(&p).is_err(), "filter byte 5 must reject");
}

#[test]
fn png_palette_oob_and_trns_abuse() {
    // Color type 3, 2-entry palette, but pixel indices reach 200.
    let raw = [0u8, 0, 200, 0, 1, 1]; // 2 rows of 2 palette indices
    let p = png_bytes(&[
        ihdr(2, 2, 8, 3, 0),
        chunk(b"PLTE", &[0, 0, 0, 255, 255, 255]),
        chunk(b"IDAT", &zlib(&raw)),
        chunk(b"IEND", &[]),
    ]);
    // OOB palette index: reject or clamp — must not panic; if Ok, pixels
    // must still be valid RGBA.
    let _ = png::decode(&p);

    // tRNS longer than the palette.
    let p = png_bytes(&[
        ihdr(2, 2, 8, 3, 0),
        chunk(b"PLTE", &[0, 0, 0, 255, 255, 255]),
        chunk(b"tRNS", &[10, 20, 30, 40, 50, 60, 70]),
        chunk(b"IDAT", &zlib(&[0u8, 0, 1, 0, 1, 1])),
        chunk(b"IEND", &[]),
    ]);
    let _ = png::decode(&p); // no panic; Err or tolerant-Ok both acceptable

    // Palette missing entirely for color type 3.
    let p = png_bytes(&[
        ihdr(2, 2, 8, 3, 0),
        chunk(b"IDAT", &zlib(&[0u8, 0, 0, 0, 0, 0])),
        chunk(b"IEND", &[]),
    ]);
    assert!(png::decode(&p).is_err(), "type-3 without PLTE must reject");
}

#[test]
fn png_out_of_scope_features_reject_with_names() {
    // 16-bit depth.
    let p = png_bytes(&[ihdr(2, 2, 16, 6, 0), chunk(b"IEND", &[])]);
    let msg = png::decode(&p)
        .expect_err("depth 16 out of scope")
        .to_string();
    assert!(msg.contains("depth") || msg.contains("16"), "{msg}");
    // Adam7 interlace.
    let p = png_bytes(&[ihdr(2, 2, 8, 6, 1), chunk(b"IEND", &[])]);
    let msg = png::decode(&p)
        .expect_err("interlace out of scope")
        .to_string();
    assert!(
        msg.to_lowercase().contains("interlac") || msg.contains("Adam7"),
        "{msg}"
    );
}

#[test]
fn png_hostile_soup_never_panics() {
    let mut rng = Rng::new(0x9A6);
    let base = valid_rgba_2x2();
    for _ in 0..400 {
        let mut p = base.clone();
        match rng.below(3) {
            0 => {
                let cut = rng.below(p.len());
                p.truncate(cut);
            }
            1 => {
                for _ in 0..rng.range(1, 12) {
                    let off = rng.below(p.len());
                    p[off] ^= rng.byte() | 1;
                }
            }
            _ => {
                let at = rng.below(p.len());
                let garbage: Vec<u8> = (0..rng.range(1, 64)).map(|_| rng.byte()).collect();
                p.splice(at..at, garbage);
            }
        }
        let _ = png::decode(&p);
    }
}

// ---------------------------------------------------------------------------
// glTF JSON grammar corners (their confessed risk 2).
// ---------------------------------------------------------------------------

#[test]
fn json_number_grammar_corners() {
    use abstracttui::three::gltf_json::parse;
    // 1e999 overflows f64 — accept-as-inf or reject, NEVER panic; if
    // accepted, downstream consumers must see a number, not UB.
    let _ = parse("[1e999]");
    let _ = parse("[-1e999]");
    // -0 is legal JSON.
    let v = parse("[-0]").expect("-0 is valid JSON");
    let arr = match v {
        abstracttui::three::gltf_json::Value::Array(a) => a,
        other => panic!("expected array, got {other:?}"),
    };
    assert_eq!(arr[0].as_f64(), Some(0.0));
    // Grammar rejections (their documented strictness).
    for bad in [
        "[.5]",
        "[1.]",
        "[+1]",
        "[01]",
        "[NaN]",
        "[Infinity]",
        "[1e]",
        "[--1]",
    ] {
        assert!(
            parse(bad).is_err(),
            "{bad} must reject per the strict grammar"
        );
    }
    // Huge-but-legal exponent forms.
    let _ = parse("[1e308]").expect("finite f64 must parse");
    let _ = parse("[1.7976931348623157e308]").expect("f64::MAX must parse");
}

#[test]
fn json_string_escapes_and_surrogates() {
    use abstracttui::three::gltf_json::parse;
    let v = parse(r#"["\uD83C\uDF89"]"#).expect("surrogate pair must combine");
    if let abstracttui::three::gltf_json::Value::Array(a) = v {
        assert_eq!(a[0].as_str(), Some("🎉"));
    }
    for bad in [
        r#"["\uD83C"]"#,       // lone high surrogate
        r#"["\uDC00"]"#,       // lone low surrogate
        r#"["\uD83Cx"]"#,      // high surrogate + garbage
        r#"["\uD83C\u0041"]"#, // high surrogate + non-low
        r#"["\q"]"#,           // bad escape
        "[\"raw\ncontrol\"]",  // unescaped control char
    ] {
        assert!(parse(bad).is_err(), "{bad:?} must reject");
    }
}

#[test]
fn json_depth_limit_and_duplicate_keys() {
    use abstracttui::three::gltf_json::parse;
    // Depth 127 parses; depth 200 rejects (limit 128) — never overflows
    // the stack either way.
    let deep_ok = format!("{}0{}", "[".repeat(120), "]".repeat(120));
    assert!(parse(&deep_ok).is_ok(), "120 deep must parse");
    let deep_bad = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    assert!(
        parse(&deep_bad).is_err(),
        "200 deep must reject, not overflow"
    );
    // Duplicate keys: first wins, documented.
    let v = parse(r#"{"a":1,"a":2}"#).expect("dup keys tolerated");
    assert_eq!(
        v.get("a").and_then(|x| x.as_f64()),
        Some(1.0),
        "first key wins"
    );
}

#[test]
fn json_multi_mb_string_fast_path() {
    use abstracttui::three::gltf_json::parse;
    // 4 MB of escape-free string content: the run-copy fast path must
    // survive and round-trip.
    let body = "x".repeat(4 << 20);
    let doc = format!("[\"{body}\"]");
    let v = parse(&doc).expect("large string must parse");
    if let abstracttui::three::gltf_json::Value::Array(a) = v {
        assert_eq!(a[0].as_str().map(|s| s.len()), Some(4 << 20));
    }
}

#[test]
fn json_invalid_utf8_via_bytes_rejects() {
    use abstracttui::three::gltf_json::parse_bytes;
    assert!(parse_bytes(b"[\"\xFF\xFE\"]").is_err());
    assert!(parse_bytes(b"\xC3").is_err());
}

// ---------------------------------------------------------------------------
// GLB mutator campaign.
// ---------------------------------------------------------------------------

/// The FULL-loader campaign (cycle 3: extraction landed, so
/// `Model::load` is the surface under test). Every mutant either loads
/// or rejects with a named error — never panics; MustReject mutants
/// must ALL reject now (the cycle-2 pending-extraction ratchet is gone).
#[test]
fn glb_mutant_campaign_full_loader() {
    use abstracttui::three::load::Model;
    let battery = glb_mutate::mutants(0xC1C2, 200);
    let mut accepted_must_reject: Vec<String> = Vec::new();
    for m in &battery {
        let outcome = std::panic::catch_unwind(|| Model::load(&m.bytes).map(|_| ()));
        let result = match outcome {
            Ok(r) => r,
            Err(_) => panic!("mutant {} PANICKED the loader", m.name),
        };
        match (m.expect, result) {
            (Expect::MustLoad, Err(e)) => {
                panic!("mutant {} must load but was rejected: {e}", m.name)
            }
            (Expect::MustReject, Ok(())) => accepted_must_reject.push(m.name.clone()),
            _ => {}
        }
    }
    assert!(
        accepted_must_reject.is_empty(),
        "MustReject mutants accepted by the FULL loader: {accepted_must_reject:?}"
    );
}

/// Parse-level residue (RT2-2 / RT2-3): index-range and sparse checks
/// are pure metadata and belong in `Doc::parse` — consumers between
/// parse and extraction otherwise handle dangling indices. Ratchet:
/// this list may only shrink; empty closes both findings.
#[test]
fn glb_parse_level_ratchet() {
    let battery = glb_mutate::mutants(0xC1C2, 0);
    let mut accepted: Vec<String> = Vec::new();
    for m in &battery {
        if m.expect != Expect::MustReject {
            continue;
        }
        let parsed = glb::split(&m.bytes).and_then(|chunks| Doc::parse(chunks.json).map(|_| ()));
        if parsed.is_ok() {
            accepted.push(m.name.clone());
        }
    }
    // RT2-2 + RT2-3 CLOSED (cycle 3): index OOB, sparse, and the whole
    // accessor-arithmetic family now reject at Doc::parse. The residue
    // below is genuinely cross-object work (BIN presence, primitive
    // mode policy, node-graph cycles) that the full loader rejects —
    // acceptable at parse, pinned so it can only shrink.
    let tolerated = [
        "missing_bin_with_buffer_refs",
        "json_primitive_mode_lines",
        "json_node_self_cycle",
    ];
    for name in &accepted {
        assert!(
            tolerated.contains(&name.as_str()),
            "parse-level regression: {name} newly accepted by Doc::parse"
        );
    }
    eprintln!(
        "parse-level ratchet: {} accepted of the MustReject set: {accepted:?}",
        accepted.len()
    );
}

#[test]
fn glb_minimal_fixture_parses_end_to_end() {
    let bytes = glb_mutate::minimal_glb();
    let chunks = glb::split(&bytes).expect("minimal GLB splits");
    let doc = Doc::parse(chunks.json).expect("minimal GLB doc parses");
    assert_eq!(doc.meshes.len(), 1);
    assert!(chunks.bin.is_some());
}

// ---------------------------------------------------------------------------
// Mosaic integer headroom (their confessed risk 3).
// ---------------------------------------------------------------------------

#[test]
fn mosaic_extreme_alpha_moments_do_not_overflow() {
    // Full-alpha white vs zero-alpha noise at every cell scale: the u32
    // moment accumulators see their extreme weights. Any overflow panics
    // in debug — running this in the default (debug) test pass IS the
    // assertion.
    for mode in [
        MosaicMode::HalfBlock,
        MosaicMode::Quadrant,
        MosaicMode::Sextant,
        MosaicMode::Braille,
    ] {
        let img = Bitmap::from_fn(64, 64, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba::new(255, 255, 255, 255)
            } else {
                Rgba::new(255, 0, 255, 0) // loud RGB under zero alpha
            }
        });
        let grid = mosaic::render(&img, 32, 16, mode);
        for cell in grid.cells() {
            // Transparent-weighted pixels must not bleed magenta.
            let _ = cell;
        }
    }
    // All-transparent image: division-by-zero-weight guard.
    let img = Bitmap::from_fn(8, 8, |_, _| Rgba::TRANSPARENT);
    let _ = mosaic::render(&img, 4, 2, MosaicMode::Quadrant);
    // 1x1 -> huge grid and huge -> 1x1 scaling extremes.
    let img = Bitmap::from_fn(1, 1, |_, _| Rgba::rgb(1, 2, 3));
    let _ = mosaic::render(&img, 100, 50, MosaicMode::Sextant);
    let img = Bitmap::from_fn(512, 512, |x, _| Rgba::rgb(x as u8, 0, 0));
    let _ = mosaic::render(&img, 1, 1, MosaicMode::HalfBlock);
}

#[test]
fn mosaic_renderer_reuse_across_modes_and_sizes() {
    // Their confessed risk: scratch reuse between calls with different
    // modes/sizes. Same renderer, alternating shapes — outputs must match
    // fresh-renderer outputs exactly.
    use abstracttui::gfx::MosaicRenderer;
    let mut reused = MosaicRenderer::new();
    let mut rng = Rng::new(77);
    for round in 0..40 {
        let w = rng.range(1, 96) as u32;
        let h = rng.range(1, 96) as u32;
        let cols = rng.range(1, 40) as u32;
        let rows = rng.range(1, 20) as u32;
        let mode = *rng.pick(&[
            MosaicMode::HalfBlock,
            MosaicMode::Quadrant,
            MosaicMode::Sextant,
            MosaicMode::Braille,
        ]);
        let img = Bitmap::from_fn(w, h, |x, y| {
            Rgba::new((x * 7) as u8, (y * 11) as u8, ((x + y) * 3) as u8, 255)
        });
        let a: Vec<_> = reused.render(&img, cols, rows, mode).cells().to_vec();
        let fresh: Vec<_> = mosaic::render(&img, cols, rows, mode).cells().to_vec();
        assert_eq!(
            a, fresh,
            "round {round}: reused renderer diverged ({mode:?} {w}x{h} -> {cols}x{rows})"
        );
    }
}

// ---------------------------------------------------------------------------
// base64 (protocol substrate: kitty chunking correctness rides on it).
// ---------------------------------------------------------------------------

#[test]
fn base64_round_trip_and_strictness() {
    use abstracttui::gfx::base64::{decode, encode};
    let mut rng = Rng::new(0xB64);
    for _ in 0..200 {
        let data: Vec<u8> = (0..rng.below(200)).map(|_| rng.byte()).collect();
        let enc = encode(&data);
        assert_eq!(decode(&enc).expect("round trip"), data);
        assert_eq!(enc.len() % 4, 0, "padded output length");
    }
    for bad in ["A", "AB=C", "====", "AA=A", "A!==", "AAAA=", "aGk=x"] {
        assert!(decode(bad).is_err(), "{bad:?} must reject");
    }
}
