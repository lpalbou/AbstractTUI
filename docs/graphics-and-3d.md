# Graphics and 3D

AbstractTUI renders real pixels in the terminal: PNG/JPEG images through
the best channel the terminal offers, and software-rasterized 3D models
(GLB) with textures, lighting, and animation — all with hand-rolled
decoders and no GPU requirement. Degradation is always labeled, never
silent: when the engine falls back to a lesser channel, the result says so.

## Images end-to-end

### Bytes to picture

`gfx::decode_image(bytes)` sniffs the magic bytes (containers lie, bytes
don't) and decodes **PNG** or **baseline JPEG** into a `gfx::Bitmap` — an
owned RGBA8 image with pixel `get`/`set`, nearest and bilinear resize,
cropping, and a box-filter mip chain. Unknown formats reject by name,
telling the caller what does decode ("PNG and baseline JPEG decode,
GIF/WebP/AVIF/TIFF do not" — a message you can show verbatim). Truncated
or hostile bytes produce named errors, never panics; the decoders are
fuzz-hardened.

### Picture to terminal: the capability ladder

The engine picks the best channel the terminal proves it supports —
`gfx::choose_channel(&caps)` — best first:

| channel | how it draws | moves / resizes | removal | requires |
| --- | --- | --- | --- | --- |
| kitty graphics | upload once by id, place by escape | cheap re-place, no retransmit | true delete | kitty graphics protocol |
| iTerm2 | full base64-PNG re-emit at the cursor | full re-emit | cells overdraw | OSC 1337 support |
| sixel | paletted raster at the cursor | full re-emit | cells overdraw | sixel + known cell pixel geometry; one shared palette |
| unicode mosaic | colored glyphs (it *is* cells) | free | free | any terminal |

Capabilities come from detection, not folklore: an instant environment
pass, then an active query probe that can raise *and* lower the answer.
Run any of the `dashboard`, `viewer3d`, or `images` examples with `--caps`
to print the report for your terminal.

### Three entry points, smallest first

- **`gfx::render_to_cells(bitmap, rect, &caps)`** — one call. Picks the
  best mosaic mode for the probed terminal and returns ready-to-blit
  `CellPatch`es.
- **`widgets::Image`** — the widget: `Image::from_path("logo.png")` or
  `Image::from_bitmap(Arc<Bitmap>)` (the `Bitmap` type is re-exported
  beside the widget and in the prelude), with `fit`
  (`Contain`/`Cover`/`Fill`/`None`), alignment, and a mosaic-mode
  override. The widget **always renders mosaic cells**: a widget draw
  closure owns cells, not escape bytes, so pixel-protocol placement lives
  one level up.
- **`gfx::ImageSession`** — pixel protocols with a lifecycle. Slots are
  keyed by the caller (`SlotKey`), content changes are declared by version
  bump, and each sync emits the minimum traffic the channel allows: kitty
  transmits once, re-places on move, and deletes on drop; iTerm2 and sixel
  honestly re-emit their full payload on any change. Bytes flow through an
  `ExternalSink` (the presenter adapts), and tmux passthrough wrapping is
  applied automatically when capabilities call for it. `SyncOutcome` tells
  you whether cells need repainting, bytes were written, or nothing
  changed.

`gfx::present_image` / `ImageRenderer` sit under all three: capability
ladder on top, `RenderConfig` for the knobs (kitty wire format, placement
z-index, sixel register budget and dithering).

### Mosaic modes

Mosaic renders pixels as colored glyphs with a two-colors-per-cell best
fit (weighted least squares):

- **HalfBlock** — 1×2 pixels per cell using `▀`. Exact and universal.
- **Quadrant** — 2×2, the 16-glyph quadrant set. Universal glyph coverage.
- **Sextant** — 2×3, the 64-pattern sextant set. Denser, but its U+1FB00
  glyphs need a recent font — explicit opt-in, since no font probe exists
  and missing glyphs render as tofu.
- **Braille** — 2×4, dots by luminance threshold. Structure rather than
  color; the strongest choice on monochrome-class terminals.

`MosaicMode::auto(&caps)` picks for you and returns the reason as a label:
non-UTF-8 locales get HalfBlock (U+2580 survives most legacy codepages),
monochrome terminals get Braille, color terminals get Quadrant.

Optional **Floyd–Steinberg dithering** (serpentine error diffusion) can
pre-quantize the source to a palette before cell fitting — worth it when
the *output* terminal is 256- or 16-color, where straight quantization
would band gradients. Sixel emission has its own configurable dithering.

### Images under tmux

Inside tmux, graphics protocols are off by default because tmux swallows
them unless the user set `allow-passthrough on` — a setting invisible from
the environment. The engine verifies passthrough per session with a
wrapped round-trip probe and only then enables the kitty/iTerm2 paths,
wrapping every payload automatically. Known cosmetic limit: tmux cannot
reflow passthrough images across scrolling or pane splits. Mosaic works
everywhere regardless.

## 3D end-to-end

### The five-line hello

```rust
use abstracttui::three::{self, Framebuffer, SceneRenderer};

let view = three::quick_view("model.glb")?;      // load + framed camera + light
let mut fb = Framebuffer::new(160, 96);
SceneRenderer::new().render(&view.scene(), &mut fb);
// fb -> mosaic cells via MosaicRenderer, or use Viewport3D below.
```

`quick_view` (and `quick_view_bytes` for in-memory GLB data) returns a
`QuickView` with public `model`, `camera`, `light`, and `stats` fields —
adjust the camera and light freely between frames, then call `.scene()`.
`stats` reports decode cost (texture decode dominates on textured models;
a 2048² JPEG-textured asset loads in the ~100 ms class — show a loading
state around it). `look_from(yaw, pitch)` re-frames the camera on the
model's bounds: the "reset camera" a viewer needs.

### What loads (the GLB subset)

Binary GLB containers with embedded buffers; TRIANGLES primitives;
positions, normals, UVs, and vertex colors; u8/u16/u32 indices or
non-indexed geometry; node TRS and matrix hierarchies; multiple scenes
(the default scene wins); `baseColorFactor` and `baseColorTexture`
(embedded PNG or baseline JPEG); `emissiveFactor`; smooth-normal
generation on request; and a 2-million-triangle budget enforced from
metadata before decode.

Rejected **by name**: sparse accessors, Draco/meshopt compression,
non-triangle primitive modes, and out-of-range anything. Labeled
degradations (the model loads, with a warning): external URIs, unsupported
texture maps (normal/metallic-roughness/occlusion), morph weights, and
CUBICSPLINE animation channels (skipped).

### Scene, camera, light

`Scene<'_>` borrows a `Model` and carries a `Camera` (orbit-style: target,
yaw, pitch, distance, vertical FOV, near/far), a `Light` (directional:
direction vector or spherical `from_angles`, ambient + diffuse terms), a
background color, and a `double_sided` flag.

Culling defaults differ by entry, deliberately: bare `Scene::new` culls
back faces (procedural meshes are consistently wound), while `Viewport3D`
and `QuickView::scene()` render double-sided (real-world GLB exports are
not, and holes read as bugs). Flip `double_sided` explicitly when the
other trade-off fits.

### The Viewport3D widget

```rust
let vp = Viewport3D::new(Arc::new(model))
    .orbit(yaw, pitch, zoom)      // plain floats each build; signals live app-side
    .mode(MosaicMode::HalfBlock)
    .animate(0, t)                // play clip 0 at time t (loops; static = rest pose)
    .light_angles(azimuth, elevation)
    .fog(0.15)
    .on_orbit(move |dyaw, dpitch| { /* write yaw/pitch signals */ })
    .on_zoom(move |steps| { /* write zoom signal */ })
    .element(&tokens);
```

The widget is pure over its props: same props, same pixels. Left-drag
orbits (the pointer is captured for the drag, so fast drags keep steering
outside the rect), the wheel zooms — but the widget only *reports* deltas
through `on_orbit`/`on_zoom`; the app owns camera state and clamping.
`element(&TokenSet)` takes no scope because the widget holds no reactive
state. Buffers persist inside the draw closure, so a steady-state repaint
allocates nothing. `light`, `background`, `spin` (caller-driven
auto-rotation), and `cull_backfaces` round out the builder.

For a complete interactive viewer, run
`cargo run --example viewer3d -- model.glb`.

### Animation playback

`Model::animations()` lists the clips; `sample_pose_full(clip, t, &mut
Pose)` produces per-instance world matrices plus per-skin joint matrices —
pure in `t`, clamped to the clip's keyframe range (loop with
`t % clip.duration()`), and allocation-free in steady state (the `Pose`
scratch is reused across frames).

Supported interpolation: **LINEAR** and **STEP**; rotations use
shortest-path nlerp. **Skinning**: up to 4 joints per vertex
(`JOINTS_0`/`WEIGHTS_0`), linear blend, sanitized at load — out-of-range
weighted joints reject, drifted weight sums renormalize with a label. An
animated, skinned test asset ships in the repository
(`src/three/fixtures/animated_bar.glb`).

### Textures and mip-mapping

Base-color textures decode through the same image pipeline (embedded PNG /
baseline JPEG) and build a box-filter mip chain. The rasterizer picks a
mip level **per triangle** from the texels-per-pixel ratio, with bilinear
sampling within the level; wrap mode is REPEAT.

## The boot splash

An optional two-second identity animation for app startup, played before
your first frame. `boot::should_splash(&caps)` is the production gate: it
returns the reason to skip when the render handle is not a tty, when
`ABSTRACTTUI_NO_SPLASH` is set (any value except `0`, so wrapper scripts
can force-enable), when `NO_COLOR` is set, when `TERM=dumb`, or when the
capability report itself says the terminal is dumb. Respect the reason —
it is ready-made for a log line.

The sequence runs 2.0 s in four beats: **arrival** (three planes fly in,
staggered, on an ease-out curve), **alignment** (at 0.9 s the planes lock
into the mark and a 12-spark burst fires), **reveal** (at 1.4 s the
wordmark tracks open from 4 cells of letter-spacing to 1), **hold**
(settle, then done). Any key skips with a fast 120 ms fade, and a hard
2.5 s wall cutoff bounds the whole thing.

Two render paths read the same identity constants: a 3D path (the mark
rendered by the `three` rasterizer, chosen on truecolor terminals) and a
pure-cell 2D path with its own particle field (everywhere else). Try both:
`cargo run --example splash` (`--3d` / `--2d` to force one).

## Honest limits

- **JPEG**: baseline sequential only; progressive and arithmetic coding
  reject by name. Scan component selectors are validated against the
  frame header; malformed scans reject rather than decode wrong.
- **PNG**: 8-bit depths, no interlacing (Adam7 rejects by name).
- **Sixel**: one palette per emission — multiple live sixel images recolor
  each other. Prefer one sixel image per screen.
- **iTerm2/sixel** have no placement model: any move or resize re-emits
  the full payload; only kitty gets placement escapes and true deletes.
- **Pixel protocols** are verified byte-for-byte against the protocol
  specifications and a protocol state model, not against every live
  terminal emulator; mosaic is the universal, always-correct path.
- **Animation**: LINEAR/STEP only; CUBICSPLINE channels and morph weights
  skip with labels; rotation interpolation is nlerp, not slerp.
- **Skinning**: `JOINTS_0`/`WEIGHTS_0` only (4 joints per vertex), linear
  blend, no inverse-transpose normal handling (an approximation under
  non-uniform scale).
- **Textures**: base color only; other maps are labeled and ignored; wrap
  is REPEAT (per-sampler modes are not read). Mip LOD is per-triangle,
  not per-pixel.
- **Mosaic**: two colors per cell, by construction; braille carries
  structure, not color.
- **Rasterizer**: near-plane and guard-band clipping, top-left fill rule,
  perspective-correct depth and UVs; vertex-color interpolation is
  screen-linear (invisible at cell scale).
- **Performance numbers are load-sensitive**: the envelope below is from
  an idle machine; medians inflate several-fold under host contention.

## Performance envelope

Measured medians, release build, on a quiet machine (ms/frame):

| asset | triangles | 160×96 | 320×192 |
| --- | --- | --- | --- |
| synthetic sphere (untextured, gouraud) | 16,128 | 0.76 | 0.97 |
| helmet (JPEG textured + mips) | 15,452 | 1.31 | 1.82 |
| helmet (untextured) | 15,452 | 1.18 | 1.59 |
| x-wing (PNG textured + mips) | 119,999 | 7.53 | 8.21 |
| skinned sphere (animated, all vertices blended) | 65,024 | 2.90 | 3.26 |

The renderer is vertex-bound at cell scale: 4× the pixels costs +9–39%,
while 7.4× the triangles costs ~5.7×. Rule of thumb: assets up to ~20k
triangles render in well under 2 ms anywhere; a 120k-triangle asset fits a
30 fps budget with 3–4× headroom on one core.

Mosaic conversion adds, for a 200×60-cell target (worst case): half-block
~50 µs, braille ~0.5 ms, quadrant ~1 ms, sextant ~3.7 ms.

Reproduce on your machine:

```bash
cargo test --release -- --ignored perf_three_envelope --nocapture
cargo test --release -- --ignored perf_mosaic_200x60 --nocapture
```
