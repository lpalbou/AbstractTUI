//! A byte-level baseline-JPEG assembler for adversarial decoding tests.
//!
//! OWNER: REDTEAM. The crate's own JPEG fixtures are `cfg(test)`-private
//! (embedded cjpeg output), so integration tests cannot reach them and,
//! more importantly, cannot make them PATHOLOGICAL. This builder emits
//! structurally-valid baseline JPEGs whose Huffman code assignment we
//! choose — so a "deep single-code tree" or a full 16-length ladder can
//! be fed through the real `gfx::jpeg::decode` and proven to decode, not
//! merely to be rejected.
//!
//! The trick that makes hand-built entropy tractable: a FLAT block needs
//! exactly two symbols — DC size-class 0 (`0x00`, a zero DC difference,
//! no magnitude bits) and AC EOB (`0x00`). Assign each a canonical code
//! of any length L (a single symbol at length L is the all-zeros code of
//! L bits), and every 8x8 block is just `L + L` zero bits. Repeat per
//! block per MCU, pad the final byte with 1s. The result is a real,
//! spec-conformant, decodable JPEG whose Huffman tables we fully control.
//!
//! Mutation helpers then perturb the tables/headers for the malformed
//! corpus while keeping the container walkable, so the decoder's build
//! and MCU paths (not just the marker scan) get hit.

/// Standard JPEG zig-zag order (T.81 Fig. A.6) — needed only if a caller
/// wants non-flat coefficients; the flat builder leaves it implicit.
#[rustfmt::skip]
pub const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// One Huffman table in DHT wire order.
#[derive(Clone)]
pub struct HuffSpec {
    /// 0 = DC, 1 = AC.
    pub class: u8,
    /// Destination id 0..=3.
    pub id: u8,
    /// Codes-per-length, index 0 => length 1.
    pub counts: [u8; 16],
    /// Symbol bytes, `counts.sum()` of them.
    pub values: Vec<u8>,
}

impl HuffSpec {
    /// A single symbol `value` at canonical code length `len` (1..=16) —
    /// the "deep single-code tree" when `len` is large. Its code is the
    /// all-zeros word of `len` bits, so entropy for it is `len` zeros.
    pub fn single(class: u8, id: u8, value: u8, len: usize) -> HuffSpec {
        assert!((1..=16).contains(&len));
        let mut counts = [0u8; 16];
        counts[len - 1] = 1;
        HuffSpec {
            class,
            id,
            counts,
            values: vec![value],
        }
    }
}

/// A component's SOF/SOS parameters.
#[derive(Clone, Copy)]
pub struct CompSpec {
    pub id: u8,
    pub h: u8,
    pub v: u8,
    pub quant_id: u8,
    pub dc_id: u8,
    pub ac_id: u8,
}

/// A builder for a flat (all-DC-zero) baseline JPEG. Every knob is
/// public so tests can drive it into hostile-but-structural shapes.
pub struct FlatJpeg {
    pub width: u16,
    pub height: u16,
    /// SOF marker: 0xC0 (baseline) or 0xC1 (extended sequential).
    pub sof_marker: u8,
    pub precision: u8,
    pub components: Vec<CompSpec>,
    pub quant_ids: Vec<u8>,
    pub huff: Vec<HuffSpec>,
    /// Restart interval (0 = none).
    pub dri: u16,
    /// The code length assigned to the two flat symbols (DC size-0 in the
    /// DC tables, EOB in the AC tables). Entropy is generated to match.
    pub flat_code_len: usize,
    /// SOS component selectors override (defaults to component ids).
    pub sos_selectors: Option<Vec<u8>>,
}

impl FlatJpeg {
    /// Grayscale 8N x 8M flat image (one component).
    pub fn grayscale(width: u16, height: u16) -> FlatJpeg {
        FlatJpeg {
            width,
            height,
            sof_marker: 0xC0,
            precision: 8,
            components: vec![CompSpec {
                id: 1,
                h: 1,
                v: 1,
                quant_id: 0,
                dc_id: 0,
                ac_id: 0,
            }],
            quant_ids: vec![0],
            huff: vec![
                HuffSpec::single(0, 0, 0x00, 2),
                HuffSpec::single(1, 0, 0x00, 2),
            ],
            dri: 0,
            flat_code_len: 2,
            sos_selectors: None,
        }
    }

    /// 4:4:4 color flat image (three 1x1 components, shared tables).
    pub fn color444(width: u16, height: u16) -> FlatJpeg {
        FlatJpeg {
            width,
            height,
            sof_marker: 0xC0,
            precision: 8,
            components: vec![
                CompSpec {
                    id: 1,
                    h: 1,
                    v: 1,
                    quant_id: 0,
                    dc_id: 0,
                    ac_id: 0,
                },
                CompSpec {
                    id: 2,
                    h: 1,
                    v: 1,
                    quant_id: 1,
                    dc_id: 1,
                    ac_id: 1,
                },
                CompSpec {
                    id: 3,
                    h: 1,
                    v: 1,
                    quant_id: 1,
                    dc_id: 1,
                    ac_id: 1,
                },
            ],
            quant_ids: vec![0, 1],
            huff: vec![
                HuffSpec::single(0, 0, 0x00, 2),
                HuffSpec::single(1, 0, 0x00, 2),
                HuffSpec::single(0, 1, 0x00, 2),
                HuffSpec::single(1, 1, 0x00, 2),
            ],
            dri: 0,
            flat_code_len: 2,
            sos_selectors: None,
        }
    }

    /// Set the flat symbols' canonical code length everywhere (rebuilds
    /// the single-symbol tables). This is the "deep tree" knob.
    pub fn with_flat_code_len(mut self, len: usize) -> FlatJpeg {
        self.flat_code_len = len;
        for h in &mut self.huff {
            *h = HuffSpec::single(h.class, h.id, 0x00, len);
        }
        self
    }

    /// MCUs across x/y given the max sampling factors — the count of
    /// entropy blocks the decoder will read.
    fn mcu_grid(&self) -> (usize, usize) {
        let max_h = self.components.iter().map(|c| c.h).max().unwrap_or(1) as usize;
        let max_v = self.components.iter().map(|c| c.v).max().unwrap_or(1) as usize;
        let mx = (self.width as usize).div_ceil(8 * max_h);
        let my = (self.height as usize).div_ceil(8 * max_v);
        (mx, my)
    }

    /// Blocks per MCU = sum over components of h*v.
    fn blocks_per_mcu(&self) -> usize {
        self.components
            .iter()
            .map(|c| c.h as usize * c.v as usize)
            .sum()
    }

    /// Generate the entropy stream: for each block, `flat_code_len` zero
    /// bits (DC size-0) then `flat_code_len` zero bits (AC EOB), with
    /// restart markers inserted every `dri` MCUs. Byte-padded with 1s.
    fn entropy(&self) -> Vec<u8> {
        let (mx, my) = self.mcu_grid();
        let blocks = self.blocks_per_mcu();
        let mut bw = BitWriter::new();
        let mut rst = 0u8;
        for mcu in 0..mx * my {
            if self.dri > 0 && mcu > 0 && mcu % self.dri as usize == 0 {
                bw.align_and_restart(rst);
                rst = (rst + 1) & 7;
            }
            for _ in 0..blocks {
                bw.zeros(self.flat_code_len); // DC size 0
                bw.zeros(self.flat_code_len); // AC EOB
            }
        }
        bw.finish()
    }

    /// Assemble the full JPEG byte stream.
    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0xFF, 0xD8]); // SOI

        // DQT: one all-ones (identity) quantizer per referenced id.
        for &qid in &self.quant_ids {
            let mut seg = vec![qid & 0x0F];
            seg.extend(std::iter::repeat_n(1u8, 64));
            push_segment(&mut out, 0xDB, &seg);
        }

        // SOF.
        let mut sof = vec![self.precision];
        sof.extend_from_slice(&self.height.to_be_bytes());
        sof.extend_from_slice(&self.width.to_be_bytes());
        sof.push(self.components.len() as u8);
        for c in &self.components {
            sof.push(c.id);
            sof.push((c.h << 4) | (c.v & 0x0F));
            sof.push(c.quant_id & 0x0F);
        }
        push_segment(&mut out, self.sof_marker, &sof);

        // DHT (one segment per table keeps it simple and legal).
        for h in &self.huff {
            let mut seg = vec![(h.class << 4) | (h.id & 0x0F)];
            seg.extend_from_slice(&h.counts);
            seg.extend_from_slice(&h.values);
            push_segment(&mut out, 0xC4, &seg);
        }

        // DRI.
        if self.dri > 0 {
            push_segment(&mut out, 0xDD, &self.dri.to_be_bytes());
        }

        // SOS.
        let selectors = self
            .sos_selectors
            .clone()
            .unwrap_or_else(|| self.components.iter().map(|c| c.id).collect());
        let mut sos = vec![self.components.len() as u8];
        for (c, &sel) in self.components.iter().zip(&selectors) {
            sos.push(sel);
            sos.push((c.dc_id << 4) | (c.ac_id & 0x0F));
        }
        sos.extend_from_slice(&[0x00, 0x3F, 0x00]); // Ss, Se, Ah/Al
        push_segment(&mut out, 0xDA, &sos);

        // Entropy + EOI.
        out.extend_from_slice(&self.entropy());
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }
}

/// MSB-first bit writer with JPEG byte stuffing (`0xFF` -> `0xFF 0x00`).
struct BitWriter {
    out: Vec<u8>,
    cur: u32,
    n: u32,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            out: Vec::new(),
            cur: 0,
            n: 0,
        }
    }

    fn zeros(&mut self, count: usize) {
        for _ in 0..count {
            self.push_bit(0);
        }
    }

    fn push_bit(&mut self, bit: u32) {
        self.cur = (self.cur << 1) | (bit & 1);
        self.n += 1;
        if self.n == 8 {
            self.emit_byte((self.cur & 0xFF) as u8);
            self.cur = 0;
            self.n = 0;
        }
    }

    fn emit_byte(&mut self, b: u8) {
        self.out.push(b);
        if b == 0xFF {
            self.out.push(0x00); // stuffing
        }
    }

    /// Pad to a byte boundary with 1-bits (JPEG convention), then write a
    /// raw restart marker (not stuffed).
    fn align_and_restart(&mut self, n: u8) {
        self.pad_ones();
        self.out.push(0xFF);
        self.out.push(0xD0 + (n & 7));
    }

    fn pad_ones(&mut self) {
        if self.n > 0 {
            while self.n != 0 {
                self.push_bit(1);
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        self.pad_ones();
        self.out
    }
}

/// Push a length-prefixed JPEG segment (`FF marker len_hi len_lo body`).
fn push_segment(out: &mut Vec<u8>, marker: u8, body: &[u8]) {
    out.push(0xFF);
    out.push(marker);
    let len = (body.len() + 2) as u16;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(body);
}
