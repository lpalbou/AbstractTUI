# REACT cycle-4 report

Scope shipped: the app-facing overlay/layer API + Modal/Toast, the
GFX3D post-present image seam, RT3-2 closed (cluster-atomic input),
KERNEL's present-caps fix, DESIGN's durable-state answer implemented
(`dyn_view_scoped`), and both confessed perf risks fixed with numbers.
All suites green: 714 lib + all integration suites (REDTEAM's
`adv_overlay.rs` passes 4/4 against the shipped API, `adv_widgets.rs`
RT3-2 test un-ignored and green). No warnings in owned code.

## 1. Overlay API (`app::overlays` + `app::popups`)

### Shape (frozen from my side)

- `App::overlays() -> Overlays` (cloneable handle; components capture
  it at mount). Three content modes per layer:
  - `overlays.layer(z, bounds) -> LayerHandle` — manual; paint via
    `with_surface(|s| ...)`.
  - `overlays.layer_draw(z, bounds, |canvas, rect| ...)` — repaints
    full-surface (transparent base) when `damage()` is called; runs in
    phase D under the draw-purity guard.
  - `overlays.layer_tree(z, bounds, modal, cx, view)` — a full `UiTree`
    world: own reactivity/layout/focus/damage, drawn damage-scoped,
    LAYER-LOCAL coordinates.
- `LayerHandle { set_offset, set_opacity, set_visible, set_blend,
  set_color_transform, set_shader, set_shader_t, with_surface, damage,
  bounds, is_alive, remove }` — `Clone`, weak-backed (ops on a dead
  store/layer are no-ops, never panics; REDTEAM pinned this). One
  deviation from the order sheet: `surface_mut` is spelled
  `with_surface(FnOnce(&mut Surface))` — handing out `&mut Surface`
  through an `Rc<RefCell>` store is not expressible without leaking a
  guard type; the closure shape is the honest Rust for it.
- `overlays.image(rect, bitmap) -> ImageHandle { set_bitmap, set_rect,
  remove }` — the GFX3D seam (§3).

### Driver integration

The ROOT layer moved INTO the overlay store (id 0, never removable);
the driver borrows the store per phase. Phase L lays out overlay trees;
phase D draws root then overlay content; phase C flattens the store's
layer vec z-sorted (the compositor's existing sort); phase P presents
cells then emits queued image payloads through
`Presenter::external_write` — cells first, protocol bytes second, ONE
write + ONE flush (RT1-16a holds).

THE BORROW RULE (the design decision worth review): no user code runs
while the store is borrowed. Phases STEAL a layer's surface (cheap Vec
swap), release the store, run draw closures/tree draws, swap back.
`LayerHandle` ops use `try_borrow_mut`: a mutation from inside a draw
closure is a debug panic naming the rule, a silent no-op in release
(skipped op = one stale frame; poisoned borrow = dead process).

### Event routing

Input routes topmost-z first, before the root tree:

- MODAL tree overlay: owns EVERYTHING while visible — keys/paste
  dispatch into it, mouse anywhere is dispatched (outside = swallowed).
  Topmost modal wins when stacked.
- Non-modal tree overlay: owns pointer events INSIDE its bounds
  regardless of handler consumption (panels are opaque; click-through
  to covered content would act on invisible things). Keys fall to the
  root (non-modal key focus is future work, flagged in §16 of the
  design doc).
- Unclaimed events fall through to `app.tree()` exactly as before.

### Modal + Toast (`app::popups`, NOT `ui::overlay` — deliberate)

The order sheet said `ui::overlay::{Modal, Toast}`; they shipped as
`app::popups::{Modal, Toast}` because they need the `Overlays` store
type and `ui` sits BELOW `app` in the layer map — a `ui` module
importing `app` is the exact upward-import pattern R4-1 just overturned
for GFX3D. Same API, one layer up. Both are pure API consumers (no
engine privileges):

- `Modal::open(&overlays, cx, viewport, size, build)` — centered,
  `overlay`-token ground, focus-trapped tree at z 1000; `close()`
  removes the layer and disposes the modal's scope.
- `Toast::show(&overlays, cx, viewport, msg, duration)` — top-right
  chip at z 2000; slides+fades in via `animate` driving
  `set_offset`/`set_opacity`, parks (ZERO wakeups — see timers below),
  dismisses via `reactive::after`, slides out, removes itself.
  `show_with_motion` exposes the flight duration for tests.

### One-shot timers (`reactive::after`)

Toast dismissal needed time-based work with idle-zero billing, so the
scheduler grew `after(delay, f)` + `next_timer_deadline()` +
`run_due_timers(now)`. Timers deliberately do NOT frame-pace: the
`App::run` idle wait sleeps until the earliest deadline and phase U
fires due timers. A parked toast costs zero wakeups and zero frames
until its deadline.

### Acceptance

`app::acceptance`: `toast_overlay_slides_fades_and_idle_returns_to_
zero_bytes` (slide in over content → parked turn emits ZERO bytes →
timer fires → slide out → layer removed → vacated cells repaint →
idle turn emits ZERO bytes) and `modal_owns_input_and_close_restores_
content` (keys swallowed from the root while open; close repaints from
below; keys flow again). REDTEAM's `adv_overlay.rs` (churn model-
exactness, toast damage scoping, image move/clear, dead handles) passes
4/4 unmodified.

## 2. RT3-2 closed: cluster-atomic input editing

`widgets::input` re-indexed from chars to GRAPHEME CLUSTERS using
RENDER's `text::segments` (one authority for boundaries AND widths).
A `ClusterMap` (byte boundaries + per-cluster widths, rebuilt per
edit/draw — single-line inputs, O(n) is honest) backs cursor motion,
selection, word jumps, Backspace/Delete, and the draw path (selection/
cursor highlight whole clusters; a wide-emoji cursor is a two-cell
block).

The subtle bug the fix itself introduced and caught: inserting a ZWJ or
combining scalar MERGES two clusters into one, so `old index +
inserted-cluster-count` can point past the post-insert cluster count.
Inserts now re-anchor by BYTE offset on the post-insert map
(`cluster_after(byte)`) — index arithmetic lies across merges, byte
offsets do not. REDTEAM's `input_backspace_deletes_whole_grapheme_
cluster` un-ignored per R4-2 (tag verified) and green; the honesty
note in `input.rs` and design-doc §15 rewritten to state the new
contract.

## 3. GFX3D post-present image seam

`overlays.image(rect, bitmap)` registers an image overlay; each frame
the driver renders DIRTY images through `gfx::ImageRenderer` (the
capability ladder, `caps.graphics()`):

- `ImageOutput::Cells` (mosaic rung) blits into the ROOT surface
  pre-flatten via `Surface::blit_mosaic` — cells travel the normal
  diff.
- `ImageOutput::Bytes` (kitty/iTerm2/sixel) queues and emits AFTER
  `presenter.emit` through `Presenter::external_write` into the SAME
  out buffer — GFX3D's requested order (cells first, payloads second,
  one flush).

Dirty discipline: `set_bitmap`/`set_rect` mark dirty; resize marks ALL
images dirty (placement geometry); caps upgrade marks all dirty (the
ladder may pick a better channel). Not yet built (flagged for GFX3D):
kitty `delete_by_id` on remove — `ImageHandle::remove` today damages
the root under the rect (cells repaint) but a kitty-placed image needs
the delete sequence; I want to route that through their session/SlotKey
machinery rather than duplicate id bookkeeping (request filed).

## 4. KERNEL fix + DESIGN answers + perf confessions

- **present_caps**: `app::driver::present_caps_from` now delegates to
  `Capabilities::present_caps()` — the hand-assembly had pinned
  `undercurl`/`underline_color` false forever. Test updated to assert
  both flow through when the caps declare them.
- **DESIGN 1a (recipe blessing)**: BLESSED with one correction — the
  outer-`dyn_view` + durable-state-outside recipe is exactly right;
  the doc now states the two-scope discipline explicitly (§15.6).
- **DESIGN 1b (internal-signal accumulation)**: fixed rather than
  tolerated — `ui::dyn_view_scoped(style, |gen_cx| ...)` passes the
  per-generation scope (disposed before each rebuild), so widget
  internals die with their generation. `dyn_view` unchanged
  (wrapped). Test: `dyn_view_scoped_disposes_generation_state_per_
  rebuild`. Widgets' `element(cx, &TokenSet)` convention CONFIRMED
  as-is: pass the scope you want internals to live on — the mount
  scope for durable-by-default, a `dyn_view_scoped` generation scope
  inside theme-rebuild galleries.
- **Hover recompute** (my cycle-3 confession): `update_hover` memoizes
  `(position, layout_epoch)` — repeated-position events (mode-1003
  streams, wheel bursts) pay one tuple compare, and the epoch half
  keeps it honest when geometry moves under a stationary pointer
  (test: `hover_memo_skips_same_position_but_honors_relayout`).
- **style_signal whole-tree re-solve** (the other confession): style
  changes now push a RESOLVE ANCHOR (nearest ancestor whose own size
  the change cannot affect — climb past Auto-sized ancestors) into
  `dirty_subtrees`; `UiTree::layout` runs `resolve_subtree` per anchor
  instead of a full solve. Measured (release, 2051-node tree, 100
  style rounds): **104 µs incremental vs 84.4 ms full** (~1 µs vs
  ~840 µs per change, ~800×). Structural changes still full-solve; a
  full solve supersedes pending anchors. Guard test pins damage
  containment inside the anchor container.

## 5. File hygiene

`ui/tree.rs` (657 after the cycle's additions) split: the paint walk
moved to `ui/draw.rs` (136 lines; same `impl UiTree`), tree.rs now 536.
New files: `app/overlays.rs` (~660 with tests — the store, handles,
dispatch, phases), `app/popups.rs` (166). `widgets/input.rs` 601.

## 6. Risks / honesty

- **Overlay z vs root content**: everything in the store flattens
  z-sorted; an app creating a layer at z<0 sits UNDER the root layer
  (compositor semantics). Not forbidden — documented as the way to do
  under-content effects, but nothing owns that use case yet.
- **Non-modal overlays have no key story** (only pointer ownership);
  future cycle if DESIGN's popups need it.
- **Invisible layers keep stale pixels** (draw skips them); they
  repaint on the visibility flip via layer frame-damage. Deliberate —
  visibility toggles are cheap, content stays warm.
- **Toast/animation tests use real time** (5 ms flights, 30–35 ms
  sleeps) — deterministic on any sane scheduler but not clock-free;
  if CI flakes, the fix is an injectable clock in the frame-task pump,
  not looser asserts.
- **resolve_anchor conservatism**: judged from ANCESTOR styles only
  (deliberately not the changed node's own old/new styles — the old
  style is gone by the time the effect runs). Percent-of-Auto edge
  cases are named in design-doc §17 risk 7 for REDTEAM; a
  counterexample demotes that class to full solve.
