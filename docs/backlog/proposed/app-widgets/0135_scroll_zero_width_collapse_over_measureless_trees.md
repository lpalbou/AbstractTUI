# 0135 — Scroll over a measureless element tree collapses to a bar-only strip (the remainder after the 0005 wave)

## Metadata
- Created: 2026-07-25
- Status: Proposed — PARTIALLY discharged the same day by the ADR-0005
  wave (see "What the 0005 wave already fixed"); this item tracks the
  REMAINDER (plain element trees + the cross-axis/width leg)
- Track: app-widgets (Scroll is 0130's lineage; the solver half
  overlaps `src/layout/`)
- Class: bug (against the module's "measured by default" promise) or
  docs gap — the fix-or-document ruling is the item
- Severity: P2 — a pane that renders only its scrollbar reads as
  totally broken; both sightings were in shipped examples
- Engine: reproduced on 0.2.18 (wave 12); remainder re-verified in
  0.2.22 (2026-07-25, post-ADR-0005)

## The evidence (wave 12, `reviews/wave12/visual-to-code-handoff.md` §2)

1. `Scroll::new(column-of-field-rows).axes(false, true)` — content an
   Element column whose CHILDREN have explicit `w`/`h` but whose column
   carries no measure — rendered as a 1-cell-wide scrollbar at the LEFT
   edge of the block with no content (the components settings card,
   run 2).
2. Sharper second sighting: the widgets example's visual panel
   (`Scroll::new(content).content_size(96, 18)`) — WITH the explicit
   hint — also rendered bar-only inside a Tabs panel (capture sweep 7).
   The hint did not rescue the WIDTH.

## What the 0005 wave already fixed (cite, do not re-file)

`docs/adr/0005-content-rendering-responsibilities.md` (Accepted,
2026-07-25) closed the CONTENT-VIEW half of this class:
`MarkdownView`/`CodeView` gained intrinsic measure + a
`basis(Cells(0))` default, so `Scroll::new(MarkdownView…)` measures
true content height out of the box and Auto-height panels size honestly
— pinned by `src/widgets/markdown_scroll_tests.rs` ("the wave-12
'Scroll over measureless content collapses' class, fixed in the ENGINE
this time"). A bare draw-only widget in a Scroll is no longer the
representative failure.

## What remains open (this item)

- **Plain element trees.** The §2 primary repro is not a content view:
  an Element column of explicit-`w`/`h` children still measures ~0 on
  the scroll axis (draw elements measure zero — documented — and the
  wrapper column derives nothing from them). `src/layout/solve.rs` /
  `flex_math.rs` are unchanged since 2026-07-22; no solver-side
  intrinsic fold landed for this shape.
- **The width/cross-axis leg.** In measured mode the content WRAPPER's
  cross axis is `Percent(1.0)` (`src/widgets/scroll.rs:242-256` — the
  cross axis does fill), but the VIEWPORT's own box
  (`scroll.rs:296-310`, `grow(1.0)` + absolute child) has no intrinsic
  width of its own: absolute children do not feed parent intrinsics,
  so inside a hug-sizing parent (the §1b-bullet-6 Tabs width hug) the
  whole Scroll solves to bar width even when `content_size` is given —
  sighting 2. The hint sizes the wrapper, never the Scroll's own slot.

## What we want

The fix-or-document ruling the code seat already accepted for this
lane (`reviews/wave12/code-to-visual-handoff.md` "#2 … likely
document-or-fix decision"):

- **Fix shape**: the viewport participates in measure — derive an
  intrinsic size from the content tree (explicit child sizes summed on
  the scroll axis; max on the cross axis) and let `content_size` feed
  the viewport's OWN measure, not only the wrapper.
- **Document shape**: scroll.rs module docs + api.md state "Scroll
  content needs a measure, and the Scroll's own slot must be stretched
  by its parent" — plus a debug notice when the viewport solves to
  ≤1 cell on an axis while mounted content exists (the 0240 #3 debug
  precedent), so the failure names itself instead of rendering a bare
  bar.

Either shape should re-read the scroll.rs module-doc guidance written
before the 0005 wave (`scroll.rs:11-24` still says "a bare
MarkdownView has no intrinsic height; wrap it in a one-item Feed or
keep the explicit hint" — now stale: MarkdownView measures).

## Validation

- Driver test: a Scroll of explicit-size rows inside a Tabs panel at
  ~108 cells — content paints, the non-scroll axis fills the viewport.
- The components settings-card shape (run 2) as a unit test.
- If the document shape wins: the debug notice fires on the §2 repro,
  and the module docs / api.md / troubleshooting rows agree.

## Non-goals

- Re-fixing the content views (done — ADR-0005 wave).
- The Tabs width-hug itself (0185's family; cross-referenced, not
  duplicated).

## Related

- ADR-0005 + `src/widgets/markdown_scroll_tests.rs` (the fixed half),
  completed app-widgets 0130 (measured extent), field-agora 0860
  (RichTextView measure — same class, filed from the field; the 0005
  wave's content-view fix pattern is its template), 0185 (measure
  inflation), field-agora 0895 (bound offset dead in Drawer pages —
  a different Scroll seam, kept separate).
