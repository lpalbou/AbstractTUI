# 0445 — GraphView: force layouts open mostly off-view; no initial centering or `.center()` verb

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §4; agreed by the code
  seat — `reviews/wave12/code-to-visual-handoff.md` "#4: reasonable
  `.center()` builder or first-render bbox centering")
- Track: extensions (`abstracttui-graph`, beside completed 0440)
- Class: UX defect (first-frame honesty of the shipped example)
- Severity: P3 — pan recovers it, but the first frame is the first
  impression and the `network` example currently fronts mostly empty
  canvas
- Engine: reproduced on 0.2.18 / abstracttui-graph 0.1.0 (wave 12);
  re-verified present 2026-07-25 (no centering code in
  `extensions/graph/src/view.rs`)

## The evidence

`network--initial.svg` (wave-12 capture family): the example's first
frame is two nodes in the top-left, two long edge strokes, and a
mostly empty right half — the force layout's mass sits away from the
(0,0) initial pan. The `workflow` (layered) example fronts fine; this
is force-specific: layered passes pack from the flow origin, while the
force pass settles mass wherever the simulation leaves it inside the
solved bounds.

## Current code reality (verified 2026-07-25)

- `extensions/graph/src/view.rs:90-92, 220-221` — `offset_x`/`offset_y`
  are optional bound signals defaulting to fresh `signal(0)`; the
  content mounts in a `Scroll::new(content).content_size(bw, bh)`
  (`view.rs:355-359`). Initial pan is therefore always the layout
  bbox's top-left corner.
- No `.center()` builder, no first-render bbox centering, no fit verb
  exists in the crate.
- The offset bindings are also the app's pan-ownership surface —
  whatever centers must not fight an app-bound signal.

## What we want

Center the layout's mass on first render when the caller did NOT bind
offsets (or provide `.center()` as the explicit builder; both were
offered in the handoff and either satisfies it):

- Default-unbound offsets initialize to
  `((bbox_w − viewport_w)/2, (bbox_h − viewport_h)/2).max(0)` instead
  of (0,0) — a pure initialization, so pan/keyboard nav behave exactly
  as today afterward.
- Caller-BOUND offsets keep full app ownership (initialize to the
  signal's value, as now) — the app may itself want a remembered pan.
- A fit-to-view zoom is explicitly out of scope (cell-grid rendering
  has no zoom); this is centering only.

## Validation

- `network` example first frame: the node mass is visibly centered
  (capture re-run shows nodes on both halves of the pane).
- `workflow` (layered) first frame byte-identical (its bbox corner and
  mass already coincide — centering must not shift it, or if it does,
  the shift is reviewed deliberately).
- A bound-offset test: pre-seeded `offset_x/y` signals are respected
  verbatim on first render.

## Non-goals

- Zoom/fit-to-view, animated pan-to-center, or re-centering on
  RELAYOUT (a relayout under user pan must never yank the view — the
  0281 offset-repair lesson transfers).

## Related

- Completed extensions 0440 (the crate + both layout passes),
  field-agora 0910 (Scroll ensure-visible — the same "the engine knows
  the geometry, let it aim the viewport" family on the core side).
