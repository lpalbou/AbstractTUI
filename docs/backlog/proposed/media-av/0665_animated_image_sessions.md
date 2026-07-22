# 0665 — Animated image sessions (kitty frames; timer fallback elsewhere)

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: study-2 lifecycle fixes (shipped); a GIF/APNG decoder does
  NOT exist in-tree and is a real dependency question for this item.
- Promotion trigger: a consumer with animated content (chat stickers,
  loading animations from real assets) — not before.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: the decoder question may touch
  the standalone-dependency posture (no new deps without a ruling) —
  flag at promotion.

## Context
The kitty protocol has a full animation lane: `a=f` transmits frame
data, `a=a` controls playback, and the TERMINAL animates autonomously —
zero bytes per frame after upload (spec §animation; gaps: WezTerm's
kitty lane lags on animation, ghostty supports it). Every other channel
must re-emit per frame (iTerm2/sixel: full payload ~30-100 KB per tick;
mosaic: cell diff — cheap). Today the engine has `set_bitmap`
(version bump = full retransmit), which animates correctly but pays the
worst-case cost on every channel including kitty.

## Current code reality
- `ImageHandle::set_bitmap` (src/app/overlays.rs:731-737) is the only
  animation surface; on kitty it deletes + retransmits the whole image
  per frame (session version-bump lane, src/gfx/session.rs:186-196).
- `KittyModel` (src/testing/kitty_model.rs:246-249) explicitly rejects
  `a=f`/`a=a` as "not modeled (extend the rig)".
- The frame clock (`reactive::animate`) can drive set_bitmap loops
  today — correct, just byte-expensive.

## Problem
Animated content on kitty should cost zero steady-state bytes (the
protocol was designed for exactly that); via set_bitmap it costs a full
transmit per frame — hundreds of KB/s for a modest sticker.

## What we want
1. `ImageSession` animation lane: upload frames once (`a=f` with frame
   numbers + delays), start/stop via `a=a`; the session tracks "the
   terminal is animating" and syncs become no-ops.
2. Honest fallback: on non-kitty channels the same API drives a
   timer-based set_bitmap loop (labeled cost), mosaic preferred.
3. KittyModel learns `a=f`/`a=a` accounting (frame count, no re-upload).

## Scope / Non-goals
Scope: session lane, fallback loop, model extension.
Non-goals: GIF/APNG decoding (dependency ruling first; the API takes
pre-decoded frames), video.

## Expected outcomes
A sticker animates at zero steady-state bytes on kitty and degrades to
a labeled timer loop elsewhere.

## Validation
- Session tests through the extended KittyModel: N frames uploaded
  once, zero traffic while "playing", stop/delete accounting clean.
- Fallback loop: virtual-clock frame advance, full-tree containment.

## Progress checklist
- [ ] KittyModel a=f/a=a accounting
- [ ] Session animation lane
- [ ] Timer fallback + labels
- [ ] Decoder dependency ruling (separate)
