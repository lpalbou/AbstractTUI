//! GLB-embedded texture decode for model loading: the severity split
//! between REJECT (malformed container) and labeled SKIP (unimplemented
//! feature). `#[path]` sibling of load.rs (file-size split).
//!
//! OWNER: GFX3D.

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;
use crate::three::doc::Doc;

/// Texture decode result: decoded, or skipped for a LABELED reason
/// (unimplemented format / external source). Malformed containers are
/// hard `Err`s — see the severity ruling at the call site.
pub(super) enum TextureOutcome {
    Decoded(Bitmap),
    Skipped(String),
}

pub(super) fn decode_texture(
    doc: &Doc,
    tex_idx: usize,
    bin: Option<&[u8]>,
) -> Result<TextureOutcome> {
    // Index validity is guaranteed by Doc::parse (RT2-2); the gets stay
    // defensive for hand-built docs.
    let tex = doc
        .textures
        .get(tex_idx)
        .ok_or_else(|| Error::Parse(format!("gltf: texture {tex_idx} out of range")))?;
    let Some(src) = tex.source else {
        return Ok(TextureOutcome::Skipped(format!(
            "texture {tex_idx} has no source image; using baseColorFactor"
        )));
    };
    let image = doc
        .images
        .get(src)
        .ok_or_else(|| Error::Parse(format!("gltf: image {src} out of range")))?;

    if let Some(uri) = &image.uri {
        return Ok(TextureOutcome::Skipped(format!(
            "external image uri {uri:?} not fetched (standalone engine); using baseColorFactor"
        )));
    }
    // Unsupported DECLARED formats skip with a label before touching
    // bytes; for png/jpeg/undeclared the MAGIC decides via the unified
    // `gfx::decode_image` entry (containers lie, bytes don't).
    match image.mime_type.as_deref() {
        Some("image/png") | Some("image/jpeg") | None => {}
        Some(other) => {
            return Ok(TextureOutcome::Skipped(format!(
                "{other} texture not decoded (PNG/JPEG only); using baseColorFactor"
            )))
        }
    }
    let Some(bv_idx) = image.buffer_view else {
        return Ok(TextureOutcome::Skipped(format!(
            "image {src} has neither bufferView nor uri; using baseColorFactor"
        )));
    };
    let view = doc
        .buffer_views
        .get(bv_idx)
        .ok_or_else(|| Error::Parse(format!("gltf: image bufferView {bv_idx} out of range")))?;
    if view.buffer != 0 {
        return Ok(TextureOutcome::Skipped(format!(
            "image buffer {} is external; using baseColorFactor",
            view.buffer
        )));
    }
    let bin = bin.ok_or_else(|| {
        Error::Parse("gltf: image references BIN but the GLB has no BIN chunk".into())
    })?;
    // Range vs the REAL BIN: a lie here is container corruption, not a
    // missing feature — reject (upgraded from a cycle-2 warning).
    let start = view.byte_offset as u64;
    let end = start
        .checked_add(view.byte_length as u64)
        .ok_or_else(|| Error::Parse("gltf: image bufferView range overflows".into()))?;
    if end > bin.len() as u64 {
        return Err(Error::Parse(format!(
            "gltf: image bufferView runs past BIN ({} bytes)",
            bin.len()
        )));
    }
    let data = &bin[start as usize..end as usize];
    let bmp = crate::gfx::decode_image(data)
        .map_err(|e| Error::Parse(format!("gltf: embedded texture corrupt: {e}")))?;
    Ok(TextureOutcome::Decoded(bmp))
}
