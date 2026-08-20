# Proposed: layout docs never state the grow-vs-intrinsic-basis interaction — content-heavy panes silently render 1:1 under a 3:1 grow intent

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — ~20min once diagnosed (by an adversarial reviewer, not the builder); one-line fix
- Class: UX defect (docs gap)

## Context
agora-tui's pane column: one wrapper Element per channel,
`LayoutStyle::column().grow(3.0)` for the focused pane and `grow(1.0)`
for the rest, each wrapping a `Block` containing a `Scroll(Feed)` with
hundreds of rows. Rendered result: equal heights — the 3:1 intent showed
1:1, silently. A minimal probe (two plain Elements, grow 3 vs 1) splits
correctly, so first-instinct blame lands on the app composition — but
nothing in the docs says WHY.

The why is standard flex semantics the docs never spell out: with the
default auto basis, each child's flex base is its INTRINSIC (content)
size, and `grow` distributes only the leftover. Two content-heavy panes
whose feeds both measure larger than the viewport leave zero leftover —
grow ratios apply to nothing. The fix is one line per wrapper:
`.basis(Dimension::Cells(0))` (then grow ratios own the full axis).

The engine itself knows this: `Scroll`'s default layout is deliberately
`grow(1.0).basis(Cells(0))` ("it absorbs overflow instead of demanding
its content size" — api.md, modal-overflow section). The knowledge just
never appears where pane authors look.

## Current code reality (0.2.8)
- `docs/getting-started.md` "Layout basics": "The rule for multi-pane
  layouts: give every pane that should share leftover space a `grow`,
  and fixed panes an explicit size." — no mention that content-sized
  children make "leftover" zero, nor of `basis` at all.
- `docs/api.md` layout section: names `grow`/`shrink`/`basis` as
  vocabulary, one line, no interaction guidance.
- `src/layout/style.rs:376` — `basis()` exists and works exactly as
  needed.

## Repro
Two sibling Elements in a column, grow 3.0 vs 1.0, each containing any
content taller than the viewport (a long Feed, a tall text block).
Observe equal heights. Add `.basis(Dimension::Cells(0))` to both;
observe 3:1.

## Workaround in the field (delete when fixed)
None needed in code — the app now sets basis explicitly
(`src/ui/panes.rs`, comment cites this finding). What the docs fix
deletes: the diagnosis session. Suggested one-paragraph addition to
"Layout basics": "grow shares the LEFTOVER after intrinsic sizes; panes
whose content can exceed the viewport should pair `grow` with
`basis(Cells(0))` so the ratio owns the axis — Scroll's default does
exactly this."

## Completion report
- Final path: docs/backlog/completed/field-agora/0840_layout_docs_grow_vs_intrinsic_basis_for_content_heavy_panes.md
- Date: 2026-08-20
- Root cause: exactly as filed — a docs gap, not a code defect. `grow`
  distributes leftover, the default basis is content, and nothing in the
  reader's path said so.
- Fix landed: the suggested paragraph, in docs/getting-started.md
  "Layout basics", covering both symptoms this trap produces — grow
  ratios applying to nothing (this item's 3:1-renders-1:1) and fixed
  siblings paying for a content-heavy pane's surplus (first-app 1330,
  filed a month later against the same trap by a different app team).
  It also names the part neither report knew: `basis` describes one
  element, so a wrapper around a `Scroll` re-derives a content-sized
  basis unless the wrapper carries `basis(Cells(0))` too. Put it on the
  element that sits directly in the pressured column.
- Also landed in the same pass: docs/api.md scopes `shrink(0.0)` to one
  element and names `clip()`/`scroll()` in the layout vocabulary;
  docs/troubleshooting.md gains a symptom-first entry.
- Not landed: making the engine infer this. Two attempts are measured in
  first-app 1330's completion report; both need a min-content sizing
  mode the solver does not have, filed as first-app 1331.
