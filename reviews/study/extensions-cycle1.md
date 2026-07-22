# Extensions study — cycle 1 (2026-07-22)

Scope: the modularity/extensions slice of the roadmap study. Output:
`docs/backlog/proposed/extensions/` (README + 0400-0470, band
0400-0490). Constraint honored: no engine code, no crate modification;
compile impact estimated from source volume and the dependency graph,
labeled as estimate throughout (0410 measures for real when executed).
Peer bands cross-referenced, never written: 0300-0390 (control-plane),
0500-0590 (app-kits).

## Method

Every claim below was read at source this cycle: Cargo.toml (no
`[features]`; integrator-owned manifest, line 18), src/lib.rs (16
unconditional modules), the per-module `use crate::` production edges
(test-only imports verified `#[cfg(test)]`), chart.rs's private
BrailleGrid, mosaic/mosaic_fit, ui::Canvas/StyledCanvas, ui::view
(Element/style_signal/draw), ui::event (capture, hover), layout
Position::Absolute, app::overlays, render::md + widgets::markdown +
widgets::feed, render surface/cell link APIs, boot player seams,
three::mod, gfx::decode/proto, prelude; ADRs 0001-0003; the roadmap
0001 and overview; mdpad's README, markdown model, and
render/mermaid.rs.

## Load-bearing findings

1. **The severability map is better than expected.** `three` (~8.2k
   shipped lines, ~11-12% of the crate) has exactly two production
   consumers (widgets/viewport3d.rs, boot/brandmark3d.rs) and the
   splash already owns a 2D degradation seam (SplashFrameSource,
   play_fallback — boot/player.rs:54,427-436). The JPEG trio (~1.1k
   lines) hides behind one magic-byte router whose named-rejection
   error is the feature-off pattern already (gfx/decode.rs:58-67).
   Protocol encoders (~0.9k) are severable but cut into
   ImageSession/driver — deeper cfg surface. `gfx` core and `boot` are
   NOT severable (widgets::Bitmap re-export, Image widget, overlays,
   Logo→boot::identity).
2. **Estimated trim** (labeled estimate; no-dep crate ⇒ build time
   tracks own code volume): `three` off ≈ 10-13% full-build saving;
   three+jpeg+proto ≈ 15%; binary .text order 100-400 KB at opt 3.
3. **The canvas layer is a promotion, not an invention.** chart.rs
   already contains the dot grid, Bresenham, braille assembly, AND the
   per-cell color rule ("one dot grid per series so colors never merge
   in a cell", chart.rs:326-328) — all private. mosaic_fit's braille
   tables serve a different job (least-squares image fit) — share
   glyph tables, not fitters. Missing entirely: curves (bezier/arc).
4. **The graph editor's interaction substrate exists.** Absolute
   positioning (layout/style.rs:95-101), pan-without-remount via
   style_signal negative insets (the Scroll technique, scroll.rs:1-9),
   pointer capture (ui/event.rs:237-247), per-node hover
   (ui/event.rs:117-124), overlay tooltips (app/overlays.rs:158-229).
   Genuinely missing: strokes (0420), stroke hit-testing (0165's
   link-id path is the cheap answer: stamp edge cells with app URIs —
   render/cell.rs:310, surface.rs:125-131), and any layout algorithm.
   Zoom cannot be continuous in a cell grid — discrete LOD tiers only.
5. **Layout scoping.** At terminal resolution (a 200x60 screen is
   ~400x240 braille dots; node cards 10-30 cells wide) full Sugiyama
   is quality invisible at this scale: v1 = longest-path ranking +
   bounded median/barycenter sweeps + labeled grid-snap fallback,
   deterministic. Force simulation conflicts with the inviolable
   zero-idle principle unless bounded and on-demand — optional,
   research, not v1.
6. **Mermaid economics.** mdpad already litigated in-terminal mermaid
   and rejected it for a lean single binary (mdpad
   src/render/mermaid.rs:1-14) — the extension family exists to
   amortize exactly that "multi-thousand-line subsystem" across apps
   (0440 builds the layout once). The subset must be a written table
   with atomic per-diagram fallback to the code fence (which the core
   parser already isolates: render/md.rs:80-85) — partial rendering
   of a half-understood diagram misleads.
7. **mdpad parity gaps are core-vocabulary gaps.** AbstractTUI's md
   subset excludes tables, images, HTML by documented design
   (render/md.rs:14-17); mdpad's model has them (mdpad
   markdown/model.rs:29,54-84). Four new core seeds (md tables, md
   images, heading anchors/TOC, search-highlight overlay) belong in
   the app-widgets band — handed to the integrator, not numbered from
   this track.
8. **Web rendering.** Full = never (charter, posture, scope). The
   defensible slice is readable-mode HTML → the markdown block
   vocabulary, as an extension, gated on four promotion criteria
   (named consumer, 0460 gaps landed, 0400 executed, fuzz owner). The
   honest cost center is the tolerant parser (~1-2k lines by analogy
   with the hand-rolled JSON at three/gltf_json.rs), not rendering.

## The architecture recommendation (0400)

Hybrid, with a gate:
- **Cargo features (default-on) trim in-tree weight**: `three`,
  `jpeg`, `proto` (0410). Features stay additive; feature-off runtime
  seams degrade with named errors/labels (house discipline).
- **Sibling crates carry new domains**: `abstracttui-graph`,
  `abstracttui-mermaid` (0430/0440/0450); public-API-only ("widgets
  have no private engine privileges", widgets/mod.rs:5-6, promoted to
  a family contract); in-repo workspace so CI builds the family
  against core HEAD.
- **Gate**: the family launches only after the 0.2 API-stability pass
  (0170) executes — ADR-0001's batched breaks are the coupling
  budget, and extensions born before the audit are born into churn.
- Needs the repo's fourth ADR; Cargo.toml/workspace changes are
  integrator acts (Cargo.toml:18).

## Open questions (hardest first)

1. **Extension coupling economics in the 0.x era**: is the
   in-repo-workspace + breaking-budget mitigation actually enough, or
   should sibling crates wait for 0.3/1.0-grade core stability? The
   0170 gate is the study's answer; it deserves adversarial review.
2. **Where the canvas layer lives** (render::vector proposed) and
   whether its blit contract (one color per grid, z-order by blit)
   stays ergonomic once mermaid needs per-edge colors at scale — or
   whether per-cell fg override during blit is needed (cost: the
   documented cell-color rule gets subtler).
3. **Edge hit-testing granularity**: is cell-level link-id resolution
   (0165 synergy) precise enough for dense diagrams, or does the
   graph crate need dot-level distance queries as the primary path
   (and if so, does that math belong in 0420 core)?
4. **Does the five-crate dependency posture bind extension crates?**
   (0400 must rule; 0450/0470 assume yes and hand-roll parsers.)
5. **Sequence-diagram value vs flowchart-only mermaid v1** — sequence
   layout is solverless (columns/rows) and cheap, but is it the
   second-most-used diagram in the target corpus? Corpus evidence
   should decide before 0450 schedules.

## Handoffs

- Integrator: fold the 0400-0490 ledger into docs/backlog/overview.md
  (deliberately not edited here — three parallel study tracks); the
  four 0460 core-gap seeds need app-widgets-band numbers; Cargo.toml
  ownership means 0410 cannot start without integrator sign-off.
- Peers: 0400's packaging ruling is written to cover the app-kits
  band (0500-0590) and any control-plane split (0300-0390) — review
  it from your bands' needs before it goes to ADR.
