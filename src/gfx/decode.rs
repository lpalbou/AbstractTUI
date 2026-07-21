//! One-call image decoding: sniff the magic bytes, route to the right
//! decoder. DESIGN's request (cycle 6) so `image.rs`/`images.rs` and
//! the GLB texture path share exactly one entry.
//!
//! The MAGIC decides, never a caller-supplied MIME string: containers
//! lie, bytes don't. Unknown formats reject by name, listing what the
//! engine actually decodes — a caller can show that message verbatim.
//!
//! OWNER: GFX3D.

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::{jpeg, png};

/// PNG signature (8 bytes) — the full spec magic, not just `\x89PNG`.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// The format `decode_image` recognized (or would recognize).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

/// Sniff the container format from leading bytes. `None` = neither a
/// PNG nor a JPEG stream.
pub fn sniff_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&PNG_MAGIC) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        // JPEG SOI marker. The third byte is the next marker's 0xFF —
        // not required here: truncated-after-SOI data should still
        // route to the JPEG decoder and fail with ITS named error.
        Some(ImageFormat::Jpeg)
    } else {
        None
    }
}

/// Decode PNG or JPEG bytes into an RGBA bitmap. Rejects other
/// formats by name; decoder errors pass through unwrapped (they are
/// already named and prefixed).
///
/// ```
/// use abstracttui::base::Rgba;
/// use abstracttui::gfx::{decode_image, png_encode, Bitmap};
///
/// let img = Bitmap::from_fn(2, 2, |x, y| Rgba::rgb((x * 200) as u8, (y * 200) as u8, 40));
/// let png_bytes = png_encode::encode(&img);
/// let decoded = decode_image(&png_bytes).unwrap();
/// assert_eq!(decoded.get(1, 1), img.get(1, 1));
///
/// // Unknown formats reject by NAME (never a panic), telling the
/// // caller what DOES decode:
/// let err = decode_image(b"GIF89a....").unwrap_err();
/// assert!(err.to_string().contains("PNG"));
/// ```
pub fn decode_image(bytes: &[u8]) -> Result<Bitmap> {
    match sniff_format(bytes) {
        Some(ImageFormat::Png) => png::decode(bytes),
        Some(ImageFormat::Jpeg) => jpeg::decode(bytes),
        None => Err(Error::Parse(format!(
            "image: unrecognized format (magic {:02X?}); PNG and baseline JPEG decode, \
             GIF/WebP/AVIF/TIFF do not",
            &bytes[..bytes.len().min(4)]
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::png_test_encoder::encode_rgba;

    #[test]
    fn routes_png_by_magic() {
        let bmp = Bitmap::from_fn(3, 2, |x, y| {
            crate::base::Rgba::rgb((x * 80) as u8, (y * 100) as u8, 7)
        });
        let png = encode_rgba(&bmp);
        let out = decode_image(&png).unwrap();
        assert_eq!((out.width(), out.height()), (3, 2));
        assert_eq!(out.get(1, 1), bmp.get(1, 1));
    }

    #[test]
    fn routes_jpeg_by_magic() {
        // Any embedded fixture: decoding succeeds through the sniffer.
        let jpg = crate::gfx::jpeg_fixtures::GRAD444;
        assert_eq!(sniff_format(jpg), Some(ImageFormat::Jpeg));
        let out = decode_image(jpg).unwrap();
        assert!(out.width() > 0 && out.height() > 0);
    }

    #[test]
    fn unknown_magic_rejects_by_name() {
        let err = decode_image(b"GIF89a....").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unrecognized format"), "{msg}");
        assert!(msg.contains("PNG"), "must name what DOES decode: {msg}");
        // Empty input: same named rejection, no panic.
        let err = decode_image(b"").unwrap_err();
        assert!(err.to_string().contains("unrecognized format"));
    }

    #[test]
    fn truncated_after_magic_fails_in_the_decoder_not_the_sniffer() {
        let err = decode_image(&[0xFF, 0xD8, 0xFF]).unwrap_err();
        // The JPEG decoder's own named error, not "unrecognized".
        assert!(!err.to_string().contains("unrecognized"), "{err}");
    }

    /// Cycle-7 hardening pass on the UNIFIED entry: the per-decoder
    /// fuzz suites cover png/jpeg internals; this drives the same
    /// hostile classes through the routing layer so a sniff/route bug
    /// can never panic either. Every outcome is Ok or Err — reaching
    /// the end IS the assertion.
    #[test]
    fn decode_image_survives_truncation_and_marker_soup() {
        // Truncation ladder over real containers, byte by byte for the
        // header region then strided for the body.
        let png = encode_rgba(&Bitmap::from_fn(9, 7, |x, y| {
            crate::base::Rgba::rgb((x * 29) as u8, (y * 37) as u8, 128)
        }));
        let jpg = crate::gfx::jpeg_fixtures::GRAD420;
        for src in [&png[..], jpg] {
            for cut in 0..src.len().min(96) {
                let _ = decode_image(&src[..cut]);
            }
            let mut cut = 96;
            while cut < src.len() {
                let _ = decode_image(&src[..cut]);
                cut += 7;
            }
        }

        // Marker soup: xorshift bytes stamped with each magic, plus
        // bit-flip mutations of the real containers (seeded, so a
        // failure reproduces by index).
        let mut state = 0xDEADBEEFCAFEF00Du64;
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for case in 0..300 {
            let len = 8 + (rng() % 300) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| rng() as u8).collect();
            match case % 3 {
                0 => bytes[..8].copy_from_slice(&PNG_MAGIC),
                1 => {
                    bytes[0] = 0xFF;
                    bytes[1] = 0xD8;
                }
                _ => {}
            }
            let _ = decode_image(&bytes);
        }
        for (i, src) in [&png[..], jpg].into_iter().enumerate() {
            for k in 0..200 {
                let mut mutated = src.to_vec();
                let pos = (rng() as usize) % mutated.len();
                mutated[pos] ^= 1 << (rng() % 8);
                // Sniff may now say "unrecognized" (magic flipped) or a
                // decoder error, or even Ok (benign flip): all fine.
                let _ = decode_image(&mutated);
                let _ = (i, k);
            }
        }
    }
}
