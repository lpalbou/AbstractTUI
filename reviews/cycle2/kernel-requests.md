# KERNEL cycle 2 — requests + contracts to other owners

## To REACT — input -> ui event conversion (your cycle-1 request 5)

`ui` sits above `input`, so the `From` impls belong on YOUR side (input
cannot import ui without inverting the layer map). Everything you need is
public and stable: `input::{Event, KeyEvent, KeyCode, KeyEventKind, Mods,
MouseEvent, MouseKind, MouseButton}`. This is the exact mapping I
recommend; the four traps are marked.

### TRAP 1 — the Mods bit layouts DIFFER. Never copy raw bits.

`input::Mods` mirrors the kitty wire encoding: SHIFT=1, **ALT=2, CTRL=4**,
SUPER=8 (+HYPER/META/CAPS/NUM above). `ui::Mods` chose SHIFT=1, **CTRL=2,
ALT=4**, SUPER=8. A `ui::Mods(m.0)` transmute silently SWAPS Ctrl and Alt
— every Ctrl shortcut becomes Alt. Convert per-flag:

```rust
fn mods(m: input::Mods) -> ui::Mods {
    let mut out = ui::Mods::NONE;
    if m.contains(input::Mods::SHIFT) { out = out | ui::Mods::SHIFT; }
    if m.contains(input::Mods::CTRL)  { out = out | ui::Mods::CTRL;  }
    if m.contains(input::Mods::ALT)   { out = out | ui::Mods::ALT;   }
    if m.contains(input::Mods::SUPER) { out = out | ui::Mods::SUPER; }
    out // HYPER/META have no ui slot: dropped (fine for routing);
        // CAPS/NUM locks are deliberately not routing modifiers.
}
```

Use `key.mods.without_locks()` as the source so caps lock never breaks a
chord.

### TRAP 2 — kitty release/repeat events double- or triple-fire shortcuts.

Under the kitty protocol one keystroke can arrive as Press + Repeat… +
Release. Route only `key.is_down()` (Press|Repeat) into `UiEvent::Key`,
or shortcuts fire on release too. `KeyEvent::{is_press, is_repeat,
is_release, is_down, chord_matches}` exist for exactly this (added this
cycle). Recommendation: drop Release events at the conversion boundary
until some widget declares a need (none should for routing).

### Key identity table (`input::KeyCode` -> `ui::Key`)

| input | ui | note |
| --- | --- | --- |
| `Char(c)` | `Char(c)` | already the UNSHIFTED identity under kitty (alternate keys use the primary code) |
| `Enter` | `Enter` | keypad Enter already folds to Enter on my side |
| `Tab` | `Tab` | Shift+Tab arrives as `Tab` + SHIFT (CSI Z decoded) |
| `Backspace` | `Backspace` | |
| `Esc` | `Escape` | name differs |
| `Left/Right/Up/Down` | same | |
| `Home/End/PageUp/PageDown` | same | |
| `Insert/Delete` | same | |
| `F(n)` | `F(n)` | n can reach 35 under kitty; ui::F(u8) holds it |
| `CapsLock/ScrollLock/NumLock/PrintScreen/Pause/Menu` | — | no ui slot: DROP (or extend ui::Key if a widget ever binds them) |
| `Modifier(_)` | — | bare modifier presses (kitty): DROP for routing |
| `Functional(u32)` | — | unmapped kitty codes (media keys): DROP |
| `Unidentified` | — | DROP |

### Mouse table (`input::MouseEvent` -> `ui::MouseEvent`)

My shape is `{kind, button, pos, mods}` (button beside kind); ui embeds
the button in the kind. Positions are already 0-based cells on both sides.

| input kind + button | ui kind |
| --- | --- |
| `Down` + L/M/R | `Down(L/M/R)` |
| `Up` + L/M/R | `Up(L/M/R)` |
| `Drag` + L/M/R | `Drag(L/M/R)` |
| `Move` (button None) | `Move` |
| `WheelUp` / `WheelDown` | `ScrollUp` / `ScrollDown` |
| `WheelLeft` / `WheelRight` | — no ui slot: DROP or extend (TRAP 3: horizontal wheels are real on macOS trackpads — recommend extending `ui::MouseKind` now) |
| `Down/Up/Drag` + `Back`/`Forward` | — no ui slot: DROP (browser-style side buttons) |

### Events that must NOT reach ui routing

| input event | destination |
| --- | --- |
| `Paste(String)` | the FOCUSED text widget via a dedicated path (routing has no paste vocabulary; do not synthesize per-char keys — TRAP 4: that reintroduces paste-injection, the attack 2004 exists to prevent) |
| `FocusGained/FocusLost` | app-level (terminal window focus; distinct from your per-widget FocusIn/FocusOut) |
| `Resize(Size)` | the layout root, not the event tree |
| `CapsReply(_)` | fold into `Capabilities` if a probe is live, else drop; never user-visible |
| `Unknown(_)` | drop (optionally debug-log) |

### Loop wiring (your request 6, delivered)

- `Terminal::waker() -> Option<TerminalWaker>` (Clone+Send+Sync; `wake()`
  from any thread) — hand it to `reactive::set_wake_callback`.
- A woken `read` returns `TermRead::Wake`; `EventReader::poll_event` maps
  it to `Ok(None)`. Contract: on EVERY `Ok(None)` drain posted jobs +
  effects, recompute your next deadline (animations/timers), and re-poll.
  Do not distinguish wake from timeout — that path split is how drains
  get skipped.
- Startup sequencing recipe (env caps first paint, probe concurrent,
  upgrade callback): `docs/design/term-input.md` §2.3. The probe helper
  returns mid-probe user input as ordinary events — feed them into
  dispatch, drop nothing.

## To RENDER

1. `PresentCaps` conversion shipped: `caps.present_caps()` /
   `impl From<&term::Capabilities> for PresentCaps` (term/caps.rs).
   Depth: truecolor > 256 > Ansi16; `sync_output_2026` env+DECRQM;
   `hyperlinks`/`undercurl` env heuristics per your request 2.
2. `NO_COLOR` folds to `Ansi16` because `ColorDepth` has no mono rung. If
   you want honest monochrome (attributes only, no 30–37/40–47), add a
   `ColorDepth::Mono` (or a `mono: bool`) and I will wire
   `Capabilities::no_color` into the conversion — the raw flag is already
   public.
3. `deferred_wrap: bool` landed on `Capabilities` (default true, RT1-5).
   It is NOT in `PresentCaps` (your struct, your call): read it off
   `Capabilities` when you add the skip-last-column strategy, or tell me
   to extend the conversion when you extend the struct.

## To GFX3D

1. `caps.graphics() -> GraphicsCaps { kitty_graphics, iterm2_images,
   sixel, sixel_max_registers, cell_pixel_size }` shipped, filled by env +
   the probe batch (your requested sequencing: kitty a=q with unique id +
   DA1 sentinel; sixel from DA1 attr 4; XTSMGRAPHICS `CSI ?1;1;0S`;
   `CSI 16 t` + TIOCGWINSZ for cell pixels). All replies route through the
   parser as `CapsReply` — nothing leaks into app events (test-pinned,
   including seconds-late replies).
2. After a resize, call `term::probe::refresh_cell_pixel_size(term, caps)`
   — cheap ioctl, no wire traffic. Known gap: a font-zoom on a terminal
   with silent ioctls (pixel fields = 0) keeps the stale wire-probed cell
   size until the next full probe; if your scaling can look wrong there,
   ask and I will add a single-query re-probe helper in cycle 3.
3. `sixel_max_registers` honesty: XTerm answers it; many sixel terminals
   do not implement XTSMGRAPHICS reads (foot answers; WT ignores). Treat
   `None` as "assume 256, emit defensively", not as "no sixel".

## To REDTEAM

1. Trait freeze (your request 1): `Terminal` is STABLE as of this cycle.
   Cycle-2 deltas you need for `CaptureTerm`: new `TermRead::Wake`
   variant; `waker() -> Option<TerminalWaker>` (default `None`, or mint
   one via `TerminalWaker::new(closure)` for scripted wake tests);
   `cell_pixel_size() -> Option<PixelSize>` (default `None`). No time
   source was added: deadlines stay caller-computed `Instant`s, your
   scripted clock stays outside the trait.
2. RT1-6 pins landed: `probe_skipped_entirely_for_dumb_terminals` (zero
   bytes written), `late_probe_replies_stay_caps_events` (parser level,
   every frame type, byte-split) and
   `late_replies_after_probe_surface_as_caps_events_only` (reader level).
   Your 2-seconds-late scripted terminal can now be written against
   `ScriptTerm`-equivalent semantics.
3. RT1-12a landed (windows re-query on every wake + timeout);
   RT1-12b landed (sgr_mouse gated on WT_SESSION/WezTerm/TERM_PROGRAM
   under cfg(windows) — note this branch is compile-checked but not
   unit-testable from macOS; it is exactly the kind of thing your Windows
   CI run should assert once one exists).
4. The manual ConPTY deferred-wrap run (your request 3) remains OPEN —
   `deferred_wrap` ships defaulted true and nothing flips it yet. Flag it
   red if cycle 3 starts without that run scheduled.

## To the integrator

1. `base::PixelSize` — adopted, thanks (trait method + probe + GraphicsCaps).
2. The `From<&Capabilities> for PresentCaps` impl lives in term/caps.rs
   and references render's type upward TEXTUALLY (single-crate, no cyclic
   compilation) — but this was measurably not free during the cycle: a
   compile error inside `render` now blocks `term`/`input` builds too
   (lived it twice this cycle: the mid-cycle `underline_color` field
   addition, and an unrelated render borrow error gating kernel test
   runs). If that friction repeats, hoisting `PresentCaps`+`ColorDepth`
   into `base` makes imports strictly downward again; both sides are one
   `use` away. Your call with RENDER.
3. `PresentCaps` grew `underline_color` mid-cycle (DESIGN request 2);
   detection is wired (same env lineage as undercurl, separate
   `Capabilities` field so a probe can split them later).
