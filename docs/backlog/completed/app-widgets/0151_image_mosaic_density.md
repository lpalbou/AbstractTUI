# 0151 — Images draw at the terminal's own mosaic density

Status: completed 2026-08-20
Owner: engine (widgets/image, widgets/markdown_image)
Effort: S

## The field ask

Operator, 2026-08-20, looking at an artifact preview in the gateway
console: "for all images rendered, is there a way to increase the
resolution a bit?"

## What was wrong

`widgets::Image` and the markdown image block both pinned
`MosaicMode::HalfBlock` — 1x2 subpixels per cell — no matter what the
terminal had proved. `docs/getting-started.md` already described the
opposite ("picks the best mosaic mode for the terminal's detected glyph
and color support"), and `gfx::render_to_cells` already did it; the two
widget paths did not.

## Shipped

- `Image::mode` became an override rather than the setting: unpinned
  images resolve `MosaicMode::auto(&app::current_caps())` — quadrants
  (2x2 per cell) wherever UTF-8 and color are proved, braille on
  monochrome-class terminals, half blocks on a legacy codepage.
- The resolution happens PER DRAW, so a capability upgrade (the active
  probe replying after first paint) sharpens the next repaint without a
  rebuild.
- Markdown image blocks resolve the same way; the mosaic LRU key gained
  the mode, so a grid rendered under the old family is never served
  after an upgrade.
- Conservative `Capabilities::default()` (nothing proved) still answers
  HalfBlock, so headless callers, tests, and goldens are unchanged.

## Evidence

Measured on a 1200x1599 photograph drawn into a 24x32 cell pane, each
glyph family scored against one common reference grid:

| family | subpixels/cell | PSNR |
| --- | --- | --- |
| HalfBlock (old) | 1x2 | 20.5 dB |
| Quadrant (new default on color terminals) | 2x2 | 23.5 dB |
| Sextant (opt-in) | 2x3 | 24.3 dB |

## Limits kept

Sextant stays OPT-IN. Its U+1FB00 glyphs are Unicode 13 and no font
probe exists, so `auto` cannot risk tofu; `.mode(MosaicMode::Sextant)`
is the door for callers who know their font. `gfx::pipeline`'s
`MosaicOpts::default()` also stays HalfBlock — that path is the
caller's explicit configuration, and the caller holds the capabilities.
