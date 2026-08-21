//! APNG (Animated PNG): the `acTL` / `fcTL` / `fdAT` chunk trio over
//! the still PNG decoder. Frames are sub-images with their own rect,
//! delay, disposal, and blend op; this module composites them into
//! complete pictures so playback is a frame swap.
//!
//! Free of new dependencies by construction: an APNG frame is a PNG
//! image, so it rides the same inflate, unfilter, and pixel expansion
//! as [`png::decode`](crate::gfx::png::decode()). A viewer that does not
//! know APNG sees the still IDAT image — which is why `is_animated`
//! decides the routing, not the file extension.

use std::time::Duration;

use crate::base::{Error, Result, Rgba};
use crate::gfx::anim::{check_budget, Animation, Frame};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::png::{self, Ihdr, Trns, MAX_PIXELS, SIGNATURE};

/// Does this PNG carry an `acTL` chunk (and therefore frames)?
/// Walks chunk headers only — no decode, no allocation.
pub fn is_animated(bytes: &[u8]) -> bool {
    let mut off = SIGNATURE.len();
    while bytes.len() - off >= 12 {
        let len = u32::from_be_bytes(match bytes[off..off + 4].try_into() {
            Ok(v) => v,
            Err(_) => return false,
        }) as usize;
        let ctype = &bytes[off + 4..off + 8];
        if ctype == b"acTL" {
            return true;
        }
        // acTL must precede the first IDAT; past that there is none.
        if ctype == b"IDAT" || ctype == b"IEND" {
            return false;
        }
        match off.checked_add(12 + len) {
            Some(next) if next <= bytes.len() => off = next,
            _ => return false,
        }
    }
    false
}

/// Disposal op (APNG spec: what the frame's rect holds afterwards).
#[derive(Copy, Clone, PartialEq, Eq)]
enum Dispose {
    None,
    Background,
    Previous,
}

struct Fctl {
    w: u32,
    h: u32,
    x: u32,
    y: u32,
    delay: Duration,
    dispose: Dispose,
    /// `true` = alpha-composite over the canvas; `false` = replace.
    blend_over: bool,
}

/// Decode every frame of an animated PNG.
pub fn decode(bytes: &[u8]) -> Result<Animation> {
    if bytes.len() < SIGNATURE.len() || bytes[..8] != SIGNATURE {
        return Err(Error::Parse("apng: bad signature".into()));
    }
    let mut ihdr: Option<Ihdr> = None;
    let mut palette: Option<Vec<Rgba>> = None;
    let mut trns = Trns::None;
    let mut loop_count: Option<u32> = None;
    let mut declared_frames = 0u32;

    // The frame being accumulated: its fcTL plus its zlib parts.
    let mut pending: Option<(Fctl, Vec<u8>)> = None;
    // IDAT belongs to the animation only when an fcTL preceded it.
    let mut idat: Vec<u8> = Vec::new();
    let mut idat_is_frame = false;
    let mut frames: Vec<Frame> = Vec::new();
    let mut canvas: Option<Bitmap> = None;
    let mut seen_iend = false;

    let mut off = SIGNATURE.len();
    while bytes.len() - off >= 12 {
        let len = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
        let ctype: [u8; 4] = bytes[off + 4..off + 8].try_into().unwrap();
        if bytes.len() - off - 12 < len {
            return Err(Error::Parse(format!(
                "apng: truncated {} chunk",
                String::from_utf8_lossy(&ctype)
            )));
        }
        let data = &bytes[off + 8..off + 8 + len];
        let crc_stored =
            u32::from_be_bytes(bytes[off + 8 + len..off + 12 + len].try_into().unwrap());
        if crc_stored != png::crc32(&bytes[off + 4..off + 8 + len]) {
            return Err(Error::Parse(format!(
                "apng: crc mismatch in {} chunk",
                String::from_utf8_lossy(&ctype)
            )));
        }
        off += 12 + len;

        match &ctype {
            b"IHDR" => {
                let hdr = png::parse_ihdr(data)?;
                canvas = Some(Bitmap::new(hdr.w, hdr.h, Rgba::TRANSPARENT));
                ihdr = Some(hdr);
            }
            b"PLTE" => {
                if len == 0 || !len.is_multiple_of(3) || len > 3 * 256 {
                    return Err(Error::Parse(format!("apng: bad PLTE length {len}")));
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
                    .ok_or_else(|| Error::Parse("apng: tRNS before IHDR".into()))?;
                trns = png::parse_trns(data, hdr.color)?;
            }
            b"acTL" => {
                if data.len() < 8 {
                    return Err(Error::Parse("apng: short acTL".into()));
                }
                declared_frames = u32::from_be_bytes(data[0..4].try_into().unwrap());
                let plays = u32::from_be_bytes(data[4..8].try_into().unwrap());
                loop_count = (plays != 0).then_some(plays);
            }
            b"fcTL" => {
                // A new fcTL closes the frame before it.
                flush(
                    &mut pending,
                    &mut idat,
                    &mut idat_is_frame,
                    &mut frames,
                    &mut canvas,
                    &ihdr,
                    palette.as_deref(),
                    &trns,
                )?;
                let hdr = ihdr
                    .as_ref()
                    .ok_or_else(|| Error::Parse("apng: fcTL before IHDR".into()))?;
                let fc = parse_fctl(data, hdr)?;
                pending = Some((fc, Vec::new()));
                idat_is_frame = idat.is_empty(); // fcTL before IDAT: the still IS frame 0
            }
            b"IDAT" => {
                if idat_is_frame {
                    if let Some((_, buf)) = pending.as_mut() {
                        buf.extend_from_slice(data);
                    }
                }
                idat.extend_from_slice(data);
            }
            b"fdAT" => {
                // fdAT = sequence number (4) + zlib data continuing the
                // current frame.
                if data.len() < 4 {
                    return Err(Error::Parse("apng: short fdAT".into()));
                }
                let (_, buf) = pending
                    .as_mut()
                    .ok_or_else(|| Error::Parse("apng: fdAT without a preceding fcTL".into()))?;
                buf.extend_from_slice(&data[4..]);
            }
            b"IEND" => {
                seen_iend = true;
                break;
            }
            _ => {
                if ctype[0] & 0x20 == 0 {
                    return Err(Error::Parse(format!(
                        "apng: unsupported critical chunk {}",
                        String::from_utf8_lossy(&ctype)
                    )));
                }
            }
        }
    }
    flush(
        &mut pending,
        &mut idat,
        &mut idat_is_frame,
        &mut frames,
        &mut canvas,
        &ihdr,
        palette.as_deref(),
        &trns,
    )?;

    let hdr = ihdr.ok_or_else(|| Error::Parse("apng: missing IHDR".into()))?;
    // A file that stops before IEND is truncated, however many frames
    // already decoded — a short read must never look like a short clip.
    if !seen_iend {
        return Err(Error::Parse("apng: missing IEND".into()));
    }
    if frames.is_empty() {
        return Err(Error::Parse("apng: no animation frames".into()));
    }
    if declared_frames != 0 && declared_frames as usize != frames.len() {
        return Err(Error::Parse(format!(
            "apng: acTL declares {declared_frames} frames, file carries {}",
            frames.len()
        )));
    }
    Ok(Animation {
        frames,
        loop_count,
        width: hdr.w,
        height: hdr.h,
    })
}

fn parse_fctl(data: &[u8], hdr: &Ihdr) -> Result<Fctl> {
    if data.len() < 26 {
        return Err(Error::Parse("apng: short fcTL".into()));
    }
    let w = u32::from_be_bytes(data[4..8].try_into().unwrap());
    let h = u32::from_be_bytes(data[8..12].try_into().unwrap());
    let x = u32::from_be_bytes(data[12..16].try_into().unwrap());
    let y = u32::from_be_bytes(data[16..20].try_into().unwrap());
    let num = u16::from_be_bytes(data[20..22].try_into().unwrap()) as u64;
    let den = match u16::from_be_bytes(data[22..24].try_into().unwrap()) {
        0 => 100, // spec: denominator 0 means 1/100 s
        d => d as u64,
    };
    if w == 0 || h == 0 {
        return Err(Error::Parse("apng: zero frame dimension".into()));
    }
    if x.saturating_add(w) > hdr.w || y.saturating_add(h) > hdr.h {
        return Err(Error::Parse(format!(
            "apng: frame {w}x{h}+{x}+{y} runs outside the {}x{} canvas",
            hdr.w, hdr.h
        )));
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(Error::Parse("apng: frame exceeds pixel budget".into()));
    }
    Ok(Fctl {
        w,
        h,
        x,
        y,
        delay: Duration::from_micros(num * 1_000_000 / den),
        dispose: match data[24] {
            1 => Dispose::Background,
            2 => Dispose::Previous,
            _ => Dispose::None,
        },
        blend_over: data[25] == 1,
    })
}

/// Composite the pending frame onto the canvas and push it.
#[allow(clippy::too_many_arguments)]
fn flush(
    pending: &mut Option<(Fctl, Vec<u8>)>,
    idat: &mut Vec<u8>,
    idat_is_frame: &mut bool,
    frames: &mut Vec<Frame>,
    canvas: &mut Option<Bitmap>,
    ihdr: &Option<Ihdr>,
    palette: Option<&[Rgba]>,
    trns: &Trns,
) -> Result<()> {
    let Some((fc, zlib)) = pending.take() else {
        return Ok(());
    };
    let hdr = ihdr
        .as_ref()
        .ok_or_else(|| Error::Parse("apng: frame before IHDR".into()))?;
    let cv = canvas
        .as_mut()
        .ok_or_else(|| Error::Parse("apng: frame before the canvas exists".into()))?;
    if zlib.is_empty() {
        return Err(Error::Parse("apng: frame carries no image data".into()));
    }
    let sub = png::decode_subimage(&zlib, fc.w, fc.h, hdr.color, palette, trns)?;

    let saved = (fc.dispose == Dispose::Previous).then(|| cv.crop(fc.x, fc.y, fc.w, fc.h));
    for row in 0..fc.h {
        for col in 0..fc.w {
            let src = sub.get(col, row).unwrap_or(Rgba::TRANSPARENT);
            let (dx, dy) = (fc.x + col, fc.y + row);
            let out = if fc.blend_over {
                over(src, cv.get(dx, dy).unwrap_or(Rgba::TRANSPARENT))
            } else {
                src
            };
            cv.set(dx, dy, out);
        }
    }
    frames.push(Frame {
        image: cv.clone(),
        delay: fc.delay,
    });
    check_budget(frames.len(), hdr.w, hdr.h)?;

    match fc.dispose {
        Dispose::None => {}
        Dispose::Background => {
            for row in 0..fc.h {
                for col in 0..fc.w {
                    cv.set(fc.x + col, fc.y + row, Rgba::TRANSPARENT);
                }
            }
        }
        Dispose::Previous => {
            if let Some(prev) = saved {
                for row in 0..fc.h {
                    for col in 0..fc.w {
                        if let Some(c) = prev.get(col, row) {
                            cv.set(fc.x + col, fc.y + row, c);
                        }
                    }
                }
            }
        }
    }
    idat.clear();
    *idat_is_frame = false;
    Ok(())
}

/// Straight-alpha source-over compositing (APNG blend op 1).
fn over(src: Rgba, dst: Rgba) -> Rgba {
    if src.a == 255 || dst.a == 0 {
        return src;
    }
    if src.a == 0 {
        return dst;
    }
    let (sa, da) = (src.a as u32, dst.a as u32);
    let out_a = sa + da * (255 - sa) / 255;
    if out_a == 0 {
        return Rgba::TRANSPARENT;
    }
    let mix = |s: u8, d: u8| -> u8 {
        ((s as u32 * sa + d as u32 * da * (255 - sa) / 255) / out_a).min(255) as u8
    };
    Rgba {
        r: mix(src.r, dst.r),
        g: mix(src.g, dst.g),
        b: mix(src.b, dst.b),
        a: out_a.min(255) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, is_animated};
    use crate::gfx::anim_fixtures as fx;

    /// APNG is lossless like its still parent: exact ground truth.
    #[test]
    fn decodes_every_frame_exactly() {
        let a = decode(fx::APNG).unwrap();
        assert_eq!(a.len(), fx::FRAMES);
        assert_eq!((a.width, a.height), fx::SIZE);
        assert_eq!(a.loop_count, None, "0 plays means forever");
        for (i, frame) in a.frames.iter().enumerate() {
            assert_eq!(frame.delay, std::time::Duration::from_millis(100));
            for y in 0..fx::SIZE.1 {
                for x in 0..fx::SIZE.0 {
                    let got = frame.image.get(x, y).unwrap();
                    let (r, g, b) = fx::expected_rgb(i, x, y);
                    assert_eq!((got.r, got.g, got.b), (r, g, b), "frame {i} ({x},{y})");
                }
            }
        }
    }

    /// The routing question: only a PNG carrying acTL is animated, and
    /// deciding it costs a chunk-header walk, never a decode.
    #[test]
    fn is_animated_reads_only_chunk_headers() {
        assert!(is_animated(fx::APNG));
        let still = crate::gfx::png_encode::encode(&crate::gfx::Bitmap::new(
            4,
            4,
            crate::base::Rgba::WHITE,
        ));
        assert!(!is_animated(&still), "a plain PNG is not animated");
        assert!(!is_animated(b"not a png"));
    }

    /// An APNG stays a valid still PNG: a viewer that ignores the
    /// animation chunks must still get frame 0's IDAT image.
    #[test]
    fn still_decoder_reads_the_default_image() {
        let still = crate::gfx::png::decode(fx::APNG).unwrap();
        assert_eq!((still.width(), still.height()), fx::SIZE);
        let animated = decode(fx::APNG).unwrap();
        assert_eq!(still.pixels(), animated.frames[0].image.pixels());
    }

    #[test]
    fn truncation_ladder_never_panics() {
        for cut in 0..fx::APNG.len() {
            assert!(decode(&fx::APNG[..cut]).is_err(), "prefix {cut} decoded");
        }
    }

    #[test]
    fn corrupt_chunks_reject_by_name() {
        // Stomp a byte inside the first fdAT payload: the CRC guard
        // must catch it rather than decoding a wrong picture.
        let mut b = fx::APNG.to_vec();
        let at = b.len() - 40;
        b[at] ^= 0xFF;
        let err = decode(&b).unwrap_err().to_string();
        assert!(err.contains("crc") || err.contains("inflate"), "{err}");
    }
}
