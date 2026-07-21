//! Sixel emitter (`DCS P1;P2;P3 q ... ST`), the ladder's third rung.
//!
//! Encoding facts (VT330/340 programmer reference ch. 14, research
//! record in docs/design/gfx-three.md §2.3):
//!
//! - Data bytes `0x3F..=0x7E` encode a 1-wide, 6-tall column slice:
//!   `byte - 63` is the row bitmask, LSB = top row of the band.
//! - `P2=1` leaves zero bits untouched (transparency); we always emit
//!   it and give fully-transparent pixels no bits in any register.
//! - Color registers: `#Pc;2;Pr;Pg;Pb` with channels scaled 0..=100
//!   (a real fidelity loss vs 0..=255 — rounded, documented, and
//!   absorbed by dithering); bare `#Pc` selects the register.
//! - `!N<byte>` repeats a byte N times, `$` rewinds to the band start
//!   (for the next register's pass), `-` advances to the next band.
//! - Raster attributes `"Pan;Pad;Ph;Pv` (aspect 1:1 + size hint) come
//!   before any data.
//!
//! ## Register strategy (RT1-11 ruling)
//!
//! Registers are terminal-global on xterm-class implementations: two
//! sixel images alive on screen with independent palettes clobber each
//! other's registers (the last emission wins; earlier imagery recolors
//! on its next repaint). v1 policy, chosen deliberately:
//!
//! **One palette per emission, registers `[base, base+N)`, default
//! base 0, N = 64 — single-live-image is a documented v1 limit.**
//!
//! Why not partitioning now: a static split (e.g. 4 x 64) caps every
//! image's fidelity to pay for a multi-image case the ladder mostly
//! avoids (kitty/iTerm2 terminals take the higher rungs; sixel-only
//! terminals in this engine's v1 show one image surface at a time),
//! and a dynamic allocator needs engine-global state that pure
//! emitters must not own. The `register_base` option is the
//! forward-compat seam: when the pipeline learns to host N live sixel
//! images it can partition without touching this module. Trade-off
//! recorded in docs/design/gfx-three.md §2.3; REDTEAM's two-image
//! golden should pin the documented clobber behavior.

use crate::gfx::bitmap::Bitmap;
use crate::gfx::dither;
use crate::gfx::quantize;

/// Hardware ceiling on modern emulators (xterm et al.): 256 registers.
pub const MAX_REGISTERS: u16 = 256;

#[derive(Clone, Debug)]
pub struct Options {
    /// Palette budget. Clamped to `2..=MAX_REGISTERS - register_base`.
    /// 64 is the fidelity/byte-cost sweet spot with dithering on.
    pub max_registers: u16,
    /// Floyd–Steinberg error diffusion before encoding (hides most of
    /// the palette + 0..=100 channel quantization loss).
    pub dither: bool,
    /// First hardware register to define (RT1-11 forward-compat knob).
    pub register_base: u16,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_registers: 64,
            dither: true,
            register_base: 0,
        }
    }
}

/// Encode a bitmap as a complete sixel DCS sequence. Fully-transparent
/// pixels become P2=1 holes. Returns an empty Vec for an empty bitmap.
pub fn encode(img: &Bitmap, opts: &Options) -> Vec<u8> {
    let w = img.width() as usize;
    let h = img.height() as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let base = opts.register_base.min(MAX_REGISTERS - 2);
    let budget = opts.max_registers.clamp(2, MAX_REGISTERS - base) as usize;

    // Quantize, then map every pixel to a register index (usize::MAX =
    // transparent hole).
    let palette = quantize::median_cut(img.pixels(), budget);
    if palette.is_empty() {
        return Vec::new(); // nothing visible
    }
    let indices: Vec<usize> = if opts.dither {
        let mut work = img.clone();
        dither::floyd_steinberg(&mut work, &palette)
    } else {
        img.pixels()
            .iter()
            .map(|p| {
                if p.a == 0 {
                    usize::MAX
                } else {
                    quantize::nearest_index(&palette, *p)
                }
            })
            .collect()
    };

    let mut out = Vec::with_capacity(w * h / 4 + palette.len() * 16 + 64);
    // DCS P1=0 (aspect from raster attrs) ; P2=1 (transparent zeros) ;
    // P3=0, then 'q'.
    out.extend_from_slice(b"\x1bP0;1;0q");
    // Raster attributes: 1:1 aspect, pixel dimensions.
    out.extend_from_slice(format!("\"1;1;{w};{h}").as_bytes());

    // Palette definitions, RGB percent-scaled with rounding.
    let pct = |v: u8| -> u32 { (v as u32 * 100 + 127) / 255 };
    for (i, c) in palette.iter().enumerate() {
        let reg = base as usize + i;
        out.extend_from_slice(
            format!("#{reg};2;{};{};{}", pct(c.r), pct(c.g), pct(c.b)).as_bytes(),
        );
    }

    // Band emission. Scratch buffers reused across bands.
    let bands = h.div_ceil(6);
    let mut present = vec![false; palette.len()];
    let mut masks: Vec<u8> = vec![0; w];
    for band in 0..bands {
        let y0 = band * 6;
        let rows = (h - y0).min(6);

        // Which registers appear in this band?
        present.fill(false);
        for dy in 0..rows {
            let row = &indices[(y0 + dy) * w..(y0 + dy) * w + w];
            for &idx in row {
                if idx != usize::MAX {
                    present[idx] = true;
                }
            }
        }

        let mut first_pass = true;
        for (pi, _) in palette.iter().enumerate() {
            if !present[pi] {
                continue;
            }
            // Column masks for this register.
            masks.fill(0);
            for dy in 0..rows {
                let row = &indices[(y0 + dy) * w..(y0 + dy) * w + w];
                let bit = 1u8 << dy;
                for (x, &idx) in row.iter().enumerate() {
                    if idx == pi {
                        masks[x] |= bit;
                    }
                }
            }
            // Trim trailing empty columns (P2=1 leaves them untouched).
            let end = masks.iter().rposition(|&m| m != 0).map_or(0, |p| p + 1);
            if end == 0 {
                continue;
            }
            if !first_pass {
                out.push(b'$'); // rewind for the next register's pass
            }
            first_pass = false;
            out.extend_from_slice(format!("#{}", base as usize + pi).as_bytes());
            rle_emit(&masks[..end], &mut out);
        }
        if band + 1 < bands {
            out.push(b'-'); // next band
        }
    }

    out.extend_from_slice(b"\x1b\\");
    out
}

/// Run-length emit a mask row: `!N<byte>` pays off at N >= 4
/// (`!` + at-least-one-digit + byte = 3+ chars vs N repeats).
fn rle_emit(masks: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < masks.len() {
        let b = 63 + masks[i];
        let mut run = 1;
        while i + run < masks.len() && masks[i + run] == masks[i] {
            run += 1;
        }
        if run >= 4 {
            out.extend_from_slice(format!("!{run}").as_bytes());
            out.push(b);
        } else {
            for _ in 0..run {
                out.push(b);
            }
        }
        i += run;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;

    /// Minimal test-side sixel interpreter: replays an emitted stream
    /// into a pixel grid so tests verify the IMAGE, not byte trivia.
    /// Registers decode percent -> 0..=255 with the inverse rounding.
    struct Decoded {
        w: usize,
        h: usize,
        px: Vec<Option<Rgba>>, // None = untouched (transparent hole)
        transparent_p2: bool,
    }

    fn decode(bytes: &[u8]) -> Decoded {
        assert!(bytes.starts_with(b"\x1bP"), "DCS intro");
        assert!(bytes.ends_with(b"\x1b\\"), "ST");
        let body = &bytes[2..bytes.len() - 2];
        let q = body.iter().position(|&b| b == b'q').expect("sixel q");
        let params: Vec<u32> = std::str::from_utf8(&body[..q])
            .unwrap()
            .split(';')
            .map(|p| p.parse().unwrap())
            .collect();
        let transparent_p2 = params.len() > 1 && params[1] == 1;

        let mut palette: Vec<Rgba> = vec![Rgba::TRANSPARENT; 256];
        let mut cur = 0usize;
        let (mut w, mut h) = (0usize, 0usize);
        let mut px: Vec<Option<Rgba>> = Vec::new();
        let (mut x, mut band) = (0usize, 0usize);
        let mut i = q + 1;
        let data = body;

        let read_num = |i: &mut usize| -> u32 {
            let s = *i;
            while *i < data.len() && data[*i].is_ascii_digit() {
                *i += 1;
            }
            std::str::from_utf8(&data[s..*i]).unwrap().parse().unwrap()
        };

        while i < data.len() {
            match data[i] {
                b'"' => {
                    i += 1;
                    let _pan = read_num(&mut i);
                    assert_eq!(data[i], b';');
                    i += 1;
                    let _pad = read_num(&mut i);
                    assert_eq!(data[i], b';');
                    i += 1;
                    w = read_num(&mut i) as usize;
                    assert_eq!(data[i], b';');
                    i += 1;
                    h = read_num(&mut i) as usize;
                    px = vec![None; w * h];
                }
                b'#' => {
                    i += 1;
                    let reg = read_num(&mut i) as usize;
                    if i < data.len() && data[i] == b';' {
                        // definition: #reg;2;r;g;b (percent scale)
                        i += 1;
                        assert_eq!(read_num(&mut i), 2, "RGB mode");
                        let mut ch = [0u8; 3];
                        for c in &mut ch {
                            assert_eq!(data[i], b';');
                            i += 1;
                            let v = read_num(&mut i);
                            assert!(v <= 100, "percent scale");
                            *c = ((v * 255 + 50) / 100) as u8;
                        }
                        palette[reg] = Rgba::rgb(ch[0], ch[1], ch[2]);
                    }
                    cur = reg;
                }
                b'!' => {
                    i += 1;
                    let n = read_num(&mut i) as usize;
                    let mask = data[i] - 63;
                    i += 1;
                    for _ in 0..n {
                        paint(&mut px, w, h, x, band, mask, palette[cur]);
                        x += 1;
                    }
                }
                b'$' => {
                    x = 0;
                    i += 1;
                }
                b'-' => {
                    x = 0;
                    band += 1;
                    i += 1;
                }
                b'?'..=b'~' => {
                    let mask = data[i] - 63;
                    paint(&mut px, w, h, x, band, mask, palette[cur]);
                    x += 1;
                    i += 1;
                }
                other => panic!("unexpected sixel byte 0x{other:02x} at {i}"),
            }
        }
        Decoded {
            w,
            h,
            px,
            transparent_p2,
        }
    }

    fn paint(
        px: &mut [Option<Rgba>],
        w: usize,
        h: usize,
        x: usize,
        band: usize,
        mask: u8,
        color: Rgba,
    ) {
        for dy in 0..6 {
            if mask & (1 << dy) != 0 {
                let y = band * 6 + dy;
                assert!(x < w && y < h, "paint outside raster ({x},{y}) vs {w}x{h}");
                px[y * w + x] = Some(color);
            }
        }
    }

    /// Colors exact under the 0..=100 percent round-trip.
    const RED: Rgba = Rgba::rgb(255, 0, 0);
    const BLUE: Rgba = Rgba::rgb(0, 0, 255);

    #[test]
    fn two_color_image_replays_exactly() {
        // 4x6: left half red, right half blue — one band, two register
        // passes; no dithering needed (exact palette).
        let img = Bitmap::from_fn(4, 6, |x, _| if x < 2 { RED } else { BLUE });
        let bytes = encode(
            &img,
            &Options {
                dither: false,
                ..Options::default()
            },
        );
        let d = decode(&bytes);
        assert!(d.transparent_p2, "P2=1 always");
        assert_eq!((d.w, d.h), (4, 6));
        for y in 0..6 {
            for x in 0..4 {
                let expect = if x < 2 { RED } else { BLUE };
                assert_eq!(d.px[y * 4 + x], Some(expect), "({x},{y})");
            }
        }
    }

    #[test]
    fn multi_band_and_partial_last_band() {
        // 3x8: two bands, the second only 2 rows tall. Gradient of two
        // exact colors split at y=4.
        let img = Bitmap::from_fn(3, 8, |_, y| if y < 4 { RED } else { BLUE });
        let bytes = encode(
            &img,
            &Options {
                dither: false,
                ..Options::default()
            },
        );
        let d = decode(&bytes);
        assert_eq!((d.w, d.h), (3, 8));
        for y in 0..8 {
            let expect = if y < 4 { RED } else { BLUE };
            for x in 0..3 {
                assert_eq!(d.px[y * 3 + x], Some(expect), "({x},{y})");
            }
        }
    }

    #[test]
    fn transparent_pixels_stay_holes() {
        // Middle column fully transparent: with P2=1 those pixels must
        // never be painted by any register pass.
        let img = Bitmap::from_fn(3, 6, |x, _| if x == 1 { Rgba::TRANSPARENT } else { RED });
        let bytes = encode(
            &img,
            &Options {
                dither: false,
                ..Options::default()
            },
        );
        let d = decode(&bytes);
        for y in 0..6 {
            assert_eq!(d.px[y * 3], Some(RED));
            assert_eq!(d.px[y * 3 + 1], None, "hole painted at y={y}");
            assert_eq!(d.px[y * 3 + 2], Some(RED));
        }
    }

    #[test]
    fn rle_compresses_solid_runs() {
        let img = Bitmap::new(40, 6, RED);
        let bytes = encode(
            &img,
            &Options {
                dither: false,
                ..Options::default()
            },
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("!40~"), "40-wide solid run should RLE: {s}");
        // And it still replays to a full red rectangle.
        let d = decode(&bytes);
        assert!(d.px.iter().all(|p| *p == Some(RED)));
    }

    #[test]
    fn register_budget_and_base_respected() {
        // 256 unique colors, budget 8, base 100: every register token
        // must sit in [100, 108).
        let img = Bitmap::from_fn(16, 16, |x, y| Rgba::rgb((x * 16) as u8, (y * 16) as u8, 0));
        let opts = Options {
            max_registers: 8,
            dither: false,
            register_base: 100,
        };
        let bytes = encode(&img, &opts);
        let s = String::from_utf8_lossy(&bytes);
        let mut regs = std::collections::HashSet::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'#' {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                regs.insert(s[start..j].parse::<usize>().unwrap());
                i = j;
            } else {
                i += 1;
            }
        }
        assert!(!regs.is_empty());
        assert!(regs.iter().all(|&r| (100..108).contains(&r)), "{regs:?}");
    }

    #[test]
    fn dithered_gradient_stays_in_palette_and_replays() {
        // Dithering on a gradient: replayed pixels must all be palette
        // colors and the mean brightness must be preserved (that is
        // dithering's contract).
        let img = Bitmap::from_fn(32, 12, |x, _| {
            let v = (x * 8) as u8;
            Rgba::rgb(v, v, v)
        });
        let bytes = encode(
            &img,
            &Options {
                max_registers: 4,
                dither: true,
                register_base: 0,
            },
        );
        let d = decode(&bytes);
        let mut sum_in = 0u64;
        let mut sum_out = 0u64;
        for (i, p) in img.pixels().iter().enumerate() {
            sum_in += p.r as u64;
            sum_out += d.px[i].expect("opaque image, no holes").r as u64;
        }
        let (mean_in, mean_out) = (sum_in / d.px.len() as u64, sum_out / d.px.len() as u64);
        assert!(
            (mean_in as i64 - mean_out as i64).abs() <= 12,
            "brightness drift: {mean_in} -> {mean_out}"
        );
    }

    #[test]
    fn empty_and_invisible_images_emit_nothing() {
        assert!(encode(&Bitmap::new(0, 0, RED), &Options::default()).is_empty());
        assert!(encode(&Bitmap::new(4, 4, Rgba::TRANSPARENT), &Options::default()).is_empty());
    }
}
