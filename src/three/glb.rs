//! GLB (binary glTF 2.0) container reader: header validation + chunk
//! split. The typed document views over the JSON chunk live in
//! `three::doc`.
//!
//! Layout per the glTF 2.0 spec (§ GLB File Format): 12-byte header
//! (magic u32le, version u32le, total length u32le), then chunks of
//! (length u32le, type u32le, data padded to 4-byte alignment). JSON
//! must be the first chunk; BIN, if present, follows; unknown chunk
//! types are skipped per spec.

use crate::base::{Error, Result};

pub const MAGIC: u32 = 0x4654_6C67; // "glTF" little-endian
pub const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
pub const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Borrowed view of a split container (zero-copy: slices into the
/// caller's buffer).
#[derive(Debug)]
pub struct GlbChunks<'a> {
    pub json: &'a [u8],
    pub bin: Option<&'a [u8]>,
}

/// Split a GLB byte buffer into its JSON and (optional) BIN chunks.
pub fn split(bytes: &[u8]) -> Result<GlbChunks<'_>> {
    if bytes.len() < 12 {
        return Err(Error::Parse("glb: shorter than the 12-byte header".into()));
    }
    let magic = u32le(bytes, 0);
    if magic != MAGIC {
        return Err(Error::Parse(format!(
            "glb: bad magic 0x{magic:08x} (not 'glTF')"
        )));
    }
    let version = u32le(bytes, 4);
    if version != 2 {
        return Err(Error::Parse(format!(
            "glb: container version {version} (only 2 supported)"
        )));
    }
    let declared = u32le(bytes, 8) as usize;
    if declared > bytes.len() {
        return Err(Error::Parse(format!(
            "glb: declared length {declared} exceeds buffer ({})",
            bytes.len()
        )));
    }
    // Trailing bytes past the declared length are ignored (spec-legal
    // when a GLB is embedded in a larger stream); chunk walking stays
    // inside the declared region.
    let region = &bytes[..declared];

    let mut off = 12usize;
    let mut json: Option<&[u8]> = None;
    let mut bin: Option<&[u8]> = None;
    let mut first = true;
    while off < region.len() {
        if region.len() - off < 8 {
            return Err(Error::Parse("glb: truncated chunk header".into()));
        }
        let len = u32le(region, off) as usize;
        let ctype = u32le(region, off + 4);
        // Checked cursor math: len is attacker-controlled u32; off+8+len
        // could overflow usize on 32-bit targets.
        let data_start = off + 8;
        let data_end = data_start
            .checked_add(len)
            .ok_or_else(|| Error::Parse("glb: chunk length overflows".into()))?;
        if data_end > region.len() {
            return Err(Error::Parse(format!(
                "glb: chunk of {len} bytes runs past declared end"
            )));
        }
        let data = &region[data_start..data_end];
        match ctype {
            CHUNK_JSON => {
                if first {
                    json = Some(data);
                } else if json.is_some() {
                    return Err(Error::Parse("glb: duplicate JSON chunk".into()));
                } else {
                    return Err(Error::Parse("glb: JSON chunk must be first".into()));
                }
            }
            CHUNK_BIN => {
                if first {
                    return Err(Error::Parse("glb: BIN chunk before JSON".into()));
                }
                // Spec: 0 or 1 BIN chunk, directly after JSON. Extra BIN
                // chunks are malformed rather than skippable-unknown.
                if bin.is_some() {
                    return Err(Error::Parse("glb: duplicate BIN chunk".into()));
                }
                bin = Some(data);
            }
            _ => {} // unknown chunk types are skipped per spec
        }
        first = false;
        // Chunks are 4-byte aligned; the declared chunk length excludes
        // padding, so round the cursor up.
        let padded = data_end
            .checked_add(3)
            .ok_or_else(|| Error::Parse("glb: chunk padding overflows".into()))?
            & !3;
        off = padded;
    }

    let json = json.ok_or_else(|| Error::Parse("glb: missing JSON chunk".into()))?;
    Ok(GlbChunks { json, bin })
}

#[inline]
fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().expect("caller checked length"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::three::doc::Doc;

    /// Assemble a syntactically valid GLB in memory (shared with doc
    /// tests via pub(crate)).
    pub(crate) fn make_glb(json: &[u8], bin: Option<&[u8]>) -> Vec<u8> {
        let pad4 = |n: usize| (n + 3) & !3;
        let json_padded = pad4(json.len());
        let mut total = 12 + 8 + json_padded;
        if let Some(b) = bin {
            total += 8 + pad4(b.len());
        }
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_padded as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        out.extend_from_slice(json);
        out.resize(out.len() + (json_padded - json.len()), 0x20); // space padding
        if let Some(b) = bin {
            out.extend_from_slice(&(pad4(b.len()) as u32).to_le_bytes());
            out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
            out.extend_from_slice(b);
            out.resize(out.len() + (pad4(b.len()) - b.len()), 0x00); // zero padding
        }
        out
    }

    pub(crate) const MIN_JSON: &[u8] = br#"{"asset":{"version":"2.0"}}"#;

    #[test]
    fn split_json_and_bin() {
        let glb = make_glb(MIN_JSON, Some(&[1, 2, 3, 4, 5]));
        let chunks = split(&glb).unwrap();
        // JSON chunk includes its space padding (parser tolerates it).
        assert!(chunks.json.starts_with(MIN_JSON));
        assert_eq!(chunks.bin.unwrap()[..5], [1, 2, 3, 4, 5]);
        // BIN padding is part of the chunk (spec: byteLength may be up
        // to 3 less than the chunk length).
        assert_eq!(chunks.bin.unwrap().len(), 8);
        // And the JSON survives its padding through the real parser.
        assert!(Doc::parse(chunks.json).is_ok());
    }

    #[test]
    fn split_json_only() {
        let glb = make_glb(MIN_JSON, None);
        let chunks = split(&glb).unwrap();
        assert!(chunks.bin.is_none());
        assert!(Doc::parse(chunks.json).is_ok());
    }

    #[test]
    fn split_rejects_malformed() {
        assert!(split(&[]).is_err(), "empty");
        assert!(split(b"glTF").is_err(), "short header");

        let mut bad_magic = make_glb(MIN_JSON, None);
        bad_magic[0] = b'x';
        assert!(split(&bad_magic).unwrap_err().to_string().contains("magic"));

        let mut bad_version = make_glb(MIN_JSON, None);
        bad_version[4] = 1;
        assert!(split(&bad_version)
            .unwrap_err()
            .to_string()
            .contains("version"));

        // Declared length longer than the buffer.
        let mut long = make_glb(MIN_JSON, None);
        long[8] = 0xFF;
        long[9] = 0xFF;
        assert!(split(&long)
            .unwrap_err()
            .to_string()
            .contains("declared length"));

        // Chunk running past the declared end.
        let mut overrun = make_glb(MIN_JSON, None);
        let json_len_off = 12;
        overrun[json_len_off] = 0xFF; // JSON chunk claims 255+ bytes
        assert!(split(&overrun).is_err());

        // Truncation at every prefix must error, never panic.
        let full = make_glb(MIN_JSON, Some(&[9; 7]));
        for cut in 0..full.len() {
            assert!(split(&full[..cut]).is_err(), "prefix {cut}");
        }
    }

    #[test]
    fn split_requires_json_first() {
        // Hand-build: BIN chunk first.
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(12u32 + 8 + 4).to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        assert!(split(&out)
            .unwrap_err()
            .to_string()
            .contains("BIN chunk before JSON"));
    }

    #[test]
    fn split_skips_unknown_chunks() {
        // JSON, then an unknown chunk type: still fine, bin stays None.
        let mut glb = make_glb(MIN_JSON, None);
        let unknown_type = 0x54534554u32; // "TEST"
        let extra = [0xAAu8; 4];
        glb.extend_from_slice(&(extra.len() as u32).to_le_bytes());
        glb.extend_from_slice(&unknown_type.to_le_bytes());
        glb.extend_from_slice(&extra);
        let total = glb.len() as u32;
        glb[8..12].copy_from_slice(&total.to_le_bytes());
        let chunks = split(&glb).unwrap();
        assert!(chunks.bin.is_none());
    }

    /// Header-only reads of the real sibling-repo assets (guarded so
    /// the suite passes on machines without them).
    #[test]
    fn real_asset_headers_split() {
        let paths = [
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb",
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/machine.glb",
            "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb",
        ];
        for path in paths {
            let p = std::path::Path::new(path);
            if !p.exists() {
                continue;
            }
            let bytes = std::fs::read(p).unwrap();
            let chunks = split(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(
                chunks.json.trim_ascii_start().starts_with(b"{"),
                "{path}: JSON chunk"
            );
            assert!(chunks.bin.is_some(), "{path}: BIN chunk expected");
        }
    }
}
