# REACT — cycle 2 build report

Author: REACT. Scope: the real `App::run` loop per the damage contract,
the RT1-2/RT1-3/RT1-15 fixes, `base::FrameRequester` adoption, theme
signal, text-measure adoption, and the headless acceptance test.

## Shipped

- `src/app/driver.rs` — `Driver` (U/L/D/C/P/S frame pipeline, damage
  translation, resize + caps-upgrade `prev` poisoning, Ctrl+C policy,
  worker-failure surfacing), `RunConfig`, `Turn`.
- `src/app/events.rs` — kernel event -> ui event conversion (explicit
  bit-by-bit modifier mapping, documented drops), default quit chord.
- `src/app/theme.rs` — `use_theme/set_theme/set_theme_by_id`, ONE theme
  signal under a leaked per-thread root (damage contract §5).
- `src/app/mod.rs` — `App::run` (panic hook first, tty check, blocking
  loop), `run_on` (any `Terminal`), `Quitter`, kept headless
  `pump/draw`, theme-watcher effect wired in `mount`.
- `src/app/acceptance.rs` — the cycle acceptance (below).
- `src/reactive` — draw-phase guard + release diagnostics (`diag.rs`),
  effect labels + per-effect flush ceiling, `spawn_worker` with
  catch_unwind reporting, `base::FrameRequester` re-export (local trait
  deleted).
- `src/ui` — `dispatch` batch-wrapped (RT1-3 option a, pinned),
  `draw_damaged` (region-clipped painting under the draw guard),
  `SurfaceCanvas` (ui -> render::Surface bridge), `ClippedCanvas`,
  themed default text color, `invalidator()` handle, text measurement
  via `text::width`.
- `docs/design/reactive-ui.md` §12a/§12b/§13 — pinned semantics + the
  loop as built.

## The loop as built (10 lines)

1. `Driver::new`: enter (caps-derived options), wire `TerminalWaker`
   into wake callback + `base::FrameRequester`, write probe queries
   (env-pass caps paint first; never probe dumb terminals).
2. `turn` (never blocks): phase U = drain posted jobs, surface worker
   failures as app errors, dispatch every available event — each in its
   own reactive batch — flush effects.
3. Ctrl+C quits unless the app consumed it; `Quitter` quits explicitly.
4. Frame wanted? (`take_frame_request` or pending tree work) else the
   turn reports idle and the outer loop blocks in `poll_event(None)`.
5. Phase L: `tree.layout()` — geometry damage folds into the ui set;
   the frame's damage seals here (posted jobs cannot run past U).
6. Phase D: clip + coalesce damage, clear each rect to theme `Bg`,
   redraw intersecting instances through `SurfaceCanvas` under the
   draw-purity guard.
7. Phase C: `Compositor::flatten(layers)` -> frame + damage union.
8. Phase P: `FrameDiff::compute` -> `Presenter::emit` -> at most one
   write + exactly one flush; zero-byte frames skip both.
9. Phase S: `prev.blit(frame)`. Resize/caps-upgrade poison `prev`
   (post-resize screen content is unknowable; upgraded caps must
   re-present) and damage all.
10. `CapsReply` events fold into `ActiveProbe` during U; the DA1
    sentinel upgrades presenter caps for the NEXT frame.

## What passes

Crate-wide: `cargo check` clean, `cargo test --lib` 541 passed / 0
failed (+1 doctest). Mine this cycle, beyond all cycle-1 suites staying
green:

- **The headless acceptance test** (`app::acceptance::
  headless_counter_end_to_end`): real App + Driver against REDTEAM's
  `CaptureTerm`; asserts via `VtScreen` that the screen reads
  "counter demo / count: N" after every scripted key; frame 2 (a `+`
  keypress) emits ~40 bytes — cursor move + SGR + the single changed
  DIGIT, not the header, not even the unchanged "count: " prefix — and
  is pinned < half of frame 1; an idle turn emits ZERO bytes; three
  keys in one turn coalesce into one frame with exactly one flush
  (RT1-16a); `q` quits through the app shortcut; leave restores the
  session; `unknown_seq_count == 0` (every emitted byte modeled).
- Ctrl+C default-quit + app-override (consuming shortcut suppresses it).
- Resize forces a truthful full repaint (prev poisoning).
- Dead worker surfaces as a labeled app error from `turn`.
- RT1-3 pinned: capture handler closing the modal containing the target
  — routing completes over the pre-write tree, no panic, disposed
  handlers never fire afterward.
- RT1-2 pinned: tracked read in a draw closure panics (debug) naming
  the node; untracked peeks and mid-draw memo recomputes stay legal.
- RT1-15 pinned: ping-pong effect pair panics at ~1k runs naming its
  label; `spawn_worker` panic arrives as a labeled failure.
- Event conversion: modifier bit-mapping (kernel ALT=2/CTRL=4 vs ui
  CTRL=2/ALT=4 — transmuting would swap them), lock stripping, release
  dropping.
- Theme: switch re-runs readers + invalidates the tree; unknown ids
  rejected.

## Seams left (documented, none blocking)

- `term::Capabilities` lacks `undercurl`/`underline_color`; both map to
  `false` in `present_caps_from` (requests 1–2 to KERNEL).
- `input::probe_active` unused by the loop (folds `CapsReply` inline in
  phase U instead) — deliberate, documented in requests #3.
- Terminal focus in/out, paste, wheel-left/right, back/forward mouse
  buttons: dropped at event conversion with a documented list
  (`app::events::convert_event`) — routing vocabulary grows when
  widgets need them (cycle 3).
- Animation deadlines: the blocking wait passes `None` (no frame pacing
  source yet). When anim lands its clock, the deadline computation slots
  into `App::run_on`'s wait call — one line.
- Draw-region skip for absolute-positioned descendants is conservative
  (documented in `draw_walk`): a subtree is skipped when its own rect
  misses the damage; absolute children that escape their parent's rect
  re-enter via their own geometry damage when they move.

## Risks for REDTEAM (this cycle's honest weak spots)

1. **Damage-coalesce + clipped-draw interaction**: overlapping damage
   rects draw twice (idempotent, but attack the clip edges — especially
   wide glyphs straddling a damage boundary; `ClippedCanvas::print`'s
   degraded per-char path is the spot).
2. **`prev` poisoning**: the "impossible color pair" assumes ui colors
   are opaque-or-transparent. A widget hand-crafting alpha-7 colors in
   a draw closure would make poisoned cells compare equal and skip
   re-emission after resize. Cheap to harden if you demonstrate it.
3. **Probe folding**: `CapsReply` events arriving AFTER the DA1 sentinel
   (probe already `None`) are dropped silently — correct per RT1-6c,
   but the multiplexer-late-reply scripting you promised would pin it
   from the outside.
4. **Turn-level reentrancy**: a handler calling `set_theme` mid-dispatch
   (inside the batch) re-runs the theme watcher during that batch's
   close — invalidation lands same-frame. A handler REMOVING the theme
   signal's readers mid-flush is the usual disposal-ordering territory.
