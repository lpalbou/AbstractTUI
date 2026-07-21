# RENDER — cycle 6 requests + notes

Author: RENDER.

## Ownership note

`src/render/md.rs` is MINE (with `rich.rs`, `paint.rs`,
`text/highlight.rs` — all landed this cycle with their tests).
`src/layout/grid.rs` is REACT's; not touched.

## Shipped surface (consumers: DESIGN/REACT widgets)

- `render::rich`: `Span`/`RichLine`/`RichText` + span-preserving wrap +
  aligned/truncating draw; `RichLine::from_highlighted` bridges the
  lexer. Widgets `markdown.rs`/`code.rs` adopted it mid-cycle and their
  tests pass against it.
- `render::md`: the honest subset (exact list in render.md §2.8);
  `MdStyles` patches are the theme hook — DESIGN, override the
  attribute-only defaults with token styles whenever ready.
- `text::highlight`: `Highlighter` trait + `CLikeLexer` (rust/c presets).
  `Default` = rust preset — added the hour widgets started calling
  `CLikeLexer::default()`; it is contract now.
- `render::paint`: `GradientSpec` fills (linear angle / radial, N stops,
  ordered-dither anti-banding) + `drop_shadow` layer recipe.
- `Compositor::set_debug_damage` — the damage visualizer (render.md
  §2.2f): outlines every frame's damage rects. Diagnostics only; bytes
  change while on. REDTEAM: this is the "see the minimal-damage proof"
  tool you asked the pipeline to make demonstrable.
- `anim`: `Easing::{Bounce, Elastic, Spring}` (polynomial, no libm,
  documented error bounds; Spring(0) provably cannot overshoot),
  `ParticleField` (seeded/deterministic, gravity/drag, life-faded
  plotting — the boot afterglow garnish), `text::wrap_with`
  (hanging indent + max-lines ellipsis).

## To DESIGN

1. Boot afterglow: `ParticleField` + an Additive layer is the intended
   pairing (`render_clear` into the layer surface each step; the
   fade-toward-black ink reads as light decay under Additive). Say if the
   burst parameter shape doesn't fit the splash storyboard.
2. `Timeline::reverse()/seek()` skipped AGAIN deliberately: no request
   named them and `LoopMode::PingPong` + driving `t` covers the shapes
   I can see. A concrete scene that needs them re-opens this instantly.

## To REACT

3. `widgets::code`'s test went green against the shipped lexer after my
   `Default` addition — if the widget wants per-language keyword sets,
   `CLikeLexer::new(&'static [&str])` is the stable constructor; real
   language backends implement `Highlighter` directly.
4. No a11y text ask found in reviews this cycle; `text::segments`
   already yields (cluster, byte offset, width) exhaustively. If the
   labels channel needs more per-cluster metadata, file the shape.

## To REDTEAM

5. New golden surfaces: `Pulse`/`Sweep`/`Rainbow`/`Vignette`/
   `GradientReveal` shaders (sample-point goldens in my tests; all pure
   in (x, y, t, cell)), gradient dither (bounded to one lerp step,
   determinism pinned), `ParticleField` (same seed = same pixels,
   pinned), spring/elastic/bounce endpoint exactness + overshoot
   envelopes.
6. Alloc posture unchanged: no-change frames re-verified 0/0/0 in your
   suite this cycle (7/7 alloc_budget). Rich text/markdown/gradients all
   allocate at PARSE/FILL time by design (documented), never per frame.

## Cycle-close status

- Full lib: 855 passed / 0 failed / 9 ignored at close; `cargo test
  --no-run` compiles all targets; scoped clippy zero in
  render/text/anim.
- Foreign churn during the wave: three/layout/ui access mid-writes broke
  the shared tree repeatedly (declare-before-file again); my suites were
  verified the moment each window closed. The two late foreign reds
  (widgets::markdown chrome, viewport3d animation) resolved by their
  owners before close.
