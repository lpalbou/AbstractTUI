//! JPEG decoder (ITU T.81, 8-bit Huffman): sequential baseline AND
//! progressive.
//!
//! - DECODES: SOF0/SOF1 (baseline + extended-sequential Huffman) and
//!   SOF2 (progressive), both 8-bit; YCbCr (3 components) and
//!   grayscale (1); sampling factors 1..=2 per axis (4:4:4, 4:2:0,
//!   4:2:2, 4:4:0 all fall out of the general MCU walk); interleaved
//!   and single-component (non-interleaved) scans, so multi-scan
//!   sequential files decode too; spectral selection and successive
//!   approximation with EOB runs; restart markers (DRI/RSTn); stuffed
//!   bytes; APPn/COM skipped (EXIF ignored, JFIF assumed for color).
//! - REJECTS BY NAME: lossless (SOF3), differential/hierarchical
//!   (SOF5-7, DHP/EXP), arithmetic coding (SOF9-11/13-15, DAC), 12-bit
//!   precision, 16-bit quant tables, 4-component (CMYK) files,
//!   sampling factors > 2, scans that reorder components against the
//!   frame, and frames where some component never receives scan data.
//! - Chroma upsampling is NEAREST (pixel replication): textures at
//!   terminal resolutions cannot show the difference; a smooth
//!   upsampler is a measured decision for later, not a default cost.
//! - Guards: pixel budget shared with PNG (`png::MAX_PIXELS`) checked
//!   BEFORE any allocation; every truncation is a named error; marker
//!   soup never panics (fuzzed in tests).
//!
//! Shape: every scan decodes into a per-component COEFFICIENT plane
//! (`i16`, zigzag order) that survives across scans — that is what
//! progressive refinement needs, and the sequential path is the
//! one-scan case of it. Dequantization and the IDCT run once per
//! block after the last scan, component by component, so the
//! coefficient buffer is released as soon as its samples exist.
//!
//! IDCT: naive separable floating-point (see `jpeg_dsp` — correctness
//! over speed, texture decode is one-time; measured in the cycle-5
//! report).

use crate::base::{Error, Result, Rgba};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::jpeg_dsp::{dequantize, idct_8x8, ycbcr_to_rgb};
use crate::gfx::jpeg_entropy::{
    decode_ac_first, decode_ac_refine, decode_block, decode_dc_first, decode_dc_refine, BitReader,
    HuffTable,
};
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
    /// Blocks across/down, padded out to whole MCUs (the interleaved
    /// geometry every scan writes into).
    blocks_w: usize,
    blocks_h: usize,
    /// Blocks a NON-interleaved scan of this component walks — the
    /// component's own size in blocks, without the MCU padding
    /// (T.81 A.2.4).
    scan_blocks_w: usize,
    scan_blocks_h: usize,
    /// `blocks_w * blocks_h` blocks of 64 zigzag coefficients.
    coeffs: Vec<i16>,
    /// Quantization table captured when the component's first scan
    /// bound it (a later DQT must not retroactively change it).
    quant: Option<[u16; 64]>,
    /// DC predictor, carried across the blocks of one scan.
    dc_pred: i32,
    /// Did any scan carry data for this component?
    scanned: bool,
    plane: Vec<u8>,
    plane_w: usize,
}

struct Frame {
    w: u32,
    h: u32,
    components: Vec<Component>,
    max_h: u32,
    max_v: u32,
    progressive: bool,
    mcus_x: usize,
    mcus_y: usize,
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
            0xC0..=0xC2 => {
                if frame.is_some() {
                    return Err(Error::Parse("jpeg: multiple frames".into()));
                }
                frame = Some(parse_sof(seg, marker == 0xC2)?);
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
                // Resume the marker walk at the next real marker: a
                // scan ends on whatever byte its last code landed in,
                // and encoders pad differently.
                pos = next_marker(bytes, pos + consumed);
                scanned = true;
            }
            // APPn, COM, and unknown length-carrying markers: skipped.
            _ => {}
        }
    }

    let mut frame = frame.ok_or_else(|| Error::Parse("jpeg: no frame header".into()))?;
    if !scanned {
        return Err(Error::Parse("jpeg: no scan data before EOI".into()));
    }
    if let Some(c) = frame.components.iter().find(|c| !c.scanned) {
        return Err(Error::Parse(format!(
            "jpeg: component {} never receives scan data",
            c.id
        )));
    }
    render(&mut frame, &quant)?;
    assemble(&frame)
}

/// First position at or after `from` holding a real marker prefix
/// (`FF` followed by anything but the `00` of a stuffed byte). Returns
/// `bytes.len()` when there is none — the marker walk then reports the
/// truncation.
fn next_marker(bytes: &[u8], from: usize) -> usize {
    let mut p = from;
    while p + 1 < bytes.len() {
        if bytes[p] == 0xFF && bytes[p + 1] != 0x00 {
            return p;
        }
        p += 1;
    }
    bytes.len()
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

fn parse_sof(seg: &[u8], progressive: bool) -> Result<Frame> {
    if seg.len() < 6 {
        return Err(Error::Parse("jpeg: truncated SOF".into()));
    }
    let precision = seg[0];
    if precision != 8 {
        return Err(Error::Parse(format!(
            "jpeg: {precision}-bit precision not supported (8-bit only)"
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
        // Duplicate component ids would make scan selectors ambiguous.
        if components.iter().any(|p: &Component| p.id == c[0]) {
            return Err(Error::Parse(format!(
                "jpeg: duplicate component id {} in SOF",
                c[0]
            )));
        }
        components.push(Component {
            id: c[0],
            h: hh,
            v: vv,
            tq,
            dc_tbl: 0,
            ac_tbl: 0,
            blocks_w: 0,
            blocks_h: 0,
            scan_blocks_w: 0,
            scan_blocks_h: 0,
            coeffs: Vec::new(),
            quant: None,
            dc_pred: 0,
            scanned: false,
            plane: Vec::new(),
            plane_w: 0,
        });
    }
    let max_h = components.iter().map(|c| c.h).max().unwrap_or(1);
    let max_v = components.iter().map(|c| c.v).max().unwrap_or(1);

    // Geometry, then the coefficient planes. Both are bounded by the
    // pixel budget checked above (≤ 2x the image dims per axis, plus
    // at most one MCU of padding).
    let mcus_x = w.div_ceil(8 * max_h) as usize;
    let mcus_y = h.div_ceil(8 * max_v) as usize;
    for c in &mut components {
        c.blocks_w = mcus_x * c.h as usize;
        c.blocks_h = mcus_y * c.v as usize;
        c.scan_blocks_w = (w * c.h).div_ceil(max_h).div_ceil(8) as usize;
        c.scan_blocks_h = (h * c.v).div_ceil(max_v).div_ceil(8) as usize;
        c.coeffs = vec![0i16; c.blocks_w * c.blocks_h * 64];
    }
    Ok(Frame {
        w,
        h,
        components,
        max_h,
        max_v,
        progressive,
        mcus_x,
        mcus_y,
    })
}

/// What one scan does to each of its blocks.
#[derive(Copy, Clone)]
enum Pass {
    /// Whole block, one shot (sequential frames).
    Sequential,
    /// Progressive DC, Ah = 0 (`al` = point transform).
    DcFirst(u32),
    /// Progressive DC, Ah > 0.
    DcRefine(u32),
    /// Progressive AC over `ss..=se`, Ah = 0.
    AcFirst(usize, usize, u32),
    /// Progressive AC over `ss..=se`, Ah > 0.
    AcRefine(usize, usize, u32),
}

/// Decode one entropy-coded scan into the components' coefficient
/// planes; returns bytes consumed AFTER the SOS segment.
fn decode_scan(
    header: &[u8],
    data: &[u8],
    f: &mut Frame,
    quant: &[Option<[u16; 64]>; 4],
    dc_tables: &[Option<HuffTable>; 4],
    ac_tables: &[Option<HuffTable>; 4],
    restart_interval: u32,
) -> Result<usize> {
    // Scan header: component selectors + entropy table bindings, then
    // the spectral band and successive-approximation bits.
    if header.is_empty() {
        return Err(Error::Parse("jpeg: truncated SOS header".into()));
    }
    let ns = header[0] as usize;
    if ns == 0 || ns > f.components.len() {
        return Err(Error::Parse(format!(
            "jpeg: scan declares {ns} components, frame has {}",
            f.components.len()
        )));
    }
    if header.len() < 1 + ns * 2 + 3 {
        return Err(Error::Parse("jpeg: truncated SOS header".into()));
    }
    // RT5-2: every scan component selector (Cs_i) must reference a
    // SOF-DECLARED component id, and T.81 B.2.3 fixes their order to
    // the frame's — a scan that reorders them would reorder data units
    // inside each MCU, so it rejects by name rather than decode wrong.
    let mut idxs: Vec<usize> = Vec::with_capacity(ns);
    for i in 0..ns {
        let cs = header[1 + i * 2];
        let at = f
            .components
            .iter()
            .position(|c| c.id == cs)
            .ok_or_else(|| {
                Error::Parse(format!(
                    "jpeg: scan component selector {cs} not declared in SOF"
                ))
            })?;
        if idxs.last().is_some_and(|&prev| at <= prev) {
            return Err(Error::Parse(format!(
                "jpeg: scan reorders component selector {cs} (frame order only)"
            )));
        }
        let td = (header[2 + i * 2] >> 4) as usize;
        let ta = (header[2 + i * 2] & 0x0F) as usize;
        if td > 3 || ta > 3 {
            return Err(Error::Parse("jpeg: entropy table id > 3".into()));
        }
        f.components[at].dc_tbl = td;
        f.components[at].ac_tbl = ta;
        idxs.push(at);
    }
    let tail = &header[1 + ns * 2..];
    let (ss, se) = (tail[0] as usize, tail[1] as usize);
    let (ah, al) = ((tail[2] >> 4) as u32, (tail[2] & 0x0F) as u32);
    if se > 63 || ss > se {
        return Err(Error::Parse(format!(
            "jpeg: spectral selection {ss}..={se} outside 0..=63"
        )));
    }
    if ah > 13 || al > 13 {
        return Err(Error::Parse(format!(
            "jpeg: successive approximation Ah={ah}/Al={al} > 13"
        )));
    }

    let pass = if !f.progressive {
        if ss != 0 || se != 63 || ah != 0 || al != 0 {
            return Err(Error::Parse(format!(
                "jpeg: sequential scan must cover the whole block \
                 (Ss=0, Se=63, Ah=Al=0; got Ss={ss}, Se={se}, Ah={ah}, Al={al})"
            )));
        }
        Pass::Sequential
    } else if ss == 0 {
        if se != 0 {
            return Err(Error::Parse(
                "jpeg: progressive DC scan must select Se=0".into(),
            ));
        }
        if ah == 0 {
            Pass::DcFirst(al)
        } else {
            Pass::DcRefine(al)
        }
    } else {
        if ns != 1 {
            return Err(Error::Parse(
                "jpeg: progressive AC scan must carry exactly one component".into(),
            ));
        }
        if ah == 0 {
            Pass::AcFirst(ss, se, al)
        } else {
            Pass::AcRefine(ss, se, al)
        }
    };
    if ah != 0 && ah != al + 1 {
        return Err(Error::Parse(format!(
            "jpeg: successive approximation Ah={ah} must be Al+1 (Al={al})"
        )));
    }

    // Capture the quantization tables in force for the components
    // this scan touches, and reset their predictors.
    for &ci in &idxs {
        let c = &mut f.components[ci];
        if c.quant.is_none() {
            c.quant = quant[c.tq];
        }
        c.dc_pred = 0;
        c.scanned = true;
    }

    let (mcus_x, mcus_y) = (f.mcus_x, f.mcus_y);
    let mut reader = BitReader::new(data);
    let mut eobrun = 0u32;
    let mut rst_n = 0u8;

    // Interleaved scans walk MCUs; a single-component scan walks that
    // component's own blocks, MCU padding excluded (T.81 A.2.4).
    let units = if ns == 1 {
        let c = &f.components[idxs[0]];
        c.scan_blocks_w * c.scan_blocks_h
    } else {
        mcus_x * mcus_y
    };

    for unit in 0..units {
        if restart_interval > 0 && unit > 0 && (unit as u32).is_multiple_of(restart_interval) {
            reader.expect_restart(rst_n)?;
            rst_n = (rst_n + 1) & 7;
            for &ci in &idxs {
                f.components[ci].dc_pred = 0;
            }
            eobrun = 0;
        }
        if ns == 1 {
            let ci = idxs[0];
            let bw = f.components[ci].scan_blocks_w;
            decode_data_unit(
                &mut reader,
                &mut f.components[ci],
                unit % bw,
                unit / bw,
                pass,
                dc_tables,
                ac_tables,
                &mut eobrun,
            )?;
        } else {
            let (mx, my) = (unit % mcus_x, unit / mcus_x);
            for &ci in &idxs {
                let (h, v) = (f.components[ci].h as usize, f.components[ci].v as usize);
                for by in 0..v {
                    for bx in 0..h {
                        decode_data_unit(
                            &mut reader,
                            &mut f.components[ci],
                            mx * h + bx,
                            my * v + by,
                            pass,
                            dc_tables,
                            ac_tables,
                            &mut eobrun,
                        )?;
                    }
                }
            }
        }
    }
    Ok(reader.byte_pos())
}

/// One data unit (8x8 block) of one component, under this scan's pass.
#[allow(clippy::too_many_arguments)]
fn decode_data_unit(
    reader: &mut BitReader<'_>,
    c: &mut Component,
    bx: usize,
    by: usize,
    pass: Pass,
    dc_tables: &[Option<HuffTable>; 4],
    ac_tables: &[Option<HuffTable>; 4],
    eobrun: &mut u32,
) -> Result<()> {
    let off = (by * c.blocks_w + bx) * 64;
    let block = c
        .coeffs
        .get_mut(off..off + 64)
        .ok_or_else(|| Error::Parse("jpeg: block index outside the component plane".into()))?;
    let dc = || {
        dc_tables[c.dc_tbl]
            .as_ref()
            .ok_or_else(|| Error::Parse(format!("jpeg: missing DC table {}", c.dc_tbl)))
    };
    let ac = || {
        ac_tables[c.ac_tbl]
            .as_ref()
            .ok_or_else(|| Error::Parse(format!("jpeg: missing AC table {}", c.ac_tbl)))
    };
    match pass {
        Pass::Sequential => decode_block(reader, dc()?, ac()?, &mut c.dc_pred, block),
        Pass::DcFirst(al) => decode_dc_first(reader, dc()?, &mut c.dc_pred, al, block),
        Pass::DcRefine(al) => decode_dc_refine(reader, al, block),
        Pass::AcFirst(ss, se, al) => decode_ac_first(reader, ac()?, ss, se, al, eobrun, block),
        Pass::AcRefine(ss, se, al) => decode_ac_refine(reader, ac()?, ss, se, al, eobrun, block),
    }
}

/// Coefficients -> samples: dequantize and inverse-DCT every block,
/// component by component, releasing each coefficient plane as soon as
/// its samples exist.
fn render(f: &mut Frame, quant: &[Option<[u16; 64]>; 4]) -> Result<()> {
    let mut block = [0u8; 64];
    for c in &mut f.components {
        let qt = c
            .quant
            .or(quant[c.tq])
            .ok_or_else(|| Error::Parse(format!("jpeg: missing quant table {}", c.tq)))?;
        let coeffs = std::mem::take(&mut c.coeffs);
        c.plane_w = c.blocks_w * 8;
        c.plane = vec![0u8; c.plane_w * c.blocks_h * 8];
        for (i, zz) in coeffs.chunks_exact(64).enumerate() {
            let coef = dequantize(zz, &qt);
            idct_8x8(&coef, &mut block);
            let px = (i % c.blocks_w) * 8;
            let py = (i / c.blocks_w) * 8;
            for (row, chunk) in block.chunks_exact(8).enumerate() {
                let start = (py + row) * c.plane_w + px;
                c.plane[start..start + 8].copy_from_slice(chunk);
            }
        }
    }
    Ok(())
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
    fn decodes_progressive_444() {
        assert_close_rgb("progressive 4:4:4", fx::GRADPROG444, 14, 4.0);
    }

    #[test]
    fn decodes_progressive_420() {
        assert_close_rgb("progressive 4:2:0", fx::GRADPROG, 20, 6.0);
    }

    #[test]
    fn decodes_progressive_420_with_restart_markers() {
        assert_close_rgb("progressive 4:2:0+RST", fx::GRADPROG420RST, 20, 6.0);
    }

    #[test]
    fn decodes_progressive_grayscale() {
        let img = decode(fx::GRAYPROG).unwrap();
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

    /// A sequential file split into one NON-interleaved scan per
    /// component: the same per-component block walk progressive AC
    /// scans use, with the sequential block decoder.
    #[test]
    fn decodes_sequential_multi_scan() {
        assert_close_rgb("sequential non-interleaved", fx::GRADSEQNONINT, 14, 4.0);
    }

    /// Progressive and sequential encodings of the same source must
    /// land within a code or two of each other — a refinement pass
    /// that dropped bits would show up as a systematic gap.
    #[test]
    fn progressive_matches_sequential_decode() {
        let seq = decode(fx::GRAD444).unwrap();
        let prog = decode(fx::GRADPROG444).unwrap();
        let mut worst = 0i32;
        for y in 0..16 {
            for x in 0..16 {
                let (a, b) = (seq.get(x, y).unwrap(), prog.get(x, y).unwrap());
                for (p, q) in [(a.r, b.r), (a.g, b.g), (a.b, b.b)] {
                    worst = worst.max((p as i32 - q as i32).abs());
                }
            }
        }
        assert!(worst <= 8, "progressive/sequential disagree by {worst}");
    }

    /// Successive-approximation bookkeeping: Ah must be exactly Al+1,
    /// and a progressive AC scan carries exactly one component.
    #[test]
    fn progressive_scan_header_validation() {
        // Locate the first SOS of the progressive fixture:
        // FFDA len(2) ns(1) [Cs,Td/Ta]*ns Ss Se AhAl
        let base = fx::GRADPROG444;
        let sos = base
            .windows(2)
            .position(|w| w == [0xFF, 0xDA])
            .expect("fixture has SOS");
        let ns = base[sos + 4] as usize;
        let ahal = sos + 5 + ns * 2 + 2;

        let mut bad = base.to_vec();
        bad[ahal] = 0x30; // Ah=3, Al=0 — not Al+1
        let err = decode(&bad).unwrap_err();
        assert!(err.to_string().contains("Al+1"), "{err}");
    }

    /// A component the scans never mention must reject by name rather
    /// than assemble from an all-zero plane.
    #[test]
    fn component_without_scan_data_rejected() {
        // GRADPROG's first scan is the interleaved DC scan; drop the
        // third component from it AND from every later scan by
        // renaming component 3 in the frame header, so no scan
        // selector can reach it.
        let mut bytes = fx::GRADPROG444.to_vec();
        let sof = bytes
            .windows(2)
            .position(|w| w == [0xFF, 0xC2])
            .expect("progressive SOF");
        // SOF: FFC2 len(2) precision(1) h(2) w(2) nf(1) then 3 bytes
        // per component; the third component's id byte:
        let c3_id = sof + 10 + 2 * 3;
        bytes[c3_id] = 0x77;
        let err = decode(&bytes).unwrap_err();
        assert!(
            err.to_string().contains("not declared in SOF")
                || err.to_string().contains("never receives scan data"),
            "{err}"
        );
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
    fn progressive_truncation_ladder_never_panics() {
        let full = fx::GRADPROG;
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
