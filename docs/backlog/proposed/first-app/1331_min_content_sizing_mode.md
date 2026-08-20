# Proposed: the solver has no min-content sizing mode — the prerequisite for every "the engine should have figured that out" layout fix

## Metadata
- Created: 2026-08-20 (filed out of 1330's completion; both of that
  item's automatic directions failed against this same wall)
- Status: Proposed
- Class: layout-solver capability gap

## ADR status
- Governing ADRs: None. ADR impact: adopting this changes flex sizing
  semantics and would deserve its own note.

## Context
`abstracttui`'s layout solver has exactly one sizing question:
`intrinsic_size` (src/layout/solve.rs), which answers "how big does this
subtree want to be" — a max-content estimate. CSS flexbox needs two. The
second, min-content, is what CSS's automatic minimum size
(`min-height: auto`) is built on: a flex item does not shrink below the
smallest size its content can take without overflowing.

Every proposal of the form "the container should not be allowed to be
shorter than the child that refuses to shrink" reduces to needing that
second answer, and the engine cannot give it.

## Current code reality
Verified 2026-08-20 against 0.3.6.

- `intrinsic_size` (src/layout/solve.rs) short-circuits on explicit
  dimensions: `need_content` is false when both axes resolve, and the
  result is `explicit_h.unwrap_or(content.h)`. It therefore **cannot see
  content through an explicitly-sized ancestor** — a min-content walk
  needs to descend past one.
- It also ignores `style.basis`, while the flex solve twelve lines below
  documents the opposite precedence (`flex-basis > explicit main size >
  intrinsic content`). The two disagree by construction.
- There is no notion of a measure callback answering a DIFFERENT question
  under min-content pressure (a wrapped text block's min-content height
  is its fully-wrapped height at the narrowest column, not its current
  one). `MeasureFn` takes an available size and returns one answer.

## Problem or opportunity
Two independent attempts, both measured, both blocked here:

1. **CSS automatic minimum size** (1330 direction 1). Implemented
   faithfully in a throwaway worktree: 25 test failures, including
   `scroll::tests::default_layout_takes_leftover_not_content_basis` —
   the content-derived minimum clamps `basis(Cells(0))` straight back
   up, structurally annihilating the 0240 follow-up #1 guarantee — plus
   both `modal_fixed_row*` acceptance tests and the `zero_collapse_*`
   suite. The aggressive non-CSS variant fails identically, because
   `intrinsic_size` cannot see content through an explicitly-sized
   ancestor.
2. **Honoring a child's `basis` in `intrinsic_size`** (attempted during
   1330). This is the root cause of the pressure in BOTH field reports
   of the class — 1330 and field-agora 0840 — and it is a consistency
   fix, not new semantics: it makes `Scroll`'s `basis(Cells(0))` default
   survive a wrapper instead of the wrapper re-deriving a content-sized
   basis one level up. It regressed
   `wave_extensions_accept::pipeline_monitor_scene_end_to_end`: with
   nothing to floor the collapse, a content-sized ancestor holding only
   a basis-0 viewport goes to zero, so a `PageHost` with no `grow`
   solved to its 2-row tab bar and the graph never got a box. CSS avoids
   exactly this with the automatic minimum — i.e. with min-content.

Each fix needs the other. Landing min-content sizing first makes both
available as ordinary, separately-testable changes.

## Candidate directions
1. A second fold, `min_content_size(tree, id)`, that descends past
   explicit dimensions and takes the max (not the sum) across a
   container's children on the main axis, with `MeasureFn` gaining an
   optional min-content answer for text-shaped leaves.
2. Cache it per node per solve — a second full recursive walk on every
   solve is not free, and the existing fold is already recursive over
   depth.
3. Then, as separate items: CSS automatic minimum size keyed off
   `Overflow` (CSS exempts clipping containers, which is exactly the
   escape hatch `Scroll` already declares), and basis-aware intrinsic
   sizing.

## Why it might matter
This is the common blocker under the whole "my row vanished / my pane
overflowed / my grow ratio did nothing" family. The engine currently
answers that family with diagnostics and documented recipes, which is
honest and cheap — but every proposal to make it automatic stops here.

## Promotion criteria
Promote when a layout-semantics cycle opens. It is a solver capability,
not a bug fix, and it wants room to be benchmarked.

## Validation ideas
- `basis(Cells(0))` on a `Scroll` inside an AUTO-basis wrapper: the
  wrapper exerts no pressure on a fixed sibling one level up, AND a
  content-sized ancestor holding only that wrapper still gets a box.
- The `pipeline_monitor_scene_end_to_end` scene keeps rendering.
- The 0240 guarantees hold unchanged.
- A benchmark: solve cost on a deep tree does not regress materially.

## Non-goals
No change to the diagnostics landed in 1330 — they stay useful whatever
the solver learns.
