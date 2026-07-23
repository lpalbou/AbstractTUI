# 0175 — `Block::shadow` consumes a row/col of the block's own slot; under crush the FIRST child dies, order-dependently

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §1b, first + third
  bullets; recorded by the code seat in
  `reviews/wave12/code-to-visual-handoff.md`)
- Track: app-widgets (widget sizing accuracy)
- Class: footgun (documented-but-easy-to-miss sizing arithmetic) +
  bug (order-dependent crush victim)
- Severity: P2 — reads as "my label randomly disappeared"; the two
  behaviors compose into silent content loss
- Engine: reproduced on 0.2.18 (wave 12); re-verified present in
  0.2.22 (2026-07-25)

## The evidence

Two independently-confirmed behaviors from the components stat-card
probes, which compose badly:

1. **The shadow lives INSIDE the box.** A shadowed block with `.h(5)`
   paints 4 glyph rows + 1 shadow row. A caller sizing
   "title + N children + border" is short by one — and the block
   CRUSHES A CHILD instead of dropping the shadow. Repro: probe card
   with `.h(4)`, title, 2 one-row children — the first child vanishes.
2. **Under interior crush, the FIRST child dies first** — swapping the
   children swaps the victim. Combined with the shadow row this is the
   "randomly disappearing label" experience.

The fixed `examples/components.rs` `stat_card` documents the working
recipe (shadow-inclusive explicit heights); the before/after pairs are
in the wave-12 capture artifacts (`untracked/review-shots/`, not
tracked).

## Current code reality (verified 2026-07-25, 0.2.22)

- `src/widgets/block.rs:248-257` — shadow adds to the chrome edges
  (`chrome.right += 1; chrome.bottom += 1` via `padding_floor`), i.e.
  the strip takes the last column/row of the block's OWN layout slot.
  This is a deliberate design ("children stay inside the lifted
  panel"), and the `.shadow()` builder doc (`block.rs:163-167`) does
  say "the panel's chrome shrinks by one cell each way to make room" —
  but nothing states the consequence a caller trips on: an `.h(N)`
  shadowed block yields N−1 visual panel rows, and the failure mode
  when the arithmetic is off is a crushed CHILD, not a dropped shadow.
- The crush-order half is solver behavior (first flex child absorbs
  the deficit), not Block-specific — but Block is where users meet it,
  because chrome makes the off-by-one likely.

## What we want

A ruling plus its execution, two candidate shapes:

- **(a) Chrome yields before content.** Under height/width pressure the
  shadow strip drops before any child crushes to zero (precedent: the
  0605 close-affordance truncation ladder — chrome yields in a pinned
  order, "nothing ever paints outside the frame"). The shadow is
  decoration; a child is content.
- **(b) Keep shadow-inside-the-slot, make the failure honest.** Docs on
  the widget (docs.rs reaches users; api.md prose does not) state the
  N−1 arithmetic with an example, and the crush-victim order becomes
  deterministic and documented (or proportional) rather than
  first-child-eats-everything — that half belongs with the 0185
  measure/shrink investigation.

Either way the order-dependence deserves a pinned test: swapping two
equal children must not change WHICH content survives.

## Validation

- A/B unit test: shadowed `.h(5)` block with title + 2 one-row
  children — under (a) all content paints and the shadow drops; under
  (b) the behavior is unchanged but documented and the victim is
  deterministic under child reorder.
- `examples/components.rs` stat cards keep their current pixels
  (their explicit heights already account for the shadow).

## Non-goals

- Changing the shadow's visual design (offset strip, pre-composited
  `shadow_ground` token — RT1-9b) or its cost model (one-time paint).
- The cross-axis stretch question from §1b bullet 2 ("a Block does not
  cross-stretch to its row slot") — same family, but that is a
  layout-default question that should ride the 0185 investigation, not
  this item.

## Related

- 0185 (measure inflation / shrink distribution), completed first-app
  0240 (fixed rows crushed in modals — the defaults ruling), completed
  app-kits 0605 (the chrome-yields truncation-ladder precedent).
