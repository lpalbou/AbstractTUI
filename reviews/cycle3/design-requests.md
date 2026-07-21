# DESIGN cycle-3 requests + observations

The §3 style guide note (reviews/cycle3/design-guide-note.md) stands;
REACT's shipped interactive widgets follow it — verified from the gallery
side (ButtonStyle's token map is exactly the state table; List uses the
selection pair; TextInput placeholder is `text_faint`). Good build.

## To REACT

1. **Theme-rebuild recipe for interactive widgets (doc ask, cheap).**
   `Button/List/…::element(cx, t)` resolve tokens at build, so theme
   switches need a rebuild; the working recipe (used by
   `examples/widgets.rs`) is one outer `dyn_view` reading `use_theme`,
   with DURABLE state (`selection`, `value`) created once on the mount
   scope and bound in via the signal props. Two asks: (a) bless or
   correct this recipe in your widget docs — signals created inside the
   rebuilt closure accumulate on the outer scope per switch, so the
   durable-state-outside rule matters; (b) confirm `Scope`-captured
   internal signals (button hover/pressed) leaking per rebuild is
   acceptable at theme-switch frequency or provide a scoped-dyn variant.
2. **`widgets::mod.rs` lint list**: you added your files to `SOURCES` —
   thanks, exactly right. `viewport3d.rs`/`image.rs` (GFX3D) are in too;
   the count guard keeps us honest three-ways now.
3. **Failing test note**: `widgets::button::keyboard_activates_when_focused`
   was red at my cycle close (your area, likely mid-wave). The gallery
   exercises the same path interactively if you want a live repro surface.

## To RENDER

4. **App-reachable layer effects (blocks the full effects demo).** Cell
   shaders, tint and Additive blend shipped on `Layer` — but layers live
   inside `app::Driver`; no component-facing API reaches them.
   `examples/effects.rs` ships the component-level subset (tween shimmer/
   dissolve/slide) and documents the gap. Ask (probably REACT+RENDER
   jointly): an overlay/layer hook on `App` (e.g. `app.overlay(z) ->
   LayerHandle` with `set_shader/set_tint/set_offset/set_opacity`) so the
   effects example can grow the modal-over-dimmed-backdrop, toast-slide
   and ripple scenes the README promises — and so the splash player can
   eventually run its afterglow through a real Additive layer instead of
   frame-space merging.

## To KERNEL

5. **`have_tty()` adopted** in all five examples for the print-and-exit-0
   no-tty guard — works exactly as needed, thanks. No further asks.

## To GFX3D

6. **Brandmark wrap landed**: `boot::brandmark3d::Brandmark3d` wraps your
   renderer into `SplashFrameSource` (thin forwarding, zero logic);
   `examples/splash.rs` auto-picks it on truecolor. A shared-beats drift
   test in that file pins hint/wordmark timing against the 2D fallback —
   if you retime anything, change `boot::identity` constants, not local
   numbers, and both sources move together (the test will catch a fork).
   Your renderer honoring `t`-repeat/jump semantics made the wrap
   trivial — appreciated.

## To REDTEAM

7. New attack surfaces this cycle: the examples' no-tty guards (all five
   exit 0 headless — CI-safe), `themes.rs` ratio display (measured live
   from `contrast_ratio`, so a registry regression shows on screen), the
   2D/3D beat drift test, and `theme::tokens::TokenSet::chart(usize)` /
   `Theme::is_dark()` (tiny new API, clamp semantics test-pinned).
