# 0670 — Cell-pixel-size refresh on resize (font zoom re-scales sixel/3D)

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: nothing — the refresh helper exists; the driver just
  never calls it.
- Promotion trigger: first sixel-terminal field report of blurry/
  misscaled images after a font-size change, or bundled into the next
  driver-images wave.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (driver-internal call).

## Context
Font zoom (Cmd+/Cmd-) usually keeps the cell GRID similar while every
cell's PIXEL size changes. Sixel rasterization and the 3D viewport
scale by `cell_pixel_size`; a stale value renders images at the old
pixel density — blurry after zoom-in, oversized after zoom-out.

## Current code reality
- `term::probe::refresh_cell_pixel_size` (src/term/probe.rs:264-272)
  exists FOR THIS ("call after a resize, where cell metrics may
  change") and is platform-cheap (TIOCGWINSZ quotient; keeps a wire
  answer when the platform cannot measure).
- It is called exactly once, inside `input::probe_active`
  (src/input/reader.rs:280) — startup only.
- `Driver::apply_resize` (src/app/driver.rs:648-677) re-dirties every
  image and poisons prev — the re-EMIT half is right — but never
  refreshes `caps.cell_pixel_size`, so the re-emitted sixel uses the
  stale pixel geometry. (kitty/iTerm2 scale server-side by cells and
  are immune; sixel and `Viewport3d` supersampling are the victims.)
- Wrinkle: a font zoom that keeps cols×rows IDENTICAL never enters
  apply_resize at all (size == self.size early-return, driver.rs:649).
  Some terminals send no resize signal in that case either — detection
  may need the SIGWINCH-less path's periodic ioctl compare
  (src/term/unix.rs:36-38) to include pixel fields.

## Problem
Zoom changes silently degrade sixel fidelity until the next restart;
the equal-grid case has no trigger at all.

## What we want
1. `apply_resize` refreshes cell size via the existing helper before
   re-dirtying images.
2. The resize-detection compare includes ws_xpixel/ws_ypixel so an
   equal-grid zoom still surfaces (as a caps-refresh event, not a fake
   resize).
3. A `#FALLBACK`-labeled note when the platform reports zeros and a
   stale wire value is kept (already the helper's documented posture —
   surface it once).

## Scope / Non-goals
Scope: the two trigger points + tests. Non-goals: re-probing `CSI 16 t`
mid-session (a resize storm of wire queries; platform ioctl suffices on
unix — Windows keeps the wire value per windows.rs:525-528).

## Expected outcomes
Sixel/3D output tracks the real glyph size across zooms without
restart.

## Validation
- Scripted-terminal test: resize with changed `cell_pixel_size` →
  next image sync's raster attrs carry the new pixel geometry.
- Unix ioctl compare covers the equal-grid pixel change (unit over the
  winsize compare fn).

## Progress checklist
- [ ] apply_resize refresh call
- [ ] Pixel-aware resize compare (unix)
- [ ] Sixel re-raster test
