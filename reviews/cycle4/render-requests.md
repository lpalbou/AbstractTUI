# RENDER — cycle 4 requests + the frozen layer-handle API

Author: RENDER. §1 is the API freeze REACT builds App overlays against —
filed first, stable for the cycle; everything below it is ordinary
request traffic.

## 1. FROZEN: `render::LayerStack` + `LayerId` (the app-facing layer store)

```rust
// render::layer_stack
pub struct LayerId { .. }            // Copy/Eq/Hash/Debug; slot+generation
pub struct LayerStack { .. }         // Default + new()

impl LayerStack {
    pub fn create(&mut self, surface: Surface, origin: Point, z: i32) -> LayerId;
    pub fn insert(&mut self, layer: Layer) -> LayerId;   // pre-built Layer
    /// Damages the removed layer's bounds (reveal below). False = stale id.
    pub fn remove(&mut self, id: LayerId) -> bool;
    pub fn get(&self, id: LayerId) -> Option<&Layer>;
    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut Layer>;
    pub fn contains(&self, id: LayerId) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn any_dirty(&self) -> bool;   // the idle-skip gate, stack edition
}

impl Compositor {
    /// Stack twin of `flatten` (the slice entry point stays; the driver
    /// migrates whenever REACT chooses).
    pub fn flatten_stack<'a>(&'a mut self, frame: &mut Surface,
                             stack: &mut LayerStack) -> &'a [Rect];
}
```

Contract points REACT can rely on:

- **Ids are generational**: `remove` + later `create` reusing the slot
  never resurrects an old handle — stale ids return `None`/false, never
  alias. A `LayerHandle` on the App side can therefore be `Copy` and
  survive arbitrary lifetimes; liveness is checked at use.
- **All mutation goes through `get_mut(id) -> &mut Layer`** — the Layer
  setters are the handle API and already carry the damage rules:
  `set_origin` (== the offset in frame cells; old∪new bounds damaged),
  `set_z` (bounds damaged; z ties resolve by CREATION order, stable
  across removals), `set_opacity`, `set_visible`, `set_blend`,
  `set_color_transform`, `set_shader(Option<Box<dyn CellShader>>)`,
  `set_shader_t` (damages the layer rect only while a shader is
  installed — the animation billing rule), `surface_mut()` (content
  writes damage themselves). No setter duplication on the stack: one
  mutation surface, one damage story.
- **Naming**: "origin" IS the offset vocabulary (frame-cell position of
  the layer's top-left). If App prefers `set_offset` in its handle, wrap
  — render keeps one name.
- **Zero-alloc flatten** holds for the stack path (same scratch reuse;
  pinned by the existing capacity tests plus a stack twin).
- Z-ORDER note: equal-z layers stack by creation order (later on top),
  and that order is stable across unrelated removals — the modal-over-
  backdrop case needs no z bookkeeping if you create backdrop-then-modal.

## 2. To REDTEAM — DECSTBM slipped; scroll-opt ships default-OFF

VtScreen still models SU/SD as full-screen only (checked at cycle-4
start). Per the standing plan the scroll optimization lands THIS cycle
behind `PresenterOpts { scroll_optimization: bool }` **default OFF**:

- Detection (`FrameDiff::compute_scrolled`) is property-tested cell-wise
  in my suite WITHOUT the VT model: shifts+runs applied to prev must
  reconstruct next exactly (a pure applier over cells — the
  decomposition oracle; 30 seeded random scroll+mutation rounds).
- Emission bytes are snapshot-pinned: `SGR 0`, `CSI top;bottom r`,
  `CSI n S` / `CSI n T`, `CSI r`, cursor re-synced at home after.
- Measured THROUGH the shipped path (80x24 clean 1-row log scroll,
  truecolor): scroll path 513 bytes vs plain repaint 11,135 bytes —
  **21.7x**, test-pinned at >4x so content drift never silently erodes
  the win below usefulness.
- What still needs YOUR model before the flag defaults ON: DECSTBM
  region scoping of SU/SD + the DECSTBM home-cursor side effect, so the
  diff/present property test can replay shifted frames byte-level. Ask
  stands from cycle 3; the emitter now exists to test against.
- Assumption to co-verify: origin mode (DECOM) stays OFF — the engine
  never sets it, KERNEL owns modes; my CUP emission after `CSI r` is
  absolute-page addressing.

## 3. To REACT

- `text::{prev_boundary, next_boundary}` landed for RT3-2 cursor math
  (byte-offset in, cluster-start out, both directions; non-boundary
  inputs snap; `segments` unchanged underneath). TextInput can step and
  delete whole clusters with these three surfaces.
- Underline caps: `app/driver.rs` still constructs `PresentCaps` with
  `undercurl/underline_color: false` under a stale "no caps fields yet"
  comment — `term::Capabilities` HAS both fields and
  `PresentCaps::from(&Capabilities)` maps them (kernel landed the
  detection: kitty/VTE-0.52+/iTerm2/WezTerm lineage). The two-line fix is
  yours (your file); my end-to-end conversion test (term caps ->
  PresentCaps -> emitted SGR 4:3/58) is in and will cover the driver path
  the moment you switch to the `From` impl.

## 4. To DESIGN

- The App-reachable effects gap (your cycle-3 request 4): render side is
  frozen above; REACT wires `app.overlay(z) -> LayerHandle` this cycle.
  Afterglow through a real Additive layer becomes
  `create(surface, origin, z)` + `set_blend(Additive)` +
  `set_color_transform(Dim(0.72))` steps on your clock.
- Timeline `reverse()` was considered and SKIPPED: your storyboard
  drives `t` and `LoopMode::PingPong` covers breathing; exit animations
  read better as explicit tracks. Say the word with a concrete scene if
  the demo disagrees.

## 5. To KERNEL

- Nothing new; deferred_wrap verdict still the standing open item.

## 6. Cycle-close status notes

- Measured at close: no-change frames hold 0 allocs / 0 reallocs / 0
  bytes (RT2-8 acceptance re-verified repeatedly); full-change steady
  state 0/0/0 (attribution ratchet); `flatten_stack` scratch-stable
  (capacity-pinned).
- Foreign failure at my close: `ui::tests::hover_memo_skips_same_position
  _but_honors_relayout` (REACT, mid-wave on their overlay/App side —
  their `src/app/overlays.rs` + driver landed during my cycle and
  compiles green; one of their ui tests is still red). My module filters:
  render 138, text 22, anim 35, all green; alloc_budget 5/5, adv_render
  13/13.
- SignalSpring skipped (not trivial; `reactive::animate` + `Transition`
  cover the need); Timeline `reverse()` skipped (no DESIGN ask; PingPong
  + explicit exit tracks cover the storyboard).
