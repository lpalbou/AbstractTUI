# Completed: Overflowing modal content silently shrinks fixed rows to zero

## Metadata
- Created: 2026-07-21
- Status: Completed (wave cycle 1, STABILITY seat — modal half landed;
  widget-defaults half filed to the app-widgets track, see the
  completion report)
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None. ADR impact: none — flex semantics stay; this is about
  defaults, diagnostics, and a documented recipe.

## Context
`abstractcode-tui`'s tool-approval modal laid out a column: title row (h 1),
an args `Scroll` (`grow(1.0)`), a button row (h 1), a hint row (h 1), inside a
fixed-size `Modal`. When the arguments were long, the approve/deny BUTTONS
rendered as zero rows — invisible — while the title, args, and hint stayed
visible. Nothing warned; the screen simply lacked the controls the modal
exists to offer. Diagnosing this cost a debugging session with a minimal
probe (two buttons in a 30x6 modal: both vanished).

## Current code reality
- The layout solver applies flexbox shrink semantics; children default to
  shrinkable, so when content overflows the fixed modal panel, fixed-height
  rows (Button is `height: Cells(1)` by default, src/widgets/button.rs:136-140)
  shrink toward zero alongside everything else.
- `Scroll` declares `grow(1.0)` but no `basis`, so its BASIS is its content
  size (src/widgets/scroll.rs:101-103) — a long transcript inside a modal
  makes the scroll's basis huge, guaranteeing overflow pressure on its
  siblings even though the scroll exists precisely to absorb overflow.
- The working recipe (validated in the field):
  `Scroll ... .layout(LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0)))`
  plus `.shrink(0.0)` on every fixed row. The dashboard's log panel uses the
  same `basis(Cells(0))` trick (examples/dashboard/main.rs:491) but no doc
  names WHY.

## Problem or opportunity
Three compounding defaults produce invisible controls with zero diagnostics:
shrinkable fixed rows, content-sized scroll basis, and no "child collapsed to
zero" signal. Every modal with a scrollable middle will hit this; the failure
mode (missing buttons in an approval dialog) is the worst possible surface
for it.

## Proposed direction
Any of these independently helps; together they close the class:
1. `Scroll::element` defaults its layout to `grow(1.0).basis(Cells(0))` —
   a viewport's basis should never be its content size; that is the widget's
   whole premise. (Smallest, highest-value fix.)
2. Widgets with intrinsic one-row heights (Button, Checkbox, TextInput's
   input row) default `shrink(0.0)` — a control that renders at zero height
   is never what the author meant.
3. A debug aid: when `Compositor::set_debug_damage`-style diagnostics are on,
   log/notice children solved to zero height whose declared height was
   positive.
4. Docs: a "modal with scrollable middle" recipe in docs/api.md (Modal
   section) naming the basis/shrink rules explicitly.

## Why it might matter
This is the engine's most likely "my buttons disappeared" ticket generator;
the fix is a defaults change measured in single lines.

## Workaround in the field (delete when defaults change)
abstractcode-tui sets `basis(Cells(0))` on every modal Scroll/List and
`shrink(0.0)` on title/button/hint rows (src/ui/modals.rs in that repo).

## Promotion criteria
Promote when an engine work cycle opens, or immediately if adopted as part of
0130 (Scroll follow-tail + size query), which already touches Scroll's layout
contract.

## Validation ideas
- Layout test: fixed 12-row column, scroll with 100-row content + 3 fixed
  rows → fixed rows keep their height, scroll gets the remainder.
- Golden screen: an over-full modal renders buttons and hint.

## Non-goals
No general flexbox semantic change (shrink stays available and default-on for
plain elements); no automatic scrollbars on overflow.

## Completion report
- Final path: docs/backlog/completed/first-app/0240_modal_overflow_shrinks_fixed_rows.md
- Date: 2026-07-21
- Root cause: three compounding defaults, as diagnosed — shrinkable
  fixed rows (flex children default shrink 1.0, min 0), content-sized
  `Scroll` basis, no zero-collapse diagnostic. Under overflow pressure
  the solver shrinks `height: Cells(1)` rows toward zero alongside
  everything else; in an approval modal that erases the buttons.
- Fix landed (the app/popups half — a min-size floor, no layout-solver
  change): `Modal::open` walks the content blueprint
  (`View::for_each_style_mut`, new crate-internal visitor in
  src/ui/view.rs) and floors every declared fixed extent —
  `height/width: Cells(n)` with no explicit minimum gets `min = n`
  (src/app/popups.rs::floor_declared_size). The existing solver already
  respects minimums in its freeze loop, so flexible children absorb the
  loss and declared fixed rows stay visible. Author opt-out preserved:
  any explicit `min_h`/`min_w` — including `min_h(0)` — is never
  overridden. Honest limits, documented on `Modal::open`: blueprint-time
  only (styles produced later by `dyn_view` build closures or
  `style_signal` are the author's own), and modal-scoped by design —
  the general-panel class closes via the widget-defaults half below.
- Reproduction verified: the item's probe (30x6 modal, title + grow
  middle with 40 lines + button row + hint row) rendered ONLY the title
  and middle against the pre-fix tree (buttons and hint at zero rows);
  with the floor all three fixed rows stay visible.
- Regression tests: `tests/wave_stability.rs::
  modal_fixed_rows_survive_content_overflow` (the probe, drawn through
  the modal layer's real tree) and
  `modal_fixed_row_floor_respects_explicit_min` (explicit `min_h(0)`
  row still collapses; unopted row survives).
- Filed (not landed here — the proposed directions living in the
  app-widgets seat's files this wave): #1 `Scroll::element` defaulting
  `basis(Cells(0))` with its `grow(1.0)` (src/widgets/scroll.rs), #2
  one-row controls defaulting `shrink(0.0)` (button/checkbox/input),
  #3 the zero-collapse debug notice (src/layout), #4 the docs/api.md
  "modal with scrollable middle" recipe. Precise specs in
  reviews/wave/stability-to-content.md.
- Field workaround: abstractcode-tui's `shrink(0.0)` on modal
  title/button/hint rows can be deleted; its `basis(Cells(0))` on modal
  Scroll/List stays useful until the filed #1 lands.
