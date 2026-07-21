//! Minimal public PNG encoder: 8-bit RGBA, one IDAT, zlib via
//! miniz_oxide's deflate. Promoted from the cycle-1 test-only encoder
//! because the iTerm2 protocol carries real image *files* (not raw
//! pixels) and kitty `f=100` is the compact channel for flat-color
//! content.
//!
//! Scope is deliberately the mirror of the decoder's critical path:
//! always color type 6 (RGBA8) + filter 0. Filter choice heuristics
//! (Sub/Up/Paeth per row) trade encode time for a few percent of
//! payload on photographic content; terminal images are small and the
//! deflate layer already absorbs most of the redundancy, so v1 keeps
//! the encoder byte-deterministic and trivially auditable instead.

use crate::gfx::bitmap::Bitmap;
use crate::gfx::png::{crc32, SIGNATURE};

/// IHDR payload for the given geometry (compression/filter/interlace 0).
pub(crate) fn ihdr_payload(w: u32, h: u32, depth: u8, color_type: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(&w.to_be_bytes());
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&[depth, color_type, 0, 0, 0]);
    out
}

/// Append one chunk (length + type + data + CRC over type+data).
pub(crate) fn push_chunk(out: &mut Vec<u8>, ctype: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ctype);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(ctype);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode a bitmap as an RGBA8 PNG. Deterministic: same pixels, same
/// bytes (fixed filter, fixed compression level).
pub fn encode(img: &Bitmap) -> Vec<u8> {
    let w = img.width();
    let h = img.height();
    // Raw scanlines: filter byte 0 + RGBA per pixel.
    let mut raw = Vec::with_capacity((h as usize) * (1 + 4 * w as usize));
    for y in 0..h {
        raw.push(0);
        for p in img.row(y) {
            raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
    }
    let idat = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);

    let mut out = Vec::with_capacity(64 + idat.len());
    out.extend_from_slice(&SIGNATURE);
    push_chunk(&mut out, b"IHDR", &ihdr_payload(w, h, 8, 6));
    push_chunk(&mut out, b"IDAT", &idat);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;
    use crate::gfx::png;

    #[test]
    fn encode_decode_round_trip() {
        let img = Bitmap::from_fn(7, 5, |x, y| {
            Rgba::new(
                (x * 36) as u8,
                (y * 50) as u8,
                ((x + y) * 20) as u8,
                255 - (x * 30) as u8,
            )
        });
        let bytes = encode(&img);
        assert_eq!(png::decode(&bytes).unwrap(), img);
    }

    #[test]
    fn encode_is_deterministic() {
        let img = Bitmap::new(3, 3, Rgba::rgb(9, 8, 7));
        assert_eq!(encode(&img), encode(&img));
    }

    #[test]
    fn zero_sized_bitmap_rejected_by_decoder() {
        // The encoder emits whatever geometry it is given; a 0x0 PNG is
        // invalid per spec and OUR decoder refuses it — the encoder's
        // callers (protocol emitters) never emit empty bitmaps.
        let bytes = encode(&Bitmap::new(0, 0, Rgba::BLACK));
        assert!(png::decode(&bytes).is_err());
    }
}
