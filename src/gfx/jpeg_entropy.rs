//! JPEG entropy layer: the stuffed-byte bit reader and canonical
//! Huffman tables (ITU T.81 §F.2). Baseline Huffman only — the
//! arithmetic-coding path is rejected upstream by name.

use crate::base::{Error, Result};

/// Bit reader over entropy-coded scan data. Handles byte stuffing
/// (`FF 00` = literal 0xFF) and STOPS at any real marker (`FF Dn`,
/// `FF D9`…) — the decoder consumes restarts explicitly via
/// [`BitReader::expect_restart`]; reading past the end of entropy data
/// is a named truncation error, never a panic.
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// Byte position of the next unread byte (marker scan resumes here
    /// after the scan's MCUs are decoded).
    pub fn byte_pos(&self) -> usize {
        self.pos
    }

    fn load_byte(&mut self) -> Result<()> {
        match self.data.get(self.pos) {
            None => Err(Error::Parse("jpeg: truncated entropy data".into())),
            Some(&0xFF) => match self.data.get(self.pos + 1) {
                Some(&0x00) => {
                    // Stuffed 0xFF data byte.
                    self.pos += 2;
                    self.bit_buf = (self.bit_buf << 8) | 0xFF;
                    self.bit_count += 8;
                    Ok(())
                }
                // A real marker: entropy data ends here. The MCU loop
                // either expected a restart (consumed explicitly) or
                // this is premature truncation.
                _ => Err(Error::Parse(
                    "jpeg: entropy data ended at a marker mid-block".into(),
                )),
            },
            Some(&b) => {
                self.pos += 1;
                self.bit_buf = (self.bit_buf << 8) | b as u32;
                self.bit_count += 8;
                Ok(())
            }
        }
    }

    #[inline]
    pub fn next_bit(&mut self) -> Result<u32> {
        if self.bit_count == 0 {
            self.load_byte()?;
        }
        self.bit_count -= 1;
        Ok((self.bit_buf >> self.bit_count) & 1)
    }

    /// Read `n` bits MSB-first (n ≤ 16; n = 0 reads nothing).
    pub fn receive(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n <= 16);
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.next_bit()?;
        }
        Ok(v)
    }

    /// Consume a restart marker: discard partial bits (spec: entropy
    /// data is byte-aligned before RSTn), expect `FF D0+n`, continue.
    pub fn expect_restart(&mut self, n: u8) -> Result<()> {
        self.bit_buf = 0;
        self.bit_count = 0;
        let want = 0xD0 + (n & 7);
        match (self.data.get(self.pos), self.data.get(self.pos + 1)) {
            (Some(&0xFF), Some(&m)) if m == want => {
                self.pos += 2;
                Ok(())
            }
            (Some(&0xFF), Some(&m)) => Err(Error::Parse(format!(
                "jpeg: expected restart marker RST{} but found 0xFF{m:02X}",
                n & 7
            ))),
            _ => Err(Error::Parse("jpeg: missing restart marker".into())),
        }
    }
}

/// Canonical Huffman table (T.81 annex C build, annex F decode).
pub struct HuffTable {
    /// Smallest code of each length (index 1..=16).
    min_code: [i32; 17],
    /// Largest code of each length, -1 when the length is unused.
    max_code: [i32; 17],
    /// Index into `values` of the first code of each length.
    val_ptr: [usize; 17],
    values: Vec<u8>,
}

impl HuffTable {
    /// Build from the DHT wire form: 16 length counts + symbol bytes.
    pub fn build(counts: &[u8; 16], values: &[u8]) -> Result<HuffTable> {
        let total: usize = counts.iter().map(|&c| c as usize).sum();
        if total != values.len() || total > 256 {
            return Err(Error::Parse(format!(
                "jpeg: DHT declares {total} symbols, carries {}",
                values.len()
            )));
        }
        let mut min_code = [0i32; 17];
        let mut max_code = [-1i32; 17];
        let mut val_ptr = [0usize; 17];
        let mut code = 0i32;
        let mut k = 0usize;
        for len in 1..=16usize {
            let n = counts[len - 1] as i32;
            if n > 0 {
                val_ptr[len] = k;
                min_code[len] = code;
                code += n;
                max_code[len] = code - 1;
                k += n as usize;
                // Canonical-code sanity: codes of length L must fit L bits.
                if max_code[len] >= (1 << len) {
                    return Err(Error::Parse(
                        "jpeg: DHT codes overflow their bit length".into(),
                    ));
                }
            }
            code <<= 1;
        }
        Ok(HuffTable {
            min_code,
            max_code,
            val_ptr,
            values: values.to_vec(),
        })
    }

    /// Decode one symbol (T.81 F.2.2.3 DECODE).
    pub fn decode(&self, r: &mut BitReader<'_>) -> Result<u8> {
        let mut code = 0i32;
        for len in 1..=16usize {
            code = (code << 1) | r.next_bit()? as i32;
            if self.max_code[len] >= 0 && code <= self.max_code[len] {
                let idx = self.val_ptr[len] + (code - self.min_code[len]) as usize;
                return self
                    .values
                    .get(idx)
                    .copied()
                    .ok_or_else(|| Error::Parse("jpeg: Huffman value index out of range".into()));
            }
        }
        Err(Error::Parse("jpeg: invalid Huffman code (>16 bits)".into()))
    }
}

/// T.81 F.2.2.1 EXTEND: map a `size`-bit magnitude to its signed value.
#[inline]
pub fn extend(v: u32, size: u32) -> i32 {
    if size == 0 {
        return 0;
    }
    if v < (1 << (size - 1)) {
        v as i32 - (1 << size) + 1
    } else {
        v as i32
    }
}

/// Decode one 8x8 block into ZIGZAG-ordered coefficients (pre-dequant).
/// `dc_pred` carries the component's DC predictor across blocks.
pub fn decode_block(
    r: &mut BitReader<'_>,
    dc: &HuffTable,
    ac: &HuffTable,
    dc_pred: &mut i32,
) -> Result<[i32; 64]> {
    let mut zz = [0i32; 64];
    // DC: size class, then the difference bits.
    let s = dc.decode(r)? as u32;
    if s > 11 {
        return Err(Error::Parse(format!("jpeg: DC size class {s} > 11")));
    }
    let diff = extend(r.receive(s)?, s);
    *dc_pred += diff;
    zz[0] = *dc_pred;

    // AC: run/size pairs, EOB, ZRL.
    let mut k = 1usize;
    while k < 64 {
        let rs = ac.decode(r)? as u32;
        let run = rs >> 4;
        let size = rs & 0x0F;
        if size == 0 {
            if run == 15 {
                k += 16; // ZRL: sixteen zeros
                continue;
            }
            break; // EOB
        }
        k += run as usize;
        if k > 63 {
            return Err(Error::Parse("jpeg: AC run past end of block".into()));
        }
        if size > 10 {
            return Err(Error::Parse(format!("jpeg: AC size class {size} > 10")));
        }
        zz[k] = extend(r.receive(size)?, size);
        k += 1;
    }
    Ok(zz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_reader_stuffing_and_markers() {
        // FF 00 is a literal FF byte; FF D9 stops the stream.
        let data = [0b1010_1010, 0xFF, 0x00, 0xFF, 0xD9];
        let mut r = BitReader::new(&data);
        assert_eq!(r.receive(8).unwrap(), 0b1010_1010);
        assert_eq!(r.receive(8).unwrap(), 0xFF);
        assert!(r.receive(1).is_err(), "marker ends entropy data");
    }

    #[test]
    fn restart_consumption() {
        let data = [0xAB, 0xFF, 0xD3, 0xCD];
        let mut r = BitReader::new(&data);
        assert_eq!(r.receive(4).unwrap(), 0xA);
        // Partial bits discarded; RST3 expected and consumed.
        r.expect_restart(3).unwrap();
        assert_eq!(r.receive(8).unwrap(), 0xCD);
        // Wrong index is a named error.
        let data = [0xFF, 0xD4];
        let mut r = BitReader::new(&data);
        assert!(r
            .expect_restart(3)
            .unwrap_err()
            .to_string()
            .contains("RST3"));
    }

    #[test]
    fn extend_matches_spec_table() {
        // T.81 table F.1: size 2 -> ranges -3..-2, 2..3.
        assert_eq!(extend(0b00, 2), -3);
        assert_eq!(extend(0b01, 2), -2);
        assert_eq!(extend(0b10, 2), 2);
        assert_eq!(extend(0b11, 2), 3);
        assert_eq!(extend(0, 0), 0);
    }

    #[test]
    fn huffman_canonical_decode() {
        // Two codes: '0' -> 5, '10' -> 9 (counts: one 1-bit, one 2-bit).
        let mut counts = [0u8; 16];
        counts[0] = 1;
        counts[1] = 1;
        let t = HuffTable::build(&counts, &[5, 9]).unwrap();
        // Grouped by CODE boundaries (0|10|0|10|0), not nibbles — the
        // grouping IS the documentation here.
        #[allow(clippy::unusual_byte_groupings)]
        let data = [0b0_10_0_10_0 << 1];
        let mut r = BitReader::new(&data);
        assert_eq!(t.decode(&mut r).unwrap(), 5);
        assert_eq!(t.decode(&mut r).unwrap(), 9);
        assert_eq!(t.decode(&mut r).unwrap(), 5);
    }

    #[test]
    fn huffman_build_rejects_lies() {
        let mut counts = [0u8; 16];
        counts[0] = 2; // two 1-bit codes is fine (0,1) but 3 would overflow
        assert!(
            HuffTable::build(&counts, &[1]).is_err(),
            "count/value mismatch"
        );
        counts[0] = 3;
        assert!(
            HuffTable::build(&counts, &[1, 2, 3]).is_err(),
            "codes overflow length"
        );
    }
}
