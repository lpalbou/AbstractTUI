//! Kitty graphics protocol emitter (APC `ESC _ G <keys> ; <base64> ESC \`).
//!
//! Spec facts this module encodes (sw.kovidgoyal.net/kitty/graphics-protocol,
//! research record in docs/design/gfx-three.md §2.1):
//!
//! - Formats: `f=32` RGBA (default), `f=24` RGB, `f=100` PNG.
//! - Chunking: base64 payload split into chunks ≤ 4096 bytes; every
//!   chunk except the last must be a multiple of 4 encoded bytes;
//!   `m=1` on all but the last chunk (`m=0`). Control keys ride ONLY
//!   the first escape; continuations carry only `m` (and `q`).
//! - `q=2` suppresses both OK and error replies — the presenter fires
//!   and forgets; replies would land in the input stream as garbage.
//! - `c=`/`r=` on the display/placement scale the image into that many
//!   cells (the terminal scales; we skip client-side resampling and
//!   ship full fidelity).
//! - `o=z` marks the (pre-base64) payload as zlib deflate.
//! - Deletes: lowercase `d` values keep transmitted data, uppercase
//!   free it.

use crate::gfx::base64;
use crate::gfx::bitmap::Bitmap;
use crate::gfx::png_encode;

/// Max *encoded* bytes per APC escape payload (spec constant; 4096 is
/// a multiple of 4, so slicing whole-payload base64 at this boundary
/// keeps every non-final chunk 4-aligned by construction).
pub const CHUNK: usize = 4096;

/// Pixel data wire format.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    /// f=32, straight-alpha RGBA bytes.
    Rgba32,
    /// f=24, RGB bytes (alpha dropped — caller pre-composites).
    Rgb24,
    /// f=100, a complete PNG file (our `png_encode`).
    Png,
}

/// Options for transmit+display. Defaults: RGBA, no cell fit (terminal
/// uses intrinsic size), z=0, zlib compression on for raw formats.
#[derive(Clone, Debug)]
pub struct Options {
    /// Client-chosen image id (i=). Non-zero: id 0 means "let the
    /// terminal pick" and makes later deletes impossible.
    pub id: u32,
    pub format: Format,
    /// Cell-fit: scale into this many columns/rows (c=/r=).
    pub fit_cols: Option<u32>,
    pub fit_rows: Option<u32>,
    /// Stacking (z=); negative draws under text.
    pub z: i32,
    /// zlib-compress raw pixel payloads (o=z). Ignored for PNG (the
    /// PNG stream is already deflate-compressed; double compression
    /// wastes cycles and bytes).
    pub compress: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            id: 1,
            format: Format::Rgba32,
            fit_cols: None,
            fit_rows: None,
            z: 0,
            compress: true,
        }
    }
}

/// Transmit + display (`a=T`): one call paints the image at the cursor.
/// Returns the full escape byte stream (one or more chunked APCs).
pub fn transmit_display(img: &Bitmap, opts: &Options) -> Vec<u8> {
    // Assemble the raw payload per format.
    let (fmt_key, payload): (u32, Vec<u8>) = match opts.format {
        Format::Rgba32 => {
            let mut raw = Vec::with_capacity(img.pixels().len() * 4);
            for p in img.pixels() {
                raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
            }
            (32, raw)
        }
        Format::Rgb24 => {
            let mut raw = Vec::with_capacity(img.pixels().len() * 3);
            for p in img.pixels() {
                raw.extend_from_slice(&[p.r, p.g, p.b]);
            }
            (24, raw)
        }
        Format::Png => (100, png_encode::encode(img)),
    };
    let compressed = opts.compress && opts.format != Format::Png;
    let payload = if compressed {
        miniz_oxide::deflate::compress_to_vec_zlib(&payload, 6)
    } else {
        payload
    };

    // Control data for the FIRST escape only.
    let mut keys = format!("a=T,f={fmt_key},q=2,i={}", opts.id);
    match opts.format {
        // Raw formats require explicit pixel geometry.
        Format::Rgba32 | Format::Rgb24 => {
            keys.push_str(&format!(",s={},v={}", img.width(), img.height()));
        }
        Format::Png => {} // PNG carries its own geometry
    }
    if compressed {
        keys.push_str(",o=z");
    }
    if let Some(c) = opts.fit_cols {
        keys.push_str(&format!(",c={c}"));
    }
    if let Some(r) = opts.fit_rows {
        keys.push_str(&format!(",r={r}"));
    }
    if opts.z != 0 {
        keys.push_str(&format!(",z={}", opts.z));
    }

    let encoded = base64::encode(&payload);
    chunked_apc(&keys, encoded.as_bytes())
}

/// Re-display an already-transmitted image (`a=p`) with a new cell fit
/// or z — no pixel retransmission.
pub fn place(id: u32, fit_cols: Option<u32>, fit_rows: Option<u32>, z: i32) -> Vec<u8> {
    let mut keys = format!("a=p,q=2,i={id}");
    if let Some(c) = fit_cols {
        keys.push_str(&format!(",c={c}"));
    }
    if let Some(r) = fit_rows {
        keys.push_str(&format!(",r={r}"));
    }
    if z != 0 {
        keys.push_str(&format!(",z={z}"));
    }
    apc(&keys, b"")
}

/// Delete placements of image `id`. `free_data = true` (uppercase I)
/// also drops the transmitted pixels; false keeps them re-placeable.
pub fn delete_by_id(id: u32, free_data: bool) -> Vec<u8> {
    let d = if free_data { 'I' } else { 'i' };
    apc(&format!("a=d,q=2,d={d},i={id}"), b"")
}

/// Delete every visible placement (screen clear for image content).
pub fn delete_all(free_data: bool) -> Vec<u8> {
    let d = if free_data { 'A' } else { 'a' };
    apc(&format!("a=d,q=2,d={d}"), b"")
}

/// One APC escape: `ESC _ G keys ; payload ESC \`.
fn apc(keys: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(keys.len() + payload.len() + 8);
    out.extend_from_slice(b"\x1b_G");
    out.extend_from_slice(keys.as_bytes());
    if !payload.is_empty() {
        out.push(b';');
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

/// Chunked emission: first escape carries all control keys + `m=1`,
/// middle chunks only `m=1`, the last `m=0`. A payload that fits one
/// chunk omits `m` entirely (non-chunked form).
fn chunked_apc(first_keys: &str, encoded: &[u8]) -> Vec<u8> {
    if encoded.len() <= CHUNK {
        return apc(first_keys, encoded);
    }
    let mut out =
        Vec::with_capacity(encoded.len() + encoded.len() / CHUNK * 16 + first_keys.len() + 16);
    let mut chunks = encoded.chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        let m = if last { 0 } else { 1 };
        let keys = if first {
            first = false;
            format!("{first_keys},m={m}")
        } else {
            // q must persist on continuations: kitty replies per-escape.
            format!("m={m},q=2")
        };
        out.extend_from_slice(&apc(&keys, chunk));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;

    fn parse_escapes(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        // Split ESC _G ... ESC \ frames into (keys, payload).
        let mut out = Vec::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            assert!(rest.starts_with(b"\x1b_G"), "APC intro");
            rest = &rest[3..];
            let end = rest.windows(2).position(|w| w == b"\x1b\\").expect("ST");
            let body = &rest[..end];
            rest = &rest[end + 2..];
            let (keys, payload) = match body.iter().position(|&b| b == b';') {
                Some(i) => (&body[..i], &body[i + 1..]),
                None => (body, &[][..]),
            };
            out.push((String::from_utf8(keys.to_vec()).unwrap(), payload.to_vec()));
        }
        out
    }

    #[test]
    fn small_rgba_single_escape() {
        let img = Bitmap::new(2, 2, Rgba::rgb(1, 2, 3));
        let opts = Options {
            compress: false,
            ..Options::default()
        };
        let frames = parse_escapes(&transmit_display(&img, &opts));
        assert_eq!(frames.len(), 1);
        let (keys, payload) = &frames[0];
        assert!(keys.contains("a=T"));
        assert!(keys.contains("f=32"));
        assert!(keys.contains("s=2") && keys.contains("v=2"));
        assert!(keys.contains("q=2"));
        assert!(!keys.contains(",m="), "single chunk must not carry m");
        let decoded = crate::gfx::base64::decode(std::str::from_utf8(payload).unwrap()).unwrap();
        assert_eq!(decoded.len(), 16);
        assert_eq!(&decoded[..4], &[1, 2, 3, 255]);
    }

    #[test]
    fn compression_round_trips() {
        let img = Bitmap::new(8, 8, Rgba::rgb(200, 100, 50));
        let opts = Options {
            compress: true,
            ..Options::default()
        };
        let frames = parse_escapes(&transmit_display(&img, &opts));
        let (keys, payload) = &frames[0];
        assert!(keys.contains("o=z"));
        let decoded = crate::gfx::base64::decode(std::str::from_utf8(payload).unwrap()).unwrap();
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&decoded).unwrap();
        assert_eq!(raw.len(), 8 * 8 * 4);
        assert_eq!(&raw[..4], &[200, 100, 50, 255]);
    }

    #[test]
    fn large_payload_chunking_rules() {
        // 64x64 RGBA uncompressed = 16384 raw -> 21848 base64 chars ->
        // 6 chunks. Verify m= sequencing and 4-alignment of non-final
        // chunks and control keys only on the first.
        let img = Bitmap::from_fn(64, 64, |x, y| {
            Rgba::new(x as u8, y as u8, (x ^ y) as u8, 255)
        });
        let opts = Options {
            compress: false,
            ..Options::default()
        };
        let frames = parse_escapes(&transmit_display(&img, &opts));
        assert!(frames.len() > 1);
        let mut reassembled = String::new();
        for (i, (keys, payload)) in frames.iter().enumerate() {
            let last = i == frames.len() - 1;
            if i == 0 {
                assert!(keys.contains("a=T") && keys.contains("m=1"));
            } else if last {
                assert!(keys.starts_with("m=0"), "{keys}");
                assert!(!keys.contains("a=T"));
            } else {
                assert!(keys.starts_with("m=1"), "{keys}");
            }
            if !last {
                assert_eq!(payload.len() % 4, 0, "chunk {i} not 4-aligned");
                assert!(payload.len() <= CHUNK);
            }
            reassembled.push_str(std::str::from_utf8(payload).unwrap());
        }
        let decoded = crate::gfx::base64::decode(&reassembled).unwrap();
        assert_eq!(decoded.len(), 64 * 64 * 4);
    }

    #[test]
    fn png_format_carries_no_geometry_keys() {
        let img = Bitmap::new(3, 2, Rgba::rgb(9, 9, 9));
        let opts = Options {
            format: Format::Png,
            ..Options::default()
        };
        let frames = parse_escapes(&transmit_display(&img, &opts));
        let (keys, payload) = &frames[0];
        assert!(keys.contains("f=100"));
        assert!(!keys.contains("s=") && !keys.contains("v="));
        assert!(!keys.contains("o=z"), "no double compression for PNG");
        let decoded = crate::gfx::base64::decode(std::str::from_utf8(payload).unwrap()).unwrap();
        assert_eq!(crate::gfx::png::decode(&decoded).unwrap(), img);
    }

    #[test]
    fn cell_fit_and_z_keys() {
        let img = Bitmap::new(2, 2, Rgba::WHITE);
        let opts = Options {
            fit_cols: Some(10),
            fit_rows: Some(5),
            z: -7,
            compress: false,
            ..Options::default()
        };
        let frames = parse_escapes(&transmit_display(&img, &opts));
        let keys = &frames[0].0;
        assert!(keys.contains("c=10") && keys.contains("r=5") && keys.contains("z=-7"));
    }

    #[test]
    fn place_and_delete_forms() {
        let frames = parse_escapes(&place(42, Some(8), Some(4), 3));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].0.contains("a=p") && frames[0].0.contains("i=42"));

        let frames = parse_escapes(&delete_by_id(42, false));
        assert!(frames[0].0.contains("d=i") && frames[0].0.contains("i=42"));
        let frames = parse_escapes(&delete_by_id(42, true));
        assert!(frames[0].0.contains("d=I"));
        let frames = parse_escapes(&delete_all(false));
        assert!(frames[0].0.contains("a=d") && frames[0].0.contains("d=a"));
    }
}
