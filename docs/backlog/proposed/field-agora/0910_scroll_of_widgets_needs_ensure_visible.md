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

### Slice 3 — the focus-driven case needs NO new engine API

`src/widgets/scroll_ensure_visible_tests.rs`. Slice 2 said naming has to
be caller-supplied. FOCUS is the loophole, and it happens to be exactly
the case this report describes: if keyboard selection moves focus, then
`UiTree::focused()` hands back the selected child's `ViewId` for free,
and `rect_of` answers correctly for it even unpainted.

`keyboard_selection_can_ensure_visible_today_with_no_new_engine_api`
builds the entire verb from public API — `focus_first`/`focus_next`,
`focused`, `rect_of`, a bound `offset_y` — over a column of MIXED-height
children (every third card 3 rows, the rest 1: the report's
folded/expanded shape). Selection walks from the top to a child far
below the fold, clamping after each step, and the selected child is
fully visible at every step. No `Element` key, no per-child offset seam,
no new primitive.

`focus_alone_does_not_scroll_the_selected_child_into_view` is why the
item stays open: the engine does not apply the clamp. Focus moves past
the fold and the bound offset never moves.

Three falsifications, all verified:

- Neuter the app-side clamp: the walk test goes red on the
  fully-visible assertion, the second test stays green.
- **Swap the layout readback for the hand-rolled uniform `index * 1`
  model — the exact thing this report is filed about — and the test
  goes red.** That is the drift hazard demonstrated rather than
  asserted, and it is the strongest evidence in the item.
- A content control asserts the true content offset of the target child
  differs from the uniform guess, so the mixed heights are load-bearing
  rather than incidental.

### Answer to (iii), and the honest remaining scope

Shape (1) does not need shape (2) underneath it, and for the
focus-driven case it needs no identity primitive either. A `Scroll`
verb here would be **sugar over a five-line clamp the app can already
write** — worth having for ergonomics and for single-sourcing the
clamp (the reason `List::ensure_visible` is shared between its
selection and `scroll_to` paths), but it is not the missing-primitive
item the report describes.

What is genuinely unanswered, and all that is left of 0910:

- selection that deliberately does NOT move focus;
- locating a child nobody has selected.

Both still need caller-supplied naming per slice 2. Neither is what the
report's evidence (`offset_of_card`, driven by keyboard selection)
actually needs — so the consumer's `offset_of_card` can be deleted
today, which is the acceptance proof the report names, without the
engine shipping anything at all.

### Slice 4: the consumer picked shape (2), and named the hazard in it

agora-tui answered from `src/ui/panes.rs` rather than from preference
(DM, 2026-08-21). Their card column is a `dyn_view_scoped` closure that
reads `selected.get()` **tracked** (`panes.rs:1039-1041`), so a selection
change rebuilds the whole column and `is_selected` is already computed
per row (`panes.rs:1177`). That settles two things that were open:

- **One rect signal per PANE, not one per card.** The binding goes on
  whichever card has `is_selected`, and re-binds to the new card on the
  next rebuild, so exactly one element publishes into it at a time. It
  lives beside the three signals they already bind — `offset`, `extent`,
  `viewport` (`panes.rs:49-70`).
- **The clamp is already an effect on their side.** Ensure-visible
  becomes another effect reading a rect and writing `w.offset`: no new
  plumbing in the consumer at all.

**Their question, answered from this tree: `Scroll` does NOT cull out of
layout.** The cull is a paint-time rect test — `draw.rs` reads the solved
rect from the layout tree and only then skips a subtree whose rect misses
the clip. A scrolled-away child is mounted, solved and hit-testable; it
simply does not paint. So a selected card off-screen still has a real
rect to publish, and shape (2) is not dead the way `Scroll`'s own verb was.

**One thing they did not ask about and would have hit.** "Not painted" is
two different states in `draw.rs`, and only one of them has a usable rect:
a culled child is off-screen with a truthful rect, while a **zero-area**
child falls through the cull entirely (an empty rect intersects nothing)
and is painted as clean absence. A rect binding must not let a collapsed
`0x0` reach the clamp as if it were a position — that is an
ensure-visible scrolling to the origin, which reads as a jump to the top
rather than as a layout collapse.

**The blocking constraint on any API this repo ships here: an unmounting
element MUST NOT publish.** Because a selection change rebuilds, the
binding moves from card A to card B in one pass; if a disposing element
writes its rect on the way out, write ordering decides whether the
consumer reads B's rect or A's corpse. The failure is silent — an
ensure-visible that scrolls to the *previous* selection looks like an
off-by-one, not like a lifetime bug.

That guarantee is this engine's to make, not the consumer's to work
around. **It did not hold, and it does now.**

The paragraph that stood here said the mechanism "already exists" and
pointed at `cx.on_cleanup`. That was reasoning about the engine rather
than measuring it, and it was wrong. `size_probe` — the published-from-
layout binding a `rect_signal` would be built on — does not write from
paint: it records the rect and defers ONE `after(0)`. Its guard asked
whether the SIGNAL was alive. That is not the same question as whether
the ELEMENT is, and the difference is exactly this topology: a pane owns
the signal, so it outlives every card, and a card disposed between
scheduling and firing published its rect anyway.

Measured, not argued —
`a_publish_scheduled_before_disposal_does_not_land_after_it` in
`tests/scroll_remount_offset.rs` fails against the old guard with a
concrete corpse write: `(43, 1)` landing after the element was gone.

Fixed by giving `size_probe` the mounting `Scope` and an `on_cleanup`
liveness flag it checks before publishing. All four call sites in
`scroll.rs` and `list.rs` pass their own `cx`, so the rule holds for
`extent_signal`, `viewport_size_signal` and `List`'s viewport probe too —
not only for the binding 0910 would add.

**Note which test caught it.** The steady-state case written first —
park, dispose, assert the signal is untouched — PASSED against the broken
guard, because by then no publish was in flight. It is decoration for
this hazard and is kept only for the case it does cover. The race needed
constructing deliberately: one turn so the probe schedules, then dispose
before settling. A disposal test that never has a pending write is
testing nothing about disposal.

The consumer offered to key the effect on a `(rect, card-key)` pair so a
stale write would be detectable. They should not have to, and now they
do not.
