# Proposed: an overflowing child is silently overpainted by its next sibling — `shrink(0.0)` saves the widget's box, not its pixels

## Metadata
- Created: 2026-08-20 (live operator report against abstractcode-tui; root-caused
  and reproduced against the abstracttui working tree at 0.3.6)
- Status: Proposed
- Class: bug / diagnostics gap (the 0240 class, one level up)

## ADR status
- Governing ADRs: None. ADR impact: none required for the diagnostic; direction 1
  (an automatic content-based minimum) is a layout-solver semantics change and
  would deserve its own note if adopted.

## Context
Operator report on the first app, 2026-08-20, verbatim: *"the prompt panel at the
bottom is limited to 2 lines (should be 3) and it doesn't follow my text, so if I
go over 2 lines, i don't see what i am writing!"*

That is two symptoms of one failure. The app's composer is a `TextArea` with
`.rows(1, 4)` inside a `Block` row. Under overflow pressure in the app's chrome
column, the solver took ONE row off the composer's `Block` (4 → 3). The `TextArea`
inside kept its full 4 rows — its own `shrink(0.0)` was honored — and painted 4
rows, correctly scrolled so that the caret's row was the last of them. That fourth
row lay outside the parent's 3-row rect. Nothing clipped it and nothing warned:
the NEXT SIBLING (the app's status bar) simply painted over those cells afterwards.

The row the user loses is therefore always the caret's row. Every keystroke past
the visible rows scrolls text the user cannot see. "The panel is too short" and
"it doesn't follow my text" are the same defect: the widget's scroll window is
right, and the row it lands on is destroyed after the fact.

## Current code reality
Verified against the working tree (0.3.6), not the published crate:

- `TextArea`'s growth style declares `height = rows.clamp(min_rows, max_rows)` with
  `shrink(0.0)`, and the comment beside it claims the guarantee this item is about:
  *"shrink 0 so an overflowing sibling can never crush the composer (0240 #2)"*
  (src/widgets/textarea.rs:414-435). The guarantee holds for the widget's own flex
  item and for nothing above it — an ANCESTOR that is shrinkable still ends up
  shorter than the widget, and the widget's surplus rows are then painted outside
  the ancestor's box.
- The widget's own scroll math is CORRECT and is not the bug: `finish_edit` sizes
  the window from the growth band (src/widgets/textarea.rs:590-596) and
  `publish_caret_cell` re-adjusts `top` against the solved `rect.h`
  (src/widgets/textarea.rs:600-608), so with a 4-row rect it paints the caret on
  row 4 exactly as intended.
- Overflow is not clipped to the parent's content box: a child solved taller than
  its parent paints past it (verified in the repro below with both a plain
  `Element` parent and a `Block` parent). Whether the surplus survives on screen
  depends entirely on whether a later sibling happens to paint the same cells.
- The zero-collapse diagnostic (0240 follow-up #3) fires ONLY at exactly zero:
  `if declared_fixed[i] > 0 && sizes[i] == 0` (src/layout/solve.rs:284-297,
  message in src/layout/tree.rs:185-206). A child crushed from 4 rows to 3 — or a
  child that overflows its parent — is completely silent. In the app's 30- and
  40-row runs, no notice was emitted while the caret was being destroyed every
  keystroke.
- `floor_declared_size` (src/app/popups.rs:81,178) already implements the "declared
  fixed extents get a minimum" idea, but only for MODAL content at blueprint time.
  The general tree has no equivalent.
- `Scroll`'s default layout is `grow(1.0).basis(Cells(0))` (src/widgets/scroll.rs:
  292-297, 0240 follow-up #1) — the right default, and it did not help here: the
  pressure came from an ancestor of the Scroll whose own basis is AUTO, i.e.
  content-derived. The basis-0 default does not survive one wrapper.

## Repro (self-contained, ~60 lines, no app needed)
A `TextArea` in a `Block` row that is one row shorter than the widget's growth
band, with a later sibling that paints its own line. Drive it through
`Driver`/`CaptureTerm` exactly as the engine's own tests do:

```rust
fn run(fixed_rows: Option<i32>) -> String {
    abstracttui::app::set_theme_by_id("abstract-dark");
    let size = Size::new(100, 10);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::app::current_theme().tokens;
        let state = abstracttui::widgets::TextAreaState::new(cx);
        let area = abstracttui::widgets::TextArea::new().state(&state).rows(1, 4);
        let inner = Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(area.element(cx, &t).autofocus().build())
            .build();
        // The composer row, one row SHORTER than the widget's band.
        let row = abstracttui::widgets::Block::new()
            .border(abstracttui::widgets::BorderKind::None)
            .fill(t.surface)
            .layout(match fixed_rows {
                Some(h) => LayoutStyle::row().height(Dimension::Cells(h)),
                None => LayoutStyle::row(),
            })
            .child(inner)
            .element(&t)
            .build();
        // A later sibling that paints its own line — the app's status bar.
        let status = Element::new()
            .style(LayoutStyle::line(1))
            .draw(|canvas, rect| {
                canvas.fill(rect, ' ', Rgba::rgb(120, 120, 120), Rgba::rgb(20, 20, 20));
            })
            .build();
        Element::new()
            .style(LayoutStyle::column().height(Dimension::Percent(1.0)))
            .child(row)
            .child(status)
            .build()
    })
    .expect("mount");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, RunConfig::default()).expect("driver");
    driver.turn(&mut app, &mut term).expect("turn");
    // Eight wrapped rows of draft (word wrap puts one LINEn per row).
    let mut draft = String::new();
    for i in 1..=8 {
        draft.push_str(&format!("LINE{i}{} ", "x".repeat(88)));
    }
    term.push_input(draft.as_bytes());
    driver.turn(&mut app, &mut term).expect("turn");
    driver.turn(&mut app, &mut term).expect("turn");
    term.screen().to_text()
}
```

Measured (working tree, 2026-08-20) — rows painted, and whether the LAST painted
row is the caret's row (`LINE8`):

| Block height | rows painted | last painted row |
| --- | --- | --- |
| unset (auto) | 4 | LINE8 — correct |
| `Cells(4)` | 4 | LINE8 — correct |
| `Cells(3)` | 3 | **LINE7 — the caret's row is gone** |

Remove the `status` sibling and the 3-row case paints all four rows, spilling one
row past the Block — the surplus is destroyed only when something else claims those
cells. No diagnostic is emitted in either case.

**Expected**: whatever the layout hands the widget, the user sees the row the
caret is on. Either the ancestor cannot be shorter than a child that refuses to
shrink (direction 1), or the overflow is truncated at the parent's boundary and
SAID (directions 2/3) — never a caret quietly overwritten by a neighbour.

## Problem or opportunity
Three properties compose into an invisible failure:
1. `shrink(0.0)` on a widget's own element is read by callers (and by the comment
   at textarea.rs:415) as "this widget keeps its rows". It guarantees only the box
   the widget is given, not that its ancestors leave room for it.
2. Overflow is neither clipped nor reported, so whether it survives depends on
   paint order — a detail no caller reasons about.
3. The one diagnostic that exists stops at exactly zero, so the whole partial-crush
   and overflow range is silent.

The 0240 completion report closed the collapse-to-zero half of this class for
modals. This is the same class for the general tree and for partial loss, and its
worst surface is exactly the one 0240 named: the control the user is typing into.

## Candidate directions (engine's call)
1. **Automatic content-based minimum on the main axis** (the CSS `min-height: auto`
   analogue): a flex container's implicit minimum is the sum of its children's
   minimums, and a child with `shrink(0.0)` contributes its basis. The solver then
   takes the row from a sibling that can actually give it, and no overflow is
   produced. This is `floor_declared_size` generalized from modal blueprints into
   the solver. Biggest behavioural change; also the only one that makes the
   textarea.rs:415 comment true.
2. **Diagnostic parity — cheapest, highest value**: extend the 0240 #3 notice from
   "collapsed to 0" to "solved outside the parent's content box" and, if you want
   the partial case too, "declared N, solved < N". A single line of stderr would
   have turned this from a live typing-is-invisible bug into a two-minute fix. The
   collapse notice's own wording already prescribes the remedy ("give it
   shrink(0.0) or an explicit min, or absorb the overflow in a Scroll") — it just
   never fires here.
3. **Honest clipping** (semantics call): clip children to the parent's content box,
   or expose an `overflow` knob. Truncation at the boundary is at least legible;
   today the surplus lands on a sibling's cells and the loss looks like a rendering
   glitch somewhere else entirely.
4. Regardless of the above: correct the comment at src/widgets/textarea.rs:415. It
   currently promises a guarantee the engine does not provide, and it is what the
   app's maintainer (and this seat's first diagnosis) trusted.

## Why it might matter
Every app-shell layout has this shape — a growing content pane above fixed chrome
rows, with a composer that must not shrink. The app hit it in the one widget where
the failure is unrecoverable for the user: you cannot see what you type. It cost a
live session and a full bisect to find, precisely because nothing pointed at the
layout.

## Workaround in the field (delete when direction 1 lands)
abstractcode-tui, both one-liners, 2026-08-20:
- `shrink(0.0)` on the composer's `Block` row (src/ui/chrome.rs:947) so the row can
  never be bought down;
- `basis(Cells(0))` beside `grow(1.0)` on the transcript pane's wrapper
  (src/ui/transcript_view.rs:1091), which removes the pressure at its source — the
  wrapper's AUTO basis was measured from the whole transcript, so the chrome column
  overflowed from the first item on. Note that the `Scroll` inside that wrapper
  already had the engine's `basis(Cells(0))` default; restoring it explicitly did
  NOT help, because the wrapper one level up re-derives a content-sized basis. That
  is worth knowing for follow-up #1's coverage claim.
Regression test: `composer_grows_to_four_rows_and_keeps_the_caret_row_visible`
(tests/headless_ui.rs:8939 in that repo), verified failing before both fixes.

## Promotion criteria
Promote 2 with the next diagnostics pass — it is small and it closes the silence.
Promote 1 when a layout-semantics cycle opens; it is the only direction that makes
`shrink(0.0)` mean what its callers already believe.

## Validation ideas
- The repro above, asserted: with the Block at `Cells(3)`, the caret's row must be
  on screen (direction 1: the Block cannot be 3; direction 3: the widget is
  truncated to 3 rows and re-scrolled so the caret row is the last one).
- Diagnostic test: a child solved past its parent's content box emits exactly one
  notice naming the child, drained through the existing notice lane.
- A negative: a deliberately overflowing decorative child must not spam notices —
  once per node per tree lifetime, as `note_zero_collapse` already does.

## Non-goals
No change to flex shrink semantics for ordinary elements. No automatic scrollbars.
No requirement that `TextArea` police its own ancestors.

## Completion report
- Final path: docs/backlog/completed/first-app/1330_overflowing_child_is_overpainted_by_its_next_sibling.md
- Date: 2026-08-20
- Verdict: the symptom is a real engine gap and the diagnosis of the
  MECHANISM is correct — a child that refuses to shrink inside a
  container solved shorter than it keeps its rows, is placed outside the
  container, and the next sibling's paint lands on those cells. The
  engine said nothing. That silence is what shipped as a fix. Two of the
  four proposed directions were declined on measurement, and one was
  already implemented.

### Landed
- **Direction 2, narrowed (the diagnostic).** `LayoutTree::note_main_overflow`
  (src/layout/tree.rs) and the solver hook beside the existing
  zero-collapse check (src/layout/solve.rs). Debug builds only, drained
  through the same notices lane, deduped once per situation. The notice
  names how many rows the children need, how many the box has, and the
  three remedies (`shrink(0.0)` / `basis(Cells(0))` on the pressuring
  pane / `clip()` or a `Scroll`).
  - Scoped deliberately: **columns only**, and only when the parent does
    not already clip. A label wider than its cell is the most ordinary
    condition in a terminal UI; reporting row overflow would have made
    the lane worthless. Measured across the full suite, the shipped
    predicate fires **zero times** in 3,792 passing tests.
  - The item's other half — "declared N, solved < N" — was NOT shipped.
    It is silent on this very bug (the composer row was AUTO-height, so
    it is not a watched declared extent) and it fires on ordinary
    shrink, which is what shrink means.
- **Direction 4 (the comment), as a scope clarification rather than a
  correction.** src/widgets/textarea.rs already said *sibling*, matching
  docs/api.md; the report read an ancestor guarantee into it. The
  comment now states the boundary explicitly.
- **Docs, which is where both field reports actually needed the fix.**
  docs/getting-started.md gains the `grow`-shares-leftover paragraph
  (this also closes field-agora 0840, filed 2026-07-23 against the same
  trap by a different app team); docs/api.md scopes `shrink(0.0)` to one
  element and surfaces `clip()`/`scroll()` in the layout vocabulary;
  docs/troubleshooting.md gains the symptom-first entry, worded as the
  operator reported it.
- Regression tests: `layout::tests::main_overflow_emits_a_debug_notice_once`
  (solver level, with the clipping / row-axis / ordinary-shrink negatives)
  and `wave_stability::main_axis_overflow_reaches_the_notices_lane`
  (the composer shape, driven through the real driver into the lane).

### Declined, with the measurement
- **Direction 1 (automatic content-based minimum, the CSS `min-height:
  auto` analogue): rejected.** Implemented faithfully in a throwaway
  worktree: **25 test failures**, including every 0240 guarantee this
  item builds on — `scroll::tests::default_layout_takes_leftover_not_content_basis`
  (the content-derived minimum clamps `basis(Cells(0))` straight back
  up, annihilating follow-up #1), the `zero_collapse_*` tests
  (follow-up #3), and both `modal_fixed_row*` acceptance tests. **And it
  does not fix this bug**: CSS's automatic minimum is
  `min(specified, content)`, which is 3 when a definite 3 is specified,
  so CSS overflows here too. The item's validation idea ("direction 1:
  the Block cannot be 3") is not what direction 1 does.
- **Direction 3 (clipping): already shipped, and it does not fix this
  bug either.** `Overflow::{Visible,Clip,Scroll}` with `LayoutStyle::clip()`
  and `.scroll()` has existed in src/layout/style.rs, is honored by the
  paint and hit paths, and is used internally as the remedy for this
  exact class (src/app/choice_prompt_view.rs). Measured with `clip()` on
  the Block: the caret's row is still gone — clipping makes the loss
  deterministic rather than paint-order-dependent, which is worth
  having, but the widget still scrolls against its own solved rect. What
  was missing was discoverability, so it is now named in the docs
  vocabulary. Making `Clip` the DEFAULT is a non-starter: it contradicts
  CSS and breaks the named contract at
  `tests/wave_size_sweep.rs::empty_parent_children_with_own_extent_still_paint`.

### Also tried and reverted (recorded because it looks right)
Making `intrinsic_size` honor a flex child's `basis` — so that
`Scroll`'s `basis(Cells(0))` default survives an AUTO-basis wrapper
instead of the wrapper re-deriving a content-sized basis one level up.
This is the actual root cause of the pressure in both field reports, and
the solver's own precedence comment (`flex-basis > explicit main size >
intrinsic content`) already implies it. It fails for a structural
reason: with no min-content resolution to floor the result, a
content-sized ancestor containing only a basis-0 viewport collapses to
nothing. It regressed
`wave_extensions_accept::pipeline_monitor_scene_end_to_end`, where a
`PageHost` with no `grow` solved to its 2-row tab bar and the graph
never got a box. This is the same wall direction 1 hits from the other
side: **both need a real min-content sizing mode first**, filed as
1331.

### Field workaround
abstractcode-tui's two one-liners stay. The `basis(Cells(0))` on the
transcript wrapper is the documented answer, not a workaround, and is
now written down in getting-started. The `shrink(0.0)` on the composer
`Block` row is likewise the documented recipe applied at the right
level.
