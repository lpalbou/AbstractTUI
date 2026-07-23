# 0585 — Global drawer system: edge-anchored overlay panels

## Metadata
- Created: 2026-07-24
- Status: Completed
- Track: app-kits
- Completed: 2026-07-24
- Depends on: nothing in-band (rides shipped machinery: `app::Overlays`
  tree/manual layers, `reactive::animate`, the Modal/Popup dismissal
  contracts). Sibling, deliberately NOT absorbed: 0580's `PanelRail`/
  `SplitPane` are IN-LAYOUT docked regions (they consume layout space
  and resize their neighbors); this item's `Drawer` is an OVERLAY
  panel (it covers the page and leaves the layout untouched). An app
  may use both: a continuum-style persistent right rail is 0580; an
  on-demand inspector or nav panel is a Drawer. Cross-referenced both
  ways; neither replaces the other.
- Validator: `examples/drawers.rs` (left nav drawer + right inspector
  drawer over a page, toggle keys, headless exit-0); the tab/page host
  wave's `examples/shell.rs` adopts a drawer in its marked region when
  that example lands.
- Promotion trigger: the maintainer's brief (2026-07-24): "a global
  drawer system (by drawer system, look at what we have done for the
  entity app)… these higher-level containers should be able to contain
  full complex pages."

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (composition over the overlay store + anim system;
  one disposal-safety guard inside `reactive::animate`, see below).

## Context
The entity app's drawer (`AfDrawer` in the AbstractUIC ui-kit, hosting
the observer's assistant/inspector panels) is the reference feel:
right-edge panel, fixed width, OPAQUE surface (the "page text bled
straight through the assistant drawer" incident made opacity a
documented rule), hairline on the leading edge, themed header with a
title and a close affordance, Esc closes the topmost layer. The
continuum screenshot pattern (right-edge vertical rail of collapsible
panels) is 0580's docked-rail territory — this item is the summonable
overlay HALF of that family. Terminal apps want the same gesture:
summon a full page (an inspector Feed, a form, a reader) from an edge
without disturbing the layout beneath, dismiss it, pay nothing while
it is closed.

## Current code reality
- **No drawer**: grep drawer/slide-panel — zero matches in src/.
- **The substrate exists whole**: `Overlays::layer_tree` mounts a full
  UI world on a z-ordered layer with modal input routing
  (overlays.rs:194); `Modal` (popups.rs:45) is the focus-trap + scrim-
  token + z-band precedent; `Popup` (anchored_owned.rs) is the
  dismissal-contract precedent (idempotent end, named reasons,
  scope-cleanup safety); `Toast` (popups.rs:146) is the transition
  precedent — a progress signal, a `reactive::animate` follower, an
  effect driving `LayerHandle::set_offset` per frame, zero frames
  parked (acceptance-pinned in `app/acceptance.rs`).
- **Slide damage is already fair**: `Layer::set_origin` damages
  old ∪ new bounds (layer.rs:252-262) — a sliding panel bills exactly
  its band, nothing else.
- **A scrim is expressible without new render machinery**: the
  compositor veils below through glyph-LESS translucent-bg cells
  (compositor.rs compose_cell, Normal branch); `TokenId::Overlay` is
  documented as "Modal scrim, composited over whatever it covers
  (carries alpha)" — a manual layer filled once with bg-only veil
  cells is a zero-per-frame scrim.
- **Keep-alive-while-closed CANNOT be a hidden mounted tree**: a
  hidden tree layer still accumulates damage from its live signals,
  `Overlays::has_pending_work` reports it (overlays.rs:310-318 checks
  trees without a visibility filter), but `draw_all` SKIPS invisible
  layers — the damage never drains, so the driver would schedule
  frames forever. The zero-idle-lawful shape is: closed = layer
  removed + mount scope disposed; persistent state lives OUTSIDE the
  builder (the documented Tabs pattern, tabs.rs:3-8), long-running
  work rides scopes that survive the mount.
- **`reactive::animate` has a disposal gap this item must close**: its
  frame task writes the follower signal with panicking accessors
  (animate.rs:90/95); a follower whose owning scope dies MID-FLIGHT
  (dyn_view regeneration, this item's replace/host-gone paths) panics
  on the next frame. `Meter`'s frame task shows the house pattern
  (`if !repaint.is_alive() { drop the task }`, meter.rs:410-415).

## Problem
Apps that want an entity-app-style side panel must hand-roll: layer
creation, slide billing, focus policy (trap vs glanceable), scrim
semantics, Esc/outside dismissal, one-per-edge arbitration, resize
behavior, and closed-state cost — eight policies, each with a wrong
default available (the hidden-mounted-tree spin is the predictable
failure).

## What we want
1. **`Drawer`** (app-layer, beside Modal/Toast — it needs the overlay
   store): edge-anchored overlay panel.
   - Edges: Left/Right (primary), Top/Bottom (same machinery, cheap).
   - Size: `DrawerSize::Cells(n)` or `::Percent(f)` of the viewport
     axis, clamped.
   - Content: any `View` built per open on a mount scope
     (`install(cx, build)` where `build: Fn(Scope) -> View`); state
     that must survive close lives outside the builder (Tabs rule,
     stated in docs).
   - Handle: `DrawerHandle` — `open`/`close`/`toggle`/`is_open`, plus
     controlled mode: `bind(Signal<bool>)` follows and writes back.
   - Transition: slide via the Toast recipe (progress signal +
     `animate` + offset effect). Frames ONLY during the flight;
     settled/parked/closed = zero (idle pin). `motion(Duration::ZERO)`
     is the instant mode — terminals cannot report a reduced-motion
     preference, so the knob is honestly app-owned.
   - Focus: `DrawerFocus::Modal` (default — focus-trapped tree layer,
     Esc closes, outside press closes when configured) vs `Passive`
     (glanceable; keys stay with the main surface until the user
     clicks into the panel — the engine's cycle-5 focused-overlay
     rule). NOTE the honest divergence: the web `AfDrawer` is
     non-modal in a mouse-first browser; in a keyboard-first terminal
     an unfocused panel cannot even scroll, so Modal is the right
     default here and Passive is the entity-app parity mode.
   - Scrim: modal-only (dimming content that stays interactive would
     lie), default ON in modal mode, `TokenId::Overlay` veil cells on
     a manual layer — painted at open/resize, never per frame.
   - Header: optional (`title(..)`) — title + muted Esc hint + a ✕
     close button; panel ground is the OPAQUE `surface` token with a
     `border` hairline on the leading edge (the AfDrawer look).
2. **Stacking laws** (documented + tested):
   - Drawer band sits BELOW `MODAL_Z`: fixed per-edge z slots
     (scrim/panel pairs), Left < Right < Top < Bottom. A modal opened
     from a drawer (`MODAL_Z = 1000`) layers above it; popups
     (`top_z()+1`) above everything live; toasts (2000) above modals.
     Fixed slots avoid both the equal-z stale-order trap (0500
     follow-up #2) and unbounded z creep.
   - One drawer per edge: opening on an occupied edge REPLACES —
     the incumbent finishes instantly with `Replaced` (an animated
     handoff would stack two panels on one z slot).
   - Resize RE-CLAMPS, never dismisses (unlike `Popup`, which closes
     on resize because its captured anchor goes stale — a drawer's
     anchor is the edge itself, which moves deterministically):
     geometry recomputes, the layer surface + tree viewport resize,
     an in-flight slide continues against the fresh geometry.
3. **Close reasons**: `DrawerCloseReason` (`#[non_exhaustive]`):
   `Api`, `Escape`, `OutsidePress`, `Replaced`, `HostGone`; `on_close`
   fires once per close with the first reason.
4. **The animate disposal guard**: the frame task checks
   `out.is_alive()` and cancels quietly when the owner died mid-flight
   (the Meter pattern) — an engine-wide disposal-safety fix this
   item's replace/host-gone paths require, and any dyn_view-hosted
   animation already needed.

## Scope / Non-goals
Scope: the Drawer component (both focus modes, scrim, header,
transition, instant mode), stacking laws + one-per-edge registry,
resize re-clamp, close reasons, the animate guard, demo example,
docs (api.md section), unit + wave tests.
Non-goals: docked/persistent rails and split panes (0580); drag-to-
resize drawers (0580's divider vocabulary, if ever); drawer stacks
per edge (one is the law; a second panel is a modal or a 0580 rail);
nested drawers; swipe gestures.

## Expected outcomes
An inspector or nav panel is one `Drawer::new(edge).install(cx, build)`
plus a keybinding; the entity-app feel (opaque panel, edge hairline,
titled header, Esc close) arrives themed by construction; closed
drawers cost literally nothing; the tab-host wave's shell example can
host full pages in drawers without touching its page layout.

## Validation
- Unit: geometry solve/clamp per edge+size; open/close/toggle both
  wire spellings (handle + bound signal, including external signal
  writes); modal focus trap + Esc; passive keeps keys with the app
  until click-in; scrim present/absent by mode + config; outside-press
  close on/off; one-per-edge replace fires `Replaced`; resize
  re-clamps geometry (no dismissal); host-scope death closes with
  `HostGone`; on_close exactly-once per close; animate disposal guard
  (dispose mid-flight, next frame-task pass is quiet).
- Wave (Driver + CaptureTerm, injected clock): slide-in emits frames
  only during the transition then `frame_tasks_pending() == 0` and an
  idle turn emits ZERO bytes (the Toast acceptance standard); mid-
  slide frames never re-emit content outside the drawer band; a Feed
  page inside the drawer scrolls; Modal-from-drawer layers above and
  returns input to the drawer on close; full open→close cycle returns
  the vacated cells to the page and idle to zero bytes.
- Gates: whole-tree cargo test green; clippy zero; fmt clean; alloc
  pins green; `cargo semver-checks` additive-clean vs 0.2.11.

## Progress checklist
- [x] animate disposal guard + test
- [x] Drawer core (geometry, registry, handle verbs, reasons)
- [x] Panel view (header, hairline, opaque ground) + scrim layer
- [x] Transition + idle pin; instant mode
- [x] Resize re-clamp; one-per-edge replace
- [x] Unit + wave tests; example; api.md; CHANGELOG

## Completion report (2026-07-24)

**Shipped.** `src/app/drawer.rs` (types, geometry, builder, handle) +
`drawer_open.rs` (`#[path]`-included private CHILD — verbs + the
one-per-edge registry; Inner's fields stay module-private) +
`drawer_view.rs` (panel chrome: opaque `surface` ground, leading-edge
`border` hairline, header row with muted esc hint + focusable ✕,
substrate-owned Escape) + `drawer_tests.rs` (16 unit tests) +
`tests/wave_drawers.rs` (5 driver-level acceptance tests) +
`examples/drawers.rs` (standalone demo, headless exit-0) + the two
marked DRAWER regions in `examples/shell.rs` (co-owned with the
page-host wave; drawers there resolve the overlay store through
reactive context and toggle via global actions `i`/`g`). One engine
fix: `reactive::animate`'s frame task cancels quietly when its
follower's scope died mid-flight (the Meter frame-task rule; was a
panic on a disposed-signal write — reachable from ANY dyn_view
regeneration that unmounts an animating component, not just drawers).

**The zero-idle proof** (`drawer_slide_frames_only_during_transition_
then_idle_zero_bytes`): on an injected clock, the open slide emits ≥2
frames, lands with `frame_tasks_pending() == 0`, and the next turn
emits ZERO bytes; the close slide removes the layers, the page
repaints, and idle is zero bytes again. Band containment
(`mid_slide_frames_stay_inside_the_drawer_band`): with the scrim off,
no mid-slide frame re-emits content outside the drawer band
(`Layer::set_origin` bills old ∪ new bounds — verified through real
present bytes).

**Decisions of record**:
- Keep-alive rejected structurally: a hidden mounted tree accumulates
  damage `draw_all` never drains (invisible layers skip) while
  `has_pending_work` keeps reporting it — a frame spin. Closed =
  removed + disposed; the Tabs state rule is the documented recipe
  (test-pinned: `state_outside_the_builder_survives_reopen`).
- Modal default, Passive parity: the web `AfDrawer` is non-modal in a
  mouse-first browser; an unfocused terminal panel cannot even scroll.
- Scrim is modal-only and STATIC (painted at open/resize, never per
  frame — a fading scrim would bill the whole viewport per transition
  frame); glyph-less `overlay`-token veil cells on a manual layer.
- Fixed per-edge z slots (Left 800/801, Right 802/803, Top 804/805,
  Bottom 806/807) — no equal-z stale-order trap, no z creep, corners
  deterministic; `MODAL_Z` stays above the whole band.
- Replace-on-claim finishes the incumbent INSTANTLY (an animated
  handoff would stack two panels on one slot).
- Resize re-clamps; an in-flight slide continues toward the fresh
  geometry (the offset effect re-reads a geometry signal — no snap).

**Follow-ups revealed** (none blocking):
1. **Footer hints in `examples/shell.rs` are page-host-owned**: the
   drawer toggle keys register as global actions (KeymapHelp-visible)
   but the visible footer line belongs to the tab-host wave — folding
   " · i inspector · g nav" into it is a one-string edit for that
   file's owner (region discipline kept me out of the line).
2. **Theme switch while open**: panel/scrim tokens resolve at open
   (the Modal rule); a mid-open theme switch lands at the next open.
   If drawers become long-lived chrome in practice, a theme-watcher
   repaint is a small follow-up shared with Modal.
3. **`DrawerHandle::close_with` is pub(super)**: the chrome's
   Esc/✕ paths use internally-named reasons; if apps want to close
   with a custom reason vocabulary, a public spelling is a one-line
   addition (deliberately deferred — reasons are engine-named today).
4. **0580 remains the docked half**: rails/split panes consume layout
   and resize neighbors; nothing here absorbs that item. The two
   compose (a persistent rail + an on-demand drawer) — cross-referenced
   in both items.
