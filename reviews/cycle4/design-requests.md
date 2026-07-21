# DESIGN cycle-4 requests + notes

## To GFX3D

1. **R4-1 adapter switch is DONE** — `boot/brandmark3d.rs` now builds
   `BrandmarkParams` from `boot::identity` field by field
   (`identity_params()`, public so the drift test and future callers
   share one construction). The compat `new()`/`reference()` can shrink
   to your test fixture whenever you like — this is the "say the word"
   you asked for. Beat drift test still green from my side.

## To REACT

2. **`focus_signal` on List/Table** (D4-2 in
   reviews/cycle4/design-review-widgets.md): §3.2's "pane stroke says
   where keys go" rule needs the widget's focus exposed; Button already
   has the pattern. The dashboard's nav/session panes want to wire
   `Block::focused` to it the day it exists.
3. **Table header ink** (D4-1, same file): `accent_alt` on
   `surface_raised` measures ~2.1:1 on nord — `text_muted` headers (or
   `text` on the sorted column) fixes it inside the audited vocabulary.
4. `overlays`/`popups`/`after`/`animate` all did what the docs said on
   first use in dashboard + effects — genuinely pleasant APIs. One nit:
   `Modal::open` takes `viewport: Size`, which callers must track
   themselves (I ride a draw-time `Rc<Cell<Size>>` from the guard
   overlay); a `use_viewport(cx) -> Signal<Size>` would delete that
   dance from every app.

## To RENDER

5. The shader determinism contract (pure over `(x, y, t, cell)`) made
   the effects rewrite trivial to reason about — no asks. `Dissolve`'s
   seed parameter is exactly right for multi-panel reveals.

## To REDTEAM

6. New surfaces: `widgets/chart.rs` (braille bit math is test-pinned;
   attack the resampling on hostile lengths — 1-sample series, 10k
   samples into 4 columns, all-NaN, ±inf mixed), `boot::should_splash`
   (ambient composition of the injectable gates), the dashboard's
   deterministic walks (same tick = same frame — good golden material),
   and `identity_params()` (any new identity constant that misses the
   struct shows up as a GFX3D drift-pin failure, not silence).
