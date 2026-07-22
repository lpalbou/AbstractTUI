# 0410 — Feature-gate the heavy in-tree modules (`three`, `jpeg`, `proto`)

## Metadata
- Created: 2026-07-22
- Status: Proposed (v1-able once 0400's ADR rules; integrator-gated —
  Cargo.toml is integrator-owned, Cargo.toml:18)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: 0400's ADR (packaging ruling — this item is its
  first execution); ADR-0001 (features must stay additive; default-on
  gating is NOT a breaking change for default builds, and
  `default-features = false` is a new, documented opt-in surface).
  ADR impact: none beyond 0400's.

## Context
The compile-trim half of the modularity brief: let a minimal app (a
form, a dashboard, a log tail) stop paying for the 3D stack, the JPEG
decoder, and pixel-protocol encoders it never calls. All three are
default-on so nothing changes for anyone who does not ask; the trim is
an explicit `default-features = false` opt-in. This is the cheap half
of modularity — no new crates, no release-cadence coupling — and it is
bounded: `gfx` core (bitmap/mosaic/png) and `render` stay
unconditional; they are the content path every app uses.

## Current code reality
Severability was mapped by grepping production `use crate::` edges
(test-only imports are behind `#[cfg(test)]`, e.g.
src/three/load.rs:727-730, src/gfx/session.rs:483-485):
- **`three` (~8.2k shipped lines)** — production consumers are exactly
  `src/widgets/viewport3d.rs` (560 lines, the widget) and
  `src/boot/brandmark3d.rs` (the 3D splash source). The splash already
  degrades: `SplashFrameSource` is a trait (src/boot/player.rs:54) and
  `play_fallback` runs the 2D identity with no 3D anywhere
  (src/boot/player.rs:427-436; src/boot/fallback2d.rs). `three`'s own
  production imports are `base` + `gfx` (+ anim/boot/render/theme in
  `brandmark.rs` only). Gate = `#[cfg(feature = "three")]` on
  `src/lib.rs:45`, the two consumers, and the prelude re-export
  (src/prelude.rs:35, `Viewport3D`).
- **JPEG decoder (~1.1k lines: jpeg.rs 599, jpeg_dsp.rs 182,
  jpeg_entropy.rs 300)** — reached solely via magic-byte routing in
  `decode_image` (src/gfx/decode.rs:58-67). The feature-off behavior
  already has its pattern in-tree: unknown formats reject by NAME,
  telling the caller what does decode (decode.rs:62-66). Feature-off
  JPEG = the same named rejection ("compiled without the `jpeg`
  feature"), never a silent format hole. `sniff_format` stays
  unconditional (it is how the error can name JPEG).
- **Protocol encoders (~0.9k lines: kitty 333, sixel 482, iterm2
  110)** — pure `Bitmap -> Vec<u8>` (src/gfx/proto/mod.rs:1-7), but
  consumed by `gfx::session::ImageSession` (585 lines) and the app
  driver (src/app/driver.rs:28, src/app/driver_images.rs:8,94-104).
  Gating them means the capability ladder ends at mosaic
  (src/gfx/mosaic.rs:2-4 documents the ladder: kitty > iterm2 > sixel
  > mosaic) — an honest degradation that must surface through the
  existing labeled-fallback discipline, not silently.
- **NOT gateable**: `gfx` bitmap/mosaic/png/decode (widgets::Bitmap
  re-export src/widgets/mod.rs:45; Image widget; overlays
  src/app/overlays.rs:35; the universal mosaic fallback), `boot`
  (Logo widget uses boot::identity, src/widgets/logo.rs:20).
- **Cross-track precondition (control-plane 0320)**: the hand-rolled
  JSON parser lives INSIDE `src/three/` (`gltf_json`, exported at
  src/three/mod.rs:22) and the control-plane band plans to consume it
  from core. The promotion to a neutral home (`base::json` or
  equivalent) must land BEFORE or WITH whichever of {this gate, 0320}
  ships first — otherwise either `three` becomes ungateable (server
  depends on it) or the gate strands the server's parser. The old
  `three::gltf_json` path stays re-exported UNDER the `three` feature.
  (Recorded in reviews/study/extensions-on-platform.md P1-2.)
- The widget lint list is `include_str!`-pinned per module file
  (src/widgets/mod.rs:123-148) and the membership test walks `pub mod`
  lines (mod.rs:171-191) — cfg'd modules need the lint arrays cfg'd
  consistently or the lint breaks in trimmed builds.
- Test topology: `tests/` has 3D/gfx-specific suites (adv_raster.rs,
  adv_gltf_anim.rs, adv_proto.rs, adv_jpeg.rs…) that must be
  feature-gated or they fail the trimmed matrix.

## Problem
Every downstream build compiles ~10k lines of code most apps never
execute; there is no supported way to opt out, and no CI evidence that
a trimmed engine even builds (feature matrices rot without a gate).

## What we want
1. Three default-on features: `three`, `jpeg`, `proto` (names final at
   0400). `default = ["three", "jpeg", "proto"]` — default builds are
   byte-identical in behavior.
2. Honest feature-off behavior at every runtime seam:
   - JPEG off: `decode_image` rejects JPEG magic by name, naming the
     missing feature (extends decode.rs:62-66's message discipline).
   - proto off: `choose_channel` never selects a pixel protocol; the
     ladder tops at mosaic with the existing labeled-degradation
     wording (mosaic.rs:36-51's `#FALLBACK`-labeling precedent).
   - three off: `Viewport3D` absent (compile error, not runtime);
     splash runs `play_fallback` unconditionally.
3. Prelude compiles under every combination (cfg'd re-exports).
4. CI matrix: `--no-default-features`, each feature alone, and default
   — build + the feature-appropriate test suites per combination.
5. Measured numbers replace 0400's estimates: `cargo build --timings`
   and binary-size deltas for default vs trimmed, recorded in this
   item's completion report and in docs (README feature table).
6. Docs: a "Features" section in README + docs/getting-started.md;
   every gated capability's doc page states its feature.

## Scope / Non-goals
Scope: the three gates, honest off-behaviors, lint/test/prelude/CI
consistency, measurements, docs. Non-goals: gating `gfx` core,
`render`, `boot`, or any widget beyond `Viewport3D`; feature-based
behavior changes (forbidden by the track non-goals); a `minimal`
meta-feature (YAGNI until a consumer asks); touching Cargo.toml without
integrator sign-off.

## Expected outcomes
A minimal app opts out in one manifest line and gets a measurably
faster build and smaller binary; the default experience is unchanged;
the trimmed matrix is CI-proven so it cannot rot.

## Validation
- CI matrix green on all feature combinations (build + suites).
- Behavior pins: JPEG-off named rejection; proto-off ladder ends at
  mosaic with label; default build passes the full existing suite
  untouched.
- Measured build-time/binary deltas recorded (replacing the estimates
  in 0400).
- `cargo doc` builds per combination (broken intra-doc links to gated
  items are the classic failure).
- The 0170 public-api diff gate runs PER FEATURE COMBINATION once it
  exists (peer note P3-13): cfg'd prelude re-exports make the public
  surface combination-dependent — exactly the drift that gate watches.

## Progress checklist
- [ ] 0400 ADR ruled; feature names final
- [ ] Integrator sign-off on Cargo.toml changes
- [ ] `three` gate (module, viewport3d, brandmark3d, prelude, lints)
- [ ] `jpeg` gate (decode routing + named rejection)
- [ ] `proto` gate (session/pipeline/driver seams + labeled ladder top)
- [ ] Test-suite feature mapping + CI matrix
- [ ] Measurements + docs (README feature table)
