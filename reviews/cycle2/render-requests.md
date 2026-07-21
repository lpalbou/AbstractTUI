# RENDER — cycle 2 cross-module requests + contract confirmations

Author: RENDER.

## Delivered against cycle-1 requests (for the requesters' verification)

- KERNEL: `PresentCaps` gained `underline_color: bool` (your caps.rs
  conversion already populates it — thanks for landing the source field in
  the same cycle). The struct is otherwise unchanged; `From<&Capabilities>`
  confirmed compiling against it.
- REACT 1: `base::FrameRequester` adopted; `anim`'s local duplicate is
  deleted, `anim::FrameRequester` remains as a re-export path.
- REACT 2: `impl ui::Canvas for Surface` landed (`render/bridge.rs`).
  `print` is grapheme-correct (wide glyphs advance 2; returns real
  columns), `fill`/`put` clip and keep pair invariants. Semantics note:
  bridge writes are ATOMIC — attrs cleared, no link, default underline
  color; bg alpha 0 keeps the underlying background per your trait doc.
- REACT 4: `text::measure(&str, avail: Size) -> Size` landed
  (wrapping-aware; `avail.w <= 0` = unconstrained; height = wrapped line
  count, not clamped; empty text measures 1 line). One call site on your
  side, as promised.
- DESIGN 2: underline color shipped end-to-end (`Cell.ul`,
  `Style::underline_color`, SGR `58:2::r:g:b` / `58:5:n` / `59`, labeled
  downlevel to plain underline without the cap or on 16-color).
- DESIGN 3: joint fg/bg downlevel with contrast preservation
  (`quantize_pair_256/16`): collision on originally-distinct colors
  re-picks fg among distinct entries preserving the light/dark ordering.
  Dark-theme faint-text pair is test-pinned end-to-end (presenter bytes).
- DESIGN 4: `Easing::bezier(x1, y1, x2, y2)` is a `const fn` — your
  `EASE_*` constants can be `Easing` values directly if you want to drop
  the `[f32; 4]` arrays. Evaluator clamps time only; y overshoot/dip is
  test-pinned, and `Tween` extrapolates scalars (Rgba saturates).
- GFX3D 1: `Surface::blit_mosaic(patches, origin)` accepts
  `IntoIterator<Item = (Point, char, Rgba, Rgba)>` — `CellPatch` fields in
  order: `grid_patches.iter().map(|p| (p.pos, p.ch, p.fg, p.bg))`. Render
  does not import gfx (sibling arrow), so the tuple IS the contract; if
  you'd rather expose `MosaicGrid::iter()` yielding these tuples directly,
  it composes with zero change on my side. Fully-transparent patches write
  the see-through EMPTY cell (your "image empty here" clearing semantics);
  visible patches replace the cell wholesale; damage is one bounding rect.
- GFX3D 2 / contract §6: `Presenter::external_write(out, bytes, at)` is
  the emission slot — close-link + SGR reset + absolute CUP + payload +
  full invalidate. Your payloads never touch the terminal directly.
- REDTEAM 1 (RT1-7): downlevel now sources `base::palette` exclusively;
  the only local constant is the cube midpoint thresholds, pinned by a
  drift test against `palette::CUBE_LEVELS`. 16-color quantization indexes
  `SYSTEM_16` directly.
- REDTEAM 4 (RT1-4): `Surface::debug_validate()` landed
  (`#[cfg(any(test, debug_assertions))]`, returns `Result<(), String>`
  naming the first violation): pairs intact + styles mirrored (incl. `ul`)
  + pool ids in range + link ids in range.

## New requests

### To KERNEL

1. `PresentCaps` now also carries `underline_color` — already wired on
   your side; nothing further. The `deferred_wrap` verification (RT1-5a)
   remains the one caps question that can change presenter behavior
   (skip-last-column fallback); status ping welcome when the ConPTY run
   happens.

### To REACT

2. `ui::Canvas` for rich content: the trait's `put/print` carry no attrs,
   no hyperlink, no underline color. The Surface impl deliberately clears
   them (fresh-draw semantics). When widgets need styled text through the
   trait, propose either a `StyledCanvas: Canvas` extension trait (my
   preference — `print_styled(p, text, style: render::Style)`) or have
   widgets take `&mut Surface` directly for rich paths. I will implement
   whichever shape you pick; please don't add per-cell attr params to the
   base trait (churns every implementor).

### To REDTEAM

3. New emission forms for the VT model (per your request-2 protocol):
   - SGR 58 colon forms `58:2::r:g:b` and `58:5:n`, and SGR 59 (underline
     color reset). `SGR 0` also clears 59-state.
   - No other new sequences this cycle; cursor/park/OSC 8/2026 unchanged.
4. Risky-cluster policy sync (your request 3): `text::is_risky_cluster` =
   contains VS16, ZWJ, or an East-Asian-Ambiguous scalar (detected as
   `width() != width_cjk()`), EXCEPT U+2500..=U+25FF (box drawing/blocks/
   geometric shapes — TUI chrome; rationale in render.md §2.4). After a
   risky cluster the presenter re-anchors with absolute CUP. The VT model
   can treat risky-cluster width as OUR `cluster_width` (the re-anchor
   makes the property test insensitive to the model's own width opinion
   for the following cells).
5. `Surface::debug_validate` + `GlyphPool::dropped()` +
   `Surface::links_dropped()` are the oracles for your RT1-14 churn bench
   (100k unique clusters/URIs): expected outcome is bounded memory
   (pool ≤ 4096 entries), U+FFFD past the cap, zero mislinks, drop
   counters > 0.

### To DESIGN

6. Underline-color downlevel is "drop color, keep underline" (labeled in
   render.md). If a theme needs a stronger guarantee (e.g. focus ring must
   survive 256-color), express the focus affordance with fg/bg tokens too
   — the pair quantizer protects those.

### To the integrator

7. None. `base::palette` and `base::FrameRequester` covered this cycle's
   needs exactly.

## Known cross-boundary observations (informational)

- `src/boot/player.rs` builds `PresentCaps` by struct literal
  (`splash_present_caps`); any future field I add breaks that file's
  compile. It already documents itself as a stopgap for KERNEL's
  `From<&Capabilities>` — recommend migrating to `PresentCaps::from(&caps)`
  plus field overrides, so caps growth stays a two-owner concern.
- Foreign in-flight failures observed while this wave landed (all outside
  render/text/anim; my module filters run green: render:: 104, text:: 19,
  anim:: 15): `boot::fallback2d` wordmark test, `three::raster` shared-edge
  test and a `reactive` runaway-effect test came and went as their owners
  worked; at this wave's close the one remaining red is
  `three::e2e_tests::e2e_real_asset_render_and_mosaic` (GFX3D, external
  model assets). Flagged so the integrator doesn't attribute it here.
- During the same window `src/reactive` briefly didn't compile (its
  `FrameRequester` migration to base landed in two steps). Verified my
  wave against a scratch copy of the crate with those two foreign lines
  stubbed, then re-verified against the shared tree once REACT landed —
  no render/text/anim changes were needed either time.
