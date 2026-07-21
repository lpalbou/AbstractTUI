# GFX3D — docs handoff (cycle 9)

Doc-ready prose for the images + 3D chapters. Everything here is
verified against the shipped code; the deep design record stays in
`docs/design/gfx-three.md` (§4.9 carries provenance + honest limits;
§2 carries research citations). Lift freely.

---

## 1. The image pipeline story

**Bytes to picture**: `gfx::decode_image(bytes)` sniffs the MAGIC
(containers lie, bytes don't) and decodes PNG or baseline JPEG into a
`gfx::Bitmap` — an owned RGBA8 image with `get`/`set`, nearest +
bilinear resize, cropping, and a box-filter mip chain
(`box_halved()`/`mip_chain()`). Unknown formats reject BY NAME,
telling the user what does decode; truncated or hostile bytes are
named errors, never panics (fuzz suites pin this).

**Picture to terminal** — the capability ladder, best first:

| channel | how it draws | moves/resizes | removal | requires |
|---|---|---|---|---|
| kitty graphics | upload once by id, place by escape | cheap re-place (`a=p`), no retransmit | true delete (`a=d,d=I`) | kitty protocol support |
| iTerm2 | full base64-PNG re-emit at the cursor | full re-emit | cells overdraw the corpse | OSC 1337 support |
| sixel | paletted raster at the cursor | full re-emit | cells overdraw | DA1 attr 4; one shared palette |
| unicode mosaic | colored glyphs (cells) | free (it IS cells) | free | any terminal |

Three entry points, smallest first:

- `gfx::render_to_cells(bitmap, rect, &caps)` — one call, picks the
  best MOSAIC mode for the probed terminal (`MosaicMode::auto`,
  labeled reasons), returns ready-to-blit `CellPatch`es.
- `widgets::Image` — the widget: `from_path`/`from_bitmap`
  (`Arc<Bitmap>`, re-exported beside the widget), `fit`
  (contain/cover/fill/none), mosaic mode override. ALWAYS renders
  mosaic — a draw closure owns cells, not escape bytes (damage
  contract §6).
- `gfx::ImageSession` — pixel protocols with a lifecycle: slots keyed
  by the caller, content versions, minimal traffic per channel
  (kitty: transmit once, re-place on move, delete on drop; iTerm2/
  sixel: honest full re-emits). `check_invariants()` audits the
  accounting; REDTEAM's KittyModel cross-checks it byte-for-byte.
  Bytes flow through `ExternalSink` (the presenter adapts) — tmux
  passthrough wraps automatically when caps say so.

**Mosaic modes** (2-color-per-cell fit, weighted least squares):
HalfBlock 1x2 (exact, universal), Quadrant 2x2 (universal glyphs),
Sextant 2x3 (denser; U+1FB00 needs a recent font — opt-in), Braille
2x4 (luminance structure, monochrome-friendly). `MosaicMode::auto`
picks by `unicode_ok` + color depth and says why. Optional
Floyd–Steinberg dithering; sixel dither strength is configurable.

## 2. The 3D story

**Five-line hello** (compiles as a doctest in `three::quick`):

```rust
let view = three::quick_view("model.glb")?;      // load + framed camera + light
let mut fb = Framebuffer::new(160, 96);
SceneRenderer::new().render(&view.scene(), &mut fb);
// fb -> mosaic cells via MosaicRenderer, or use Viewport3D below.
```

`QuickView` exposes `model`/`camera`/`light`/`stats` (stats = decode
cost — show "loading": ~100 ms for the helmet's 2048² JPEG).

**GLB subset** (the honest matrix lives in gfx-three.md; summary):
binary GLB containers; TRIANGLES primitives; positions/normals/uvs/
vertex colors; u8/u16/u32 indices (or non-indexed); node TRS +
matrix hierarchies, multiple scenes (default scene wins);
baseColorFactor + baseColorTexture (PNG + baseline JPEG, embedded);
emissiveFactor; smooth-normal generation on request; a 2M-triangle
budget enforced from metadata. Rejected BY NAME: sparse accessors,
Draco/meshopt compression, non-triangle modes, CUBICSPLINE (channel
skip + label), out-of-range anything. Labeled degradations: external
URIs, unsupported texture maps (normal/MR/occlusion), morph weights.

**Animation**: `Model::animations()` lists clips;
`sample_pose_full(clip, t, &mut Pose)` produces per-instance worlds +
skin joint matrices — pure in `t`, clamped to the clip (loop with
`t % duration()`), zero steady-state allocation. LINEAR + STEP
interpolation; rotations nlerp (shortest path). Skinning: 4 joints
per vertex, linear blend, sanitized at load (out-of-range weighted
joints reject; drifted weight sums renormalize with one label). An
animated+skinned asset ships in-repo:
`src/three/fixtures/animated_bar.glb` (2-bone bending bar; the
doctest and skin tests play it).

**Viewport3D** (the widget): `Viewport3D::new(Arc<Model>)` +
`.orbit(yaw, pitch, zoom)` + `.mode(mosaic_mode)` +
`.animate(clip, t)` (loops; static models draw rest) +
`.light_angles`/`.fog`/`.background` + `.on_orbit`/`.on_zoom`
(drag/wheel deltas — camera STATE lives app-side in signals; the
widget is pure over its props). `element(&tokens)` — no `Scope`
(holds no reactive state; RT8-3 split documented in the module doc).

**Culling defaults differ by entry (deliberate)**: bare `Scene::new`
culls back faces (procedural meshes are wound); `Viewport3D` and
`QuickView::scene()` render double-sided (real exports are not).
Documented on `Scene::double_sided`.

## 3. Honest limits (final list — publish verbatim)

Lifted from gfx-three.md §4.9, current as of cycle 9:

1. JPEG: BASELINE sequential only; progressive/arithmetic reject by
   name. Scan component selectors are validated against SOF ids;
   reordered scans reject by name (cycle-9 RT5-2 closure).
2. PNG: 8-bit depths, no interlace (Adam7 rejects by name).
3. Sixel: one palette per emission — multiple live sixel images
   recolor each other; prefer one per screen.
4. iTerm2/sixel have no placement model: moves re-emit the payload;
   only kitty gets placement escapes + true deletes.
5. Pixel protocols are BYTE-verified (spec + KittyModel cross-check),
   not live-terminal-verified; mosaic is the universal path.
6. Animation: LINEAR/STEP only; CUBICSPLINE + morph weights skip with
   labels; nlerp not slerp.
7. Skinning: JOINTS_0/WEIGHTS_0 only (4 joints/vertex), linear blend,
   no inverse-transpose normals (non-uniform scale approximation).
8. Textures: baseColorTexture only; other maps labeled + ignored;
   wrap is REPEAT (per-sampler modes not read). Mip LOD is
   per-triangle, not per-pixel.
9. Mosaic: 2-color-per-cell ceiling; braille is structure, not color.
10. Rasterizer: near + guard-band clipping, top-left fill,
    perspective-correct depth/UV; COLOR interpolation is
    screen-linear (invisible at cell scale).
11. Perf numbers are load-sensitive: cite the idle-box envelope;
    medians inflate several-fold under host contention.

## 4. Perf envelope (idle box, release, median ms/frame)

| asset | triangles | 160x96 | 320x192 |
|---|---|---|---|
| synthetic sphere (untextured, gouraud) | 16,128 | 0.76 | 0.97 |
| helmet (JPEG textured + mips) | 15,452 | 1.31 | 1.82 |
| helmet (untextured) | 15,452 | 1.18 | 1.59 |
| x-wing (PNG textured + mips) | 119,999 | 7.53 | 8.21 |
| skinned sphere (animated, all-blended) | 65,024 | 2.90 | 3.26 |

Vertex-bound at cell scale: 4x the pixels costs +9..+39%, 7.4x the
triangles ~5.7x. Rule of thumb: ≤20k tris renders sub-2 ms anywhere;
120k fits 30 fps with 3-4x headroom on one core. Mosaic conversion
adds (200x60 cells, worst case): half-block ~50 µs, quadrant ~1 ms,
braille ~0.5 ms, sextant ~3.7 ms. Reproduce:
`cargo test --release -- --ignored perf_three_envelope --nocapture`
and `... perf_mosaic_200x60 --nocapture`.

## 5. Cycle-9 closures for the acceptance record

- RT5-2 CLOSED (was Known Limit): SOS selectors validated against
  SOF ids — undeclared selectors and reordered scans reject by name
  (a reordered scan would have decoded silently wrong before).
- RT6-2 CLOSED (was Known Limit): `src/three/fixtures/animated_bar.glb`
  is a real on-disk animated+skinned asset; the full
  load→rig→pose path plays it from `include_bytes!` in tests and in
  the `Model::load` doctest.
- RT8-4 CLOSED: `Bitmap` re-exported beside `Image` (and already in
  the prelude); `from_bitmap` doc shows the `Arc`.
- RT8-3 DOCUMENTED: the `element(&TokenSet)`-without-`Scope` split is
  stated in both image + viewport3d module docs (stateless widgets).
- RT8-8 (my share) CLOSED: no ```ignore doctests remain in gfx/three/
  image/viewport3d — all compile as `no_run` or run.
- QuickView/Scene culling asymmetry DOCUMENTED on `Scene::double_sided`.
- `Model::load` doctest no longer teaches `testing::` (embedded
  fixture bytes).
