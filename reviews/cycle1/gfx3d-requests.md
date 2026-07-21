# GFX3D cycle-1 cross-module requests

Author: GFX3D. Everything here is a request, not a change — I own only
`src/gfx/**`, `src/three/**`.

## To KERNEL (term/input) — capability detection plumbing

The gfx ladder (kitty > iterm2 > sixel > mosaic, see
`docs/design/gfx-three.md` §1–2) needs runtime facts only the terminal
layer can gather. Requested, for cycle 2 emitters:

1. **A `TermGraphicsCaps`-shaped struct** (kernel-owned type, gfx will
   consume it read-only) carrying at least: `kitty_graphics: bool`,
   `iterm2_file: bool`, `sixel: bool`, `cell_pixel_size: Option<(u16, u16)>`,
   and ideally `sixel_max_registers: Option<u16>`.
2. **Probe sequencing**: kitty detection = send a 1x1 `a=q,i=<id>,q=2`
   APC query immediately followed by DA1 (`CSI c`); a graphics reply
   before the DA1 answer proves support without hanging on terminals
   that swallow APC. Sixel = DA1 attributes containing `4`.
   `sixel_max_registers` via `CSI ? 1 ; 1 ; 0 S` (XTSMGRAPHICS) where
   answered. iTerm2 = XTVERSION (`CSI > q`) allowlist
   (iTerm2/WezTerm/Konsole) — name tables published online contradict
   each other, so dynamic probes take precedence over any table.
3. **Cell pixel size**: TIOCGWINSZ `ws_xpixel/ws_ypixel` when nonzero,
   else `CSI 16 t` reply. Needed to compute how many pixels an
   image/3D viewport of N x M cells should be rendered at.
4. **Input parser passthrough**: the responses above (APC `ESC _ G ... ESC \`,
   DA1, XTVERSION, XTSMGRAPHICS) must be consumable by whatever probe
   mechanism kernel exposes rather than leaking into the app event
   stream as key garbage.

None of this blocks cycle 1 (mosaic is terminal-independent).

## To RENDER (render/anim) — cell bridge + emission slots

1. **CellPatch bridge**: `gfx::mosaic` outputs `MosaicCell`/`CellPatch`
   (`{ pos: Point, ch: char, fg: Rgba, bg: Rgba }`) and deliberately
   does not import `render::Cell` (decoupling per the build brief). The
   integrator or RENDER decides where the bridging helper lives —
   likely a `Surface::blit_mosaic(&MosaicGrid, origin)` on the render
   side. Semantics gfx guarantees: every cell in the grid is emitted
   (fully-transparent cells mean "image empty here", which matters for
   clearing stale content); fg/bg are straight-alpha RGBA and may be
   `Rgba::TRANSPARENT`.
2. **Pixel-protocol emission slot (cycle 2)**: kitty/iTerm2/sixel
   payloads are byte blobs that must be written at a specific cursor
   position during present, outside the cell diff. I will produce
   `Vec<u8>` (or `&[u8]` chunks) + a target cell rect; the presenter
   owns cursor save/restore and interleaving with SGR runs. A hook of
   the shape `present_overlay(bytes: &[u8], at: Point)` (or an enum
   layer kind) in the compositor pipeline would be enough. Flagging
   now so the presenter design can reserve the seam.
3. FYI (resolved during the cycle, kept for the record): `src/text/mod.rs`
   briefly declared `mod truncate; mod wrap;` before the files landed,
   breaking crate-wide builds (E0583) for ~a minute. Suggest declaring
   modules and files in the same write when working in the shared crate.

## To REDTEAM — suggested attack surface (self-reported)

The three riskiest spots in my cycle-1 code, in order:

1. **PNG chunk walk + unfilter** (`gfx/png.rs`): adversarial chunk
   lengths, CRC bypass attempts, filter-byte garbage, zlib bombs
   (inflate limit is exact expected size — try to make expected size
   huge via IHDR), palette indices out of range, tRNS length abuse.
2. **JSON parser** (`three/gltf_json.rs`): deep nesting (limit 128),
   surrogate-pair edge cases, number grammar corners (`1e999`,
   `-0`, huge exponents), duplicate keys, multi-MB strings without
   escapes (fast-path run copying), invalid UTF-8 via `parse_bytes`.
3. **Mosaic 2-color fit** (`gfx/mosaic_fit.rs` + `gfx/mosaic.rs`): the
   integer-overflow headroom claims in `fit_two_color` (u32 moment
   accumulators at alpha-weighted extremes), tie-break stability across
   platforms, `MAX_GRID_DIM` clamp, and the renderer-reuse path
   (scratch bitmap resize between calls with different modes/sizes).

## To integrator — none

No `base` additions needed; `Point`/`Rgba` covered cycle-1 gfx/three.
No new dependencies requested (miniz_oxide inflate + deflate-for-tests
both come from the already-approved crate).
