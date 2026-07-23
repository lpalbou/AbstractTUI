//! Header-only image dimension probing (0144): answer "how big is this
//! image?" WITHOUT running the decoder — the markdown image block needs
//! sizes at typeset time while full decode stays lazy (first draw).
//!
//! Bytes in, dimensions out; magic-routed like
//! [`decode_image`](crate::gfx::decode_image). A probe success is NOT
//! a decode guarantee (a
//! well-formed header can front a truncated body — the decoder stays
//! the authority and its failure is reported at draw time); a probe
//! FAILURE on the formats we decode is definitive. Correctness is
//! test-pinned against the real decoders: on every fixture both
//! decoders accept, probe and decode must report identical sizes.
//!
//! OWNER: READER (app-widgets wave 3).

use super::decode::{sniff_format, ImageFormat};

/// Dimensions `(width, height)` in pixels from container headers only.
/// `None`: not a PNG/JPEG stream, or the header region is truncated or
/// malformed. Never panics on any byte soup (fuzz-pinned); performs no
/// allocation and reads no further than it must (PNG: the IHDR chunk;
/// JPEG: the marker walk up to the first frame header).
///
/// ```
/// use abstracttui::gfx::probe_dimensions;
/// assert_eq!(probe_dimensions(b"not an image"), None);
/// ```
pub fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match sniff_format(bytes)? {
        ImageFormat::Png => png_dimensions(bytes),
        ImageFormat::Jpeg => jpeg_dimensions(bytes),
    }
}

/// PNG: the IHDR chunk is mandated first — width/height are the first
/// eight payload bytes after the 8-byte magic + 4-byte length + "IHDR".
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // magic(8) len(4) type(4) width(4) height(4) = 24 bytes minimum.
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let len = u32::from_be_bytes(bytes[8..12].try_into().ok()?);
    if len < 13 {
        return None; // IHDR payload is exactly 13 bytes by spec
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    // Zero dimensions are invalid per spec; the decoder rejects them
    // too — report "no usable size" instead of a lying (0, 0).
    if w == 0 || h == 0 {
        None
    } else {
        Some((w, h))
    }
}

/// JPEG: walk the marker stream to the first frame header (SOF0-15,
/// minus DHT/JPG/DAC which share the C-range but are not frames) and
/// read height/width from its payload. Mirrors the decoder's marker
/// discipline: fill bytes skipped, standalone markers have no length,
/// segment lengths include their own two bytes.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2; // past SOI (sniff verified FF D8)
    loop {
        // Find the next marker: skip anything up to 0xFF, then skip
        // fill bytes (repeated 0xFF).
        while *bytes.get(i)? != 0xFF {
            i += 1;
        }
        while *bytes.get(i)? == 0xFF {
            i += 1;
        }
        let marker = *bytes.get(i)?; // first non-FF byte
        i += 1;
        match marker {
            // Standalone: no length payload.
            0x01 | 0xD0..=0xD7 => continue,
            // EOI or SOS before any SOF: no frame header to read.
            0xD9 | 0xDA => return None,
            // Frame headers (SOF0..SOF15 except DHT C4 / JPG C8 / DAC CC).
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                // len(2) precision(1) height(2) width(2)
                let seg = bytes.get(i..i + 7)?;
                let len = u16::from_be_bytes([seg[0], seg[1]]);
                if len < 9 {
                    return None; // frame payload cannot fit
                }
                let h = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                let w = u16::from_be_bytes([seg[5], seg[6]]) as u32;
                // Height 0 is legal mid-stream (DNL-deferred) but
                // useless for layout; treat as unprobeable.
                return if w == 0 || h == 0 { None } else { Some((w, h)) };
            }
            _ => {
                // Skip a length-prefixed segment.
                let seg = bytes.get(i..i + 2)?;
                let len = u16::from_be_bytes([seg[0], seg[1]]) as usize;
                if len < 2 {
                    return None; // malformed length
                }
                i += len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;
    use crate::gfx::decode::decode_image;
    use crate::gfx::{jpeg_fixtures, png_test_encoder};
    use crate::testing::hostile_corpus;

    /// THE correctness pin: on every container the real decoders
    /// accept, the probe reports exactly the decoded dimensions.
    #[test]
    fn probe_matches_decoder_dimensions() {
        for (w, h) in [(1u32, 1u32), (3, 2), (17, 9), (64, 48)] {
            let bmp = crate::gfx::Bitmap::from_fn(w, h, |x, y| {
                Rgba::rgb((x * 31) as u8, (y * 17) as u8, 99)
            });
            let png = png_test_encoder::encode_rgba(&bmp);
            assert_eq!(probe_dimensions(&png), Some((w, h)), "png {w}x{h}");
        }
        for jpg in [jpeg_fixtures::GRAD444, jpeg_fixtures::GRAD420] {
            let decoded = decode_image(jpg).expect("fixture decodes");
            assert_eq!(
                probe_dimensions(jpg),
                Some((decoded.width(), decoded.height())),
                "jpeg probe must match the decoder"
            );
        }
    }

    #[test]
    fn rejects_non_images_and_zero_dimensions() {
        assert_eq!(probe_dimensions(b""), None);
        assert_eq!(probe_dimensions(b"GIF89a...."), None);
        assert_eq!(probe_dimensions(b"\x89PNG\r\n\x1a\n"), None, "magic only");
        // A PNG header claiming 0 width: probeable bytes, useless size.
        let mut zero = Vec::new();
        zero.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
        zero.extend_from_slice(&13u32.to_be_bytes());
        zero.extend_from_slice(b"IHDR");
        zero.extend_from_slice(&0u32.to_be_bytes());
        zero.extend_from_slice(&7u32.to_be_bytes());
        assert_eq!(probe_dimensions(&zero), None);
    }

    /// Hostile fuzz: byte soup, truncation ladders over real
    /// containers, magic-stamped garbage — never a panic; when the
    /// probe answers on a decodable input, it answers truthfully
    /// (checked against the decoder wherever the decoder accepts).
    #[test]
    fn probe_survives_hostile_input_and_truncation() {
        for chunk in hostile_corpus(0x0144, 300) {
            let _ = probe_dimensions(&chunk);
        }
        let bmp = crate::gfx::Bitmap::from_fn(9, 7, |x, y| {
            Rgba::rgb((x * 29) as u8, (y * 37) as u8, 128)
        });
        let png = png_test_encoder::encode_rgba(&bmp);
        for src in [&png[..], jpeg_fixtures::GRAD444] {
            for cut in 0..src.len() {
                let probed = probe_dimensions(&src[..cut]);
                if let (Some(p), Ok(d)) = (probed, decode_image(&src[..cut])) {
                    assert_eq!(p, (d.width(), d.height()), "truncation at {cut}");
                }
            }
        }
        // Marker soup with real magics stamped on random bytes.
        let mut rng = crate::testing::Rng::new(0x0144_0144);
        for case in 0..300 {
            let len = 8 + rng.below(200);
            let mut bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
            if case % 2 == 0 {
                bytes[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
            } else {
                bytes[0] = 0xFF;
                bytes[1] = 0xD8;
            }
            let _ = probe_dimensions(&bytes);
        }
    }
}
