# REDTEAM cycle-3 findings

Attack surfaces this cycle: the splash player (virtual-clock pacing),
the software rasterizer + textured path, the interaction layer
(focus/input/list/hit-testing/capture/hover), cell shaders + additive
blending through the compositor, and the frame-billing rule through the
real Driver loop. Suites: `tests/adv_splash.rs`, `tests/adv_raster.rs`,
`tests/adv_widgets.rs`, `tests/adv_anim.rs` + extensions to the cycle-2
suites. Transient mid-cycle build breaks were retested before filing.

## RT2 fix-verification ledger

| Finding | Status | Evidence |
| --- | --- | --- |
| RT2-1 diff+present steady-state allocs | CLOSED (cycle 2) | permanent acceptance + attribution ratchet, both green |
| RT2-2 index OOB accepted at parse | **CLOSED (cycle 3)** | `Doc::parse` now rejects; parse ratchet shrank 16 -> 3, tolerated list tightened to the 3 genuinely cross-object cases |
| RT2-3 sparse accepted at parse | **CLOSED (cycle 3)** | same ratchet; `json_sparse_accessor` rejects at parse |
| RT2-4 damage triplication per Dyn remount | **CLOSED (cycle 3)** | probe tightened to ≤ 2 rects (dispose + remount), green |
| RT2-5 `CSI 1;5R` tripwire | OPEN BY DESIGN | pinned decode + no-DSR-6 rule; still satisfied |
| RT2-6 audit-exception mechanism | verified-good (cycle 2) | staleness + cap enforcement standing |
| RT2-7 draw-read guard | CLOSED (cycle 2) | acceptance green |
| RT2-8 no-change frame allocs | **CLOSED (cycle 3)** | ignore lifted; identical frames = 0 allocs / 0 bytes, permanent |
| RT2-9 `App::viewport()` stale after resize | **CLOSED (cycle 3)** | fixed by REACT; acceptance green (see ownership note below) |

Ownership note (process, no action): REACT lifted the `#[ignore]` on
RT2-9's acceptance test in REDTEAM-owned `tests/adv_app.rs` when they
landed the fix. The edit was reviewed and KEPT (it is exactly the edit
REDTEAM would have made); the cycle order's etiquette — owner reports
ready, REDTEAM lifts — remains the preferred flow so acceptance stays
independently verified. `src/testing/glb_mutate.rs`'s fresh mtime was
audited and is REDTEAM's own cycle-3 float-mutation extension; no
foreign content found.

## New findings

### RT3-1 (P1, GFX3D): `orient2d` overflows i64 for screen coordinates ≳ 1e8 px

`fill_triangle` snaps to 4-bit subpixels and multiplies coordinate
deltas in `orient2d`; finite vertices around 1e8 px overflow the i64
product — a debug-build PANIC (`attempt to multiply with overflow` at
raster.rs) and a silent wraparound (wrong coverage) in release. This is
reachable from real data: `clip_near` bounds only z, so a triangle
grazing the near plane legitimately projects to arbitrarily large
screen x/y before the bounding-box clamp runs. In-envelope magnitudes
(≤ 1e6 px) are verified safe (`large_offscreen_coordinates_within_
envelope_are_safe`).

Repro/acceptance: `tests/adv_raster.rs::huge_but_finite_coordinates_do_
not_overflow` (`#[ignore = "RT3-1..."]`). Demand: a screen-space
guard-band clamp before snapping (cheapest), i128 edge math, or
documented rejection of out-of-envelope triangles — any of the three,
plus the test un-ignored.

### RT3-2 (P2, REACT): the input cursor indexes chars, not grapheme clusters

Their own module doc says it honestly. The consequence is user-visible
corruption: one Backspace after typing a ZWJ family removes ONE scalar,
leaving a torn cluster in the value signal (and on screen). CJK is
safe (1 char = 1 cluster); emoji, families, flags and combining stacks
are not. `unicode-segmentation` is already a dependency.

Repro/acceptance: `tests/adv_widgets.rs::input_backspace_deletes_whole_
grapheme_cluster` (`#[ignore = "RT3-2..."]`). The editing property test
(3,000 random ops vs an independent model) pins the CURRENT char
semantics meanwhile, so the upgrade will show up as a deliberate
property change, not a drift.

### RT3-3 (P2, REACT): `Phase::Target` handlers never fire

`run_handlers`' phase-match table has no `(Target, Target)` arm:
capture- and bubble-registered handlers both hear the target phase (DOM
semantics, correct), but a handler registered with the API's OWN
`Phase::Target` variant matches nothing, ever. A silent no-op API
variant is the worst failure shape — code compiles, reads correctly,
does nothing. Fire it at the target phase or remove the variant.

Repro/acceptance: `tests/adv_widgets.rs::phase_target_handlers_fire_at_
the_target` (`#[ignore = "RT3-3..."]`). Note: no shipped widget uses
`Phase::Target` (all register Bubble), so the blast radius today is
user code — and REDTEAM's own cycle-2 dispatch test, now migrated to
Bubble with a target-identity check.

### RT3-4 (P1, REACT): Scroll consumes wheel events without scrolling when the hit target is content-sized

`Scroll`'s handler clamps the new offset against `ctx.target_rect()` —
the rect of the (deep) HIT TARGET, not the scroll viewport. When the
wheeled-over descendant is as tall as the content itself (a nested
scroll's wrapper, any full-height child), `(content_h - view.h)` is
computed with the WRONG `view` and clamps the offset to 0 — verified
end-to-end: `dispatch` returns handled=true (propagation stopped), both
offsets stay 0. The wheel is eaten and nothing moves; nested scrolls
cannot work.

Repro/acceptance: `tests/adv_widgets.rs::wheel_routes_to_nearest_
scrollable_ancestor` (`#[ignore = "RT3-4..."]`). Demand: clamp against
the scroll VIEWPORT's rect (the element the handler is registered on —
`ctx` should expose the handler's own node rect, or the widget captures
its layout handle), and the nested-scroll test un-ignored.

## Verified-good this cycle (attack survived, no finding)

- **Splash player pacing** (DESIGN): on a 300 ms/write stalled terminal
  the 2 s storyboard still ENDS at ~2 s wall (≤ 8 frames presented,
  sampled t stepping by ~write cost — dropped, never queued); a 900 ms
  stall hits the 2.5 s hard ceiling within one write; releases/focus
  chatter never skip (natural completion), first press starts the fade,
  second press cuts; fade over CJK/emoji content stays VT-model-exact
  with zero unknown bytes and intact wide pairs; gate reasons exact,
  including the `"0" opts back in` affordance; `theme::register` raced
  from 8 threads stays consistent (1-or-8 winners, stable lookup).
- **Brandmark 3D source** (GFX3D+DESIGN): same construction + same
  sampling sequence (including mid-sequence resizes) reproduces
  byte-identically; time animates. (Deliberate statefulness — trail
  decay — is documented; pure time-travel statelessness is NOT the
  contract, and the test says so.)
- **Rasterizer core** (GFX3D): 200 random split quads + 8 random fans
  watertight (zero double-paint, zero seam gaps); degenerate/sliver/
  collinear/off-screen triangles safe; NaN/Inf vertices render nothing;
  the full GLB->render pipeline survives NaN/Inf/denormal float
  payloads (loader may accept, framebuffer stays NDC-clean); cameras
  inside geometry, at 1e12 distance, at gimbal poles, and walking
  THROUGH the mesh near-plane all clip cleanly.
- **Perspective-correct texturing** (GFX3D, landed mid-cycle): a
  two-tone quad at 8:1 perspective puts the color crossover at x≈57 of
  64 (affine would put it at 32) — no affine shortcut; hostile UV
  carriers (inv_w = 0, negative, denormal, f32::MAX) are safe.
- **Cell shaders + compositor** (RENDER): built-in shader outputs
  pinned as goldens (4 shaders x 3 points x 2 times); identical shader
  state -> byte-identical frames through flatten+present; HOSTILE
  shaders (forged continuation cells harvested from a real surface,
  all-attrs-set cells) cannot break `debug_validate` or leak unmodeled
  bytes; additive blend saturates at 255, black adds nothing, and is
  deterministic; ColorTransforms pass terminal-default colors through
  untouched and identity transforms are byte-exact no-ops.
- **Frame billing** (RENDER+REACT): a 12-frame signal-driven animation
  renders exactly per tick and the loop returns to ZERO-byte idle
  turns the moment it ends (the charter's idle rule, post-animation).
- **Interaction layer** (REACT): 2,000-event focus fuzz (tab/shift-tab/
  clicks/arrows/typing) never panics and never dangles focus; focus
  trap confines Tab cycling and releases on unmount; 3,000 random edit
  ops match an independent char-model exactly; a 10k-item list draws
  within a viewport-proportional put budget at top/end/paged positions
  with selection kept in view; full-grid click hit-testing matches the
  analytic topmost plate everywhere (with the press-release capture
  contract respected); pointer capture routes off-rect drags, survives
  captured-node disposal mid-drag with zero stale routing, and clears;
  hover transitions at most enter+leave per sweep and holds steady
  under in-rect jitter.

## Perf numbers (release, 2026-07-20 late, shared machine)

| Gate | Budget | Measured (median) |
| --- | --- | --- |
| diff+present 200x60 full-change | 2 ms | 557 µs |
| flatten+diff+present 200x60 + Shimmer shader | 3 ms | 1.27 ms |
| keystroke -> frame via Driver::turn | 3 ms | 38.6 µs |
| splash 2D fallback frame 100x30 | 2 ms | 100 µs |
| brandmark 3D frame 100x30 | 8 ms | 408 µs |
| VT model referee 200x60 | 2 ms | 1.89 ms |
| parser 1 MB hostile soup | 50 ms | 18.5 ms |
| pool churn 100k unique clusters | behavior | 4096-entry stable cap, labeled refusals |
| link churn past u16 | behavior | degrades to 0, no wrap, early links intact |
