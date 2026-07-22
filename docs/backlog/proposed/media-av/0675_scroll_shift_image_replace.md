# 0675 — Scroll shift × live images: re-place instead of disabling the win

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: the study-2 guard (shipped 2026-07-22): the driver takes
  the PLAIN diff while byte-channel images are live — correct, but it
  forfeits the measured 19.3× scroll byte win whenever a picture is on
  screen.
- Promotion trigger: a log/feed app that keeps a persistent image
  (avatar rail, chart panel) and measures scroll cost.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none.

## Context
Terminals scroll images WITH the text (kitty spec: "when scrolling the
screen … images must be scrolled along with text"; sixel pixels scroll
on xterm-class emulators). A DECSTBM+SU/SD emission therefore moves
terminal-held placements while the `ImageSession` still believes the
old rects — the desync the shipped guard prevents by not emitting
shifts at all while `live_byte_slots() > 0` (src/app/driver.rs, phase P).

## Current code reality
- Guard: driver.rs phase P chooses `FrameDiff::compute` over
  `compute_scrolled` when `ImageSession::live_byte_slots() > 0`
  (src/gfx/session.rs — the census shipped with the study-2 wave).
- Kitty re-place is CHEAP and ghost-safe now: `kitty::place` carries the
  fixed placement id, so re-asserting a rect after a shift is one ~30
  byte escape per image (src/gfx/proto/kitty.rs).
- The shift token is structural (`render::ScrolledRuns`,
  src/render/scroll.rs:50-78) — the driver knows exactly which band
  moved by how much BEFORE emitting.

## Problem
An app with one persistent kitty image loses the entire scroll
optimization for its main log surface — the guard is honest but blunt.

## What we want
1. When a shift is emitted and kitty slots are live: compute each
   slot's post-shift rect; fully-inside-band slots get a `place()`
   re-assertion at the shifted rect (and the session rect updates);
   slots straddling the band boundary fall back to the plain diff for
   that frame (clipped scroll of an image is not re-placeable).
2. iTerm2/sixel slots keep the plain-diff guard (their pixels cannot be
   re-placed; re-emitting per scroll tick would cost more than the win).
3. The decision stays in one place (phase P) with the same test seams.

## Scope / Non-goals
Scope: kitty re-place lane + straddle fallback + tests.
Non-goals: predicting terminal scroll of images outside the emitted
band (margins clip per spec — trust it, verify in the recipe).

## Expected outcomes
Log apps keep the scroll byte win with a kitty image on screen; the
session bookkeeping stays truthful (KittyModel referee extended with a
scroll-aware position check if feasible — it is currently id-only).

## Validation
- Driver test: scrolling content + live kitty slot → shift emitted AND
  an `a=p` re-assertion for the slot; session rect matches; plain-diff
  fallback pinned for straddling rects and for sixel caps.

## Progress checklist
- [ ] Post-shift rect math + straddle detection
- [ ] Kitty re-place lane
- [ ] Tests through the referee
