# DESIGN cycle-2 requests (cross-module needs)

Carry-overs acknowledged: RENDER is landing underline-color attrs +
`Ease::bezier` semantics + joint downlevel mapping; REACT the theme signal
+ context; KERNEL `is_tty` + env-pass caps. Thanks — cycle-1 requests 1-5
are all in motion. New/refined asks below.

## To RENDER

1. **`Capabilities -> PresentCaps` conversion (re-file of cycle-1 #3
   footnote).** `boot::player::splash_present_caps` is a boot-scoped
   stopgap mapping env-pass caps to `PresentCaps` (truecolor/256/16 +
   sync_2026, everything else conservative-false). When your official
   conversion lands, the stopgap should delegate or die — one mapping,
   one owner. Note `undercurl`/`underline_color` are not env-detectable
   today; if the active probe learns them, the conversion is where that
   knowledge belongs.
2. **`Surface: ui::Canvas` impl** (REACT filed this in cycle 1; +1 from
   widgets): my widgets draw through `dyn Canvas` and are tested on
   `BufferCanvas`; the moment Surface implements Canvas they run on the
   real pipeline unchanged — and the fallback splash can drop its direct
   Surface coupling too.
3. **Attrs through Canvas**: `Canvas::put/print` carry `(char, fg, bg)`
   only — no BOLD/UNDERLINE/attr path, so `Badge` cannot bold its label
   and `Logo` cannot bold the wordmark. Either a `put_styled(Point, char,
   render::Style)` on Canvas or a style-carrying print variant. Not
   urgent (color carries the hierarchy today), but the widget set will
   want emphasis before the dashboard example.

## To REACT

4. **Theme signal wiring point**: registry + runtime registration are
   ready (`theme::get/resolve/themes` return `'static`; `theme::register`
   is audited at runtime per RT1-9a). When the app-level
   `Signal<&'static Theme>` lands, please point widgets' docs at ONE
   canonical read pattern (context lookup) so widget examples stay
   uniform; happy to co-write the `themes_gallery` example as its
   acceptance test (cycle 3).
5. **Widget builders return `Element`**: `Block/Badge/Progress/…::element
   (&TokenSet) -> Element` so callers attach handlers before `.build()`.
   If the component model wants a different convention (e.g. everything
   returns `View`), tell me this cycle while the surface is still small.

## To KERNEL

6. **`Terminal::is_tty(&self) -> bool` (or equivalent)** on the trait or
   the concrete terminals: the splash gate needs the ttyness of the REAL
   render handle (RT1-10c). Today the example uses "UnixTerminal::new()
   succeeded" as the proof, which is correct but only by construction —
   an explicit accessor is the honest surface. `TermRead::Wake` handled
   in the splash adapter (treated as idle during boot; posted work drains
   when the real loop starts) — flag if boot should do more.

## To GFX3D

7. **The 3D frame source seam is frozen**: `boot::player::
   SplashFrameSource::render(&mut self, t_seconds, size, theme) ->
   &Surface`. The player owns pacing/skip/cutoff/fade; your source owns
   pixels only. `t` arrives in wall-clock seconds and may repeat or jump
   forward (frame drop) — keep the source stateless over `t` (the 2D
   fallback in `boot/fallback2d.rs` is the reference implementation).
   Fade is a player-side post-process; don't implement your own.

## To REDTEAM

8. **Pinned surfaces ready for attack**: `theme::register` strict/labeled
   paths (RT1-9a), `boot::player` pacing (virtual-clock tests in-module —
   `VClock` scripts to reuse), the widget no-color-arithmetic lint
   (`widgets::lint_tests`). The everforest-light exception survived the
   cycle-2 elimination attempt (both colors verbatim ports); the doc
   records the reasoning if you want to contest it.
