//! PNG decoder for the common critical path: 8-bit gray / gray+alpha /
//! RGB / RGBA / palette, non-interlaced, with PLTE + tRNS. Compression
//! is delegated to `miniz_oxide` (the one allowed inflate dependency);
//! signature/chunk/CRC/unfilter logic is ours.
//!
//! Out of scope in v1 — rejected with precise `Error::Parse` messages,
//! never silently mangled: bit depths other than 8, Adam7 interlacing,
//! and every ancillary transform (gamma, ICC...). Real assets this
//! decoder must handle today (`abstract3d/out/**/*.png`) are 8-bit
//! non-interlaced, verified 2026-07-20 (see docs/design/gfx-three.md).
//!
//! Hard limits: dimensions are capped (`MAX_PIXELS`) *before* any
//! allocation, and the inflate output limit is the exact expected
//! stream size — a PNG whose IDAT inflates to any other size is
//! corrupt, and the cap doubles as decompression-bomb protection.

use crate::base::{Error, Result, Rgba};
use crate::gfx::bitmap::Bitmap;

/// 8-byte PNG signature.
pub const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Decoded-image pixel budget (16.7M px ≈ 4K x 4K): terminal-destined
/// images are orders of magnitude smaller; anything past this is either
/// a mistake or an attack.
pub const MAX_PIXELS: u64 = 1 << 24;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ColorType {
    Gray,      // 0
    Rgb,       // 2
    Palette,   // 3
    GrayAlpha, // 4
    Rgba,      // 6
}

impl ColorType {
    fn from_byte(b: u8) -> Result<ColorType> {
        match b {
            0 => Ok(ColorType::Gray),
            2 => Ok(ColorType::Rgb),
            3 => Ok(ColorType::Palette),
            4 => Ok(ColorType::GrayAlpha),
            6 => Ok(ColorType::Rgba),
            _ => Err(Error::Parse(format!("png: invalid color type {b}"))),
        }
    }

    /// Bytes per pixel at bit depth 8.
    fn bytes_per_pixel(self) -> usize {
        match self {
            ColorType::Gray | ColorType::Palette => 1,
            ColorType::GrayAlpha => 2,
            ColorType::Rgb => 3,
            ColorType::Rgba => 4,
        }
    }
}

struct Ihdr {
    w: u32,
    h: u32,
    color: ColorType,
}

/// Transparency info from tRNS, interpreted per color type.
enum Trns {
    None,
    /// Palette alpha table (may be shorter than PLTE; rest opaque).
    PaletteAlpha(Vec<u8>),
    /// Colorkey: exact-match pixels become fully transparent.
    GrayKey(u8),
    RgbKey(u8, u8, u8),
}

/// Decode a PNG byte stream into an RGBA bitmap.
pub fn decode(bytes: &[u8]) -> Result<Bitmap> {
    if bytes.len() < SIGNATURE.len() || bytes[..8] != SIGNATURE {
        return Err(Error::Parse("png: bad signature".into()));
    }

    let mut ihdr: Option<Ihdr> = None;
    let mut palette: Option<Vec<Rgba>> = None;
    let mut trns = Trns::None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_iend = false;

    let mut off = SIGNATURE.len();
    while off < bytes.len() {
        // Chunk = len(4 BE) type(4) data(len) crc(4, over type+data).
        if bytes.len() - off < 12 {
            return Err(Error::Parse("png: truncated chunk header".into()));
        }
        let len = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        let ctype: [u8; 4] = bytes[off + 4..off + 8].try_into().unwrap();
        if bytes.len() - off - 12 < len {
            return Err(Error::Parse(format!(
                "png: truncated {} chunk (want {len} bytes)",
                String::from_utf8_lossy(&ctype)
            )));
        }
        let data = &bytes[off + 8..off + 8 + len];
        let crc_stored =
            u32::from_be_bytes(bytes[off + 8 + len..off + 12 + len].try_into().unwrap());
        let crc_actual = crc32(&bytes[off + 4..off + 8 + len]);
        if crc_stored != crc_actual {
            return Err(Error::Parse(format!(
                "png: crc mismatch in {} chunk",
                String::from_utf8_lossy(&ctype)
            )));
        }
        off += 12 + len;

        match &ctype {
            b"IHDR" => {
                if ihdr.is_some() {
                    return Err(Error::Parse("png: duplicate IHDR".into()));
                }
                ihdr = Some(parse_ihdr(data)?);
            }
            b"PLTE" => {
                if len == 0 || !len.is_multiple_of(3) || len > 3 * 256 {
                    return Err(Error::Parse(format!("png: bad PLTE length {len}")));
                }
                palette = Some(
                    data.chunks_exact(3)
                        .map(|c| Rgba::rgb(c[0], c[1], c[2]))
                        .collect(),
                );
            }
            b"tRNS" => {
                let hdr = ihdr
                    .as_ref()
                    .ok_or_else(|| Error::Parse("png: tRNS before IHDR".into()))?;
                trns = parse_trns(data, hdr.color)?;
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => {
                seen_iend = true;
                break;
            }
            // Ancillary chunks (bit 5 of first byte set = safe to skip).
            // Unknown *critical* chunks mean we cannot render faithfully.
            _ => {
                if ctype[0] & 0x20 == 0 {
                    return Err(Error::Parse(format!(
                        "png: unsupported critical chunk {}",
                        String::from_utf8_lossy(&ctype)
                    )));
                }
            }
        }
    }

    let ihdr = ihdr.ok_or_else(|| Error::Parse("png: missing IHDR".into()))?;
    if !seen_iend {
        return Err(Error::Parse("png: missing IEND".into()));
    }
    if idat.is_empty() {
        return Err(Error::Parse("png: no IDAT data".into()));
    }
    if ihdr.color == ColorType::Palette && palette.is_none() {
        return Err(Error::Parse("png: palette image without PLTE".into()));
    }

    // Inflate with the exact expected size as the limit: h scanlines of
    // (1 filter byte + w*bpp). Any other size means corruption.
    let bpp = ihdr.color.bytes_per_pixel();
    let stride = 1 + ihdr.w as usize * bpp;
    let expected = ihdr.h as usize * stride;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&idat, expected)
        .map_err(|e| Error::Parse(format!("png: inflate failed: {e:?}")))?;
    if raw.len() != expected {
        return Err(Error::Parse(format!(
            "png: decompressed size {} != expected {expected}",
            raw.len()
        )));
    }

    let scanlines = unfilter(raw, ihdr.w as usize, ihdr.h as usize, bpp)?;
    to_bitmap(&scanlines, &ihdr, palette.as_deref(), &trns)
}

fn parse_ihdr(data: &[u8]) -> Result<Ihdr> {
    if data.len() != 13 {
        return Err(Error::Parse(format!(
            "png: IHDR length {} != 13",
            data.len()
        )));
    }
    let w = u32::from_be_bytes(data[0..4].try_into().unwrap());
    let h = u32::from_be_bytes(data[4..8].try_into().unwrap());
    let (depth, color, compression, filter, interlace) =
        (data[8], data[9], data[10], data[11], data[12]);
    if w == 0 || h == 0 {
        return Err(Error::Parse("png: zero dimension".into()));
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(Error::Parse(format!("png: {w}x{h} exceeds pixel budget")));
    }
    let color = ColorType::from_byte(color)?;
    if depth != 8 {
        return Err(Error::Parse(format!(
            "png: unsupported bit depth {depth} (v1 handles 8)"
        )));
    }
    if compression != 0 {
        return Err(Error::Parse(format!(
            "png: unknown compression method {compression}"
        )));
    }
    if filter != 0 {
        return Err(Error::Parse(format!("png: unknown filter method {filter}")));
    }
    if interlace == 1 {
        return Err(Error::Parse(
            "png: Adam7 interlace not supported (v1)".into(),
        ));
    }
    if interlace != 0 {
        return Err(Error::Parse(format!(
            "png: unknown interlace method {interlace}"
        )));
    }
    Ok(Ihdr { w, h, color })
}

fn parse_trns(data: &[u8], color: ColorType) -> Result<Trns> {
    match color {
        ColorType::Palette => Ok(Trns::PaletteAlpha(data.to_vec())),
        // Colorkeys are stored as 16-bit samples even at depth 8; the
        // significant byte is the low one.
        ColorType::Gray => {
            if data.len() != 2 {
                return Err(Error::Parse("png: gray tRNS length != 2".into()));
            }
            Ok(Trns::GrayKey(data[1]))
        }
        ColorType::Rgb => {
            if data.len() != 6 {
                return Err(Error::Parse("png: rgb tRNS length != 6".into()));
            }
            Ok(Trns::RgbKey(data[1], data[3], data[5]))
        }
        // Alpha color types must not carry tRNS (spec).
        ColorType::GrayAlpha | ColorType::Rgba => Err(Error::Parse(
            "png: tRNS forbidden with alpha color type".into(),
        )),
    }
}

/// Reverse per-scanline filtering in place; returns the raw bytes with
/// filter bytes stripped (w*bpp per row).
fn unfilter(raw: Vec<u8>, w: usize, h: usize, bpp: usize) -> Result<Vec<u8>> {
    let stride = 1 + w * bpp;
    let row_len = w * bpp;
    let mut out = vec![0u8; h * row_len];
    for y in 0..h {
        let (done, rest) = out.split_at_mut(y * row_len);
        let prev: &[u8] = if y == 0 {
            &[]
        } else {
            &done[(y - 1) * row_len..]
        };
        let cur = &mut rest[..row_len];
        let filter = raw[y * stride];
        let src = &raw[y * stride + 1..y * stride + stride];
        match filter {
            0 => cur.copy_from_slice(src),
            1 => {
                // Sub: predict from the pixel to the left.
                for i in 0..row_len {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    cur[i] = src[i].wrapping_add(a);
                }
            }
            2 => {
                // Up: predict from the pixel above.
                for i in 0..row_len {
                    let b = prev.get(i).copied().unwrap_or(0);
                    cur[i] = src[i].wrapping_add(b);
                }
            }
            3 => {
                // Average of left and above; the sum cannot overflow u16.
                for i in 0..row_len {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 } as u16;
                    let b = prev.get(i).copied().unwrap_or(0) as u16;
                    cur[i] = src[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                for i in 0..row_len {
                    let a = if i >= bpp { cur[i - bpp] } else { 0 };
                    let b = prev.get(i).copied().unwrap_or(0);
                    let c = if i >= bpp {
                        prev.get(i - bpp).copied().unwrap_or(0)
                    } else {
                        0
                    };
                    cur[i] = src[i].wrapping_add(paeth(a, b, c));
                }
            }
            f => {
                return Err(Error::Parse(format!(
                    "png: unknown filter type {f} on row {y}"
                )))
            }
        }
    }
    Ok(out)
}

/// Paeth predictor (PNG spec §9.4). Chooses whichever of left (a),
/// above (b), upper-left (c) is closest to the linear estimate
/// `p = a + b − c`. The tie-break order a, then b, then c is
/// NORMATIVE: encoders picked their filter assuming it, so a different
/// order yields wrong pixels that still "look plausible" — which is
/// why the tests pin hand-computed vectors rather than round-trips.
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn to_bitmap(scan: &[u8], ihdr: &Ihdr, palette: Option<&[Rgba]>, trns: &Trns) -> Result<Bitmap> {
    let (w, h) = (ihdr.w, ihdr.h);
    let n = (w as usize) * (h as usize);
    let mut px = Vec::with_capacity(n);
    match ihdr.color {
        ColorType::Gray => {
            for &v in &scan[..n] {
                let a = match trns {
                    Trns::GrayKey(k) if *k == v => 0,
                    _ => 255,
                };
                px.push(Rgba::new(v, v, v, a));
            }
        }
        ColorType::GrayAlpha => {
            for c in scan.chunks_exact(2).take(n) {
                px.push(Rgba::new(c[0], c[0], c[0], c[1]));
            }
        }
        ColorType::Rgb => {
            for c in scan.chunks_exact(3).take(n) {
                let a = match trns {
                    Trns::RgbKey(r, g, b) if (*r, *g, *b) == (c[0], c[1], c[2]) => 0,
                    _ => 255,
                };
                px.push(Rgba::new(c[0], c[1], c[2], a));
            }
        }
        ColorType::Rgba => {
            for c in scan.chunks_exact(4).take(n) {
                px.push(Rgba::new(c[0], c[1], c[2], c[3]));
            }
        }
        ColorType::Palette => {
            let pal = palette.expect("checked in decode");
            let alphas = match trns {
                Trns::PaletteAlpha(a) => a.as_slice(),
                _ => &[],
            };
            for &i in &scan[..n] {
                let color = *pal.get(i as usize).ok_or_else(|| {
                    Error::Parse(format!(
                        "png: palette index {i} out of range ({})",
                        pal.len()
                    ))
                })?;
                let a = alphas.get(i as usize).copied().unwrap_or(255);
                px.push(color.with_alpha(a));
            }
        }
    }
    Bitmap::from_pixels(w, h, px).ok_or_else(|| Error::Parse("png: pixel count mismatch".into()))
}

/// CRC-32 (ISO 3309, the PNG variant), table built at compile time so
/// there is no lazy-init state and no runtime cost on first use.
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[n] = c;
            n += 1;
        }
        t
    };
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::png_test_encoder as enc;

    /// 2x2 RGB, hand-assembled with a *stored* (uncompressed) zlib block
    /// so every byte of the fixture is explainable: rows are
    /// (red, green) / (blue, white), filter 0.
    const FIXTURE_RGB_2X2: [u8; 82] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd,
        0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x19, 0x49, 0x44, 0x41, 0x54, 0x78, 0x01, 0x01, 0x0e,
        0x00, 0xf1, 0xff, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff,
        0xff, 0xff, 0x1f, 0xee, 0x05, 0xfb, 0xde, 0xdd, 0xec, 0x2b, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// 2x1 gray+alpha with a Sub-filtered row: pixels (10,200), (30,100);
    /// filtered bytes are 10,200,20,156 (20 = 30−10, 156 = 100−200 mod 256).
    const FIXTURE_GA_SUB: [u8; 70] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0x5e,
        0x2b, 0xb7, 0x01, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xe4,
        0x3a, 0x21, 0x32, 0x07, 0x00, 0x03, 0x4e, 0x01, 0x84, 0x0b, 0xaf, 0xa6, 0xa7, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn decode_byte_exact_rgb_fixture() {
        let b = decode(&FIXTURE_RGB_2X2).unwrap();
        assert_eq!((b.width(), b.height()), (2, 2));
        assert_eq!(b.get(0, 0).unwrap(), Rgba::rgb(255, 0, 0));
        assert_eq!(b.get(1, 0).unwrap(), Rgba::rgb(0, 255, 0));
        assert_eq!(b.get(0, 1).unwrap(), Rgba::rgb(0, 0, 255));
        assert_eq!(b.get(1, 1).unwrap(), Rgba::WHITE);
    }

    #[test]
    fn decode_gray_alpha_sub_filter_fixture() {
        let b = decode(&FIXTURE_GA_SUB).unwrap();
        assert_eq!((b.width(), b.height()), (2, 1));
        assert_eq!(b.get(0, 0).unwrap(), Rgba::new(10, 10, 10, 200));
        assert_eq!(b.get(1, 0).unwrap(), Rgba::new(30, 30, 30, 100));
    }

    #[test]
    fn paeth_reference_vectors() {
        // Hand-walked against spec §9.4 (p = a + b − c).
        assert_eq!(paeth(0, 0, 0), 0);
        assert_eq!(paeth(10, 20, 5), 20); // p=25: pa=15 pb=5 pc=20 -> b
        assert_eq!(paeth(20, 10, 5), 20); // p=25: pa=5 pb=15 pc=20 -> a
        assert_eq!(paeth(10, 20, 30), 10); // p=0:  pa=10 pb=20 pc=30 -> a
        assert_eq!(paeth(50, 60, 55), 55); // p=55: pa=5 pb=5 pc=0  -> c
        assert_eq!(paeth(255, 0, 128), 128); // p=127: pa=128 pb=127 pc=1 -> c
    }

    #[test]
    fn paeth_tie_break_order() {
        // Ties are NORMATIVE (a, then b, then c). With distinct values a
        // pa==pb tie forces c to the exact midpoint where pc==0 wins, so
        // the observable ties are against pc:
        assert_eq!(paeth(8, 11, 10), 8); // p=9: pa=1 pb=2 pc=1, pa==pc -> a
        assert_eq!(paeth(11, 8, 10), 8); // p=9: pa=2 pb=1 pc=1, pb==pc -> b
        assert_eq!(paeth(7, 7, 7), 7); // all distances 0 -> a
    }

    #[test]
    fn round_trip_all_color_types() {
        let img = Bitmap::from_fn(5, 4, |x, y| {
            Rgba::new((x * 50) as u8, (y * 60) as u8, ((x + y) * 30) as u8, 255)
        });
        let rgb = decode(&enc::encode_rgb(&img)).unwrap();
        assert_eq!(rgb, img);

        let img_a = Bitmap::from_fn(3, 3, |x, y| {
            Rgba::new(
                (x * 80) as u8,
                10,
                (y * 80) as u8,
                (255 - x * 60 - y * 20) as u8,
            )
        });
        let rgba = decode(&enc::encode_rgba(&img_a)).unwrap();
        assert_eq!(rgba, img_a);

        let gray = Bitmap::from_fn(4, 2, |x, y| {
            let v = (x * 40 + y * 100) as u8;
            Rgba::rgb(v, v, v)
        });
        assert_eq!(decode(&enc::encode_gray(&gray)).unwrap(), gray);
    }

    #[test]
    fn round_trip_palette_with_trns() {
        let pal = [
            Rgba::rgb(255, 0, 0),
            Rgba::rgb(0, 255, 0),
            Rgba::rgb(0, 0, 255),
        ];
        let alphas = [255u8, 128];
        let indices = [0u8, 1, 2, 1, 0, 2];
        let bytes = enc::encode_palette(3, 2, &pal, &alphas, &indices);
        let b = decode(&bytes).unwrap();
        assert_eq!(b.get(0, 0).unwrap(), Rgba::rgb(255, 0, 0));
        assert_eq!(
            b.get(1, 0).unwrap(),
            Rgba::new(0, 255, 0, 128),
            "tRNS alpha"
        );
        assert_eq!(
            b.get(2, 0).unwrap(),
            Rgba::rgb(0, 0, 255),
            "beyond tRNS -> opaque"
        );
    }

    #[test]
    fn round_trip_exercises_all_filters() {
        // The test encoder can force a specific filter per row; run each
        // filter over content designed to have gradients both ways.
        let img = Bitmap::from_fn(8, 6, |x, y| {
            Rgba::new((x * 31) as u8, (y * 43) as u8, (x * x + y) as u8, 255)
        });
        for filter in 0..=4u8 {
            let bytes = enc::encode_rgb_with_filter(&img, filter);
            let out = decode(&bytes).unwrap_or_else(|e| panic!("filter {filter}: {e}"));
            assert_eq!(out, img, "filter {filter} round-trip");
        }
    }

    #[test]
    fn truncation_never_panics() {
        let full = enc::encode_rgba(&Bitmap::from_fn(6, 6, |x, y| {
            Rgba::new(x as u8 * 40, y as u8 * 40, 77, 255)
        }));
        // Every prefix must produce Err, never panic. (IEND-missing
        // catches the prefixes that stop at chunk boundaries.)
        for cut in 0..full.len() {
            assert!(
                decode(&full[..cut]).is_err(),
                "prefix of {cut} bytes decoded"
            );
        }
    }

    #[test]
    fn corrupt_data_is_rejected() {
        let mut bad_sig = FIXTURE_RGB_2X2;
        bad_sig[0] = 0x88;
        assert!(matches!(decode(&bad_sig), Err(Error::Parse(_))));

        // Flip one IDAT payload byte: CRC must catch it.
        let mut bad_crc = FIXTURE_RGB_2X2;
        bad_crc[50] ^= 0x01;
        let err = decode(&bad_crc).unwrap_err();
        assert!(err.to_string().contains("crc"), "{err}");

        // Valid chunks, garbage zlib stream.
        let mut garbage_idat = Vec::new();
        garbage_idat.extend_from_slice(&SIGNATURE);
        enc::push_chunk(&mut garbage_idat, b"IHDR", &enc::ihdr_payload(2, 2, 8, 2));
        enc::push_chunk(&mut garbage_idat, b"IDAT", &[0xDE, 0xAD, 0xBE, 0xEF]);
        enc::push_chunk(&mut garbage_idat, b"IEND", &[]);
        assert!(decode(&garbage_idat).is_err());
    }

    #[test]
    fn unsupported_features_named_in_error() {
        let mut interlaced = Vec::new();
        interlaced.extend_from_slice(&SIGNATURE);
        let mut ih = enc::ihdr_payload(2, 2, 8, 2);
        ih[12] = 1; // Adam7
        enc::push_chunk(&mut interlaced, b"IHDR", &ih);
        let err = decode(&interlaced).unwrap_err();
        assert!(err.to_string().contains("Adam7"), "{err}");

        let mut depth16 = Vec::new();
        depth16.extend_from_slice(&SIGNATURE);
        enc::push_chunk(&mut depth16, b"IHDR", &enc::ihdr_payload(2, 2, 16, 2));
        let err = decode(&depth16).unwrap_err();
        assert!(err.to_string().contains("bit depth"), "{err}");
    }

    #[test]
    fn pixel_budget_enforced_before_allocation() {
        let mut huge = Vec::new();
        huge.extend_from_slice(&SIGNATURE);
        enc::push_chunk(
            &mut huge,
            b"IHDR",
            &enc::ihdr_payload(0x00FF_FFFF, 0x00FF_FFFF, 8, 6),
        );
        let err = decode(&huge).unwrap_err();
        assert!(err.to_string().contains("budget"), "{err}");
    }

    #[test]
    fn decodes_real_asset_when_present() {
        // Real-world integration check, skipped on machines without the
        // sibling repos (guarded, per the cross-machine test rule).
        let path = std::path::Path::new(
            "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/preview.png",
        );
        if !path.exists() {
            return;
        }
        let bytes = std::fs::read(path).unwrap();
        let img = decode(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (420, 420));
        // Sanity: not all one color.
        let first = img.get(0, 0).unwrap();
        assert!(img.pixels().iter().any(|p| p != &first));
    }
}
