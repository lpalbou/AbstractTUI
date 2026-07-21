# RENDER — cycle 3 requests + reconciliation notes

Author: RENDER.

## Reconciliation (interrupted-session cleanup, for the record)

- `src/render/layer.rs` + `src/render/compositor_tests.rs` were mid-write
  when the session cut. Both are now finished and reconciled: `Layer`
  moved from `compositor.rs` into `layer.rs` with the cycle-3 additions
  (Blend::Additive, ColorTransform, CellShader + shader_t);
  `compositor.rs` keeps flatten/damage only; all cycle-2 compositor tests
  survive unchanged in `compositor_tests.rs` plus the new blend/grade/
  shader coverage. `render::{Blend, Layer}` re-export paths are stable
  (the app driver compiles untouched).
- An earlier mid-write draft had the compositor own a `u64` frame counter
  as the shader clock; reconciliation REPLACED it with per-layer
  `shader_t` driven by the app clock — replaying the same layer state now
  composes the same frame with no hidden compositor time, and shader
  animation costs are billed per layer (the damage rule) instead of
  globally.

## RT2-8 closure (REDTEAM)

- Acceptance test `presenter_no_change_frame_emits_and_allocates_nothing`
  now measures **0 allocs / 0 reallocs / 0 bytes emitted** (verified over
  repeated runs and in the full suite). I un-ignored it per the finding's
  "un-ignore on fix" clause — the one-line attribute edit in your
  `tests/alloc_budget.rs` is the only foreign-file change this cycle;
  revert and ping me if you want the flip to come from your side instead.
- Honest attribution: the fix landed WITH the RT2-1 scratch rework (the
  same-cycle rework your finding credits) — at reconciliation the
  no-change path already measured zero. What the rework changed for this
  path: diff scratch (`spans`/`runs`) is fully reused with no per-row
  re-allocation, no runs are pushed when nothing changed, and the
  presenter's empty-runs early-return emits nothing and touches no
  buffers. The functional twin now lives in-module
  (`render::diff::tests::no_change_frame_is_scratch_only_and_byteless`)
  so a regression fails in my suite before it reaches your allocator rig.

## New emission/model requests

### To REDTEAM

1. **DECSTBM + region-scoped SU/SD in `VtScreen`** (one-liner-ish):
   `CSI Pt;Pb r` setting the scroll region, SU/SD honoring it, cursor
   homing per DECSTBM semantics. This gates the scroll-region
   optimization (docs/design/render.md §2.7): measured 19.3x byte win on
   1-row 200x60 log scrolls (5,374 -> 279 bytes), 4.5x on 5-row — but I
   will not emit sequences the referee cannot replay, so the emitter is
   deferred to cycle 5 behind `PresenterOpts { scroll_optimization }`
   default OFF, detection property-tested against your model first.
2. **Shader goldens**: `anim::shaders::{Shimmer, ScanlineFade, HueDrift,
   Dissolve}` are pure in `(x, y, t, cell)` + params, no libm (triangle
   wave + integer hash; `cell_hash` goldens pinned in my tests:
   `(0,0,0)=0.0`, `(3,7,42)=0.725_178_1`, `(80,24,0xDEAD)=0.470_138_55`).
   Sample-point goldens are in `anim/shaders_tests.rs`; full-frame goldens
   through flatten are yours to extend. The identity pin you asked about
   holds: defaults-off compositing is byte-identical
   (`compositor_tests::defaults_are_byte_identical_to_ungraded_compositor`).

### To REACT

3. **StyledCanvas landed render-side** (`render::bridge::StyledCanvas`,
   re-exported at `render::StyledCanvas`): `print_styled(p, text, Style)
   -> i32` + `fill_styled(rect, Style)`, implemented for `Surface`,
   grapheme-correct, attrs/underline-color/link carrying. You own the
   final home: either re-export/alias it from `ui` (widgets then import
   one path) or declare your preferred trait in `ui::canvas` and I'll impl
   it for Surface and retire mine — the Surface behavior is the contract,
   the trait's address is yours. `BufferCanvas`'s impl (attr-less: apply
   colors, ignore attrs, or grow attr storage) is your call.
4. **Transition/Timeline ready for signal wiring**: `anim::Transition`
   (tick(now) -> value, `is_settled`, retarget-from-live-value) and
   `anim::Timeline` (progress(track, t), LoopMode) are engine-only as
   agreed — the binding pattern is tick-then-request-frame-while-active.

### To DESIGN

5. Your cycle-1 request 8 items are in: `Blend::Additive` (afterglow/
   particles), per-layer `ColorTransform::{Dim, Tint, Grayscale}` (trail
   decay ×0.72/100ms = `Dim(0.72)` steps driven by your clock), per-layer
   opacity (already there), and the shader hook if the splash wants
   full custom (e.g. `Dissolve` for the skip fade). `Timeline` +
   `Easing::bezier` cover the identity storyboard's phase constants —
   `timeline::tests::identity_storyboard_shape_composes` sketches your
   constants as a smoke test. Depth fog toward theme bg (your request 8's
   third item) is best expressed as `ColorTransform::Tint(bg, fog(z))`
   per 3D layer — tell me if you need a per-CELL fog input instead (that
   would be a shader, and GFX3D owns the z-buffer values it would read).

### To KERNEL

- Nothing new. (The deferred_wrap ConPTY verdict remains the standing
  open item from cycle 2.)

## Foreign-failure note (reconciliation evidence)

At resume, 2 tests were failing crate-wide (`widgets::button` hover/
pressed — DESIGN's, token-related) and 3 more (ui x3, three::validate)
had already recovered by the time I measured; at this cycle's close the
full suite is 656 passed / 0 failed / 4 ignored, so all foreign failures
resolved during the wave. None touched render/text/anim paths.
