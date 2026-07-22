# Retro games on AbstractTUI: feasibility (FIELD, study 2)

Maintainer prompt: "we may even want to create retro style games… rpg or
battletech dos." This report grounds that in the real engine — every claim
cites engine code — and separates what a cell-based game gets TODAY from
what needs items. Four items filed in `docs/backlog/proposed/games/`
(band 0700-0790); audio is cross-referenced to MEDIA's band, save-games to
control-plane 0340. Verdict up front:

- **Turn-based roguelike / RPG: buildable TODAY.** Tap-and-auto-repeat
  movement dispatches (repeats route as presses), a dungeon is one draw
  closure or Surface, `interval` animates ambience, particles + shaders
  ship. The gaps are boilerplate (grid math, saves), not blockers.
- **Hex tactics (BattleTech-class): buildable today with hand-rolled hex
  math**; item 0730 deletes that boilerplate and its aspect-projection
  traps.
- **Real-time action (30-60 fps, move-while-held): BLOCKED on held-key
  state (0700)** and awkward without a sanctioned game tick (0710). The
  frame-pacing architecture itself is NOT the blocker — the loop already
  paces at ~60 fps when animations are in flight.

---

## 1. Game loop: the damage-driven frame model vs a fixed timestep

What exists, verified:

- The drive loop already paces frames: while `frame_tasks_pending() > 0`
  it waits at most `FRAME_INTERVAL` = 16 ms (~60 fps) per turn instead of
  blocking (src/app/mod.rs:363-377); with nothing pending it blocks on
  the earliest timer deadline or indefinitely (mod.rs:378-389). Phase U
  runs `run_due_timers(now)` + `run_frame_tasks(now)` every turn
  (src/app/driver.rs:260-271).
- `reactive::interval` is the sanctioned repeating cadence: fixed-delay,
  zero wakeups between fires, missed ticks coalesce ("the period is
  therefore a MINIMUM… a job that must know real elapsed time reads its
  own clock inside `f`" — src/reactive/interval.rs:1-28).
- `anim::Clock` gives real/virtual time (src/anim/mod.rs:63-119);
  `ParticleField::step(dt)` is explicitly "fixed-timestep-friendly"
  (src/anim/particles.rs:5-6, 130-147); cell shaders are pure functions
  of `(x, y, t, cell)` driven by `LayerHandle::set_shader_t`
  (src/anim/shaders.rs:1-27; src/app/overlays.rs:622-634).

The honest answer to "can a 30 fps game run without fighting zero-idle":
**yes — the zero-idle design accommodates it by construction.** A 33 ms
`interval` (or the `after`-recursion the effects example uses,
examples/effects.rs:27, 69-104) sleeps on the timer heap between fires;
each fire mutates signals/layers → damage → one paint. Pause = cancel the
interval; the app is then perfectly idle. Nothing spins.

What's missing is the CONVENTION, and the engine's own example shows the
cost: examples/effects.rs advances its clock by `+= FRAME.as_millis()`
per fire (effects.rs:88-90) — assumed dt, which drifts under load exactly
as the interval contract warns. And the real per-frame lane —
`register_frame_task`, which receives the frame's true `now` — is
PRIVATE (src/reactive/animate.rs:107-111); `animate()` is its only
public consumer. A game simulation wants precisely that lane: called once
per paced frame with honest time, plus the standard accumulator pattern
(fixed simulation dt, render interpolation optional, spiral-of-death
clamp, pause/speed). **Item 0710** asks for the public frame-task surface
+ a small fixed-timestep helper encoding the pattern once.

## 2. Input: held keys, releases, repeats

What exists, verified:

- The engine already REQUESTS release visibility: enter-time kitty flags
  are `DISAMBIGUATE | REPORT_EVENT_TYPES` (`KittyFlags::standard()`,
  src/term/options.rs:54-73, wired at options.rs:179), and the parser
  decodes Press/Repeat/Release (src/input/mod.rs:227-239; kitty release
  tests src/input/kitty.rs:144-151).
- Then the routing seam DROPS it: `convert_event` returns `None` for
  every Release (src/app/events.rs:80-82) and `ui::KeyEvent` carries no
  kind (events.rs:83-87), so widgets/apps cannot distinguish press from
  repeat nor observe release. No key-state tracking exists anywhere in
  ui/app (grep verified).
- Legacy terminals only ever produce Press (auto-repeat = more presses,
  src/input/mod.rs:227-229) — releases are structurally unavailable
  there.

Consequences: a roguelike is fine TODAY (each press/repeat = one move —
repeats dispatch as presses). Move-while-held with clean stop (and
chorded diagonals: Up+Right held together) is impossible at the app
layer even on kitty terminals, because the information is decoded and
then discarded one seam later. **Item 0700** files the key-state service:
route releases (or expose a `keys_down()` snapshot + edge callbacks),
with the honest legacy degradation being repeat-timeout hold detection.
This is the same press/release primitive MEDIA's push-to-talk finding
needs — one item serves both consumers (cross-referenced in the item).

## 3. Sprites / tiles / maps

What exists, verified:

- `Surface::blit` copies a sub-rect surface-to-surface with clipping,
  pool adoption, and wide-pair repair (src/render/surface.rs:421-460) —
  but it copies EVERY cell including empties (surface.rs:439-447), so a
  sprite blit ERASES the background under its transparent corners.
  Cross-LAYER transparency exists (`Glyph::EMPTY` is see-through under
  `Blend::Normal`, src/render/compositor.rs module doc), so today's
  workaround is one overlay layer per sprite — heavyweight past a
  handful of entities (each layer carries surface + damage + z entry,
  src/app/overlays.rs:158-166).
- Layers give games real leverage already: `set_offset` (smooth-scroll a
  map layer without repainting it), `set_opacity`, `Blend::Additive`
  (glow/lighting washes), `ColorTransform` (tint/grayscale),
  `CellShader` + `set_shader_t` (src/app/overlays.rs:602-641;
  src/render/compositor.rs blending model).
- Asset pipeline half-exists: `gfx::decode_image` (PNG/JPEG),
  `gfx::mosaic` fits bitmaps to half/quadrant/braille cells with
  least-squares quality (src/gfx/mosaic_fit.rs:5-14), dither/quantize
  ship. Nothing slices a sprite SHEET into frames, nothing converts a
  bitmap region into a reusable cell-art `Surface` at load time, and
  nothing palette-swaps cell art (the classic retro trick).
- Grid math: none. A BattleTech hex map needs axial coordinates,
  neighbors, distance/range, line-of-sight traversal, and — the
  terminal-specific trap — hex→cell projection under the ~1:2 cell
  aspect the engine corrects for everywhere else (particles halve
  vertical velocity, src/anim/particles.rs:120-122; mosaic contains the
  same correction). `BrailleGrid` (the stroke substrate a hex outline
  wants) is private (src/widgets/chart.rs:49-115) — extensions 0420
  files its promotion and games inherit it; not re-filed here.

**Item 0720** (sprite/tile toolkit: masked blit, sheet slicing,
bitmap→cell-sprite, palette swap) and **item 0730** (board-grid math:
square + hex coords, neighbors, range, line, aspect-corrected
projection) cover the two gaps. Both are pure additions over Surface —
no compositor changes.

## 4. Audio: cross-reference, not an item here

The engine's only sound today is `Terminal::bell` (src/term/mod.rs:
254-259) with the notify degradation ladder (src/term/verbs.rs). Games
need trigger-shaped SFX ("play this cue now", fire-and-forget, a few
concurrent voices) — NOT streams. That is MEDIA's band
(proposed/media-av/): this study's ask to MEDIA is that the audio item's
API include a zero-latency trigger surface usable from a frame callback,
and that its absence degrade to `bell()` honestly. Filed as a
cross-reference in 0710/0720's non-goals, not duplicated as a games item.

## 5. Persistence: covered

Save-games are exactly control-plane **0340 Persist** (declared keys,
atomic phase-boundary snapshots, crash marker, restore-on-start —
proposed/control-plane/0340). A roguelike's save = declared keys for run
seed/depth/inventory; the phase-boundary snapshot moment ("phases L..S
run no user code", cited in 0340) is the same torn-read-free instant a
game wants. Nothing to file; games become another named consumer when
0340 promotes.

## 6. Verdict in five lines

1. Roguelike/RPG: **build one now** — no engine blockers; grid/save
   boilerplate is the only tax (0730, 0340).
2. Hex tactics: **now, with hand-rolled hex math**; 0730 deletes the
   boilerplate, 0720 the sprite plumbing.
3. Real-time action: **blocked on 0700** (held keys); 0710 turns the
   already-adequate frame pacing into a sanctioned game tick.
4. Audio is MEDIA's band (trigger-shaped SFX); persistence is 0340;
   vector strokes are 0420 — three dependencies, zero duplicated items.
5. Filed: 0700 (key state), 0710 (game tick), 0720 (sprites/tiles),
   0730 (grid math) in proposed/games/ — each additive, each citing the
   engine code above.
