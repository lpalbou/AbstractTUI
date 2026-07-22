# 0430 — Interactive node-graph editor widget (`abstracttui-graph`)

## Metadata
- Created: 2026-07-22
- Status: Proposed (needs-design; sibling-crate candidate under 0400's
  ruling; risk retired first by the read-only view 0440)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: 0400's ADR (this is a sibling-crate consumer of the
  anchor surface — public API only); ADR-0001 (its releases ride
  core's coupling budget). ADR impact: none in core; whatever core
  gaps the build reveals are filed as core items, never hooks.

## Context
The reference UX is a dataflow editor: node cards with title bars,
colored typed-port dots, inline fields; bezier curves between ports;
canvas panning; selection, drag, tooltips. The app classes: visual
pipeline editors (data/media/agent flows), patch-bay UIs, shader/audio
graphs, workflow builders. This is the single most demanded "beyond
lists and tables" surface in TUI ecosystems and none of the incumbent
Rust TUI stacks ship it — a differentiator, but only honest as an
EXTENSION: it is domain UI, not substrate (0400 decision table: its
own cadence, and a minimal app must not pay for it).

## Current code reality
The engine already supplies more of this than expected; the study
verified each mechanism at source:
- **Node cards are Element trees at absolute positions**:
  `Position::Absolute` + `Inset` exist in the layout model
  (src/layout/style.rs:95-101,150; builder `absolute()` at
  style.rs:382) and the solver honors them (src/layout/solve.rs:104,197).
- **Pan without remount is a solved pattern**: `Scroll` repositions
  mounted content by driving a reactive layout style
  (`Element::style_signal`, src/ui/view.rs:144-152) with negative
  absolute insets — "no remount, real solved rects, so hit testing and
  focus inside scrolled content keep working"
  (src/widgets/scroll.rs:1-9). A graph canvas pans the same way; node
  widget state (inline `TextInput` fields, src/widgets/input.rs)
  survives by construction.
- **Drag**: mouse-down auto-captures the target; `EventCtx::
  capture_pointer`/`release_pointer` are the explicit form
  (src/ui/event.rs:237-247) — scrollbar thumbs already ride this
  (scroll.rs:36-38), node dragging is the same gesture.
- **Hover/tooltips**: per-node `MouseEnter`/`MouseLeave` with
  subtree semantics (src/ui/event.rs:117-124), `hover_signal` sugar
  (src/ui/view.rs:271-277). Tooltip SURFACES consume app-kits 0500's
  anchored-popup substrate in its **TOOLTIP routing mode** (passive,
  non-interactive, `layer_draw`-backed, hover-timed — 0500 §3, which
  names this item's hovered-node-rect case in its consumer table) —
  NOT raw `Overlays::layer_draw`/`layer_tree` calls: the raw overlay
  API (src/app/overlays.rs:158-229) is the substrate 0500 builds on,
  and this widget stays one abstraction up so placement/flip/clamp
  and dismiss semantics are never re-derived here (cycle-4 residue
  from appkits, folded).
- **Edges**: no curve primitive exists today — chart.rs owns the only
  Bresenham on a private grid (src/widgets/chart.rs:82-105). 0420
  supplies bezier strokes; edges draw in a draw-closure layer under
  the node cards.
- **Edge/port activation, the 0165 synergy — with a named core gap**:
  cells carry a surface-local link id (`Cell::link`,
  src/render/cell.rs:310-311; `Surface::register_link` with capped,
  counted drops, src/render/surface.rs:125-131), and a `render::Style`
  can CARRY an id through `SurfaceCanvas::print_styled`
  (src/render/style.rs:48-49, src/ui/canvas.rs:234-239). But nothing
  reachable from a widget draw closure can REGISTER a URI to mint an
  id:   `resolve_link` works against `&mut Surface` directly
  (src/render/rich.rs:302-306), a type extensions never hold — the
  `StyledCanvas` trait has no registration surface (cycle-2 peer
  finding, appkits-on-extensions P1-1, verified). The seam is now
  FULLY SPECIFIED as **0480** (this band): a defaulted
  `StyledCanvas::register_link(uri) -> u16` (0 = no link) with
  SurfaceCanvas/ClippedCanvas/BufferCanvas overrides — the producer
  half of the link channel, standalone-valuable (OSC 8 terminal-side
  activation works without 0165). 0165 (band 0100) remains the
  consumer half (app-side hit-testing). Until BOTH halves land, edge
  activation uses the documented fallback below. Cell-granularity
  (not dot-granularity) — honest and sufficient for 2x4-dot strokes.
- **Zoom is NOT a solved pattern and cannot be continuous**: terminals
  do not scale glyphs. What exists: nothing. What is honest: discrete
  level-of-detail tiers (full card → compact card → colored dot),
  which is a view-model concern the extension owns.

## Problem
Building this app-side today means re-deriving edge strokes (no curve
API), edge hit-testing (per-widget mouse math — the exact anti-pattern
0165 files), and LOD/pan/selection conventions with no reference
implementation. The pieces exist; the composition is the product.

## What we want (proposed shape — needs-design)
A sibling crate `abstracttui-graph` (name per 0400) providing:
1. **Model**: `GraphState` (reactive, signal-backed like `FeedState`,
   src/widgets/feed.rs:167-187): nodes (id, position, title, ports
   typed by a small vocabulary the app extends, inline field slots),
   edges (from-port → to-port), selection set.
2. **Node cards**: composed from core widgets (Block chrome, text,
   TextInput fields, port dot rows) — zero custom chrome primitives;
   the card is an app-visible component recipe, overridable.
3. **Edge layer**: bezier curves (0420) with per-edge color from the
   theme chart ramp discipline; orthogonal-routing mode later
   (needs-design flag; obstacle avoidance is v2 at the earliest).
4. **Interaction**: pan (drag empty canvas / wheel), node drag
   (capture), port-to-port edge creation with live rubber-band curve,
   selection (click, additive with mods), delete/duplicate hooks as
   app callbacks (`Callback` convention), tooltips on hover.
5. **LOD zoom**: 2-3 discrete tiers, app-selectable; the API says
   "tier", never "scale factor" — honesty in the name.
6. **Edge hit-testing**: v1 = link-id stamping (0480, producer half)
   + 0165 resolution (consumer half) once both land; fallback
   (pre-seam) = sampled-polyline distance query answered by the
   extension (documented approximation).
7. Read-only rendering shares everything with 0440 (one crate, two
   entry widgets; 0440's auto-layout feeds initial positions here).
8. **Keyboard parity per milestone** (peer finding P1-3, accepted —
   the engine discipline is keyboard-first): node focus traversal
   rides the existing spatial focus (`UiTree::focus_next_in`, pinned
   at src/ui/mod.rs:214-227) since cards are focusable Elements;
   milestone M1 ships arrow/tab node traversal + keyboard pan;
   M2 ships keyboard edge creation (start-port → target-port
   selection mode); selection/delete/duplicate get chords beside
   pointer paths. No milestone ships a pointer-only interaction.

Staging (peer finding P1-3, accepted — one item was carrying an
editor's worth of scope; each milestone has its own acceptance gate):
- **M1 canvas core**: GraphState + card recipe + pan + node drag +
  selection + keyboard traversal, rendering shared with 0440.
- **M2 ports & edges**: typed port rows, rubber-band edge creation
  (pointer + keyboard), edge hit-testing (fallback path).
- **M3 polish**: tooltips (0500's anchored-popup TOOLTIP mode —
  hovered node rect as anchor; in-card dropdowns use its SELECT mode),
  LOD tiers, link-channel hit-testing when 0480 + 0165 land,
  auto-arrange via 0440.

## Scope / Non-goals
Scope: the crate, model, card recipe, edge layer, pan/drag/selection,
LOD tiers, tooltips, an editor example, tests over CaptureTerm —
staged M1/M2/M3 as above.
Non-goals: continuous zoom (impossible honestly); auto-layout while
editing (0440's algorithms run on demand, not per-frame); minimap
(later, needs a consumer); undo/redo stack (app-side; the model
exposes apply/serialize hooks); persistence formats (app-side).
Cross-band consumption (by band, peer finding P2-9): inline one-of-N
fields in node cards are app-kits 0500 (`Select`) — whose anchored
popup must open from an absolutely-positioned, panned card (placement
case recorded on their side); an inspector panel beside the canvas is
0580 (`SplitPane`); node status dots are 0540 vocabulary. Nothing
moves bands; the editor consumes them as public API when they land.

## Expected outcomes
A dataflow editor is an afternoon of app code on the extension: define
port types, hand `GraphState` to the widget, wire callbacks. The
engine's zero-idle/damage story holds (pan damages the moved region
via style_signal, idle graph costs nothing).

## Validation
- CaptureTerm acceptance: drag node (capture holds outside rect), pan
  canvas, rubber-band edge creation lands on a port, tooltip on
  hover, selection visuals, LOD tier switch.
- Edge stroke determinism (0420 pins) + link-id stamping counted
  against the cap semantics (src/render/surface.rs:123-131).
- Zero-idle pin: an idle mounted graph schedules nothing (extends the
  engine's inviolable idle discipline to the extension's CI).
- Widget-state survival: text typed in a node's inline field survives
  pan/drag (the no-remount guarantee, scroll.rs:1-9 precedent).

## Progress checklist
- [ ] 0400 ruled; crate skeleton against published core
- [ ] GraphState model + card recipe
- [ ] Edge layer over 0420 (bezier, rubber-band)
- [ ] Pan/drag/selection/tooltips
- [ ] LOD tiers
- [ ] Hit-testing (0165 path + documented fallback)
- [ ] Example editor + CaptureTerm suite + idle pin
