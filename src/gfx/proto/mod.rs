//! Pixel-protocol emitters: kitty graphics (APC _G), iTerm2 inline
//! images (OSC 1337), sixel (DCS q). Every emitter is a pure function
//! `Bitmap (+ options) -> Vec<u8>` — NO terminal I/O here. The bytes
//! reach the terminal exclusively through RENDER's
//! `Presenter::external_write(bytes, at)` per the damage contract §6
//! (presenter custody, RT1-5b); `gfx::pipeline` pairs payloads with
//! their target cell position.
//!
//! Protocol references and quirk notes live in
//! `docs/design/gfx-three.md` §2.

pub mod iterm2;
pub mod kitty;
pub mod sixel;
