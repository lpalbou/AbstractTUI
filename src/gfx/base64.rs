//! Base64 (RFC 4648, standard alphabet, padded). Needed by the kitty
//! graphics and iTerm2 emitters (cycle 2); the decoder exists for tests
//! and for parsing protocol responses that carry base64 payloads.
//!
//! Kitty note: chunked transfer requires every non-final chunk to be a
//! multiple of 4 bytes of *encoded* data. Encoding the whole payload
//! with this module and slicing at 4096-byte boundaries satisfies that
//! by construction (4096 % 4 == 0), so no incremental encoder state is
//! needed.

use crate::base::{Error, Result};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encoded length for `n` input bytes (padded form).
pub const fn encoded_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Encode to a fresh String.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(encoded_len(data.len()));
    encode_into(data, &mut out);
    out
}

/// Append the encoding of `data` to `out` — protocol emitters reuse one
/// String across frames to keep the hot path allocation-free.
pub fn encode_into(data: &[u8], out: &mut String) {
    out.reserve(encoded_len(data.len()));
    let mut chunks = data.chunks_exact(3);
    for c in &mut chunks {
        let n = (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        out.push(ALPHABET[n as usize & 63] as char);
    }
    match chunks.remainder() {
        [] => {}
        [a] => {
            let n = (*a as u32) << 16;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            let n = (*a as u32) << 16 | (*b as u32) << 8;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
            out.push('=');
        }
        _ => unreachable!("chunks_exact(3) remainder is < 3"),
    }
}

/// Strict decoder: requires canonical padded input (length % 4 == 0, `=`
/// only at the end, no whitespace). Strictness is deliberate — protocol
/// responses we will parse are machine-generated, so any deviation means
/// corruption, and silently skipping bytes would mask it.
pub fn decode(s: &str) -> Result<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(Error::Parse(format!(
            "base64: length {} not a multiple of 4",
            bytes.len()
        )));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let inv = |b: u8| -> Result<u32> {
        match b {
            b'A'..=b'Z' => Ok((b - b'A') as u32),
            b'a'..=b'z' => Ok((b - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((b - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(Error::Parse(format!("base64: invalid symbol 0x{b:02x}"))),
        }
    };
    for (i, quad) in bytes.chunks_exact(4).enumerate() {
        let last = (i + 1) * 4 == bytes.len();
        let pads = quad.iter().rev().take_while(|&&b| b == b'=').count();
        if pads > 0 && !last {
            return Err(Error::Parse("base64: padding before end of input".into()));
        }
        if pads > 2 {
            return Err(Error::Parse("base64: more than 2 padding chars".into()));
        }
        let mut n = 0u32;
        for &b in &quad[..4 - pads] {
            if b == b'=' {
                return Err(Error::Parse("base64: '=' inside quad".into()));
            }
            n = n << 6 | inv(b)?;
        }
        match pads {
            0 => out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            1 => {
                // 18 significant bits; the low 2 must be zero in canonical
                // encoding (reject non-canonical to catch corruption).
                if n & 0b11 != 0 {
                    return Err(Error::Parse("base64: non-canonical trailing bits".into()));
                }
                out.extend_from_slice(&[(n >> 10) as u8, (n >> 2) as u8]);
            }
            2 => {
                if n & 0b1111 != 0 {
                    return Err(Error::Parse("base64: non-canonical trailing bits".into()));
                }
                out.push((n >> 4) as u8);
            }
            _ => unreachable!(),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        // The canonical test vectors from RFC 4648 §10.
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), enc);
            assert_eq!(decode(enc).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn round_trip_binary() {
        let data: Vec<u8> = (0..=255u8).collect();
        let enc = encode(&data);
        assert_eq!(enc.len(), encoded_len(256));
        assert_eq!(decode(&enc).unwrap(), data);
    }

    #[test]
    fn encode_into_appends() {
        let mut s = String::from("prefix:");
        encode_into(b"hi", &mut s);
        assert_eq!(s, "prefix:aGk=");
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode("Zg=").is_err(), "bad length");
        assert!(decode("Z*==").is_err(), "bad symbol");
        assert!(decode("Zg==Zg==").is_err(), "padding before end");
        assert!(decode("Z===").is_err(), "3 pads");
        assert!(decode("aGk==\n").is_err(), "whitespace");
        // Non-canonical trailing bits: 'Zh==' decodes to 'f' only if the
        // decoder ignores the dirty low bits — we reject instead.
        assert!(decode("Zh==").is_err(), "non-canonical bits");
    }

    #[test]
    fn kitty_chunk_alignment_property() {
        // Whole-payload encode sliced at 4096 keeps non-final chunks % 4.
        let enc = encode(&vec![0xAB; 5000]);
        let first: &str = &enc[..4096.min(enc.len())];
        assert_eq!(first.len() % 4, 0);
    }
}
