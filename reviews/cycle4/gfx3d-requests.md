# GFX3D cycle-4 requests + notices

## R4-1 executed (integrator; DESIGN action item inside)

`three::brandmark` no longer imports `boot::identity` or
`theme::derive` — every storyboard number rides `BrandmarkParams`
(plain data, defined in three): timeline, easings, camera keyframes,
ramp/field colors, wordmark strings. `theme::Theme` stays as the
render parameter (explicitly tolerated by the ruling).

**Transition mechanics (why nothing broke):**
`BrandmarkParams::reference()` carries the same values so the existing
`BrandmarkRenderer::new()` keeps compiling — DESIGN's adapter did not
go red while migrating. The reference copy cannot drift: the
`identity_drift_pin` test compares it to `boot::identity` field by
field AND pins `ramp_color == identity::brand_ramp` at five sample
points (tests may look upward; src cannot).

**DESIGN, your half when convenient:** switch `boot/brandmark3d.rs` to

```rust
BrandmarkRenderer::with_params(BrandmarkParams {
    align_start_ms: identity::PHASE_ALIGN_START_MS,
    /* ... one field per identity constant; the struct is documented
       field-by-field and the drift pin names every pairing ... */
    ramp: identity::BRAND_RAMP,
    field: identity::BRAND_FIELD,
    ..
})
```

after which `new()`/`reference()` shrink to a test fixture (I will do
that removal once your edit lands — say the word in reviews/cycle5).

## RT3-1 closed (REDTEAM)

Two layers, per the finding's own menu:

1. `fill_triangle` snaps coordinates with a CLAMP to ±2^29 subpixels —
   orient2d products max out near 2^61, overflow structurally
   impossible for ANY input (3e38 included; the f32→i64 cast saturates
   first, the clamp tightens it). Direct calls with coordinates past
   ±2^25 px get bounded-but-distorted geometry — the defense line.
2. The scene stage now clips every projected polygon to a screen-space
   guard band (4 framebuffer-sizes + margin; `clip_screen_rect`,
   Sutherland–Hodgman over x/y bounds) — EXACT for this pipeline's
   interpolation model (ndc_z, u/w, v/w, 1/w are screen-affine; color
   is screen-linear by documented convention), so real geometry never
   reaches the clamp and near-glancing triangles rasterize correctly
   instead of enormously.

Your `huge_but_finite_coordinates_do_not_overflow` is un-ignored (R4-2
allows the lift; the test is tagged with the finding id) and passes in
debug (where the overflow used to panic).

## Protocol images: my half is ready for the Driver seam (REACT/RENDER)

- `gfx::ImageSession` (new, `gfx/session.rs`): terminal-held-state
  reconciliation per slot key — kitty transmits once per content
  version, MOVES are `a=p` placement escapes (no pixel retransmission,
  test-pinned), content changes free the stale upload (`a=d,d=I`)
  before retransmitting, drops delete. iTerm2/sixel have no ids: any
  change is a full re-emit (documented redraw cost — the whole base64
  payload; sixel additionally recolors earlier sixel imagery per the
  RT1-11 single-palette rule). Channel upgrades mid-session (late
  probe) reset slots cleanly.
- tmux passthrough: `ImageRenderer::render` wraps protocol payloads
  via `term::tmux_wrap` when `GraphicsCaps.wrap == Some(Tmux)`
  (KERNEL's verified-passthrough detection); session-authored
  place/delete escapes wrap too (test-pinned). Routing, not
  degradation — no `#FALLBACK` label.
- **Your Driver landed mid-cycle — reviewed against it** (driver.rs
  `render_images` + overlays.rs `ImageEntry`): the wiring direction is
  right (mosaic → blit_mosaic, bytes → post-present external_write
  bracket, one flush). Two upgrades `ImageSession` gives you
  drop-in:
  1. **kitty upload leak**: removing an image overlay damages the root
     cells but never sends `a=d` — on kitty terminals every dropped
     overlay's pixels stay resident terminal-side, unbounded.
     `ImageSession::release(key)` deletes by id.
  2. **full retransmission per dirty mark**: `ImageRenderer::render`
     re-uploads all pixels on every dirty=true; `ImageSession::sync`
     re-places MOVED images by id (tiny `a=p` escape) and only
     retransmits when the CONTENT version bumps — `ImageEntry` would
     carry `version: u64` (bump in `set_bitmap`, not in `set_rect`).
  The sink adapter on your side is ~6 lines (`ExternalSink` impl over
  `(presenter, out)` — same bracket you already call). Happy to review
  the swap; my half is stable.
- **Verification honesty**: kitty/iTerm2/sixel/tmux traffic is
  byte-level verified (frame parsers + a sixel interpreter in tests) —
  NOT yet exercised against a live kitty/iTerm2/tmux. First live smoke
  belongs with the Driver integration (cycle 5?); expect quirks in the
  delete-then-retransmit ordering on real emulators.

## Raster perf wave (numbers in the report)

The structural change: `SceneRenderer` (reusable scratch) projects and
shades each vertex ONCE per instance (cycle-3 projected per triangle
corner — 3x the work on shared vertices), with a no-clip fast path for
triangles fully in front of the near plane, screen-bbox rejection
before fill setup, and the guard-band clip only for the rare huge
triangle. `render()` stays as the one-shot wrapper. Free fn callers
(widgets, brandmark) get the win by holding a `SceneRenderer`.

## MosaicOpts (RT follow-up surface)

`RenderConfig.mosaic: MosaicOpts { mode, dither: Option<u16> }` —
optional median-cut + Floyd–Steinberg pre-pass for low-color output
targets; truecolor stays the default. Quality goldens live in
`gfx/mosaic_quality_tests.rs` (hard edges pick partition glyphs,
gradients invent no structure, seeded-noise exact-grid snapshots for
all three block modes) — a chooser change now diffs a golden, not a
feeling.
