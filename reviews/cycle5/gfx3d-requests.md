# GFX3D cycle-5 requests + notices

## Baseline JPEG decoder shipped (`gfx::jpeg` + entropy/dsp helpers)

- **Decodes**: SOF0/SOF1 (baseline + extended-sequential Huffman,
  8-bit), YCbCr + grayscale, sampling factors 1..=2 per axis (4:4:4,
  4:2:0, 4:2:2, 4:4:0 through one general MCU walk), DRI/RSTn restart
  markers, stuffed bytes, APPn/COM skipped.
- **Rejects by name**: progressive (SOF2), lossless, differential/
  hierarchical, arithmetic coding (SOF9+/DAC), 12-bit precision,
  16-bit quant tables, 4-component (CMYK), sampling factors > 2,
  multi-scan sequential.
- **IDCT**: naive separable floating-point, transcribed from T.81
  A.3.3 — correctness over speed (one-time decode; measured below). A
  transposition bug in the first draft was caught by the
  asymmetric-coefficient test that pins the separable form against
  the direct 4-loop definition — that test is the reason to keep the
  naive reference around forever.
- Chroma upsampling is NEAREST (labeled in the module docs) —
  invisible at terminal cell scale; a smooth upsampler is a measured
  later decision.
- Fixtures: real cjpeg output embedded with regeneration commands in
  `gfx/jpeg_fixtures.rs` (mozjpeg `cjpeg -sample 1x1/2x2/2x1`,
  `-grayscale`, `-restart 1`, `-progressive`), decoded against the
  generator formula within JPEG-honest tolerances; plus a full
  truncation ladder and 600-case deterministic marker-soup fuzz.

**Helmet consequence**: the flagship asset now renders TEXTURED — the
`#FALLBACK image/jpeg` label is gone; `e2e_helmet_renders_textured`
pins that the textured render differs from a baseColorFactor-only
render of identical geometry. Undeclared-MIME images sniff PNG/JPEG
magic instead of guessing.

## R4-1 epilogue (DESIGN, integrator)

DESIGN's adapter builds `BrandmarkParams` from identity constants
(`boot::brandmark3d::identity_params`) — so the compat constructor is
now DELETED as promised: `BrandmarkRenderer::with_params` is the one
constructor; `BrandmarkParams::reference()` shrank to `#[cfg(test)]`
(still drift-pinned against `boot::identity` by `identity_drift_pin`).
Production three -> boot imports: zero.

## ImageSession adoption review (REACT, RT4-1) — verdict: CORRECT

Read `driver.rs::render_images` + `overlays.rs::ImageEntry` as landed:
`version` bumped ONLY by `set_bitmap` (moves keep it — kitty re-places
by id without retransmission), `release(key)` on overlay removal (the
upload leak I flagged in cycle 4 is closed), one `ImageSession` per
terminal session, sink adapter through the post-present bracket.
Exactly the designed lifecycle; no corrections to file. Your
acceptance tests were mid-edit while I reviewed — if any of them wants
a byte-level assert on the `a=p`-not-`a=T` move path, the session's
own `kitty_lifecycle_transmit_place_delete` test shows the shape.

## Viewer support for DESIGN's examples/viewer3d.rs

`Viewport3D` additions (all tested): `DEFAULT_ORBIT` (the reset-camera
target; `new()` starts exactly there, pinned), `.light_angles(azimuth,
elevation)` (spherical key-light control; `three::Light::from_angles`
under it), `.fog(strength)` (depth fog toward the widget background —
needs an opaque `.background(token)`; `Framebuffer::depth_fog` is now
a public post-process shared with the brandmark), mode switch and
fit-to-bounds framing already existed (`.mode()`, framing per draw at
zoom 1.0). The widget now holds a `SceneRenderer` across draws (the
cycle-4 scratch reuse) — a spinning viewer allocates nothing per
frame.

## To the integrator

No new dependencies (the JPEG decoder is hand-rolled per the ruling;
miniz_oxide untouched by it). New gfx modules declared in gfx/mod.rs:
`jpeg` (public), `jpeg_entropy`/`jpeg_dsp` (private), `jpeg_fixtures`
(test-only).
