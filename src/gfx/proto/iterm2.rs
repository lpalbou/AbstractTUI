//! iTerm2 inline image emitter (`OSC 1337 ; File = keys : base64 BEL`).
//!
//! Spec facts (iterm2.com/documentation-images.html, research record in
//! docs/design/gfx-three.md §2.2):
//!
//! - The payload is a complete image FILE (we ship PNG from
//!   `png_encode`), not raw pixels.
//! - `inline=1` renders at the cursor; without it the terminal
//!   DOWNLOADS the payload to disk — always set it.
//! - `width`/`height` accept `N` (cells), `Npx`, `N%`, `auto`; we emit
//!   cell units so the terminal scales and no client resampling is
//!   needed.
//! - `preserveAspectRatio=0` allows stretching to the exact cell box
//!   (default 1 letterboxes inside it).
//! - `size=` is the payload byte count (progress indication only, but
//!   cheap to provide and some implementations use it to pre-allocate).
//! - Terminator: BEL (0x07), the form every implementation accepts.

use crate::gfx::base64;
use crate::gfx::bitmap::Bitmap;
use crate::gfx::png_encode;

#[derive(Clone, Debug)]
pub struct Options {
    /// Render width/height in character cells (None = `auto`).
    pub fit_cols: Option<u32>,
    pub fit_rows: Option<u32>,
    /// Keep the image's aspect ratio inside the cell box.
    pub preserve_aspect: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            fit_cols: None,
            fit_rows: None,
            preserve_aspect: true,
        }
    }
}

/// Emit an inline PNG at the cursor position.
pub fn inline_png(img: &Bitmap, opts: &Options) -> Vec<u8> {
    let png = png_encode::encode(img);
    let mut keys = format!("inline=1;size={}", png.len());
    match opts.fit_cols {
        Some(c) => keys.push_str(&format!(";width={c}")),
        None => keys.push_str(";width=auto"),
    }
    match opts.fit_rows {
        Some(r) => keys.push_str(&format!(";height={r}")),
        None => keys.push_str(";height=auto"),
    }
    if !opts.preserve_aspect {
        keys.push_str(";preserveAspectRatio=0");
    }

    let mut out = Vec::with_capacity(png.len() * 4 / 3 + keys.len() + 16);
    out.extend_from_slice(b"\x1b]1337;File=");
    out.extend_from_slice(keys.as_bytes());
    out.push(b':');
    let mut b64 = String::new();
    base64::encode_into(&png, &mut b64);
    out.extend_from_slice(b64.as_bytes());
    out.push(0x07); // BEL
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Rgba;

    fn parse(bytes: &[u8]) -> (String, Vec<u8>) {
        assert!(bytes.starts_with(b"\x1b]1337;File="));
        assert_eq!(*bytes.last().unwrap(), 0x07);
        let body = &bytes[b"\x1b]1337;File=".len()..bytes.len() - 1];
        let colon = body.iter().position(|&b| b == b':').expect("colon");
        let keys = String::from_utf8(body[..colon].to_vec()).unwrap();
        let payload =
            crate::gfx::base64::decode(std::str::from_utf8(&body[colon + 1..]).unwrap()).unwrap();
        (keys, payload)
    }

    #[test]
    fn inline_round_trips_the_png() {
        let img = Bitmap::from_fn(4, 3, |x, y| {
            Rgba::new((x * 60) as u8, (y * 80) as u8, 5, 255)
        });
        let (keys, payload) = parse(&inline_png(&img, &Options::default()));
        assert!(keys.contains("inline=1"), "{keys}");
        assert!(keys.contains(&format!("size={}", payload.len())));
        assert!(keys.contains("width=auto") && keys.contains("height=auto"));
        assert!(!keys.contains("preserveAspectRatio"), "default omitted");
        assert_eq!(crate::gfx::png::decode(&payload).unwrap(), img);
    }

    #[test]
    fn cell_fit_and_stretch_keys() {
        let img = Bitmap::new(2, 2, Rgba::WHITE);
        let opts = Options {
            fit_cols: Some(20),
            fit_rows: Some(10),
            preserve_aspect: false,
        };
        let (keys, _) = parse(&inline_png(&img, &opts));
        assert!(keys.contains("width=20") && keys.contains("height=10"));
        assert!(keys.contains("preserveAspectRatio=0"));
    }
}
