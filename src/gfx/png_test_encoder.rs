//! Test-only PNG encoder variants: filter-per-row + all color types,
//! for round-tripping the decoder and generating byte-exact fixtures.
//! The production RGBA8 encoder lives in `gfx::png_encode` (promoted
//! cycle 2); this module keeps only what tests need beyond it (forced
//! filters, gray/palette color types) and reuses its chunk helpers.

use crate::base::Rgba;
use crate::gfx::bitmap::Bitmap;
use crate::gfx::png::SIGNATURE;
// Re-exported so decoder tests keep addressing the chunk helpers
// through this module (they are test fixtures' vocabulary).
pub(crate) use crate::gfx::png_encode::{ihdr_payload, push_chunk};

/// Forward-apply a PNG filter to one scanline (the encoder direction:
/// filtered = raw − predictor, mod 256).
fn filter_row(filter: u8, cur: &[u8], prev: &[u8], bpp: usize, out: &mut Vec<u8>) {
    out.push(filter);
    for i in 0..cur.len() {
        let a = if i >= bpp { cur[i - bpp] } else { 0 };
        let b = prev.get(i).copied().unwrap_or(0);
        let c = if i >= bpp {
            prev.get(i - bpp).copied().unwrap_or(0)
        } else {
            0
        };
        let pred = match filter {
            0 => 0,
            1 => a,
            2 => b,
            3 => ((a as u16 + b as u16) / 2) as u8,
            4 => {
                // Same Paeth as the decoder — kept inline so the encoder
                // stays a self-contained fixture generator.
                let p = a as i32 + b as i32 - c as i32;
                let (pa, pb, pc) = (
                    (p - a as i32).abs(),
                    (p - b as i32).abs(),
                    (p - c as i32).abs(),
                );
                if pa <= pb && pa <= pc {
                    a
                } else if pb <= pc {
                    b
                } else {
                    c
                }
            }
            _ => panic!("test encoder: bad filter {filter}"),
        };
        out.push(cur[i].wrapping_sub(pred));
    }
}

fn assemble(
    w: u32,
    h: u32,
    color_type: u8,
    extra_chunks: &[(&[u8; 4], Vec<u8>)],
    raw_rows: &[u8],
    bpp: usize,
    filter: u8,
) -> Vec<u8> {
    let row_len = w as usize * bpp;
    let mut filtered = Vec::with_capacity(h as usize * (row_len + 1));
    for y in 0..h as usize {
        let cur = &raw_rows[y * row_len..(y + 1) * row_len];
        let prev = if y == 0 {
            &[][..]
        } else {
            &raw_rows[(y - 1) * row_len..y * row_len]
        };
        filter_row(filter, cur, prev, bpp, &mut filtered);
    }
    let idat = miniz_oxide::deflate::compress_to_vec_zlib(&filtered, 6);

    let mut out = Vec::new();
    out.extend_from_slice(&SIGNATURE);
    push_chunk(&mut out, b"IHDR", &ihdr_payload(w, h, 8, color_type));
    for (ctype, data) in extra_chunks {
        push_chunk(&mut out, ctype, data);
    }
    push_chunk(&mut out, b"IDAT", &idat);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

pub(crate) fn encode_rgb(img: &Bitmap) -> Vec<u8> {
    encode_rgb_with_filter(img, 0)
}

/// RGB with a forced filter type on every row — lets the decoder tests
/// exercise each unfilter branch deterministically.
pub(crate) fn encode_rgb_with_filter(img: &Bitmap, filter: u8) -> Vec<u8> {
    let mut raw = Vec::with_capacity(img.pixels().len() * 3);
    for p in img.pixels() {
        raw.extend_from_slice(&[p.r, p.g, p.b]);
    }
    assemble(img.width(), img.height(), 2, &[], &raw, 3, filter)
}

pub(crate) fn encode_rgba(img: &Bitmap) -> Vec<u8> {
    let mut raw = Vec::with_capacity(img.pixels().len() * 4);
    for p in img.pixels() {
        raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
    }
    assemble(img.width(), img.height(), 6, &[], &raw, 4, 0)
}

/// Grayscale from the red channel (callers pass gray bitmaps).
pub(crate) fn encode_gray(img: &Bitmap) -> Vec<u8> {
    let raw: Vec<u8> = img.pixels().iter().map(|p| p.r).collect();
    assemble(img.width(), img.height(), 0, &[], &raw, 1, 0)
}

/// Paletted image with optional tRNS alpha entries.
pub(crate) fn encode_palette(
    w: u32,
    h: u32,
    palette: &[Rgba],
    alphas: &[u8],
    indices: &[u8],
) -> Vec<u8> {
    assert_eq!(indices.len(), (w * h) as usize);
    let mut plte = Vec::with_capacity(palette.len() * 3);
    for p in palette {
        plte.extend_from_slice(&[p.r, p.g, p.b]);
    }
    let mut extra: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"PLTE", plte)];
    if !alphas.is_empty() {
        extra.push((b"tRNS", alphas.to_vec()));
    }
    assemble(w, h, 3, &extra, indices, 1, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_stream_is_wellformed() {
        // The encoder's own sanity: signature + IHDR-first + IEND-last.
        let img = Bitmap::new(2, 2, Rgba::rgb(1, 2, 3));
        let bytes = encode_rgb(&img);
        assert_eq!(&bytes[..8], &SIGNATURE);
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
    }
}
