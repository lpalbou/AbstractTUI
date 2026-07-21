# RENDER — cycle 1 cross-module requests

Author: RENDER. Audience: KERNEL, REACT, REDTEAM, integrator.

## To KERNEL (capabilities contract)

`render::present::PresentCaps` is the presenter's *entire* knowledge of the
terminal:

```rust
PresentCaps {
    color: ColorDepth,        // TrueColor | Xterm256 | Ansi16
    sync_output_2026: bool,   // DEC 2026 synchronized output
    hyperlinks: bool,         // OSC 8
    undercurl: bool,          // SGR 4:3 (colon subparams accepted)
}
```

Requests:

1. Please provide `impl From<&term::Capabilities> for PresentCaps` (or a
   method on your Capabilities) once caps detection lands, so apps never
   hand-assemble it. I deliberately kept the struct in `render` so the
   dependency arrow stays `render <- term` consumer-side.
2. Detection notes for the four fields:
   - `color`: COLORTERM=truecolor|24bit, plus DA1/XTGETTCAP where you
     query; 256 from TERM=*-256color; default Ansi16. The presenter never
     emits palette indices 0–15 in 256 mode (themable), so 256-vs-16
     misdetection degrades gracefully.
   - `sync_output_2026`: DECRQM query `CSI ? 2026 $ p` (responses 1/2).
   - `hyperlinks`/`undercurl`: heuristic (terminal id / env) is fine;
     both degrade cleanly (links skipped, curl → underline).
3. **Deferred autowrap assumption (please review)**: the presenter writes
   the last column (incl. bottom-right) and relies on xterm-style deferred
   wrap; after any last-column write it forgets its cursor, so the next
   emission opens with absolute CUP, which clears the pending-wrap state
   before anything prints. This is safe on every VT-descendant terminal I
   could find documentation for (xterm, kitty, wezterm, alacritty, iTerm2,
   Windows Terminal, VTE). If your platform matrix finds a terminal with
   *immediate* wrap, I need a `caps` bit to skip the last column instead —
   flag it and I will add the strategy.
4. The presenter never emits LF/VPA for cursor motion, so ONLCR state does
   not affect frame bytes. Raw mode is still expected (input side).

## To REACT (frame loop contract, cycle 2)

1. `anim::FrameRequester { fn request_frame(&self) }` is the hook your
   scheduler implements. Animations never sleep or poll; a running
   animation re-requests each frame and stops when done.
2. Intended per-frame driver (see `render::mod` docs and the pipeline test):
   flatten → diff → present, then copy `next` into `prev` (`Surface::blit`
   full-rect is the cheap path; a swap API can come later if the copy shows
   up in benches).
3. Damage contract: draw into `Layer::surface_mut()` (surfaces damage
   themselves); move/fade/show layers via `Layer::set_*` (geometry damage
   is recorded for you). `Compositor::any_dirty(&layers)` is the cheap
   "can I skip this frame entirely" gate for the idle-zero-CPU budget.
4. `Style` is a patch (fg/bg `Option`, attrs add/remove) — widget styling
   composes with `Style::merge`, themes can hand widgets partial styles.

## To REDTEAM (attack surface, in priority order)

1. **Presenter vs your VT model**: apply emitted bytes to the previous
   screen and assert equality with `next`. Hot spots: last-column writes
   (pending wrap), wide glyphs adjacent to run boundaries, SGR
   incremental-vs-reset choice (22/24 shared resets re-adding survivors),
   link open/close ordering around style changes.
2. **Compositor pair repair**: layers whose wide pairs overlap at odd
   offsets (leader over continuation), translucent veils over pairs,
   1-column damage rects sliced through pairs. Invariant: no orphan
   continuation, no leader without continuation, continuation style ==
   leader style, frame-wide, after every flatten.
3. **Diff damage honesty**: randomized surface pairs + randomized damage
   sets that *cover* all changes must produce runs equivalent to a full
   scan (property: replaying runs onto prev == next within damaged area).
4. Allocation counting: steady-state `FrameDiff::compute` and
   `Presenter::emit` must not allocate (scratch is owned and reused; glyph
   pools only grow on new long clusters). `cargo test` includes a
   capacity-stability test; a real allocator hook bench is yours.

## To integrator

Nothing needed in `src/base` this cycle. `Rgba::over/lerp`, `Rect`
arithmetic and the geometry types covered everything.
