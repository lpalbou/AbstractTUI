# 0185 — Measure inflation around Tabs: fixed-height siblings crushed while grow slack exists

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review, `reviews/wave12/visual-to-code-handoff.md` §1;
  accepted by the code seat as "the top engine investigation for the
  next wave" — `reviews/wave12/code-to-visual-handoff.md` "#1")
- Track: app-widgets (core-widget/layout accuracy; the solver half
  overlaps `src/layout/`)
- Class: bug (layout solver / widget measure interaction)
- Severity: P2 — visibly breaks shells (a header row silently loses a
  line); `shrink(0.0)` works around it but only relocates the pressure
- Engine: reproduced on 0.2.18 (wave 12); re-verified present in
  0.2.22 (2026-07-25 — no solver or `tabs.rs` measure change since)

## The evidence

The wave-12 probe (`wave12_probe`, temporary example — recipe recorded
in the handoff): a column of `[header row .h(2), Tabs .grow(1.0),
footer]` at 110x32 rendered the header at ONE row (the Logo tagline
vanished) while the List inside the tab panel showed THREE EMPTY SLACK
ROWS. A crushed fixed-height sibling coexisting with grow slack in the
same column means the measure pass over-reports the column's desired
height, so shrink fires on siblings the solved layout did not need to
crush.

Reproduction (from the handoff, enough to write the failing test):
mount `column[ row.h(2)[Logo.tagline(true), Badge], Tabs{ one tab:
padded column with a 10-item List in a Block.min_h(6).grow }, text ]`
at 110x32 — the tab bar lands on row 3 instead of row 4.

## Current code reality (verified 2026-07-25, 0.2.22)

- `src/widgets/tabs.rs:164-168` — the BAR is already `h(2).shrink(0.0)`
  (the 0240 #2 fix: "a tight box crushes the PANEL, never the tabs").
  The bar is not the victim here; the SIBLINGS outside Tabs are.
- `src/widgets/tabs.rs:227-233` — the panel is
  `dyn_view(LayoutStyle::default().grow(1.0), …)`; its mounted content
  (the List in a `Block.min_h(6).grow`) carries intrinsic measures that
  propagate into the column's measure pass. Suspect (named in the
  handoff): Tabs' intrinsic measure of its content inflates the
  column's desired height beyond the viewport, so the solver's shrink
  pass hits fixed-height siblings even though the post-solve grow
  region holds slack rows.
- `src/layout/solve.rs` / `flex_math.rs` — unchanged since 2026-07-22;
  no fix has landed.
- The example-side mitigation (also the field workaround): `shrink(0.0)`
  on the fixed rows — which works, but moves the phantom deficit into
  the grow region instead of removing it.

## What we want

1. A minimal repro pinned as a FAILING layout test first (the code
   seat's stated precondition — layout-solver work without a pin would
   muddy behavior-neutrality proofs for everything else in flight).
2. The invariant restored: a fixed-height sibling must not shrink while
   the same axis holds unconsumed grow slack after solving. Whether the
   fix is measure-side (the grow-basis content should not inflate the
   parent's desired size — compare `Scroll`'s deliberate
   `basis(Cells(0))` posture, `src/widgets/scroll.rs:217-224`, and the
   0005-wave `basis(Cells(0))` default on the content views) or
   shrink-distribution-side is the investigation.

## Validation

- The handoff recipe as a unit/driver test: header keeps its 2 rows at
  110x32; the tab bar lands on row 4; the slack rows sit in the grow
  region only.
- `examples/widgets.rs` / `examples/components.rs` captures re-run
  (`cargo run --example capture -- review`) show no crushed chrome.
- Unrelated goldens byte-identical (the fix must not re-litigate 0240's
  defaults).

## Non-goals

- Redefining `shrink` semantics or defaulting more widgets to
  `shrink(0.0)` (0240 already ruled where that default belongs).
- The Block-internal crush-order question — that is 0175's half
  (first child dies first under interior crush).

## Related

- `0175_block_shadow_consumes_its_own_slot.md` (interior crush order),
  `0135_scroll_zero_width_collapse_over_measureless_trees.md` (the
  measure seam's width twin), completed first-app 0240 (the modal
  variant of fixed-row crush), wave-12 §1b bullet 6 (width hug inside
  Tabs panels — same measure-inflation family on the cross axis).
