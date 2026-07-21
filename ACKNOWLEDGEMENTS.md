# Acknowledgements

AbstractTUI is built almost entirely from the Rust standard library, but it
stands on the work of others in three ways: a handful of dependencies, public
specifications, and prior art that shaped the design.

## Dependencies

| Crate | Purpose | License |
| --- | --- | --- |
| [libc](https://crates.io/crates/libc) | Unix terminal FFI | MIT OR Apache-2.0 |
| [windows-sys](https://crates.io/crates/windows-sys) | Windows console FFI | MIT OR Apache-2.0 |
| [unicode-width](https://crates.io/crates/unicode-width) | Cell-width measurement | MIT OR Apache-2.0 |
| [unicode-segmentation](https://crates.io/crates/unicode-segmentation) | Grapheme segmentation | MIT OR Apache-2.0 |
| [miniz_oxide](https://crates.io/crates/miniz_oxide) | DEFLATE (PNG decoding) | MIT OR Zlib OR Apache-2.0 |

## Specifications

The format and protocol support was implemented from public documentation:

- **PNG** — RFC 2083 and the W3C PNG specification
- **JPEG** — ITU-T Recommendation T.81
- **glTF 2.0 / GLB** — the Khronos Group glTF specification
- **sixel** — DEC VT330/VT340 programmer documentation
- **kitty graphics and keyboard protocols** — specifications by Kovid Goyal
- **iTerm2 inline images protocol** — iTerm2 documentation
- **Unicode** — the Unicode Standard and its annexes (width, segmentation)

## Prior art

These projects informed the design; no code was reused from them:

- [ratatui](https://github.com/ratatui/ratatui) — the reference point for
  Rust TUI ergonomics
- [notcurses](https://github.com/dankamongmen/notcurses) — proof of how far
  terminal graphics can be pushed
- [textual](https://github.com/Textualize/textual) — application-grade
  widget and styling ambitions in a terminal
- [SolidJS](https://www.solidjs.com/) — the fine-grained signal reactivity
  model that AbstractTUI adapts to terminal rendering

## Themes

The built-in theme registry includes palettes faithfully ported from color
schemes created and maintained by their communities: Catppuccin, Rosé Pine,
Tokyo Night, Nord, One (Atom), Dracula, Monokai, Gruvbox, Solarized, and
Everforest. The palette values belong to those projects; thank you for the
color.
