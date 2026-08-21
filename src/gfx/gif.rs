//! Animated GIF decoder (GIF87a/89a): LZW image data, the Graphics
//! Control Extension's delay and disposal, local or global palettes,
//! transparency index, and the NETSCAPE loop count.
//!
//! Why this format is in-tree at all: it is the one moving-picture
//! format a permissive, dependency-free library can decode honestly —
//! LZW's patents expired in 2004, the whole codec is a dictionary and
//! a bit reader, and every frame is an independent raster composited
//! over the last. No motion compensation, no entropy models, no
//! patent pool. See `docs/graphics-and-3d.md` for the ladder that
//! decides what the engine decodes itself and what it delegates.
//!
//! Guards (a GIF is untrusted input): the canvas is checked against
//! `png::MAX_PIXELS` BEFORE allocation, the whole animation against
//! [`MAX_ANIMATION_PIXELS`]
//! as frames accumulate, and the LZW decoder rejects a code stream
//! that overruns its dictionary or its output rather than growing
//! without bound. Every rejection is a named `Error::Parse`.

use std::time::Duration;

use crate::base::{Error, Result, Rgba};
use crate::gfx::anim::{Animation, Frame, MAX_ANIMATION_PIXELS, MAX_FRAMES};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::png::MAX_PIXELS;

/// `GIF87a` / `GIF89a`.
pub const SIGNATURE: [u8; 3] = *b"GIF";

/// What the frame leaves behind for the next one (GIF89a GCE bits
/// 2..4, table in the spec's Appendix F).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Disposal {
    /// Leave the canvas as it is.
    Keep,
    /// Clear the frame's rect back to transparent.
    Background,
    /// Restore what was under the frame's rect before it drew.
    Previous,
}

impl Disposal {
    fn from_bits(b: u8) -> Disposal {
        match b {
            2 => Disposal::Background,
            3 => Disposal::Previous,
            // 0 (unspecified) and 1 (do not dispose) both mean "keep";
            // 4..=7 are reserved — treat as keep rather than reject, so
            // one odd encoder cannot make a whole file undecodable.
            _ => Disposal::Keep,
        }
    }
}

/// Decode every frame of a GIF into an [`Animation`].
pub fn decode(bytes: &[u8]) -> Result<Animation> {
    // Header: "GIF" + version(3), then the Logical Screen Descriptor.
    if bytes.len() < 13 || bytes[..3] != SIGNATURE {
        return Err(Error::Parse("gif: bad signature".into()));
    }
    let cw = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
    let ch = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
    if cw == 0 || ch == 0 {
        return Err(Error::Parse("gif: zero canvas dimension".into()));
    }
    if (cw as u64) * (ch as u64) > MAX_PIXELS {
        return Err(Error::Parse(format!(
            "gif: {cw}x{ch} canvas exceeds pixel budget"
        )));
    }
    let flags = bytes[10];
    let mut pos = 13usize;
    let global: Option<Vec<Rgba>> = if flags & 0x80 != 0 {
        let n = 2usize << (flags & 0x07);
        Some(read_palette(bytes, &mut pos, n)?)
    } else {
        None
    };

    let mut canvas = Bitmap::new(cw, ch, Rgba::TRANSPARENT);
    let mut frames: Vec<Frame> = Vec::new();
    let mut loop_count: Option<u32> = None;
    // Graphics Control Extension state — applies to the NEXT image.
    let (mut delay_cs, mut transparent, mut disposal) = (0u16, None::<u8>, Disposal::Keep);

    loop {
        let block = *bytes
            .get(pos)
            .ok_or_else(|| Error::Parse("gif: truncated before trailer".into()))?;
        pos += 1;
        match block {
            0x3B => break, // trailer
            0x21 => {
                // Extension: label + sub-blocks.
                let label = *bytes
                    .get(pos)
                    .ok_or_else(|| Error::Parse("gif: truncated extension".into()))?;
                pos += 1;
                let data = read_sub_blocks(bytes, &mut pos)?;
                match label {
                    0xF9 => {
                        // Graphics Control Extension.
                        if data.len() < 4 {
                            return Err(Error::Parse("gif: short graphic control block".into()));
                        }
                        disposal = Disposal::from_bits((data[0] >> 2) & 0x07);
                        delay_cs = u16::from_le_bytes([data[1], data[2]]);
                        transparent = (data[0] & 0x01 != 0).then_some(data[3]);
                    }
                    // Application Extension: only NETSCAPE2.0's loop
                    // count means anything here.
                    0xFF if data.len() >= 16 && &data[..11] == b"NETSCAPE2.0" && data[12] == 1 => {
                        let n = u16::from_le_bytes([data[13], data[14]]) as u32;
                        // 0 = forever, which is what `None` means.
                        loop_count = (n != 0).then_some(n);
                    }
                    _ => {} // comment, plain text, unknown: skipped
                }
            }
            0x2C => {
                let frame = read_image(
                    bytes,
                    &mut pos,
                    &mut canvas,
                    global.as_deref(),
                    transparent,
                    disposal,
                    delay_cs,
                )?;
                frames.push(frame);
                if frames.len() > MAX_FRAMES {
                    return Err(Error::Parse(format!("gif: more than {MAX_FRAMES} frames")));
                }
                if (frames.len() as u64) * (cw as u64) * (ch as u64) > MAX_ANIMATION_PIXELS {
                    return Err(Error::Parse(
                        "gif: animation exceeds the total pixel budget".into(),
                    ));
                }
                // GCE state is consumed by the image it precedes.
                (delay_cs, transparent, disposal) = (0, None, Disposal::Keep);
            }
            other => {
                return Err(Error::Parse(format!(
                    "gif: unknown block introducer 0x{other:02X}"
                )))
            }
        }
    }

    if frames.is_empty() {
        return Err(Error::Parse("gif: no image data".into()));
    }
    Ok(Animation {
        frames,
        loop_count,
        width: cw,
        height: ch,
    })
}

fn read_palette(bytes: &[u8], pos: &mut usize, entries: usize) -> Result<Vec<Rgba>> {
    let end = *pos + entries * 3;
    let raw = bytes
        .get(*pos..end)
        .ok_or_else(|| Error::Parse("gif: truncated color table".into()))?;
    *pos = end;
    Ok(raw
        .chunks_exact(3)
        .map(|c| Rgba::rgb(c[0], c[1], c[2]))
        .collect())
}

/// Read a chain of length-prefixed sub-blocks into one buffer.
fn read_sub_blocks(bytes: &[u8], pos: &mut usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let n = *bytes
            .get(*pos)
            .ok_or_else(|| Error::Parse("gif: truncated sub-block chain".into()))?
            as usize;
        *pos += 1;
        if n == 0 {
            return Ok(out);
        }
        let chunk = bytes
            .get(*pos..*pos + n)
            .ok_or_else(|| Error::Parse("gif: truncated sub-block".into()))?;
        out.extend_from_slice(chunk);
        *pos += n;
        // A sub-block chain cannot legitimately exceed the canvas
        // budget's worth of indices.
        if out.len() as u64 > MAX_PIXELS {
            return Err(Error::Parse("gif: sub-block chain exceeds budget".into()));
        }
    }
}

/// One Image Descriptor + its LZW data, composited onto `canvas`.
#[allow(clippy::too_many_arguments)]
fn read_image(
    bytes: &[u8],
    pos: &mut usize,
    canvas: &mut Bitmap,
    global: Option<&[Rgba]>,
    transparent: Option<u8>,
    disposal: Disposal,
    delay_cs: u16,
) -> Result<Frame> {
    let d = bytes
        .get(*pos..*pos + 9)
        .ok_or_else(|| Error::Parse("gif: truncated image descriptor".into()))?;
    let fx = u16::from_le_bytes([d[0], d[1]]) as u32;
    let fy = u16::from_le_bytes([d[2], d[3]]) as u32;
    let fw = u16::from_le_bytes([d[4], d[5]]) as u32;
    let fh = u16::from_le_bytes([d[6], d[7]]) as u32;
    let flags = d[8];
    *pos += 9;
    if fw == 0 || fh == 0 {
        return Err(Error::Parse("gif: zero frame dimension".into()));
    }
    if fx + fw > canvas.width() || fy + fh > canvas.height() {
        return Err(Error::Parse(format!(
            "gif: frame {fw}x{fh}+{fx}+{fy} runs outside the {}x{} canvas",
            canvas.width(),
            canvas.height()
        )));
    }
    let local = if flags & 0x80 != 0 {
        let n = 2usize << (flags & 0x07);
        Some(read_palette(bytes, pos, n)?)
    } else {
        None
    };
    let palette = local.as_deref().or(global).ok_or_else(|| {
        Error::Parse("gif: frame has neither a local nor a global palette".into())
    })?;
    let interlaced = flags & 0x40 != 0;

    let min_code = *bytes
        .get(*pos)
        .ok_or_else(|| Error::Parse("gif: truncated LZW header".into()))?;
    *pos += 1;
    let data = read_sub_blocks(bytes, pos)?;
    let indices = lzw_decode(&data, min_code, (fw as usize) * (fh as usize))?;

    // Save what the "restore previous" disposal will need.
    let saved = (disposal == Disposal::Previous).then(|| canvas.crop(fx, fy, fw, fh));

    for row in 0..fh as usize {
        // Interlaced GIFs store rows in four passes.
        let dst_row = if interlaced {
            deinterlace_row(row, fh as usize)
        } else {
            row
        };
        for col in 0..fw as usize {
            let idx = indices[row * fw as usize + col];
            if Some(idx) == transparent {
                continue; // transparent index leaves the canvas alone
            }
            let color = *palette.get(idx as usize).ok_or_else(|| {
                Error::Parse(format!("gif: color index {idx} outside the palette"))
            })?;
            canvas.set(fx + col as u32, fy + dst_row as u32, color);
        }
    }

    // The frame IS the whole canvas after compositing — playback wants
    // complete images, not deltas it has to replay.
    let image = canvas.clone();

    match disposal {
        Disposal::Keep => {}
        Disposal::Background => {
            for y in fy..fy + fh {
                for x in fx..fx + fw {
                    canvas.set(x, y, Rgba::TRANSPARENT);
                }
            }
        }
        Disposal::Previous => {
            if let Some(prev) = saved {
                for y in 0..fh {
                    for x in 0..fw {
                        if let Some(c) = prev.get(x, y) {
                            canvas.set(fx + x, fy + y, c);
                        }
                    }
                }
            }
        }
    }

    Ok(Frame {
        image,
        // Browsers clamp 0 and 1 centisecond delays to 100 ms; a GIF
        // written with delay 0 means "as fast as you can", which in a
        // terminal means "unwatchable". Match the browser rule.
        delay: Duration::from_millis(if delay_cs <= 1 {
            100
        } else {
            delay_cs as u64 * 10
        }),
    })
}

/// GIF interlacing: rows arrive in passes 0/8, 4/8, 2/4, 1/2.
fn deinterlace_row(row: usize, height: usize) -> usize {
    let p1 = height.div_ceil(8);
    let p2 = p1 + (height.saturating_sub(4)).div_ceil(8);
    let p3 = p2 + (height.saturating_sub(2)).div_ceil(4);
    if row < p1 {
        row * 8
    } else if row < p2 {
        (row - p1) * 8 + 4
    } else if row < p3 {
        (row - p2) * 4 + 2
    } else {
        (row - p3) * 2 + 1
    }
}

/// Variable-width LZW (GIF flavor: codes grow after the dictionary
/// fills, CLEAR resets, codes are packed LSB-first).
fn lzw_decode(data: &[u8], min_code_size: u8, expected: usize) -> Result<Vec<u8>> {
    if !(2..=11).contains(&min_code_size) {
        return Err(Error::Parse(format!(
            "gif: LZW minimum code size {min_code_size} outside 2..=11"
        )));
    }
    let clear = 1u16 << min_code_size;
    let end = clear + 1;
    // Dictionary entries are (prefix, byte); the first `clear` entries
    // are the roots. Storing a prefix INDEX instead of a growing Vec
    // per entry keeps the whole table flat and bounded (4096 * 3 B).
    let mut prefix = vec![u16::MAX; 4096];
    let mut suffix = vec![0u8; 4096];
    for i in 0..clear {
        suffix[i as usize] = i as u8;
    }
    let mut next = end + 1;
    let mut code_size = min_code_size + 1;
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    let mut prev: Option<u16> = None;
    let mut scratch: Vec<u8> = Vec::with_capacity(4096);

    let (mut bit_pos, mut acc, mut acc_bits) = (0usize, 0u32, 0u32);
    loop {
        // Refill: GIF packs codes little-endian, low bit first.
        while acc_bits < code_size as u32 {
            let Some(&b) = data.get(bit_pos) else {
                // Ran out of codes without an end marker: accept what
                // we have if the frame is complete, reject otherwise.
                if out.len() == expected {
                    return Ok(out);
                }
                return Err(Error::Parse(
                    "gif: LZW data ended before the frame was complete".into(),
                ));
            };
            bit_pos += 1;
            acc |= (b as u32) << acc_bits;
            acc_bits += 8;
        }
        let code = (acc & ((1u32 << code_size) - 1)) as u16;
        acc >>= code_size;
        acc_bits -= code_size as u32;

        if code == clear {
            next = end + 1;
            code_size = min_code_size + 1;
            prev = None;
            continue;
        }
        if code == end {
            break;
        }

        // Expand the code into `scratch` by walking its prefix chain.
        let first = {
            let start = if code < next {
                code
            } else if let Some(p) = prev {
                // The KwKwK case: this code is the one being defined.
                p
            } else {
                return Err(Error::Parse(
                    "gif: LZW code before any dictionary entry".into(),
                ));
            };
            scratch.clear();
            let mut walk = start;
            loop {
                scratch.push(suffix[walk as usize]);
                match prefix[walk as usize] {
                    u16::MAX => break,
                    p => {
                        walk = p;
                        if scratch.len() > 4096 {
                            return Err(Error::Parse("gif: cyclic LZW dictionary".into()));
                        }
                    }
                }
            }
            let first = *scratch.last().unwrap();
            if code >= next {
                // KwKwK: the entry is prefix + its own first byte.
                scratch.insert(0, first);
            }
            scratch.reverse();
            first
        };

        if out.len() + scratch.len() > expected {
            return Err(Error::Parse("gif: LZW output overruns the frame".into()));
        }
        out.extend_from_slice(&scratch);

        // Define prev + first as the next dictionary entry.
        if let Some(p) = prev {
            if next < 4096 {
                prefix[next as usize] = p;
                suffix[next as usize] = first;
                next += 1;
                if next as u32 == (1u32 << code_size) && code_size < 12 {
                    code_size += 1;
                }
            }
        }
        prev = Some(code.min(next.saturating_sub(1)));
    }

    if out.len() != expected {
        return Err(Error::Parse(format!(
            "gif: frame decoded {} pixels, descriptor declares {expected}",
            out.len()
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::gfx::anim_fixtures as fx;
    use crate::gfx::gif::decode;

    /// The fixture's ground truth, frame by frame: a red bar stepping
    /// one column of three per frame over a fixed gradient. GIF is
    /// LOSSLESS, so this is exact equality, not a tolerance.
    #[test]
    fn decodes_every_frame_exactly() {
        let a = decode(fx::GIF).unwrap();
        assert_eq!(a.len(), fx::FRAMES);
        assert_eq!((a.width, a.height), fx::SIZE);
        assert_eq!(a.loop_count, None, "loop 0 means forever");
        for (i, frame) in a.frames.iter().enumerate() {
            assert_eq!(
                frame.delay,
                std::time::Duration::from_millis(100),
                "frame {i} delay"
            );
            for y in 0..fx::SIZE.1 {
                for x in 0..fx::SIZE.0 {
                    let got = frame.image.get(x, y).unwrap();
                    let (r, g, b) = fx::expected_rgb(i, x, y);
                    assert_eq!((got.r, got.g, got.b), (r, g, b), "frame {i} at ({x},{y})");
                    assert_eq!(got.a, 255, "opaque fixture");
                }
            }
        }
    }

    /// A GIF is also a still image: the first frame is what a viewer
    /// shows before anything moves.
    #[test]
    fn routes_through_the_still_decoder_as_frame_zero() {
        let still = crate::gfx::decode_image(fx::GIF).unwrap();
        let a = decode(fx::GIF).unwrap();
        assert_eq!(still.pixels(), a.frames[0].image.pixels());
    }

    #[test]
    fn truncation_ladder_never_panics() {
        for cut in 0..fx::GIF.len() {
            assert!(decode(&fx::GIF[..cut]).is_err(), "prefix {cut} decoded");
        }
    }

    #[test]
    fn byte_soup_never_panics() {
        let mut state = 0x1234_5678u32;
        let mut rand = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..800 {
            let mut b = fx::GIF.to_vec();
            for _ in 0..1 + rand() % 8 {
                let off = (rand() as usize) % b.len();
                b[off] ^= (rand() & 0xFF) as u8 | 1;
            }
            let _ = decode(&b);
        }
    }

    #[test]
    fn garbage_inputs_reject_by_name() {
        assert!(decode(b"").unwrap_err().to_string().contains("signature"));
        assert!(
            decode(b"GIF89a").is_err(),
            "header without a screen descriptor"
        );
    }
}
