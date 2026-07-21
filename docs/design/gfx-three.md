# gfx + three — design notes (GFX3D)

Owner: GFX3D. Scope: `src/gfx/**` (bitmaps, mosaic, image protocols, PNG,
base64, dithering) and `src/three/**` (GLB loading, 3D math, software
rasterizer). This document records the cycle-1 research (with citations),
the capability ladder, and the design decisions + rejected alternatives
behind the shipped code.

## 1. Capability ladder

One pipeline, four output channels, best-first:

| Rank | Channel | Fidelity | Detected by | Used for |
| --- | --- | --- | --- | --- |
| 1 | kitty graphics | full RGBA, ids/placements/z-index | APC probe + DA1 fence | images, 3D viewport, boot splash (hi-fi path) |
| 2 | iTerm2 OSC 1337 | full RGBA (PNG payload), cell/px/% sizing | XTVERSION allowlist | images, 3D stills |
| 3 | sixel | ≤256 palette registers, no placement model | DA1 attribute `4` | images/3D on xterm-class terminals |
| 4 | unicode mosaic | cell-resolution (2x2 / 2x3 / 2x4 subpixels) | always available (UTF-8), glyph set gated | universal fallback, low-cost animation, braille line art |

Degradation is explicit and labeled (vision charter): the chosen channel is
recorded per present, and a downgrade emits a labeled warning once. Detection
is **dynamic, never name-table-only**: published support matrices contradict
each other (e.g. terminfo.dev lists iTerm2/ghostty sixel as "no" while
ansicode lists both as working out of the box). Probes settle it at runtime;
XTVERSION (`ESC [ > q`) is only a hint. Detection machinery is KERNEL's
domain — see `reviews/cycle1/gfx3d-requests.md`.

Within mosaic, the sub-ladder mirrors notcurses' degradation chain:
sextant → quadrant → half-block (and braille as an opt-in style for line
art), gated on font/unicode capability, since "bad font support can ruin"
the denser blitters ([notcurses_visual(3)](https://www.notcurses.com/notcurses_visual.3.html)).

## 2. Research record

### 2.1 kitty graphics protocol

Source: [kitty graphics protocol spec](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
(also mirrored in [kitty repo docs](https://github.com/kovidgoyal/kitty/blob/master/docs/graphics-protocol.rst)).

- Frame: `ESC _ G <control data> ; <payload> ESC \` (APC). Control data is
  comma-separated `key=value`; payload is base64.
- Formats: `f=24` RGB, `f=32` RGBA (default), `f=100` PNG. PNG is explicitly
  recommended as "a compact way of transmitting paletted images".
- Chunking: base64 payload split into chunks ≤ 4096 bytes; **every chunk but
  the last must have a size that is a multiple of 4**; `m=1` on all chunks
  except the final `m=0`. Continuation escapes carry only `m` (and `q`).
- Actions: `a=t` transmit, `a=T` transmit+display, `a=p` place by `i=<id>`,
  `a=d` delete, `a=q` query, `a=f`/`a=a` animation frames.
- Ids: client-chosen `i` (u32); `p` placement id lets one image have many
  placements; `q=1` suppresses OK responses, `q=2` also suppresses errors
  (fire-and-forget emission uses `q=2`).
- z-index: `z` signed; negative draws under text; below `INT32_MIN/2`
  (−1,073,741,824) draws under cells with non-default background too.
- Delete (`a=d,d=<X>`): `a/A` all visible, `i/I` by id (+`p`), `n/N` newest
  by number, `c/C` at cursor, `p/P` at cell, `q/Q` cell+z, `x/X`/`y/Y`/`z/Z`
  by column/row/z, `r/R` id range. Lowercase keeps data; uppercase frees it.
- Unicode placeholders: image can be bound to `U+10EEEE` cells; `a=p,U=1`
  creates a virtual placement (`c=`/`r=` cols/rows); row/col are encoded via
  combining diacritics and the image id rides the foreground color. This is
  the only placement style that survives tmux passthrough.
- Detection handshake: send a 1x1 `a=q` query with an id, then DA1
  (`ESC [ c`); a graphics reply arriving before the DA1 response proves
  support without hanging on terminals that ignore APC.
- Compression: `o=z` marks the payload as zlib-deflate — we already ship
  miniz_oxide (deflate is available), so this is a cheap cycle-2 win for
  large RGBA transfers.
- Quirks (support pages + [ansicode summary](https://ansicode.eversources.app/en/sequence/dcs-kitty-graphics)):
  ghostty and wezterm implement the core protocol natively; konsole is
  partial; wezterm's implementation lags on animation and unicode
  placeholders; notcurses distinguishes three kitty generations
  (static / animated / self-ref, [notcurses_capabilities(3)](https://man.archlinux.org/man/notcurses_capabilities.3.en)).
  We will treat "core transmit/place/delete with ids" as the baseline and
  gate placeholders/animation separately.

### 2.2 iTerm2 OSC 1337 `File=`

Source: [iTerm2 images documentation](https://iterm2.com/documentation-images.html),
[iTerm2 escape codes](https://iterm2.com/documentation-escape-codes.html).

- Frame: `ESC ] 1337 ; File = <args> : <base64 file contents> BEL` (ST also
  accepted). Args are semicolon-separated `key=value`.
- Keys: `name` (base64 filename), `size` (bytes, progress only), `width` /
  `height` (`N` cells, `Npx` pixels, `N%`, `auto`), `preserveAspectRatio`
  (default 1), `inline=1` (else the terminal *downloads* the file — always
  set `inline=1`).
- Payload is a real image **file** (anything macOS renders: PNG, JPEG, GIF,
  PDF) — not raw pixels. So this channel requires a PNG *encoder*; the
  test-only encoder shipped this cycle (filter 0 + miniz_oxide zlib) is
  written to be promotable to `src/gfx` proper in cycle 2.
- Large payloads: `MultipartFile` / `FilePart` / `FileEnd` chunked variant
  (needed under tmux; also implemented by Otty).
- `ReportCellSize` returns the cell size in points — useful but proprietary;
  our primary cell-pixel-size source stays TIOCGWINSZ / `CSI 14t`/`16t`
  (KERNEL request).
- Adoption: iTerm2 (origin), WezTerm, Konsole 22.04+, VS Code/Otty subset
  ([ansicode OSC 1337 page](https://ansicode.eversources.app/en/sequence/osc-iterm-image)).

### 2.3 sixel

Sources: [VT330/VT340 programmer reference ch. 14](https://www.vt100.net/docs/vt3xx-gp/chapter14.html),
[Wikipedia: Sixel](https://en.wikipedia.org/wiki/Sixel),
[terminfo.dev/sixel](https://terminfo.dev/sixel).

- Frame: `DCS P1 ; P2 ; P3 q <data> ST` (`ESC P ... ESC \`). `P2` controls
  background: 0/2 = pixels at 0 painted in background color, **1 = zero
  pixels left untouched (transparency)**.
- Raster attributes: `" Pan ; Pad ; Ph ; Pv` (aspect ratio numerator/
  denominator, then width/height hints) must precede data if used.
- Color registers: `# Pc ; Pu ; Px ; Py ; Pz` defines register `Pc`
  (0..=255 on modern emulators; the VT340 had 16); `Pu=2` selects RGB with
  channels **scaled 0–100 percent** (not 0–255 — quantize with rounding, it
  is a real fidelity loss), `Pu=1` selects HLS. Bare `#Pc` selects the
  register for subsequent data.
- Data bytes `0x3F..=0x7E` encode a 1-wide, 6-tall pixel column
  (`byte − 63` = bitmask, LSB = top row). `!N<byte>` run-length repeats,
  `$` returns carriage within the 6-row band (for painting another register
  over the same band), `-` advances to the next band.
- Emission strategy (cycle 2): quantize to 16–64 registers via median cut
  over the (dithered) image, one pass per register per band using `$`
  rewinds; Floyd–Steinberg dithering (shipped this cycle in
  `gfx::dither`) before encoding hides most of the palette loss. This is
  the approach chafa/libsixel converge on.
- Support: xterm (`-ti vt340`/compile flag), foot, mlterm, contour, WezTerm,
  Windows Terminal 1.22+; kitty declined it (own protocol); published
  tables disagree about iTerm2/ghostty — hence detection via **DA1
  response containing attribute `4`**, never a name list.

### 2.4 chafa & notcurses (mosaic state of the art)

Sources: [chafa symbol-selection port notes](https://docs.rs/chafa-syms-rs/latest/chafa_syms_rs/),
[chafa issue #201 discussion](https://github.com/hpjansson/chafa/issues/201),
[ChafaSymbolMap reference](https://hpjansson.org/chafa/ref/chafa-ChafaSymbolMap.html),
[notcurses_visual(3)](https://www.notcurses.com/notcurses_visual.3.html).

- chafa's core: each cell picks the (symbol, fg, bg) triple that best
  reconstructs the cell's pixels; symbols carry 8x8 coverage bitmaps; the
  selector does an exhaustive MSE search at high work levels ("we use the
  same algorithm at `-w 9`; MSE exhaustive search" — hpjansson). Colors for
  a candidate partition come from mean extraction over covered/uncovered
  pixels (chafa's work-cell; its MEDIAN extractor is "extremely slow and
  makes almost no difference"). All-integer math.
- chafa never blits full blocks: a uniform cell is emitted as a space with
  the background set (cheaper SGR runs). Our fg/bg-swap canonicalization
  (below) reproduces this for free.
- notcurses blitters: `2x1` halves, `2x2` quadrants, `3x2` sextants, `4x2`
  octants (Unicode 16), braille 4x2; graceful degradation pixel → sextant →
  quadrant → half → ascii. Octants/sedecimants are a future densification
  option; chafa found 16x16 glyph bitmaps not worth the complexity over 8x8.
- Both stress that font quality gates the dense blitters — the reason our
  mosaic mode is caller-selectable rather than hardcoded densest-first.

### 2.5 GLB container + minimal glTF 2.0 subset

Sources: [glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
[Khronos glTF 2.0 reference guide](https://www.khronos.org/files/gltf20-reference-guide.pdf),
[Kaitai GLB spec](https://formats.kaitai.io/gltf_binary/index.html).

- GLB: little-endian; 12-byte header `magic=0x46546C67` ("glTF"),
  `version=2`, `length` (total file bytes). Then chunks:
  `chunkLength u32`, `chunkType u32`, `chunkData` (start and end 4-byte
  aligned). First chunk MUST be JSON (`0x4E4F534A`); BIN (`0x004E4942`) is
  optional and second. JSON pads with spaces `0x20`, BIN with zeros `0x00`;
  BIN may be up to 3 bytes longer than `buffer.byteLength`. Readers must
  skip unknown chunk types.
- Verified against real assets (headers hex-dumped 2026-07-20):
  `meshvault/frontend/testmodels/helmet.glb` (magic ok, len 3,773,916 ==
  file size, JSON chunk 2,148 B), `machine.glb` (len 1,472, JSON 1,108 B),
  `abstract3d/out/x-wing/scene.glb` (len 6,160,492, JSON 1,348 B).
- Minimal static-mesh subset for `three`:
  - `buffers` (GLB buffer 0 = BIN chunk, `uri` undefined), `bufferViews`
    (`buffer`, `byteOffset`, `byteLength`, optional `byteStride` for vertex
    data), `accessors` (`bufferView`, `byteOffset`, `componentType`,
    `count`, `type`, optional `normalized`).
  - componentType: 5120 i8, 5121 u8, 5122 i16, 5123 u16, 5125 u32,
    5126 f32. type → component count: SCALAR 1, VEC2 2, VEC3 3, VEC4 4,
    MAT4 16. We need VEC3/f32 (POSITION, NORMAL), VEC2/f32 (TEXCOORD_0,
    later), SCALAR/u16|u32 (indices; u8 tolerated).
  - `meshes[].primitives[]`: `attributes` (POSITION required for us),
    `indices` (optional — non-indexed draws exist), `material`, `mode`
    (default 4 = TRIANGLES; we accept only 4 in v1, reject the rest loudly).
  - `nodes`: `matrix` (column-major, 16 floats) XOR TRS (`translation`
    vec3, `rotation` quaternion **xyzw**, `scale` vec3), `children`,
    `mesh`; `scenes[scene].nodes` roots. Composition order T·R·S.
  - `materials[].pbrMetallicRoughness.baseColorFactor` (RGBA, default
    [1,1,1,1]); everything else (textures, KHR extensions, Draco/meshopt
    compression) is out of scope and must fail with a clear `Error::Parse`
    naming the unsupported feature (helmet_draco.glb / helmet_meshopt.glb
    are the negative fixtures).

## 3. Shipped this cycle (and why it looks the way it does)

### 3.1 `gfx::bitmap`

`Bitmap { w, h, Vec<Rgba> }`, row-major, straight (non-premultiplied) RGBA.
`from_fn` for tests, `fill`, checked `get`/`set`, nearest + bilinear resize
with `_into` variants that reuse the destination allocation (mosaic and the
3D viewport re-render per frame; per-frame Vec churn is the enemy).
Bilinear resampling **premultiplies before filtering and unpremultiplies
after**: filtering straight RGBA lets fully-transparent neighbors (whose
RGB is meaningless) bleed color halos into edges. Premultiply helpers are
public because kitty `f=32` and PNG want straight alpha while compositing
and filtering want premultiplied.

### 3.2 `gfx::mosaic` + `gfx::mosaic_fit`

Pure (no terminal I/O). `render(bitmap, cols, rows, mode) -> MosaicGrid` of
`(char, fg, bg)`; `blit_to_cells` flattens to `CellPatch { pos, ch, fg, bg }`
for the integrator to bridge into RENDER's `Surface` (we deliberately do
not import `render::Cell` — coupling would let either side's refactor break
the other silently). Grid orchestration (scaling, buffer reuse) lives in
`mosaic.rs`; the per-cell selection math and glyph tables in
`mosaic_fit.rs`, so the two evolve and get attacked independently.

Modes and glyph sources:

- `HalfBlock`: 1x2 px/cell, `▀` U+2580, fg = top px, bg = bottom px. Exact.
- `Quadrant`: 2x2 px/cell, the 16-glyph quadrant set (U+2596..U+259F,
  halves, space, full).
- `Sextant`: 2x3 px/cell, the 64-pattern space: space, `█`, `▌`/`▐` for the
  two column patterns, and U+1FB00..U+1FB3B for the remaining 60
  ("Symbols for Legacy Computing"; the codepoint is formulaic —
  `0x1FB00 + (b−1) − [b>21] − [b>42]` — no table needed).
- `Braille`: 2x4 px/cell, U+2800 + dot bits. Not a 2-color coverage fit:
  braille dots render sparse (a lit dot covers far less than 1/8 cell), so
  the coverage-MSE model that justifies the block fit is wrong for it.
  Instead: luminance threshold against the per-cell weighted mean (strictly
  above ⇒ lit), fg = mean lit color, bg = mean unlit (TRANSPARENT if none).
  This matches braille's real use: structure/line art, not color fields.

**The 2-color fit** (Quadrant/Sextant), the part REDTEAM should read
carefully: a candidate pattern `P` paints covered subpixels with one color
and uncovered with another. For fixed `P`, the total squared error
`Σ_covered w·‖c−fg‖² + Σ_uncovered w·‖c−bg‖²` is a weighted least-squares
problem solved per channel by the weighted means of each side (`w` =
alpha, so transparent pixels influence nothing). So the search is: for each
pattern, take the two weighted means, sum the squared error, keep the
argmin. A pattern and its complement describe the same partition (swap
fg/bg), so we canonicalize on "subpixel 0 is background" and search 8
patterns (quadrant) / 32 (sextant) instead of 16/64. Side effect: a uniform
cell canonicalizes to pattern 0 = space + bg, chafa's SGR-run trick, free.

Error metric: **plain squared distance in sRGB, integer math** (chafa
parity). Rejected: CIEDE2000/Oklab per-subpixel (an order of magnitude more
arithmetic for invisible gains at 2x3 px granularity), redmean weighting
(kept in mind as a one-line tweak if REDTEAM shows a bad case), and the
sum-of-squares/variance identity trick (`argmax Σ S²/W` avoids computing
means but makes comparisons fractional — cross-multiplying denominators
costs more than it saves at ≤32 patterns; revisit if octants (2x4 coverage
fit, 128 canonical patterns) land).

Weighted means round to nearest (`+w/2` before the divide) — deterministic,
and the ≤0.5/channel bias is far under one quantization step of the error
metric. Ties in the pattern search resolve to the lower pattern index
(fewer fg bits ⇒ more bg-only cells ⇒ cheaper SGR downstream).

Hot path discipline: `MosaicRenderer` owns the scaled scratch `Bitmap` and
the output grid; per-cell work uses fixed `[..; 8]` stack arrays; zero heap
allocation per cell, one optional resize per frame (skipped entirely when
the source is already cell-exact, which is how the 3D viewport will call
it).

Dithering (`gfx::dither`): Floyd–Steinberg error diffusion onto an
arbitrary palette, serpentine scan (halves the worm artifacts for the cost
of an index flip), error carried in i16 per channel with saturating adds.
Used before sixel encoding (cycle 2) and for low-color mosaic targets; the
mosaic fit itself stays truecolor.

### 3.3 `gfx::png`

Decoder for the common critical path: signature → chunk walk with CRC32
verification (const-fn table, no runtime init) → IHDR validation → PLTE /
tRNS → IDAT concatenation → `miniz_oxide` inflate **with an exact output
limit** (`h·(1+w·bpp)` — a wrong-sized stream is corrupt; the limit also
caps decompression bombs) → per-scanline unfilter (None/Sub/Up/Average/
Paeth) → RGBA `Bitmap`.

Scope v1 (documented, enforced with precise `Error::Parse` messages): bit
depth 8 only, color types 0/2/3/4/6, no Adam7 (rejected, not silently
mangled), palette + `tRNS` (including gray/RGB colorkey), ancillary chunks
skipped, `IEND` required. 16-bit depth and sub-8-bit palette depths are the
known cycle-2 gap (real-world icons use depth-4 palettes; the x-wing
`*.png` textures in `abstract3d/out/` are depth-8 so the critical path is
covered).

Paeth note (why the reference formula, byte order matters): the predictor
picks whichever of left/up/up-left is closest to `left + up − upleft`; ties
break **a, then b, then c** — a different tie order produces different
(wrong) pixels that still "look plausible", which is why the tests pin
hand-computed vectors, not round-trips alone.

The test-only encoder (`#[cfg(test)]`) emits filter-0 scanlines +
`miniz_oxide` zlib and doubles as the fixture generator; it is deliberately
structured to be promoted to a public minimal encoder in cycle 2 (iTerm2
needs real PNG payloads; kitty `f=100` benefits for paletted content).

### 3.4 `gfx::base64`

RFC 4648 standard alphabet, padded; `encode_into` appends to a caller
buffer (kitty chunking wants one reused String); strict decoder (rejects
bad symbols, bad length, data after padding) for tests and future protocol
response parsing. Kitty constraint honored by construction: encoding whole
payloads then slicing at 4096-byte boundaries keeps every non-final chunk a
multiple of 4.

### 3.5 `three::math`

`Vec3`/`Vec4`/`Mat4` (column-major, matching glTF's `node.matrix` layout so
loading is a memcpy, and OpenGL convention so every reference formula
transcribes 1:1). Right-handed view space (camera looks −Z), NDC z ∈
[−1,1], `perspective` = gluPerspective, `look_at` = gluLookAt,
`from_quat` (glTF xyzw order) for node TRS in cycle 2. `normalize` of a
near-zero vector returns zero (rasterizer treats degenerate normals as
unlit rather than NaN-poisoning the framebuffer — NaN is the thing property
tests can't see past).

### 3.6 `three::gltf_json` + `three::glb`

Hand-rolled recursive-descent JSON: strict grammar (leading zeros, bare
`.5`/`1.`, `+1`, NaN/Infinity all rejected), full string escapes incl.
`\uXXXX` with surrogate-pair combining (lone surrogates rejected), a
**depth limit of 128** (deep nesting is stack exhaustion — the classic
parser DoS; glTF legitimately nests ~6 deep), duplicate keys keep first
(lookup order-preserving Vec, no HashMap needed at glTF object sizes).
Number conversion: we lex the token per the JSON grammar, then hand the
validated slice to `f64::from_str` — writing a correct decimal→binary64
converter is its own project and std's is exact; the *grammar* stays ours.
Rejected: streaming/SAX parser (glTF JSON chunks are ≤ a few MB and
random-access lookups dominate; a DOM is simpler to test exhaustively).

`glb::split` validates magic/version/declared-length-vs-buffer, walks
chunks with checked arithmetic (u32 lengths near `u32::MAX` must not
overflow the cursor), requires JSON first, takes the first BIN, skips
unknown chunk types per spec, and returns borrowed slices (zero-copy).
`three::doc` holds the typed views (`Doc`: accessors/bufferViews/meshes/
primitives/nodes/materials), with extraction implemented at the metadata
level — reading vertex bytes out of BIN (strides, component conversion)
comes next cycle together with the rasterizer. `Doc::parse` enforces
asset.version 2.x and refuses `extensionsRequired` by name (the Draco and
meshopt helmet variants are the negative fixtures). Integration tests
split + parse the three real assets' headers and JSON chunks when the
paths exist (guarded by `Path::exists`, skip silently elsewhere).

## 4. Cycle-2 additions

### 4.1 Protocol emitters (`gfx::proto`) + pipeline

All three emitters are pure `Bitmap (+ options) -> Vec<u8>`; bytes reach
the terminal ONLY through `Presenter::external_write(bytes, at)` (damage
contract §6). `gfx::pipeline::ImageRenderer` is the facade: bitmap +
target cell `Rect` + `term::caps::GraphicsCaps` (the read-only view
KERNEL built on our cycle-1 request) → either `CellPatch`es (mosaic) or
`(bytes, at)`. Degradations carry `#FALLBACK`-prefixed warning strings.

- **kitty** (`proto::kitty`): `a=T` transmit+display with `f=32`/`f=24`
  (+ `o=z` zlib via miniz_oxide) or `f=100` (PNG from `png_encode`);
  base64 chunking at 4096 with `m=1/0` (whole-payload encode sliced at
  4096 keeps non-final chunks 4-aligned by construction); `q=2`
  everywhere (fire-and-forget — replies would land as input garbage);
  client-chosen non-zero ids (id 0 = terminal-picks = undeletable);
  `c=`/`r=` cell fit so the TERMINAL scales (no client resample, full
  fidelity); `a=p` re-place, `a=d` delete with the lowercase/uppercase
  (keep/free) distinction.
- **iTerm2** (`proto::iterm2`): `OSC 1337 File=inline=1;size=N;width=
  <cells>;height=<cells>[;preserveAspectRatio=0] : base64(PNG) BEL`.
  Payload is a real PNG file — the cycle-1 test encoder was promoted to
  `gfx::png_encode::encode` (public, RGBA8, filter 0, miniz_oxide zlib,
  byte-deterministic; per-row filter heuristics rejected: terminal
  images are small and deflate absorbs most redundancy — auditability
  wins).
- **sixel** (`proto::sixel`): median-cut quantization
  (`gfx::quantize`, ≤ `sixel_max_registers` honored, default 64) →
  optional Floyd–Steinberg (`gfx::dither`) → `DCS 0;1;0 q` with raster
  attributes, percent-scaled RGB register definitions (rounded, the
  documented 0–100 fidelity loss), per-band per-register passes with
  `$` rewinds, `!n` RLE (pays at n ≥ 4), trailing-empty-column trim,
  P2=1 transparency (fully transparent pixels touch no register).
  Tests replay emissions through a test-side sixel interpreter —
  verifying the IMAGE, not byte trivia.

**RT1-11 ruling (sixel registers):** one palette per emission in
registers `[base, base+N)`, default base 0 — single-live-image is the
documented v1 limit. Rejected for now: static partitioning (caps every
image's fidelity to pay for a case the ladder mostly avoids — kitty and
iTerm2 terminals take the higher rungs) and a dynamic allocator (needs
engine-global state that pure emitters must not own). The
`register_base` option is the forward-compat seam: the pipeline can
partition later without touching the emitter. REDTEAM's two-image
golden should pin the clobber behavior this documents.

The mosaic grid additionally exposes
`MosaicGrid::cell_patches(origin) -> impl Iterator<Item = (Point, char,
Rgba, Rgba)>` — the exact shape RENDER's `Surface::blit_mosaic`
consumes (integrator contract, this cycle).

### 4.2 GLB extraction + model loading (`three::extract`, `three::load`)

RT1-8 hostile rules, all enforced with NAMED rejections and pinned by
REDTEAM's mutator battery (`testing::glb_mutate`, driven end-to-end in
`load.rs` tests — MustLoad loads with triangles, MustReject errors,
byte soup never panics):

- `from_le_bytes` on byte slices only (unaligned offsets in real files
  just work; a big-endian port fails in review, not in rendering);
- all offset/stride/span math checked in u64 BEFORE any allocation
  (`count = u32::MAX` rejects, never allocates);
- stride < element, stride % component != 0, span past view, view past
  BIN, external buffers (buffer != 0), sparse accessors, non-TRIANGLES
  modes, float indices: each rejects naming the rule;
- indices are bounds-checked against the vertex count at extraction so
  the rasterizer never re-checks.

`load::Model` flattens the node hierarchy iteratively (matrix XOR TRS
per node, `T·R·S` order, revisit detection — a node reachable twice is
a named error, killing both cycles and shared children; depth cap 256)
and extracts per-placement. Materials keep `baseColorFactor` and decode
GLB-embedded PNG `baseColorTexture`s with our `png.rs`; JPEG textures
(helmet.glb) and external URIs degrade with `#FALLBACK` labels — the
model still loads, base color stands in. Verified live: helmet 15,452
tris + labeled jpeg fallbacks; machine 24 tris; x-wing 119,999 tris +
decoded PNG texture.

### 4.3 The rasterizer (`three::raster`, `three::scene`)

Pipeline: model → view (per-instance `view * world`) → per-vertex
lambert in VIEW space (gouraud when normals exist, face normals
otherwise; vertex colors + baseColorFactor modulate in linear space) →
Sutherland–Hodgman clip against the near plane in view space (exact:
view space is affine; output ≤ 4 vertices, fan-triangulated) →
perspective + viewport (y flip) → integer edge-function fill.

Decisions worth defending:

- **Near clip in view space** (not clip space): attribute lerp is exact
  there, the plane test is one comparison (`z <= -near`), and the
  camera-inside-geometry case degrades to stable clipping instead of
  w≈0 projection explosions (test-pinned).
- **Top-left fill rule in 1/16-subpixel integer math.** Derived for
  y-down, positive-area winding: TOP edge = horizontal pointing right,
  LEFT edge = pointing up; non-top-left edges get a −1 bias. The
  two-triangles-sharing-a-diagonal test pins exactly-once coverage.
  Floats wobble on shared edges; integers do not. Barycentrics use the
  UNBIASED edge values (the bias gates coverage only; folding it into
  weights makes them not sum to the area — constant attributes would
  drift, which the quad repaint test caught live).
- **Depth = NDC z, screen-linear.** `z_ndc` is an affine function of
  screen x,y (that is what the perspective divide does), so linear
  interpolation IS perspective-correct for depth. Color stays
  screen-linear too — a documented approximation, invisible at
  160x96-px triangle sizes; uv/texturing (cycle 3) will need real 1/w
  interpolation.
- **Winding**: glTF front faces are CCW in y-up; after the screen y
  flip they produce negative `orient2d` area, so the scene stage swaps
  two vertices to canonicalize and culls positive-area (back) faces.
  `Scene::double_sided` rasterizes both (trimesh-exported assets are
  not consistently wound; the e2e tests run double-sided).
- **sRGB-ish output**: lighting in linear space (gamma-2 square
  approximation both ways — `powf(2.4)` per pixel is the wrong place
  to spend the 33 ms budget; the mosaic quantization hides the
  residue).
- z-test LESS with NDC range clamp (beyond-far geometry rejected per
  pixel); NaN vertices skip their triangle (hostile files can smuggle
  NaN through valid f32 bits — the framebuffer must never be poisoned).

Camera: orbit (yaw/pitch/distance/target, pitch clamped ±88.8° for the
up-vector guard) + `framing(bounds)` fitting the bounding sphere with
15% margin. Perf: per-pixel work is allocation-free; per-instance
vertex buffers are reused across instances within a render call.
Budget (charter): 160x96 px shaded helmet ≥ 30 fps single-thread —
pinned by the `#[ignore]`d `perf_three_helmet_160x96` test
(measured: see cycle-2 report).

## 4.4 Cycle-3 additions

### Parse-time validation (`three::validate`, RT2-2/RT2-3)

Everything checkable from metadata alone now rejects at `Doc::parse`,
by name: dangling indices anywhere in the reference graph (accessor →
view → buffer, primitive → accessor/material, node → mesh/children,
scene → node, material → texture → image → view), sparse accessors,
zero counts, stride games, spans past DECLARED view/buffer lengths,
core-spec attribute shapes (POSITION/NORMAL = VEC3/f32; TEXCOORD_0 =
VEC2 f32|norm-u8/u16; COLOR_0 = VEC3/4 f32|norm-u8/u16; indices =
SCALAR u8/u16/u32; no byteStride on index views). The dividing line:
spec-INVALID metadata rejects at parse; spec-LEGAL-but-unsupported
(mode != TRIANGLES) and anything needing the REAL BIN or a graph walk
(node cycles, missing-BIN) rejects at extraction/load — extraction
re-validates against actual bytes via the SHARED
`validate::accessor_layout/accessor_span` arithmetic (one copy, no
drift). REDTEAM's parse-level ratchet dropped 16 → 3 tolerated
entries (the three by-design load-level cases).

Severity rulings from the cycle-2 self-flags: malformed containers
REJECT (image bufferView past the real BIN, corrupt embedded PNG);
unimplemented features DEGRADE with `#FALLBACK` labels (JPEG texture,
external uri). byteStride on an index view REJECTS (spec forbids it;
honoring one would misread interleaved bytes as indices).

### Textured rasterization

UV interpolation is perspective-correct: vertices carry u/w, v/w, 1/w
(the screen-affine quantities), pixels divide back. Vertex COLOR stays
screen-linear — the documented asymmetry: at cell-scale triangle sizes
color error is invisible, UV error would visibly swim. Sampling
(`three::texture`): bilinear (default) + nearest, repeat/clamp wraps,
texel-center convention, sRGB→linear at the texel (`srgb8_to_linear`,
gamma-2 — the exact inverse of the rasterizer's sqrt output; factors
and vertex colors are declared linear by glTF and never converted).
Textured iff material texture decoded AND mesh has UVs; modulation
order = baseColorFactor x vertexColor x lighting x texel (glTF
semantics). Measured (release, 160x96, double-sided): helmet 15k tris
4.8 ms plain → 7.7 ms with a synthetic 256² texture (its real
textures are JPEG → labeled fallback); x-wing 120k tris + real PNG
texture ~71 ms — geometry-bound (untextured measures the same class),
report-only, 8x past the charter's asset class.

### Widgets (`widgets::image`, `widgets::viewport3d`)

`Image`: bitmap or PNG-path source (decode errors = labeled
broken-image state in `text_faint`, per the §3 style guide),
contain/cover/fill/none fits with per-axis alignment (cell aspect
assumed 1:2 until caps geometry reaches the widget layer), mosaic mode
override. Draw closures own only cells, so the widget is mosaic-only
BY DESIGN; the protocol path is the app-level
`gfx::pipeline::present_image(renderer, sink, …)` over the
`ExternalSink` seam (bytes → `Presenter::external_write`), and full
widget-protocol integration (post-present overlay pass) is filed for
cycle 6. `Viewport3D`: pure over props (yaw/pitch/zoom/spin as plain
floats, signals live app-side), framing camera from `Model::bounds`,
drag-orbit with pointer capture + wheel zoom reported as deltas via
callbacks, per-mode framebuffer density, buffers persist in the FnMut
closure.

### The brandmark (`three::brandmark` + `boot::brandmark3d`)

`BrandmarkRenderer::render(t, size, theme) -> &Surface` implements the
full storyboard from `boot::identity` constants: staggered arrival
(EASE_ARRIVAL), settle overshoot (EASE_SETTLE), camera yaw −35°→−6° +
dolly 5.2→4.4, ramp-gradient emissive planes (vertex colors,
sRGB→linear), depth fog toward the theme ground, radial BRAND_FIELD
vignette, 12-spark burst, afterglow trail (decay-per-100 ms constant,
per-channel-max merge — additive EMULATED in pixel space: RENDER's
`Blend::Additive` is layer-level and the `SplashFrameSource` seam is a
single Surface, so compositor blending cannot apply), wordmark
tracking collapse + underline sweep + tagline + skip hint. DESIGN's
`boot::brandmark3d::Brandmark3d` wraps it in the one-line trait impl
(layer direction stays clean; `three` imports only `boot::identity`
constants + `theme::Theme`, the recorded exception). Deterministic per
fresh renderer (trail is frame-history by design, matching the
player's drop-not-queue pacing). Measured: 0.65 ms median per frame at
100x30 release (budget 8 ms).

## 4.5 Cycle-4 additions

- **RT3-1 (rasterizer overflow)**: two layers — `fill_triangle` clamps
  snapped coordinates to ±2^29 subpixels (orient2d overflow now
  structurally impossible for any input) and the scene stage clips
  projected polygons to a screen-space guard band
  (`raster::clip_screen_rect`, exact under this pipeline's
  interpolation model since ndc_z/u/w/v/w/1/w are screen-affine).
  Real geometry never reaches the clamp; near-glancing triangles
  rasterize correctly.
- **R4-1 (brandmark layering)**: `BrandmarkParams` (plain data in
  three) replaced the `boot::identity` import; DESIGN's adapter builds
  it from identity constants; `BrandmarkParams::reference()` +
  `identity_drift_pin` (test-only upward look) keep the compat copy
  honest. Color mixing moved to `base::Rgba::lerp`.
- **SceneRenderer**: per-vertex projection ONCE per instance (was per
  triangle corner), reusable SoA scratch across frames, fast path for
  fully-in-front triangles, screen-bbox reject before fill setup.
  x-wing before/after in the cycle-4 report.
- **ImageSession** (`gfx/session.rs`): placement lifecycle per channel
  — kitty id reuse (move = `a=p` only, content change = delete+
  retransmit, drop = delete), iTerm2/sixel full re-emit (no ids —
  documented), channel-upgrade reset, tmux passthrough on every
  payload when `GraphicsCaps.wrap == Some(Tmux)` (KERNEL's verified
  detection; wrapping is routing, not degradation).
- **MosaicOpts**: `dither: Option<u16>` pre-pass (median-cut + FS) for
  low-color targets; quality goldens pin the chooser's decisions on
  hard edges / gradients / seeded noise (`mosaic_quality_tests.rs`).

## 4.6 Cycle-5 additions

- **Baseline JPEG decoder** (`gfx::jpeg`, entropy + dsp helpers):
  SOF0/SOF1 Huffman 8-bit, YCbCr/gray, sampling 1..=2 per axis via one
  general MCU walk, DRI/RSTn, APPn skipped; progressive/lossless/
  arithmetic/12-bit/16-bit-DQT/CMYK/multi-scan rejected BY NAME. IDCT
  = naive separable float (T.81 A.3.3 transcription; correctness over
  speed for a one-time decode — and its test pins the separable form
  against the direct 4-loop definition with asymmetric coefficients,
  which caught a real transposition bug in the first draft). Chroma
  upsampling nearest (labeled). Fixtures are real cjpeg output with
  regeneration commands embedded; truncation ladder + marker-soup
  fuzz pin the no-panic posture. Pixel budget shared with PNG.
- **Helmet textured**: `three::load` decodes image/jpeg textures (and
  sniffs undeclared MIME by magic); the flagship asset's fallback
  label is gone, e2e pins real texture sampling.
- **Viewer API**: `Viewport3D::DEFAULT_ORBIT` + `.light_angles()` +
  `.fog()` (over the now-public `Framebuffer::depth_fog`), scratch
  reuse via `SceneRenderer` in the draw closure. Brandmark's compat
  constructor deleted (adapter builds params from identity — R4-1
  epilogue); `BrandmarkParams::reference()` is test-only.

## 4.7 Cycle-6 additions: animation + skinning

- **Animation data model** (`three/animation.rs`): `Animation` =
  named track list; `Track` = (node, times, values, interpolation)
  with `TrackValues::{Translation, Rotation, Scale}`. `sample(t,
  &mut [NodePose])` overrides only the animated fields of rest poses;
  `t` CLAMPS to the keyframe range (looping is the caller's
  `t % duration` — the widget does it, the library never guesses).
  Keyframe lookup is `partition_point` binary search; duplicate times
  are spec-legal hard cuts (hold-left semantics).
- **Interpolation scope**: LINEAR and STEP. Rotations use NLERP with
  shortest-path sign correction (negate an endpoint when the dot is
  negative) — normalized linear, not slerp: at cell resolution and
  real keyframe densities the angular-velocity difference is
  invisible, and the spec permits the approximation. CUBICSPLINE
  channels SKIP WITH A LABEL (`#FALLBACK`): their output accessors
  carry in-tangent/value/out-tangent triplets, so sampling them as
  values would play garbage, and rejecting the file would kill the
  channels that play fine. Morph `weights` channels skip with a label
  (no morph pipeline). Unknown interpolations and paths reject by
  name; decreasing or non-finite keyframe times reject by name.
- **Rig** (`load.rs`): built whenever a model has animations OR skins
  — `RigNode` (rest TRS, optional matrix, children), scene roots,
  animations, skins, and `instance_skins` (per-instance skin binding
  kept as a rig-side PARALLEL ARRAY rather than a `MeshInstance`
  field: adding skinning must not change the struct shape every
  constructor in the crate depends on — the cycle-6 lesson).
  Matrix-form nodes never animate (spec: animation targets use TRS).
- **Pose sampling**: `Model::sample_pose_full(anim, t, &mut Pose)` →
  per-instance worlds + per-skin joint matrices
  (`world(joint) * inverseBind`). `Pose` owns the sampling scratch
  (rest poses, world array, DFS stack), so steady-state playback
  allocates NOTHING (pinned by a capacity-stability test).
  `sample(t)` is pure: same t, same pose, bit-exact (REDTEAM's
  determinism suite hashes the matrices).
- **Skinning** (glTF `skins`): JOINTS_0 (VEC4 u8/u16) + WEIGHTS_0
  (VEC4 f32 or normalized u8/u16) extract with both-or-neither and
  count==POSITION rules; inverseBindMatrices (MAT4 f32) length must
  cover the joint list; ABSENT inverseBindMatrices = identity (spec
  default, not an error). Load-time sanitation with the skin context:
  joint indices bound by the joint list WHERE THE WEIGHT IS NONZERO
  (exporters pad unused slots with garbage), weights must be finite
  and non-negative, zero sums reject by name, drifted sums (>1%)
  renormalize with one `#FALLBACK` per primitive (real exporters
  quantize).
- **Skinned vertex stage** (`scene.rs`): per instance the joint
  matrices are pre-multiplied into VIEW space once; each vertex blends
  up to 4 of them (`blend4`, plain weighted matrix sum) and transforms
  ONCE — skinned vertices ignore the node's own world transform (glTF:
  the skin overrides it). Without a sampled pose, skinned meshes draw
  their authored bind pose rigidly. Normals transform through the
  blended matrix without inverse-transpose — exact under
  rotation+translation, approximate under non-uniform scale
  (documented; invisible at cell resolution).
- **Scene::pose** is `Option<&Pose>` from `sample_pose_full`; missing
  indices fall back to rest per-instance rather than panicking.
- **Viewport playback**: `Viewport3D::animate(index, t)` — t comes
  from the app's clock signal and LOOPS over the clip inside the
  widget; play/pause/speed are app-side signal policy (pause = stop
  advancing the time signal), preserving the widget-purity contract
  (`spin` precedent). Unknown index/static model = honest rest pose.
- **Test assets**: every GLB in the sibling repos is STATIC (verified
  by scanning each file's JSON chunk), so proofs are synthetic:
  a node-TRS animated GLB (two-node hierarchy, LINEAR translation +
  STEP rotation) in `load.rs` tests, and a 2-bone BENDING BAR with
  skin + IBM + rotation channel in `three/skin_tests.rs` (hand-checked
  expected positions: tip vertex (0.2,2,0) swings to (-1,1.2,0) at
  t=1). A 65k-tri skinned sphere (every vertex a 2-joint blend) is the
  scale/perf proof.
- **Triangle budget**: `MAX_TRIANGLES` (2M, ~8x the largest real
  asset) enforced from ACCESSOR METADATA before extraction allocates —
  a hostile file can declare counts against buffers it never ships;
  memory stays bounded on the declaration alone.
- **Normals**: `MeshData::compute_smooth_normals` (area-weighted
  accumulation — raw cross products, so face size weights its vote;
  degenerate faces contribute zero and NaN faces are skipped);
  `Model::ensure_smooth_normals` applies it where normals are absent.
  The rasterizer's flat per-face fallback remains the default.
- **Materials**: `emissiveFactor` parsed and ADDED after lighting
  (gouraud folds it into vertex colors; flat paths add after the
  face-intensity multiply so lambert never scales it). `normalTexture`
  presence degrades with a label — no tangent pipeline.
- **`gfx::decode_image`** (`gfx/decode.rs`): one entry for PNG+JPEG,
  routed by MAGIC bytes (containers lie, bytes don't); unknown formats
  reject naming what DOES decode. The GLB texture path now uses it
  (declared-MIME rejections for other formats stay label-based).
- **Perf (release, this machine)**: helmet 15,452 tris textured
  3.11 ms / plain 3.42 ms at 160x96 (pin ≤ 33 ms holds); skinned
  sphere 65k tris: pose sample <0.01 ms, skinned render 8.55 ms vs
  rigid 7.14 ms (skinning delta ≈ +20% on a worst-case all-blended
  mesh); x-wing 120k tris 17.98 ms (report-only). Perf pins:
  `perf_three_animated_160x96` (sample ≤ 10 ms, skinned ≤ 120 ms).

## 4.8 Cycle-7: verification honesty, robustness, perf wave 2

### What is LIVE-verified vs BYTE-verified (protocol honesty)

- **LIVE-verified** (executes against a real implementation in CI):
  the mosaic path end to end (bitmap -> cells -> canvas — pure code,
  the "terminal" is our own compositor); PNG/JPEG decoding against
  byte-exact fixtures generated by real encoders (`cjpeg`/`sips`);
  GLB loading against real exporter output (helmet/machine/x-wing).
- **BYTE-verified ONLY** (no live kitty/iTerm2/sixel terminal in the
  loop): every pixel-protocol emitter. What that means concretely:
  the emitted escapes are checked against the SPECS and against
  REDTEAM's `KittyModel` (an independent parser that replays our
  bytes and tracks what a compliant terminal would hold — ids,
  placements, deletes, tmux unwrapping), and `ImageSession`'s own
  accounting must agree with that model at every step
  (`check_invariants()` + the cross-check test). What it does NOT
  mean: pixels on real Ghostty/WezTerm/Konsole glass. Terminal quirks
  (documented in §2) are mitigated by conservative encoding choices,
  not verified against live builds. Anyone wiring a real terminal
  should run `examples/` and report; until then this boundary is the
  honest one.
- `ImageSession::check_invariants()` (cycle 7): audits the session's
  terminal-state bookkeeping — kitty slots always carry an id,
  cursor-paint slots never do, ids are unique, and upload/delete
  accounting balances (live == transmits − deletes; nothing leaks,
  nothing double-frees). `live_kitty_ids()`/`kitty_traffic()` expose
  the state for cross-checks.

### Geometry robustness (cycle 7)

- RT6-1 CLOSED: `locate()` clamps NaN sample times to the first
  keyframe (`!(t > times[0])` catches NaN in the same branch as the
  before-range clamp); REDTEAM's acceptance test un-ignored.
- NLERP degeneracy: sign-correction makes true antipodal ties
  impossible for unit keys (see `nlerp_quat`'s doc); zero/denormal or
  NaN-poisoned INPUT keys resolve deterministically down a documented
  chain (normalized blend -> normalized left key -> identity) — unit
  quaternion out, always.
- `Camera::framing`/`orbit` are TOTAL: per-axis-finite bounds whose
  SPAN overflows f32 (hostile coordinates — found by rendering every
  load-tolerant mutant, cycle 7) clamp the radius instead of feeding
  inf into `perspective`'s near/far assertion. The mutator campaign
  now RENDERS every mutant that loads (load tolerance without render
  tolerance was half a defense).

### Perf wave 2 (measured, idle box, release)

Vertex-stage wins on the 120k-tri x-wing (vertex-bound: ~6 ms of the
frame is vertex work at 82,787 verts, re-run per instance):

- **Sparse perspective apply**: the projection is always
  `Mat4::perspective`'s shape (debug-asserted), so the per-vertex
  clip transform is 4 mul + 1 madd + 1 reciprocal instead of a full
  `mul_vec4` + guarded `project()` (16 madd + 3 div).
- **Reject-before-shade**: the fast path's beyond-far and off-screen
  bbox rejects now run BEFORE flat shading's per-face cross+sqrt
  (output-identical — rejects never depended on shading).
- Dead `view_z` buffer removed (written, never read).

| bench (160x96 px) | cycle 6 | cycle 7 | delta |
|---|---|---|---|
| x-wing 119,999 tris textured | 17.98 ms | **7.5–11.7 ms** | −35..58% |
| x-wing vertex stage (1x1 fb) | ~9.5 ms | 6.15 ms | −35% |
| helmet 15,452 tris textured | 3.11 ms | 1.31–1.88 ms | — |

Numbers move ±50% with shared-box load (six agents build here);
ranges span a loaded and an idle run, deltas compare matching
conditions. The x-wing budget is PINNED at 60 ms — sized to catch an
order-of-magnitude regression without flaking when six agents build
release trees in parallel on this box (loaded medians reached ~27 ms;
the human regression bar stays the idle median: investigate past
~20 ms idle).

### Perf envelope (app-author budgets; idle box, release, median)

| asset | triangles | framebuffer | ms/frame |
|---|---|---|---|
| synthetic sphere (untextured, gouraud) | 16,128 | 160x96 | 0.76 |
| synthetic sphere (untextured, gouraud) | 16,128 | 320x192 | 0.97 |
| helmet (JPEG textured + mips) | 15,452 | 160x96 | 1.31 |
| helmet (JPEG textured + mips) | 15,452 | 320x192 | 1.82 |
| helmet (untextured) | 15,452 | 160x96 | 1.18 |
| helmet (untextured) | 15,452 | 320x192 | 1.59 |
| x-wing (PNG textured + mips) | 119,999 | 160x96 | 7.53 |
| x-wing (PNG textured + mips) | 119,999 | 320x192 | 8.21 |
| skinned sphere (animated, all-blended) | 65,024 | 160x96 | 2.90 |
| skinned sphere (animated, all-blended) | 65,024 | 320x192 | 3.26 |

Reading the table: the pipeline is VERTEX-bound at these sizes —
quadrupling the pixel count (160x96 → 320x192) costs +9..+39%, while
7.4x triangles (helmet → x-wing) costs ~5.7x. Budget rule of thumb:
≤20k tris is sub-2 ms anywhere; 120k tris fits a 30 fps budget with
3-4x headroom on one core. Reproduce:
`cargo test --release -- --ignored perf_three_envelope --nocapture`.

Rejected candidate: whole-instance bounding-sphere frustum culling —
every real asset here is one instance filling the frame; zero win, a
new code path to attack. Filed for a multi-instance future.

### Texture mips (cycle 7 — shipped)

`Bitmap::box_halved()`/`mip_chain()` build a box-filtered chain at
LOAD (`MaterialData::mips`, ~1/3 extra memory; build time reported in
`LoadStats::mip_build`). The renderer picks a level PER TRIANGLE from
the texels-per-pixel ratio (UV area in level-0 texels / screen area;
level = floor(log2(tpp)/2), magnification always level 0). Near-clip
(slow-path) triangles keep level 0 — they graze the camera where
level 0 is correct. Deliberate golden change: minified checkerboards
now read as their local MEAN instead of aliased extremes (that is the
shimmer fix — pinned by `mips_average_minified_checkerboards`, which
requires the mip render's min/max spread to collapse vs the raw one).
Cost on the helmet at 160x96: textured 1.88 ms vs plain 1.75 ms
(≈+7% — the level pick is one log2 per textured triangle; sampling
small mips is cache-friendlier than level 0).

### Mosaic fit cost (200x60 cells, photo-noise worst case, release)

Measured by `perf_mosaic_200x60` (seeded noise = no uniform-cell
early-outs; real images are cheaper, and damage-driven repaints
touch a fraction of cells):

| mode | full-viewport fit |
|---|---|
| HalfBlock | 46–56 µs |
| Quadrant | ~0.97 ms |
| Sextant | 3.6–3.9 ms |
| Braille | 0.32–0.76 ms |

The fit SCORER was rewritten this cycle: for a fixed partition the
optimal side colors are the weighted means, so minimizing the
residual is maximizing `Σ (Σwc)²/Σw` per side — two divisions per
pattern instead of six, and the w·c² moment drops entirely. The
comparison now uses exact f64 sums (all integers < 2^40, exact)
instead of u8-quantized means: on near-ties a different-but-
equivalent glyph could win vs cycle 6 — every mosaic golden was
re-run and ZERO flips occurred. Wall-clock at 200x60 stayed flat
(the set-bit gather dominates, not the divisions); the rewrite is
kept for the simpler math and the removed moment array. Pattern LUTs
are NOT adopted: sextant's 3.6 ms is the absolute worst case (full
viewport of noise, every cell repainted) and damage-driven frames
touch a fraction of that; revisit only if a real app profile
disagrees.

## 4.9 Cycle-8: API ergonomics + freeze notes

- **Conveniences**: `three::quick_view(path)` / `quick_view_bytes` →
  `QuickView { model, camera, light, stats }` with `.scene()` — the
  5-line 3D hello (load + framed camera + default light; hides
  boilerplate, not behavior). `gfx::render_to_cells(bitmap, rect,
  &Capabilities)` — one-call picture-to-cells over `MosaicMode::auto`
  + `MosaicRenderer` (per-call scratch; hold a renderer or an
  `ImageSession` for animation).
- **Doctests** (compile + run in CI) on the eight most-used entries:
  `decode_image`, `Bitmap`, `MosaicRenderer`/`MosaicMode::auto`,
  `Model::load` (+ animation sampling), `three::render` (scene/camera
  hello), `ImageSession` (sink + sync + invariants),
  `render_to_cells`, `three::quick` module. `cargo doc --no-deps`
  is warning-free in gfx/three.
- **Error style (frozen)**: every rejection is `Error::Parse` with a
  lowercase format prefix — `glb:`, `gltf:`, `json:`, `png:`,
  `jpeg:`, `base64:`, `image:` — then the offending index/field and
  the rule ("accessor 3: span exceeds bufferView byteLength").
  Surveyed this cycle: zero unprefixed rejections in gfx/three.
- **Pruned**: the `read_vec3_f32_pub` crate-internal wrapper
  (animation now shares extract's reader directly). Kept-public by
  intent (young API, real app uses): `Bitmap::premultiply`,
  `sniff_format`, session accounting (`live_kitty_ids`/
  `kitty_traffic`/`check_invariants`), `grid_model`/`grid_lines`,
  `ensure_smooth_normals`, `load_glb_with_stats`.

### Provenance (for the docs cycle)

Every decoder/encoder in `gfx` and `three` is HAND-ROLLED from public
specifications; none is a port or derivation of existing decoder code
(GPL or otherwise). Sources used:

- PNG: RFC 2083 / the W3C PNG specification (chunk layout, filter
  types, Paeth predictor); inflate via the `miniz_oxide` crate
  (MIT/Apache-2.0 dual license — the crate's own licensing).
- JPEG: ITU-T T.81 (baseline sequential DCT, Huffman coding, restart
  markers); IDCT is the textbook separable float transform.
- glTF/GLB: the Khronos glTF 2.0 specification (container layout,
  accessor/bufferView rules, node/TRS semantics, animation and skin
  chapters). No Khronos sample code was used.
- Sixel: DEC VT330/VT340 programmer references + the de-facto
  register conventions documented by modern implementations.
- Kitty graphics: the kitty terminal's published protocol document.
- iTerm2 images: the iTerm2 OSC 1337 File= documentation.
- Unicode mosaics: Unicode block-elements / Symbols for Legacy
  Computing code charts (glyph allocation rule verified in tests);
  the ERROR-MINIMIZATION IDEA follows the approach chafa and
  notcurses describe publicly, implemented independently (see §2
  research record for citations).
- Base64: RFC 4648.

### Honest limits (docs cycle: publish these verbatim)

- JPEG: BASELINE sequential only — progressive and arithmetic-coded
  JPEGs reject by name (no silent fallback).
- PNG: 8-bit depths, no interlace (Adam7 rejects by name), no 16-bit.
- Sixel: ONE palette per emission — the last-emitted image owns the
  shared color registers; multiple live sixel images recolor each
  other (documented v1 limit; prefer one per screen).
- iTerm2/sixel have NO placement model: any move/resize re-emits the
  full payload; only kitty gets cheap placement escapes and true
  deletes (`ImageSession` reconciles all three honestly).
- Pixel protocols are BYTE-VERIFIED, not live-terminal-verified (see
  §4.8) — mosaic is the universally-correct path.
- Animation: LINEAR + STEP interpolation; CUBICSPLINE channels skip
  with a label; morph-target `weights` channels skip with a label;
  rotation interpolation is nlerp (shortest path), not slerp.
- Skinning: 4 joints per vertex (JOINTS_0/WEIGHTS_0 only — no second
  set), matrices blended linearly (no dual quaternions); normals skip
  the inverse-transpose (visible only under non-uniform scale).
- Textures: baseColorTexture only; normal/metallic-roughness/
  occlusion/emissive TEXTURES are ignored with a label
  (emissiveFactor — the scalar — IS applied). Sampler wrap modes
  default to REPEAT (per-sampler modes not read).
- Mip LOD is per-triangle, not per-pixel: extreme depth-spanning
  triangles pick one level.
- Mosaic color fidelity: the 2-color-per-cell fit is the terminal
  cell model's ceiling; braille is luminance-structured, not color-
  accurate (by design — see the mode docs).
- Rasterizer: near-plane + guard-band clipping, top-left fill rule,
  perspective-correct depth/UV; COLOR interpolation is screen-linear
  (not perspective-correct — invisible at cell scale, documented).

## 5. Open follow-ups (cycle 7+)

1. Kitty animation frames (`a=f`/`a=a`) for the boot splash's hi-fi
   path; unicode placeholders for tmux passthrough.
2. Sixel register partitioning via `register_base` when the pipeline
   hosts multiple live sixel images.
3. PNG: 16-bit + sub-8-bit palette depths, Adam7 (only if a real asset
   demands it).
4. Mosaic densification: octants (Unicode 16) once font coverage
   justifies it; measure before adopting a 4x2 coverage fit.
5. Normal matrices (inverse-transpose) for non-uniformly scaled nodes;
   mesh-data dedup for multi-instanced meshes; per-sampler wrap modes.
6. Widget protocol path: post-present overlay pass (with REACT/RENDER
   — ask filed in reviews/cycle3/gfx3d-requests.md).
7. Mip chain + per-triangle level selection for distant-texture
   shimmer (deferred this cycle for animation priority); CUBICSPLINE
   real evaluation if an asset ever needs it; morph targets.
8. Normal mapping (tangent pipeline) — skipped as non-trivial per the
   cycle-6 priority call.
