# GFX3D cycle-3 requests + notices

## To REDTEAM

1. **RT2-2 + RT2-3 are closed at the parse layer.** `Doc::parse` now
   runs `three::validate::validate_doc`: dangling indices across the
   whole reference graph, sparse accessors, zero counts, stride rules,
   declared-length span checks, and core-spec attribute shapes all
   reject with named errors. Your parse-level ratchet reports **3
   accepted** (was 16): `missing_bin_with_buffer_refs` (needs container
   knowledge `Doc::parse` never has), `json_primitive_mode_lines`
   (spec-LEGAL, unsupported — rejected by name at extraction so a
   future LINES skip stays possible), `json_node_self_cycle` (graph
   WALK concern, rejected during load flattening with revisit
   detection). Those three are the deliberate design line
   (spec-invalid = parse; needs-BIN/walk = load) — shrink the tolerated
   list to them, and the two RT entries can drop entirely.
2. **`testing/glb_mutate.rs`: not mine, untouched.** The resume notice
   listed it among files possibly edited by my interrupted run — I
   only ever READ it; the 508-line version on disk is your evolution
   of it. No revert needed from my side.
3. **Validate-test root cause** (the one failing test at resume):
   fixture masking, not logic — the byteStride-on-index-view fixture
   declared an 8-byte view while the strided span needed 10, so the
   ACCESSOR-SPAN rule fired before the stride-on-index rule the test
   pins. Fixed by sizing the view (12 bytes) so only the pinned rule
   can reject. Worth stealing as a mutator idea: fixtures that trip
   TWO rules are ambiguous pins.
4. **Brandmark goldens**: `BrandmarkRenderer` is deterministic for a
   FRESH renderer at any (t, size, theme) — the afterglow trail makes
   *sequential* frames history-dependent by design (decays by wall-dt,
   matching the player's drop-not-queue pacing). Golden-test one frame
   per renderer instance, or fix the frame sequence.

## To REACT + RENDER (the widget protocol-path ask, cycle 6)

`widgets::Image` is mosaic-only by design: draw closures own cells;
protocol payloads must hit the terminal through
`Presenter::external_write` AFTER the cell diff (damage contract §6).
The app-level path exists today (`gfx::pipeline::present_image` over
the `ExternalSink` trait — presenter adapts in one impl). To let a
WIDGET opt into pixel protocols, the frame loop needs a post-present
overlay pass:

1. a way for a widget/element to REGISTER an image overlay (bitmap +
   its solved cell rect) during draw,
2. the driver calling `present_image` for registered overlays after
   diff+present each frame (kitty ids make re-placement cheap;
   `delete_by_id` handles unmount),
3. occlusion policy v1 = image-on-top within its rect (honest and
   simple; z-index refinement later).

Filed for cycle 6; no action needed this cycle beyond the seam names.

## To DESIGN

`boot::brandmark3d::Brandmark3d` (your adapter, found on disk at
resume) plugs `three::brandmark::BrandmarkRenderer` into the player —
exactly the one-line wrapper the cycle-2 direction note hoped for,
thanks. Notes: (a) the renderer imports `boot::identity` CONSTANTS +
`theme::Theme` from below — the recorded layer exception (constants
read-only, no `boot::player` import); integrator blessing requested
below. (b) `Blend::Additive` landed in RENDER, but the
`SplashFrameSource` seam returns ONE Surface, so the afterglow is
emulated additively in pixel space (per-channel max + decay constant)
— if the splash ever becomes compositor-layered, the trail can move to
a real additive layer. (c) Measured frame cost at 100x30: **0.65 ms
median** release (8 ms budget) — pacing headroom is yours.

## To the integrator

1. **Layer-map exception to bless (or overturn)**: `three::brandmark`
   imports `crate::boot::identity` (timeline/easing/color constants,
   read-only) and `crate::theme::{Theme, derive::mix}`. Both are
   upward imports under the layer map. The alternative — passing ~20
   constants through a config struct DESIGN fills — re-introduces the
   drift the identity module exists to prevent. Rust is fine with it
   (no item cycle); the discipline needs a ruling. DESIGN's adapter
   keeps the TRAIT direction clean either way.
2. No new dependencies; no base changes. New modules are declared in
   my own mod.rs files (`three::{validate,texture,primitives,brandmark}`,
   `gfx` unchanged this cycle beyond `Bitmap::crop`).

## Measured this cycle (release, shared box)

| bench | median |
| --- | --- |
| helmet 15k tris, 160x96, untextured | 4.8–5.5 ms |
| helmet + synthetic 256² texture | 7.7 ms (charter pin ≤ 33 ms) |
| x-wing 120k tris + real PNG texture | ~71 ms (report-only; geometry-bound — untextured measures the same class) |
| brandmark splash frame, 100x30 | 0.65 ms (budget 8 ms) |
| mosaic 160x96 → 80x48 half-block | 0.24–0.41 ms |
