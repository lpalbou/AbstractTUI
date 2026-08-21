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

## Design findings (engine seat, 2026-08-21)

Two slices of investigation before any API. Both are pinned by tests
that go red if the engine moves under them.

### Slice 1 — the app-side substitute cannot be built (`926ab89`)

`src/widgets/scroll_child_probe_tests.rs`. The obvious substitute is
the engine's own idiom: a paint closure per child recording the solved
rect it is handed (exactly how `extent_signal` works). It cannot work,
structurally: the child an ensure-visible verb must locate is BY
DEFINITION outside the viewport, and `draw.rs`'s cull skips any subtree
whose solved rect misses the clip, so a culled child never paints and
its probe never fires. The exemption that fixes it —
`Element::probe_when_culled`, which keeps a culled node's own paint
alive AND hands it the true solved rect — is `pub(crate)`.

### Slice 2 — the rect readback is already public; the NAME is missing

`src/widgets/scroll_child_identity_tests.rs`. Slice 1's conclusion was
one step too strong. `UiTree::rect_of` is already `pub` and reads the
LAYOUT solver, not paint — culling is a paint optimisation, so the
solved rect of an off-screen child is correct and reachable today:

- `off_screen_child_rect_is_already_public_through_rect_of` names a
  child while it is visible, scrolls it off the top, and shows
  `rect_of` reporting the true negative offset while the paint probe on
  the same node is stale at its last-painted row. The two disagree and
  `rect_of` is the one telling the truth.
- `a_below_the_fold_child_cannot_be_named_by_hit_test` sweeps every
  cell of the viewport and never reaches the child at content row 12 —
  the only reason being that it is off-screen, which the control
  proves by scrolling it in and re-running the identical sweep.

So the missing primitive is **child identity, not a rect**. `Element`
and `View` carry no identity field (`view.rs`: style, style_fn,
measure, drag_zone, draw, handlers, shortcuts, focusable, focus_trap,
focus_memory, autofocus, probe_when_culled, padding_floor, access,
children — no id, no key), and every public source of a `ViewId` is
post-mount and event-shaped: `UiTree::mount` (root only), `focused()`,
`hit_test(Point)`, `EventCtx::target()/current()`. Each needs the node
to be reachable by pointer or focus, which the target child is not.

### What this does to the report's two shapes

Answering question (i) — "how does a caller NAME a child?" — narrows
both. Naming has to be **caller-supplied**, because the blueprint layer
has no identity to hand out and `ViewId` does not exist until mount.

That is also what both engine precedents already do, and neither reads
a child rect back from the solver:

- `List` takes a caller-supplied `item_heights` closure, builds a
  prefix sum over it, and its private `ensure_visible` clamps the bound
  offset on content rows (`list.rs`). Its `scroll_to(Signal<Option<
  usize>>)` is the command surface.
- The graph extension's own `ensure_visible` probes only the
  **VIEWPORT** rect; the card rect comes from `nav[next].1`, the graph
  layout it authored itself (`extensions/graph/src/view.rs`). The
  report's "solved-rect readback" reading of this precedent is not what
  the code does.

Every working ensure-visible in this tree gets the child's position
from a model the caller already owns. Shape (2) — "`Scroll` exposes
per-child solved offsets" — would be the first that does not, and it is
a standing promise about internal layout that shape (1) does not need.

### Remaining open questions (unchanged, for the next slice)

(ii) whether a probe on EVERY child is affordable, since the exemption
makes culled children paint and the whole point of the cull is that
they do not — note this now only bites a design that keeps the paint
route, and `rect_of` avoids it entirely; (iii) whether (1) can ship
without exposing (2). Slice 2's answer to (i) suggests it can: a
caller-supplied key on the child, resolved to a `ViewId` at mount and
read through the already-public `rect_of`, is a strictly smaller
surface than a per-child offset seam.
