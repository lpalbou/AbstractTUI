# The Damage Contract (integrator ruling on RT1-1, RT1-16)

Binding for REACT (loop, ui, layout), RENDER (compositor, diff, present),
and everyone who draws. Amendments via reviews/ only.

## 1. Frame phases — user code never runs past phase U

Every frame is one strictly-sequenced pass on the UI thread:

```
U. USER      drain posted jobs -> dispatch input events (whole dispatch
             wrapped in reactive batch) -> flush effects (Dyn remounts,
             layout re-solve requests happen here)
L. LAYOUT    re-solve dirty layout subtrees; collect geometry damage
D. DRAW      run draw closures ONLY for damaged regions into layer
             surfaces. Draw closures are pure over captured data:
             tracked signal reads in draw are a debug panic (RT1-2).
C. COMPOSE   Compositor::flatten(layers, damage) -> frame + damage union
P. PRESENT   diff(prev, next, damage) -> runs -> presenter bytes ->
             exactly ONE Terminal::flush per frame (RT1-16a)
S. SWAP      next becomes prev; damage bookkeeping cleared
```

Phases L..S run no user code, therefore no signal writes, therefore no
re-entrant damage — this is what makes the epoch rule below airtight.
A `Dyn` whose effect runs in phase U does NOT paint; it marks damage and
(re)registers its draw closures. Painting happens only in phase D.

## 2. Frame epoch rule

The frame's damage set is sealed when phase L begins. Signal writes from
other threads can only arrive as posted jobs; posted jobs run only in
phase U. A post landing while phases L..S are running wakes the loop and
is drained by the NEXT frame's phase U. Late damage is therefore never
lost and never double-painted. There is no "mid-present write" — by
construction, not by discipline.

## 3. Damage vocabulary — one, in screen-cell coordinates

- The single entry point is per-layer damage on `render::Compositor`
  layers. All rects are in SCREEN cell coordinates (layer origin applies
  when a layer is offset; the layer records damage in its own surface
  space and the compositor translates — RENDER owns that translation).
- REACT's App loop is the sole translator from UI vocabularies:
  `UiTree::take_damage()` (Dyn remounts, focus) and
  `LayoutTree::take_geometry_damage()` (old ∪ new rects of moved nodes)
  are unioned and applied to the root layer before phase D draws them.
- Overlay/popup/3D layers damage themselves via `Layer::set_*` /
  surface writes exactly as RENDER documented; nothing else changes.

## 4. Cursor policy (RT1-16b)

Default: terminal-native cursor (DECSET 25 + presenter cursor park).
Idle really is zero wakeups. A composited/animated cursor is an
ANIMATION: it requests frames and is billed as one. Widgets choose.

## 5. Theme switching (RT1-16c)

One app-level `Signal<&'static Theme>`. Widgets resolve tokens at view
build (styles are resolved-Rgba POD — DESIGN request 1 accepted). Theme
switch = the theme signal writes, every Dyn that read it re-runs, their
regions damage; a `damage_all()` escape hatch exists for the root layer
but the reactive path is the design. No per-token signals.

## 6. External bytes (gfx protocols) — presenter custody (RT1-5b)

All bytes reach the terminal through the presenter. Pixel-protocol
payloads (kitty APC / iTerm2 OSC / sixel DCS) go through
`Presenter::external_write(bytes, at: Point)` which (a) flushes pending
cell runs, (b) moves the real cursor, (c) emits the payload, (d)
invalidates the virtual cursor and SGR assumptions. GFX3D never calls
`Terminal::write` directly. REDTEAM asserts byte custody in integration
tests.

## 7. Shared vocabulary now in base (integrator, done)

- `base::FrameRequester` — the one frame-request trait. `anim` and
  `reactive` re-export it; local duplicates are removed in cycle 2.
- `base::palette::{SYSTEM_16, XTERM_256, xterm_256()}` — the ONE xterm
  table. RENDER's downlevel and REDTEAM's VT model both import it
  (RT1-7); hand-typed copies are deleted in cycle 2.
- `base::PixelSize` — cell-pixel geometry for gfx scaling (never mixed
  with cell `Size`).
