//! Pixel graphics: RGBA bitmaps, scaling, dithering; cell-mosaic
//! rendering (half blocks, quadrants, sextants, braille) for universal
//! support; native protocols (kitty graphics, iTerm2 OSC 1337, sixel)
//! when the terminal offers them; PNG decode (miniz_oxide inflate + our
//! own chunk/defilter code).
//!
//! OWNER: GFX3D. Protocol choice is capability-driven with an explicit
//! quality ladder: kitty > iterm2 > sixel > mosaic. Degradation is
//! labeled, never silent. Design notes + research citations live in
//! `docs/design/gfx-three.md`.
//!
//! Everything in this module is pure with respect to the terminal: no
//! I/O, no escape emission. Protocol emitters (cycle 2) will produce
//! byte buffers that the render/present layer owns writing out.

pub mod base64;
pub mod bitmap;
pub mod decode;
pub mod dither;
pub mod jpeg;
mod jpeg_dsp;
mod jpeg_entropy;
pub mod mosaic;
mod mosaic_fit;
pub mod pipeline;
pub mod png;
pub mod png_encode;
pub mod proto;
pub mod quantize;
pub mod session;

#[cfg(test)]
mod jpeg_fixtures;
#[cfg(test)]
mod mosaic_quality_tests;
#[cfg(test)]
mod png_test_encoder;

pub use bitmap::Bitmap;
pub use decode::{decode_image, sniff_format, ImageFormat};
pub use mosaic::{
    blit_into, blit_to_cells, render_to_cells, CellPatch, MosaicCell, MosaicGrid, MosaicMode,
    MosaicRenderer,
};
pub use pipeline::{
    choose_channel, present_image, Channel, ExternalSink, ImageOutput, ImageRenderer, RenderedImage,
};
pub use session::{ImageSession, SlotKey, SyncOutcome};
