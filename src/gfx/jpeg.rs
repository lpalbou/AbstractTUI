//! Baseline JPEG decoder (ITU T.81 sequential DCT, Huffman, 8-bit).
//!
//! Scope (cycle 5, deliberately tight — the flagship asset's textures,
//! not the whole format):
//!
//! - DECODES: SOF0/SOF1 (baseline + extended-sequential Huffman, both
//!   8-bit), YCbCr (3 components) and grayscale (1), sampling factors
//!   1..=2 per axis (4:4:4, 4:2:0, 4:2:2, 4:4:0 all fall out of the
//!   general MCU walk), restart markers (DRI/RSTn), stuffed bytes,
//!   APPn/COM skipped (EXIF ignored, JFIF assumed for color).
//! - REJECTS BY NAME: progressive (SOF2), lossless (SOF3),
//!   differential/hierarchical (SOF5-7, DHP/EXP), arithmetic coding
//!   (SOF9-11/13-15, DAC), 12-bit precision, 16-bit quant tables,
//!   4-component (CMYK) files, sampling factors > 2, multi-scan
//!   sequential files.
//! - Chroma upsampling is NEAREST (pixel replication): textures at
//!   terminal resolutions cannot show the difference; a smooth
//!   upsampler is a measured decision for later, not a default cost.
//! - Guards: pixel budget shared with PNG (`png::MAX_PIXELS`) checked
//!   BEFORE allocation; every truncation is a named error; marker soup
//!   never panics (fuzzed in tests).
//!
//! IDCT: naive separable floating-point (see `jpeg_dsp` — correctness
//! over speed, texture decode is one-time; measured in the cycle-5
//! report).

use crate::base::{Error, Result, Rgba};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::jpeg_dsp::{dequantize, idct_8x8, ycbcr_to_rgb};
use crate::gfx::jpeg_entropy::{decode_block, BitReader, HuffTable};
use crate::gfx::png::MAX_PIXELS;

struct Component {
    /// SOF-declared component identifier (C_i) — SOS scan selectors
    /// must reference these (RT5-2).
    id: u8,
    h: u32,
    v: u32,
    tq: usize,
    dc_tbl: usize,
    ac_tbl: usize,
    plane: Vec<u8>,
    plane_w: usize,
}

struct Frame {
    w: u32,
    h: u32,
    components: Vec<Component>,
    max_h: u32,
    max_v: u32,
}

/// Decode a JPEG byte stream into an opaque RGBA bitmap.
pub fn decode(bytes: &[u8]) -> Result<Bitmap> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(Error::Parse("jpeg: missing SOI marker".into()));
    }
    let mut pos = 2usize;
    let mut quant: [Option<[u16; 64]>; 4] = [None, None, None, None];
    let mut dc_tables: [Option<HuffTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<HuffTable>; 4] = [None, None, None, None];
    let mut restart_interval = 0u32;
    let mut frame: Option<Frame> = None;
    let mut scanned = false;

    loop {
        // Marker: fill bytes (0xFF) then the marker code.
        let mut ff = pos;
        while bytes.get(ff) == Some(&0xFF) {
            ff += 1;
        }
        if ff == pos || ff >= bytes.len() {
            return Err(Error::Parse("jpeg: truncated marker stream".into()));
        }
        let marker = bytes[ff];
        pos = ff + 1;
        match marker {
            0xD9 => break,                  // EOI
            0x01 | 0xD0..=0xD7 => continue, // standalone (stray RST tolerated between segments)
            0xC2 => {
                return Err(Error::Parse(
                    "jpeg: progressive JPEG not supported (baseline only)".into(),
                ))
            }
            0xC3 => return Err(Error::Parse("jpeg: lossless JPEG not supported".into())),
            0xC5..=0xC7 => {
                return Err(Error::Parse(
                    "jpeg: differential/hierarchical JPEG not supported".into(),
                ))
            }
            0xC9..=0xCF => {
                return Err(Error::Parse(
                    "jpeg: arithmetic-coded JPEG not supported".into(),
                ))
            }
            _ => {}
        }

        // Everything else carries a big-endian length that includes
        // its own two bytes.
        if bytes.len() - pos < 2 {
            return Err(Error::Parse("jpeg: truncated segment length".into()));
        }
        let len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        if len < 2 || bytes.len() - pos < len {
            return Err(Error::Parse(
                "jpeg: segment length runs past the file".into(),
            ));
        }
        let seg = &bytes[pos + 2..pos + len];
        pos += len;

        match marker {
            0xDB => parse_dqt(seg, &mut quant)?,
            0xC4 => parse_dht(seg, &mut dc_tables, &mut ac_tables)?,
            0xC0 | 0xC1 => {
                if frame.is_some() {
                    return Err(Error::Parse("jpeg: multiple frames".into()));
                }
                frame = Some(parse_sof(seg)?);
            }
            0xDD => {
                if seg.len() != 2 {
                    return Err(Error::Parse("jpeg: bad DRI length".into()));
                }
                restart_interval = u16::from_be_bytes([seg[0], seg[1]]) as u32;
            }
            0xDA => {
                let f = frame
                    .as_mut()
                    .ok_or_else(|| Error::Parse("jpeg: SOS before SOF".into()))?;
                let consumed = decode_scan(
                    seg,
                    &bytes[pos..],
                    f,
                    &quant,
                    &dc_tables,
                    &ac_tables,
                    restart_interval,
                )?;
                pos += consumed;
                scanned = true;
            }
            // APPn, COM, and unknown length-carrying markers: skipped.
            _ => {}
        }
    }

    let frame = frame.ok_or_else(|| Error::Parse("jpeg: no frame header".into()))?;
    if !scanned {
        return Err(Error::Parse("jpeg: no scan data before EOI".into()));
    }
    assemble(&frame)
}

fn parse_dqt(mut seg: &[u8], quant: &mut [Option<[u16; 64]>; 4]) -> Result<()> {
    while !seg.is_empty() {
        let pq = seg[0] >> 4;
        let tq = (seg[0] & 0x0F) as usize;
        if pq == 1 {
            return Err(Error::Parse(
                "jpeg: 16-bit quantization tables not supported (baseline is 8-bit)".into(),
            ));
        }
        if pq > 1 || tq > 3 {
            return Err(Error::Parse(format!(
                "jpeg: bad DQT precision/id {pq}/{tq}"
            )));
        }
        if seg.len() < 65 {
            return Err(Error::Parse("jpeg: truncated DQT".into()));
        }
        let mut t = [0u16; 64];
        for (i, v) in seg[1..65].iter().enumerate() {
            if *v == 0 {
                return Err(Error::Parse("jpeg: zero quantizer".into()));
            }
            t[i] = *v as u16;
        }
        quant[tq] = Some(t);
        seg = &seg[65..];
    }
    Ok(())
}

fn parse_dht(
    mut seg: &[u8],
    dc: &mut [Option<HuffTable>; 4],
    ac: &mut [Option<HuffTable>; 4],
) -> Result<()> {
    while !seg.is_empty() {
        if seg.len() < 17 {
            return Err(Error::Parse("jpeg: truncated DHT".into()));
        }
        let tc = seg[0] >> 4;
        let th = (seg[0] & 0x0F) as usize;
        if tc > 1 || th > 3 {
            return Err(Error::Parse(format!("jpeg: bad DHT class/id {tc}/{th}")));
        }
        let mut counts = [0u8; 16];
        counts.copy_from_slice(&seg[1..17]);
        let total: usize = counts.iter().map(|&c| c as usize).sum();
        if seg.len() < 17 + total {
            return Err(Error::Parse("jpeg: DHT symbols truncated".into()));
        }
        let table = HuffTable::build(&counts, &seg[17..17 + total])?;
        if tc == 0 {
            dc[th] = Some(table);
        } else {
            ac[th] = Some(table);
        }
        seg = &seg[17 + total..];
    }
    Ok(())
}

fn parse_sof(seg: &[u8]) -> Result<Frame> {
    if seg.len() < 6 {
        return Err(Error::Parse("jpeg: truncated SOF".into()));
    }
    let precision = seg[0];
    if precision != 8 {
        return Err(Error::Parse(format!(
            "jpeg: {precision}-bit precision not supported (8-bit baseline only)"
        )));
    }
    let h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
    let w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
    if w == 0 || h == 0 {
        return Err(Error::Parse("jpeg: zero dimension".into()));
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(Error::Parse(format!("jpeg: {w}x{h} exceeds pixel budget")));
    }
    let nf = seg[5] as usize;
    if nf != 1 && nf != 3 {
        return Err(Error::Parse(format!(
            "jpeg: {nf}-component images not supported (grayscale or YCbCr only; CMYK rejected)"
        )));
    }
    if seg.len() < 6 + nf * 3 {
        return Err(Error::Parse("jpeg: truncated SOF components".into()));
    }
    let mut components = Vec::with_capacity(nf);
    for i in 0..nf {
        let c = &seg[6 + i * 3..9 + i * 3];
        // Grayscale MCUs are always one block: sampling factors carry
        // no meaning for a single-component scan (T.81 A.2.2) — clamp.
        let (mut hh, mut vv) = ((c[1] >> 4) as u32, (c[1] & 0x0F) as u32);
        if nf == 1 {
            hh = 1;
            vv = 1;
        }
        if hh == 0 || vv == 0 || hh > 2 || vv > 2 {
            return Err(Error::Parse(format!(
                "jpeg: sampling factor {hh}x{vv} not supported (1..=2)"
            )));
        }
        let tq = (c[2] & 0x0F) as usize;
        if tq > 3 {
            return Err(Error::Parse("jpeg: quant table id > 3".into()));
        }
        components.push(Component {
            id: c[0],
            h: hh,
            v: vv,
            tq,
            dc_tbl: 0,
            ac_tbl: 0,
            plane: Vec::new(),
            plane_w: 0,
        });
    }
    let max_h = components.iter().map(|c| c.h).max().unwrap_or(1);
    let max_v = components.iter().map(|c| c.v).max().unwrap_or(1);
    Ok(Frame {
        w,
        h,
        components,
        max_h,
        max_v,
    })
}

/// Decode the entropy-coded scan; returns bytes consumed AFTER the SOS
/// segment (the caller's marker walk resumes there).
fn decode_scan(
    header: &[u8],
    data: &[u8],
    f: &mut Frame,
    quant: &[Option<[u16; 64]>; 4],
    dc_tables: &[Option<HuffTable>; 4],
    ac_tables: &[Option<HuffTable>; 4],
    restart_interval: u32,
) -> Result<usize> {
    // Scan header: component selectors + entropy table bindings.
    if header.is_empty() || header.len() < 1 + header[0] as usize * 2 + 3 {
        return Err(Error::Parse("jpeg: truncated SOS header".into()));
    }
    let ns = header[0] as usize;
    if ns != f.components.len() {
        return Err(Error::Parse(
            "jpeg: multi-scan sequential JPEG not supported (one interleaved scan only)".into(),
        ));
    }
    for i in 0..ns {
        // RT5-2: the scan component selector (Cs_i) must reference a
        // SOF-DECLARED component id — a malformed selector used to be
        // silently accepted (harmless only by accident of positional
        // binding). T.81 also lets a scan REORDER components, which
        // would reorder data units inside each MCU; this decoder reads
        // MCUs in frame order, so reordered scans reject by name
        // rather than decode wrong.
        let cs = header[1 + i * 2];
        if f.components[i].id != cs {
            let known = f.components.iter().any(|c| c.id == cs);
            return Err(Error::Parse(if known {
                format!("jpeg: scan reorders component selector {cs} (frame order only)")
            } else {
                format!("jpeg: scan component selector {cs} not declared in SOF")
            }));
        }
        let td = (header[2 + i * 2] >> 4) as usize;
        let ta = (header[2 + i * 2] & 0x0F) as usize;
        if td > 3 || ta > 3 {
            return Err(Error::Parse("jpeg: entropy table id > 3".into()));
        }
        f.components[i].dc_tbl = td;
        f.components[i].ac_tbl = ta;
    }

    // Plane allocation (budget-checked geometry: ≤ 2x the checked
    // image dims per axis).
    let mcus_x = f.w.div_ceil(8 * f.max_h) as usize;
    let mcus_y = f.h.div_ceil(8 * f.max_v) as usize;
    for c in &mut f.components {
        let pw = mcus_x * (c.h as usize) * 8;
        let ph = mcus_y * (c.v as usize) * 8;
        c.plane_w = pw;
        c.plane = vec![0u8; pw * ph];
    }

    let mut reader = BitReader::new(data);
    let mut dc_pred = vec![0i32; f.components.len()];
    let mut rst_n = 0u8;
    let mut block = [0u8; 64];

    for mcu in 0..mcus_x * mcus_y {
        if restart_interval > 0 && mcu > 0 && (mcu as u32).is_multiple_of(restart_interval) {
            reader.expect_restart(rst_n)?;
            rst_n = (rst_n + 1) & 7;
            dc_pred.fill(0);
        }
        let (mcu_x, mcu_y) = (mcu % mcus_x, mcu / mcus_x);
        for (ci, c) in f.components.iter_mut().enumerate() {
            let dc = dc_tables[c.dc_tbl]
                .as_ref()
                .ok_or_else(|| Error::Parse(format!("jpeg: missing DC table {}", c.dc_tbl)))?;
            let ac = ac_tables[c.ac_tbl]
                .as_ref()
                .ok_or_else(|| Error::Parse(format!("jpeg: missing AC table {}", c.ac_tbl)))?;
            let qt = quant[c.tq]
                .as_ref()
                .ok_or_else(|| Error::Parse(format!("jpeg: missing quant table {}", c.tq)))?;
            for by in 0..c.v as usize {
                for bx in 0..c.h as usize {
                    let zz = decode_block(&mut reader, dc, ac, &mut dc_pred[ci])?;
                    let coef = dequantize(&zz, qt);
                    idct_8x8(&coef, &mut block);
                    // Blit the 8x8 into the component plane.
                    let px = (mcu_x * c.h as usize + bx) * 8;
                    let py = (mcu_y * c.v as usize + by) * 8;
                    for (row, chunk) in block.chunks_exact(8).enumerate() {
                        let start = (py + row) * c.plane_w + px;
                        c.plane[start..start + 8].copy_from_slice(chunk);
                    }
                }
            }
        }
    }
    Ok(reader.byte_pos())
}

/// Component planes -> RGBA bitmap (nearest chroma upsampling).
fn assemble(f: &Frame) -> Result<Bitmap> {
    let (w, h) = (f.w, f.h);
    let mut px = Vec::with_capacity((w as usize) * (h as usize));
    let sample = |c: &Component, x: u32, y: u32, f: &Frame| -> u8 {
        let sx = (x * c.h / f.max_h) as usize;
        let sy = (y * c.v / f.max_v) as usize;
        c.plane[sy * c.plane_w + sx.min(c.plane_w - 1)]
    };
    match f.components.len() {
        1 => {
            let c = &f.components[0];
            for y in 0..h {
                for x in 0..w {
                    let v = sample(c, x, y, f);
                    px.push(Rgba::rgb(v, v, v));
                }
            }
        }
        3 => {
            for y in 0..h {
                for x in 0..w {
                    let yy = sample(&f.components[0], x, y, f);
                    let cb = sample(&f.components[1], x, y, f);
                    let cr = sample(&f.components[2], x, y, f);
                    let (r, g, b) = ycbcr_to_rgb(yy, cb, cr);
                    px.push(Rgba::rgb(r, g, b));
                }
            }
        }
        n => return Err(Error::Parse(format!("jpeg: {n} components at assembly"))),
    }
    Bitmap::from_pixels(w, h, px).ok_or_else(|| Error::Parse("jpeg: pixel count mismatch".into()))
}

#[cfg(test)]
mod tests {
    /// RT5-2 closure: SOS component selectors must reference SOF-
    /// declared ids; reordered scans reject by name (positional MCU
    /// decode would silently produce wrong pixels otherwise).
    #[test]
    fn sos_selector_validation_rejects_by_name() {
        let base = crate::gfx::jpeg_fixtures::GRAD444;
        // Locate the SOS marker (FFDA): [len_hi, len_lo, ns, Cs1, Td/Ta1, ...]
        let sos = base
            .windows(2)
            .position(|w| w == [0xFF, 0xDA])
            .expect("fixture has SOS");
        let cs1 = sos + 5; // FFDA(2) + len(2) + ns(1) -> first selector
        assert_eq!(base[sos + 4], 3, "YCbCr fixture: 3 scan components");

        // Sanity: the untouched fixture decodes.
        crate::gfx::jpeg::decode(base).unwrap();

        // Undeclared selector: named rejection.
        let mut bad = base.to_vec();
        bad[cs1] = 0x99;
        let err = crate::gfx::jpeg::decode(&bad).unwrap_err();
        assert!(err.to_string().contains("not declared in SOF"), "{err}");

        // Reordered (but declared) selectors: named rejection, never a
        // silently wrong decode. Swap Cs1 and Cs2.
        let mut swapped = base.to_vec();
        swapped.swap(cs1, cs1 + 2);
        let err = crate::gfx::jpeg::decode(&swapped).unwrap_err();
        assert!(err.to_string().contains("reorders"), "{err}");
    }

    use super::*;
    use crate::gfx::jpeg_fixtures as fx;

    /// Decode a fixture and compare against the generator formula.
    /// Tolerances: q92 JPEG on a smooth gradient stays within a few
    /// codes; chroma subsampling adds a little on the color channels.
    fn assert_close_rgb(name: &str, bytes: &[u8], max_err: i32, mean_budget: f32) {
        let img = decode(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((img.width(), img.height()), (16, 16), "{name}");
        let mut total = 0i64;
        for y in 0..16 {
            for x in 0..16 {
                let got = img.get(x, y).unwrap();
                let (r, g, b) = fx::expected_rgb(x, y);
                for (a, e) in [(got.r, r), (got.g, g), (got.b, b)] {
                    let d = (a as i32 - e as i32).abs();
                    assert!(
                        d <= max_err,
                        "{name}: ({x},{y}) off by {d} (got {got:?}, want {r},{g},{b})"
                    );
                    total += d as i64;
                }
            }
        }
        let mean = total as f32 / (16.0 * 16.0 * 3.0);
        assert!(mean <= mean_budget, "{name}: mean error {mean}");
    }

    #[test]
    fn decodes_444() {
        assert_close_rgb("4:4:4", fx::GRAD444, 14, 4.0);
    }

    #[test]
    fn decodes_420() {
        assert_close_rgb("4:2:0", fx::GRAD420, 20, 6.0);
    }

    #[test]
    fn decodes_422() {
        assert_close_rgb("4:2:2", fx::GRAD422, 20, 6.0);
    }

    #[test]
    fn decodes_420_with_restart_markers() {
        // -restart 1: an RSTn between every MCU row — exercises DRI,
        // marker consumption and DC predictor resets.
        assert_close_rgb("4:2:0+RST", fx::GRAD420RST, 20, 6.0);
    }

    #[test]
    fn decodes_grayscale() {
        let img = decode(fx::GRAY).unwrap();
        assert_eq!((img.width(), img.height()), (16, 16));
        for y in 0..16 {
            for x in 0..16 {
                let got = img.get(x, y).unwrap();
                assert_eq!(got.r, got.g, "gray must be neutral");
                assert_eq!(got.g, got.b);
                let d = (got.r as i32 - fx::expected_gray(x, y) as i32).abs();
                assert!(d <= 12, "({x},{y}) off by {d}");
            }
        }
    }

    #[test]
    fn progressive_rejected_by_name() {
        let err = decode(fx::GRADPROG).unwrap_err();
        assert!(err.to_string().contains("progressive"), "{err}");
    }

    #[test]
    fn arithmetic_rejected_by_name() {
        // Patch the SOF0 marker of a valid fixture into SOF9 (0xC9).
        let mut bytes = fx::GRAD444.to_vec();
        let sof = bytes.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        bytes[sof + 1] = 0xC9;
        let err = decode(&bytes).unwrap_err();
        assert!(err.to_string().contains("arithmetic"), "{err}");
    }

    #[test]
    fn dimension_bomb_guarded_before_allocation() {
        // Patch SOF dims to 65535x65535 (4.29 G px).
        let mut bytes = fx::GRAD444.to_vec();
        let sof = bytes.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        // SOF payload: len(2) precision(1) h(2) w(2)...
        for i in 0..4 {
            bytes[sof + 5 + i] = 0xFF;
        }
        let err = decode(&bytes).unwrap_err();
        assert!(err.to_string().contains("pixel budget"), "{err}");
    }

    #[test]
    fn truncation_ladder_never_panics() {
        let full = fx::GRAD420;
        for cut in 0..full.len() {
            assert!(decode(&full[..cut]).is_err(), "prefix {cut} decoded");
        }
    }

    #[test]
    fn marker_soup_fuzz_never_panics() {
        // Deterministic xorshift mutations of a valid fixture: byte
        // stomps, truncations, splices. Any Result is fine; panics are
        // the failure.
        let base = fx::GRAD420;
        let mut state = 0x9E3779B9u32;
        let mut rand = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..600 {
            let mut b = base.to_vec();
            match rand() % 3 {
                0 => {
                    let cut = (rand() as usize) % b.len();
                    b.truncate(cut);
                }
                1 => {
                    for _ in 0..1 + rand() % 8 {
                        let off = (rand() as usize) % b.len();
                        b[off] ^= (rand() & 0xFF) as u8 | 1;
                    }
                }
                _ => {
                    let at = (rand() as usize) % b.len();
                    let garbage: Vec<u8> =
                        (0..(rand() % 24)).map(|_| (rand() & 0xFF) as u8).collect();
                    b.splice(at..at, garbage);
                }
            }
            let _ = decode(&b);
        }
    }

    #[test]
    fn garbage_and_empty_inputs() {
        assert!(decode(&[]).is_err());
        assert!(decode(b"not a jpeg at all").is_err());
        assert!(decode(&[0xFF, 0xD8]).is_err(), "SOI alone");
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err(), "no frame/scan");
    }
}
