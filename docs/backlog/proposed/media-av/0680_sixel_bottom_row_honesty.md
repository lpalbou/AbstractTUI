# 0680 — Sixel bottom-row/off-screen honesty (clamp + DECSET 8452 probe)

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: nothing.
- Promotion trigger: first sixel-terminal validation pass (the
  images-truth recipe) reporting a scroll, or a sixel-capable consumer.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none.

## Context
Most sixel terminals put the text cursor on the line BELOW the emitted
image; an image whose bottom edge touches the last screen row therefore
SCROLLS the whole screen (chafa#192 catalogues the per-terminal
behavior; foot/contour/WezTerm implement the DEC placement, xterm is
off-by-one). DECSET 8452 ("cursor to the right of the graphic") avoids
it where supported, detectable via DECRQM — machinery the probe already
has for modes 2026/1016. iTerm2 images share a milder version of the
same hazard (cursor lands under the image like text).

## Current code reality
- The engine CUPs to the rect origin and emits; nothing prevents a rect
  whose bottom is the last row (src/render/present.rs:external_write —
  absolute CUP, payload verbatim; src/gfx/pipeline.rs sizes the raster
  to rows×cell.h exactly, so the pixels FIT — the hazard is the
  terminal's cursor-after-image, not our geometry).
- The presenter invalidates its cursor model after external writes, so
  a non-scrolling cursor surprise is already absorbed; a SCROLL is not
  absorbable (the whole screen shifts under prev).
- `examples/images.rs` clamps its own placement rect away from the last
  row (study-2 fix) — app-side, not enforced anywhere.
- DECRQM plumbing: `ActiveProbe` folds `DecMode` replies
  (src/term/probe.rs:140-147); adding 8452 is one query + one arm.

## Problem
A correct app can wedge its whole frame model by placing a sixel/iTerm2
image touching the bottom row — one scroll and every cell of prev is
wrong until the next full repaint.

## What we want
1. Driver-level clamp: cursor-paint placements (sixel/iTerm2) whose
   rect touches the LAST row are shrunk by one row with a labeled
   `#FALLBACK` warning (the ladder's warning lane now reaches notices).
2. Probe DECRQM 8452; when the terminal supports it, set it during
   `enter` for graphics-capable sessions and drop the clamp (cursor
   stays beside the image; xterm's buggy support stays clamped via the
   terminal-names-itself check).
3. Post-scroll detection stays out of scope — the clamp PREVENTS.

## Scope / Non-goals
Scope: clamp + label, 8452 probe/enter bit, recipe update.
Non-goals: kitty (placements do not scroll the screen), reflow
recovery after a terminal-side scroll we did not cause.

## Expected outcomes
Bottom-row placements degrade one row with a visible label instead of
corrupting the whole frame; 8452-capable terminals keep the full rect.

## Validation
- Pipeline/driver test: rect touching the last row on sixel caps →
  emitted raster height shrinks one row + warning queued; with a
  scripted 8452-supported reply → full rect, mode set on enter.

## Progress checklist
- [ ] Clamp + warning
- [ ] DECRQM 8452 probe + enter wiring
- [ ] Tests + recipe note
