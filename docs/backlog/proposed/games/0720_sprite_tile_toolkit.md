# 0720 — Sprite/tile toolkit: masked blit, sprite sheets, cell-art palette swap

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: games (band 0700–0790)
- Completed: N/A
- Depends on: nothing hard. Composes with gfx (`decode_image`, mosaic
  fitting) for the sheet half; the masked blit stands alone on
  `Surface`. Extensions 0420 (public dot canvas) is a SIBLING, not a
  dependency — strokes and sprites are different draw vocabularies.
- Cross-band consumers: icon/cell-art anywhere (file-manager glyph
  tiles, map thumbnails in dashboards, brandmark-style flourishes);
  0430's node-graph cards want masked blits for card chrome reuse.
- Promotion trigger: the first game example reaching its render phase,
  or a second consumer hand-rolling cell-by-cell sprite copies.

## ADR status
- Governing ADRs: ADR-0001 (additive: new `Surface` method + a new
  small module; no existing signatures change). ADR impact: none.

## Context
Retro cell games are sprites over tiles: a @ hero over a dungeon floor,
a 'Mech token over a hex, an animated 2-frame torch. The engine has the
halves — owned cell buffers with damage, wholesale blits, bitmap
decode + cell fitting, per-layer tint — but no way to stamp a shaped
sprite INTO a scene surface without erasing what's under its empty
corners, and no way to load sprite art from an image once and reuse it
as cells.

## Current code reality
- **`Surface::blit` is wholesale**: it copies EVERY source cell into
  the destination — including empties — with clipping, pool adoption,
  and wide-pair repair (src/render/surface.rs:421-460; the per-cell
  loop at 439-447 has no transparency test). Correct for panels; wrong
  for sprites: a 3×2 sprite with rounded corners erases the floor
  behind its corner cells.
- **Cross-LAYER transparency already exists**: under `Blend::Normal`
  a `Glyph::EMPTY` cell is see-through and lower layers show
  (src/render/compositor.rs module doc, "Blending model"). So today's
  workaround is one overlay layer per sprite — real, but heavyweight
  past a handful of entities: every handle mints a full layer
  (creation at src/app/overlays.rs:158-179; the carried per-layer
  state — surface, origin, z, opacity, blend, transform, shader,
  frame damage — is `render::layer::Layer`, src/render/layer.rs:
  160-172), and layer-count scales the flatten loop.
- **Layers still earn their keep at the SCENE granularity**: map layer
  + entity layer + fx layer, with `set_offset` for smooth map scroll,
  `Blend::Additive` for glow, `ColorTransform` for tint/day-night,
  shaders for weather (src/app/overlays.rs:602-641) — the toolkit
  should compose with this, not replace it.
- **The asset pipeline half-exists**: `gfx::decode_image` (PNG/JPEG →
  `Bitmap`), mosaic fitting with least-squares half/quadrant/braille
  cell choice (src/gfx/mosaic_fit.rs:5-14), dither/quantize
  (src/gfx/). Nothing slices a sheet into frames; nothing converts a
  bitmap region into a reusable cell `Surface` at LOAD time (mosaic
  renders per-frame to patches instead — the right shape for photos,
  wasteful for a 4-frame sprite reused thousands of times).
- **Palette swap has a per-LAYER answer only**: `ColorTransform`
  tints a whole layer (src/app/overlays.rs:618-620); classic
  palette-swapped variants (red team / blue team from one sprite) need
  per-BLIT recoloring, which nothing offers.
- Precedent for cell-art authoring in-tree: the Logo widget and
  `three::brandmark` draw cell art programmatically; games want the
  same result from PNG assets without hand-coding draw closures.

## Problem
A game scene today is either one layer per entity (scales badly) or
hand-rolled per-cell copies re-implementing clipping and wide-pair
rules that `Surface` already owns. Sprite ASSETS have no load story:
every consumer re-invents bitmap→cells conversion and frame slicing,
and palette variants require duplicate art.

## What we want
1. **Masked blit on `Surface`** (suggested `blit_masked(src, src_rect,
   dst)`): identical to `blit` except source cells that are fully
   empty (`Cell::EMPTY`-equivalent: no glyph, transparent bg) are
   SKIPPED, leaving the destination cell untouched. Same clipping,
   same pool adoption, same pair repair, same damage rules
   (surface.rs:439-458 is the template; the skip is one test per
   cell). Optionally a `blit_tinted` variant applying a color map
   during the copy (see 3).
2. **Sprite sheets** (suggested `gfx::sprites` or `widgets::sprite`):
   - `SpriteSheet::from_bitmap(bitmap, tile_w, tile_h, mode)` slices a
     decoded image into a grid of frames and converts each ONCE into a
     cell `Surface` via the existing mosaic fitters (mode =
     half/quadrant/braille — reuse `gfx::mosaic_fit`, never a second
     fitter); transparent pixels (alpha under threshold) become empty
     cells so masked blits work naturally;
   - `frame(ix) -> &Surface` for blitting; frames are plain surfaces —
     no new render concepts.
3. **Palette swap for cell art**: a small recolor map applied at blit
   time (exact-color→color pairs, the retro semantic) or as a
   `Surface::recolored(map) -> Surface` preprocessing step. Alpha-0
   ("terminal default") passes through untouched — the same rule every
   shader follows (src/anim/shaders.rs:15-18).
4. **Aspect honesty in docs**: sprite art is authored in PIXELS but
   lands on ~1:2 cells; the sheet API documents the same correction
   the engine applies everywhere else (particles halve vertical
   velocity, src/anim/particles.rs:120-122; mosaic contain-fit) so
   sprite authors size art at 2:1 pixel aspect per cell.

## Scope / Non-goals
Scope: masked blit (+ tinted variant), sheet slicing over existing
fitters, recolor map, docs + a small example scene (map layer + masked
entity blits).
Non-goals: animation state machines (app policy — 0710's ticker drives
frame indices); collision detection; a scene graph; ASCII/ANSI art
FILE formats (a bitmap pipeline exists; text-art loaders can ride a
later item if a consumer appears); changes to compositor blending.

## Expected outcomes
A dungeon renders as one scene surface: floor tiles blitted wholesale,
entities masked-blitted over them, one layer total for the scene (fx
layers stay layers). Sprite art loads from one PNG sheet; team-color
variants are a recolor map, not duplicate assets.

## Validation
- Masked blit: golden tests — empty-cornered sprite over patterned
  ground leaves ground visible at corners; wide-pair edges repaired
  identically to `blit` (reuse the surface_tests.rs blit cases,
  src/render/surface_tests.rs:147-260, with the masked twin).
- Sheet slicing: a synthetic 2×2-frame bitmap slices into 4 surfaces;
  alpha-thresholded pixels become empty cells (masked-blit-ready);
  mode parity with mosaic fitters pinned by comparing one frame
  against `render_to_cells` output.
- Recolor: exact-match swap hits only mapped colors; alpha-0 passes
  through; idempotent on unmapped surfaces.
- Damage: masked blit damages exactly the destination span (the
  existing blit rule, surface.rs:458).

## Progress checklist
- [ ] `Surface::blit_masked` (+ pair-repair parity tests)
- [ ] Recolor map (`blit_tinted` or `recolored`)
- [ ] `SpriteSheet::from_bitmap` over gfx fitters + alpha threshold
- [ ] Example: scene surface with masked entity sprites over tiles
- [ ] Docs: sprite pipeline + aspect-authoring note
