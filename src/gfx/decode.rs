//! One-call image decoding: sniff the magic bytes, route to the right
//! decoder. DESIGN's request (cycle 6) so `image.rs`/`images.rs` and
//! the GLB texture path share exactly one entry.
//!
//! The MAGIC decides, never a caller-supplied MIME string: containers
//! lie, bytes don't. Unknown formats reject by name, listing what the
//! engine actually decodes — a caller can show that message verbatim.
//!
//! OWNER: GFX3D.

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::{jpeg, png};

/// PNG signature (8 bytes) — the full spec magic, not just `\x89PNG`.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// The format `decode_image` recognized (or would recognize).
///
/// `#[non_exhaustive]`: this set grows whenever a decoder lands, so
/// match on it with a `_` arm. Adding `Gif` in 0.4.0 was a breaking
/// change for every exhaustive match downstream — the marker is here so
/// the next format is not.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFormat {
    Png,
    Jpeg,
    /// GIF87a/89a. An animated GIF decodes here as its FIRST frame —
    /// the whole sequence comes from
    /// [`decode_animation`](crate::gfx::decode_animation).
    Gif,
}

/// Sniff the container format from leading bytes. `None` = none of the
/// formats this crate decodes (PNG, JPEG, GIF).
pub fn sniff_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&PNG_MAGIC) {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&crate::gfx::gif::SIGNATURE) {
        Some(ImageFormat::Gif)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        // JPEG SOI marker. The third byte is the next marker's 0xFF —
        // not required here: truncated-after-SOI data should still
        // route to the JPEG decoder and fail with ITS named error.
        Some(ImageFormat::Jpeg)
    } else {
        None
    }
}

/// Decode PNG, JPEG, or GIF bytes into an RGBA bitmap (an animated
/// GIF answers its first frame — [`decode_animation`] plays the whole
/// sequence). Rejects other formats by name; decoder errors pass
/// through unwrapped (they are already named and prefixed).
///
/// [`decode_animation`]: crate::gfx::decode_animation
///
/// ```
/// use abstracttui::base::Rgba;
/// use abstracttui::gfx::{decode_image, png_encode, Bitmap};
///
/// let img = Bitmap::from_fn(2, 2, |x, y| Rgba::rgb((x * 200) as u8, (y * 200) as u8, 40));
/// let png_bytes = png_encode::encode(&img);
/// let decoded = decode_image(&png_bytes).unwrap();
/// assert_eq!(decoded.get(1, 1), img.get(1, 1));
///
/// // Unknown formats reject by NAME (never a panic), telling the
/// // caller what DOES decode:
/// let err = decode_image(b"RIFF\x24\x00\x00\x00WEBPVP8 ").unwrap_err();
/// assert!(err.to_string().contains("PNG"));
///
/// // A format that IS decoded, merely truncated, rejects with its own
/// // decoder's message instead:
/// let err = decode_image(b"GIF89a....").unwrap_err();
/// assert!(err.to_string().contains("gif:"));
/// ```
/// Video containers are recognized ONLY to refuse them well: a user
/// who points a picture viewer at a `.mp4` deserves better than a hex
/// dump of its magic bytes. No demuxer, no codec table — the container
/// name and the command that turns the file into something the engine
/// does decode.
///
/// The engine will not decode video itself: H.264, H.265, VP9, and
/// MPEG-4 are patent-pooled, and a correct real-time decoder for any
/// of them is larger than this crate.
fn video_container(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 {
        return None;
    }
    // ISO base media (`.mp4`, `.mov`) — `ftyp` is the first box.
    if &bytes[4..8] == b"ftyp" || &bytes[4..8] == b"moov" {
        return Some("mp4/mov");
    }
    if &bytes[..4] == b"RIFF" && &bytes[8..12] == b"AVI " {
        return Some("avi");
    }
    // Matroska / WebM.
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some("webm/mkv");
    }
    // MPEG program stream (`.mpg`).
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0xBA]) {
        return Some("mpeg");
    }
    // MPEG transport stream (`.ts`) has no magic — only a 0x47 sync
    // byte every 188 bytes. Demand THREE in a row: 0x47 alone is also
    // ASCII 'G', which would swallow every GIF in the world.
    if bytes.len() > 376 && bytes[0] == 0x47 && bytes[188] == 0x47 && bytes[376] == 0x47 {
        return Some("mpeg-ts");
    }
    None
}

pub fn decode_image(bytes: &[u8]) -> Result<Bitmap> {
    if let Some(container) = video_container(bytes) {
        return Err(Error::Parse(format!(
            "{container}: video is not decoded (animated GIF and APNG are). \
             Convert with: ffmpeg -i IN -vf 'fps=12,scale=480:-1' OUT.gif"
        )));
    }
    match sniff_format(bytes) {
        Some(ImageFormat::Png) => png::decode(bytes),
        Some(ImageFormat::Jpeg) => jpeg::decode(bytes),
        // A still view of a GIF is its first frame — what a viewer
        // shows before anything moves.
        Some(ImageFormat::Gif) => crate::gfx::gif::decode(bytes).and_then(|a| {
            a.frames
                .into_iter()
                .next()
                .map(|f| f.image)
                .ok_or_else(|| Error::Parse("gif: no frames".into()))
        }),
        None => Err(Error::Parse(format!(
            "image: unrecognized format (magic {:02X?}); PNG, JPEG, and GIF decode, \
             WebP/AVIF/TIFF do not",
            &bytes[..bytes.len().min(4)]
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::png_test_encoder::encode_rgba;

    #[test]
    fn routes_png_by_magic() {
        let bmp = Bitmap::from_fn(3, 2, |x, y| {
            crate::base::Rgba::rgb((x * 80) as u8, (y * 100) as u8, 7)
        });
        let png = encode_rgba(&bmp);
        let out = decode_image(&png).unwrap();
        assert_eq!((out.width(), out.height()), (3, 2));
        assert_eq!(out.get(1, 1), bmp.get(1, 1));
    }

    #[test]
    fn routes_jpeg_by_magic() {
        // Any embedded fixture: decoding succeeds through the sniffer.
        let jpg = crate::gfx::jpeg_fixtures::GRAD444;
        assert_eq!(sniff_format(jpg), Some(ImageFormat::Jpeg));
        let out = decode_image(jpg).unwrap();
        assert!(out.width() > 0 && out.height() > 0);
    }

    /// Video containers are named, not decoded — and the sniffing that
    /// names them must not swallow a real picture. The trap here is
    /// real: MPEG-TS's sync byte is 0x47, which is also the `G` that
    /// starts every GIF.
    #[test]
    fn video_containers_are_named_without_stealing_pictures() {
        let mp4 = crate::gfx::anim_fixtures::H264_MP4;
        let err = decode_image(mp4).unwrap_err().to_string();
        assert!(err.contains("mp4/mov"), "{err}");
        assert!(err.contains("ffmpeg -i"), "carries the fix: {err}");

        // A GIF is a picture, not a transport stream.
        let gif = crate::gfx::anim_fixtures::GIF;
        assert_eq!(gif[0], 0x47, "the trap: GIF starts with the TS sync byte");
        assert!(decode_image(gif).is_ok(), "a GIF must still decode");

        // A real transport stream needs its sync byte on the 188-byte
        // grid, three times over.
        let mut ts = vec![0u8; 400];
        for at in [0usize, 188, 376] {
            ts[at] = 0x47;
        }
        assert!(decode_image(&ts)
            .unwrap_err()
            .to_string()
            .contains("mpeg-ts"));
    }

    #[test]
    fn unknown_magic_rejects_by_name() {
        // WebP: a real format the engine does NOT decode.
        let err = decode_image(b"RIFF\x24\x00\x00\x00WEBPVP8 ").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unrecognized format"), "{msg}");
        assert!(msg.contains("PNG"), "must name what DOES decode: {msg}");
        // Empty input: same named rejection, no panic.
        let err = decode_image(b"").unwrap_err();
        assert!(err.to_string().contains("unrecognized format"));
        // A RECOGNIZED format that is merely truncated rejects with
        // ITS decoder's message, not the unrecognized one.
        let err = decode_image(b"GIF89a....").unwrap_err().to_string();
        assert!(err.starts_with("gif:") || err.contains("gif:"), "{err}");
    }

    #[test]
    fn truncated_after_magic_fails_in_the_decoder_not_the_sniffer() {
        let err = decode_image(&[0xFF, 0xD8, 0xFF]).unwrap_err();
        // The JPEG decoder's own named error, not "unrecognized".
        assert!(!err.to_string().contains("unrecognized"), "{err}");
    }

    /// Cycle-7 hardening pass on the UNIFIED entry: the per-decoder
    /// fuzz suites cover png/jpeg internals; this drives the same
    /// hostile classes through the routing layer so a sniff/route bug
    /// can never panic either. Every outcome is Ok or Err — reaching
    /// the end IS the assertion.
    #[test]
    fn decode_image_survives_truncation_and_marker_soup() {
        // Truncation ladder over real containers, byte by byte for the
        // header region then strided for the body.
        let png = encode_rgba(&Bitmap::from_fn(9, 7, |x, y| {
            crate::base::Rgba::rgb((x * 29) as u8, (y * 37) as u8, 128)
        }));
        let jpg = crate::gfx::jpeg_fixtures::GRAD420;
        for src in [&png[..], jpg] {
            for cut in 0..src.len().min(96) {
                let _ = decode_image(&src[..cut]);
            }
            let mut cut = 96;
            while cut < src.len() {
                let _ = decode_image(&src[..cut]);
                cut += 7;
            }
        }

        // Marker soup: xorshift bytes stamped with each magic, plus
        // bit-flip mutations of the real containers (seeded, so a
        // failure reproduces by index).
        let mut state = 0xDEADBEEFCAFEF00Du64;
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for case in 0..300 {
            let len = 8 + (rng() % 300) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| rng() as u8).collect();
            match case % 3 {
                0 => bytes[..8].copy_from_slice(&PNG_MAGIC),
                1 => {
                    bytes[0] = 0xFF;
                    bytes[1] = 0xD8;
                }
                _ => {}
            }
            let _ = decode_image(&bytes);
        }
        for (i, src) in [&png[..], jpg].into_iter().enumerate() {
            for k in 0..200 {
                let mut mutated = src.to_vec();
                let pos = (rng() as usize) % mutated.len();
                mutated[pos] ^= 1 << (rng() % 8);
                // Sniff may now say "unrecognized" (magic flipped) or a
                // decoder error, or even Ok (benign flip): all fine.
                let _ = decode_image(&mutated);
                let _ = (i, k);
            }
        }
    }
}
