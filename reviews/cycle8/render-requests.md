# RENDER — cycle 8 requests + notices (API freeze)

## To REACT

1. **`StyledCanvas` unified — the render-side duplicate is gone.**
   `render::bridge` declared a second `StyledCanvas` (same name, drifted
   signature: by-value `Style`, char-less `fill_styled`) that only
   `Surface` implemented and nothing consumed. Deleted; `ui::StyledCanvas`
   is THE styled trait, and `Surface` now implements it directly with
   full fidelity (grapheme-correct print, patch-semantics fill, space
   fast path — byte-identical behavior to your `SurfaceCanvas` wrapper).
   Consequence for you: `&mut Surface` now coerces to
   `&mut dyn StyledCanvas`, so `SurfaceCanvas` is redundant for the
   plain-wrap case — keep it or retire it, your call (no urgency; both
   routes end in `draw_text`).
2. FYI, freeze surface: `render::StyledCanvas` re-export removed (zero
   consumers, verified by rg across src/ + examples/ + tests/).

## To DESIGN

1. **Paint helpers still have zero consumers** (`fill_gradient`,
   `GradientSpec`, `drop_shadow` — your C6 effects-round-out ask).
   Kept public through the freeze because headers/shadows are
   docs-worthy app polish, but cycle 9 is the demo cycle: claim them in
   a demo or they get pruned to crate-private in the docs cycle rather
   than documented forever.
2. `MdStyles::with_ink` + the terse `Style` builders (`.bold()`,
   `.dim()`, ...) are live — your `md_styles(t)` widget mapping can
   collapse to one `with_ink` call if you want the shared path.
3. `render::snapshot_styles(&surface)` exists now — per-row style-run
   annotations (fg/bg/attrs/link). Handy for theme debugging ("why is
   this chip not raised") without a VT replay.

## To REDTEAM

1. New adversarial surface: `render::snapshot`/`snapshot_styles` are
   DIAGNOSTIC (documented never-in-goldens). If you see them creep into
   byte-golden tests anywhere, that is a bug by contract.
2. The freeze made `text::is_risky_cluster` crate-private (it was the
   presenter's cursor defense, not a public width oracle). Your
   adv_unicode suites consume `width`/`cluster_width`/`segments` — all
   unchanged.

## Notices

- Rustdoc: missing-docs in render/text/anim went 207 → 0; zero doc-link
  warnings; 7 compiling doctests added on the six most-used entry
  points (all green in `cargo test --doc`).
- Style gained `.bold()/.dim()/.italic()/.underline()/.strike()/.reverse()`
  shorthands (equivalence to `.attrs(...)` test-pinned); `Surface` docs
  name `draw_text` as the canonical direct-draw call vs the widget
  canvas vocabulary.
- Full lib suite 901 green, doctests 27 green, alloc pins 8/8 green
  (serial), clippy zero in my files. `tests/adv_splash.rs` was mid-write
  (foreign, E0639) at my close — everything else compiled and passed.
