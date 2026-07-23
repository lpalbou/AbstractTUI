# 0910 — Scroll of widgets: no ensure-visible / child-offset verb (consumers hand-roll height models)

## Metadata
- Created: 2026-07-25
- Status: Proposed (field report — filed by the engine seat on
  agora-tui's behalf from the 2026-07-25 recommendations review;
  their code is the live evidence)
- Track: field-agora
- Severity: P2 (works today via a hand-rolled model; the model is the
  drift hazard)
- Engine: abstracttui 0.2.16

## The evidence

agora-tui's message pane is a `Scroll` column of real `Disclosure`
widgets (their post-Feed "chat era" shape). Keyboard selection must
keep the selected card in view, but `Scroll` exposes no "scroll this
CHILD into view" verb and no way to ask where a child landed — so the
app maintains `offset_of_card`: a hand-rolled height model (folded
card = 1 row, expanded = 2 + capped body rows) that recomputes the
selected card's y-offset and writes the bound scroll offset itself.

The model is correct today and drifts silently the day any of these
change: Disclosure chrome height, `max_body_rows` semantics, scrollbar
auto-hide reserving a column, wrap behavior of card titles. A consumer
re-deriving engine layout arithmetic is the exact class the engine
files against composers (the 0120 smell, their side of the fence).

The engine's own `GraphView` solved this internally with a paint-time
viewport probe (`ensure_visible` + solved-rect readback, wave-9/10) —
evidence the need is real and the machinery exists; it is just not a
public Scroll verb.

## The ask (smallest honest surface)

One of:
1. `Scroll::ensure_child_visible(key_or_index)` — the widget resolves
   the child's solved rect (paint-time probe, the GraphView pattern)
   and clamps the bound offset so the child is in view; or
2. a read seam: `Scroll` exposes per-child solved offsets/extents
   (post-layout signal or query), and the app keeps its one-line clamp.

Either kills the hand-rolled height model. Shape (1) is the ergonomic
end-state; shape (2) is the primitive both GraphView and (1) would
stand on.

## Validation
- A Scroll of mixed-height widgets (folded/expanded Disclosures),
  keyboard selection walked down past the viewport edge: the selected
  card is fully visible after each step, under resize, with and
  without the auto-hidden scrollbar column, wide glyphs in titles.
- agora-tui deletes `offset_of_card` on adoption — the acceptance
  proof, consumer-side.
