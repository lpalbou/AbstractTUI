# 1335 — Select mode steals the drag from every widget that owns one

Status: completed 2026-08-21
Owner: engine (ui/view + ui/tree_dispatch seam, app/selection, five widgets)
Effort: M

## The field ask

Reported 2026-08-21, against 0.4.0:

> we have a scrollbar but when text selection is activated, it tries to
> select text instead of dragging the scrollbar

Reproduced immediately as a failing acceptance test
(`adv_selection::thumb_drag_scrolls_with_select_mode_on`): with
`selection().set_enabled(true)`, a press on the thumb followed by a drag
left the offset at 0 and painted a highlight over the content the user
was trying to scroll.

## Why it happened

Click-through (0285) let the Down pass to the widgets so a Button stays
clickable in select mode. That is right for a widget that owns CLICKS.
It is not enough for a widget that owns DRAGS:

1. `Driver::handle_event` routes every event through
   `Selection::on_input` before the trees see it.
2. The left Down armed a selection anchor and PASSED, so the scrollbar
   strip recorded its grab and took pointer capture.
3. The first drag off the anchor cell returned `SelectionAct::Claim`,
   and the driver's claim handling calls `cancel_pointer_press()` on
   every tree — which is exactly the right thing for the Button case it
   was built for, and exactly wrong here: it killed the grab the strip
   had just taken.
4. Every later Drag reached the strip with no grab. `scroll.rs`
   correctly refuses to steer on a grab-less drag, so the gesture became
   a selection and the thumb was inert.

The layer had no way to ask "does this cell already belong to a drag?",
so it claimed everything.

## Not just the scrollbar

Five drag surfaces in the engine had the identical defect whenever
select mode was on:

| Surface | Handler |
| --- | --- |
| `Scroll` thumb (the report) | `src/widgets/scroll.rs` |
| `List` internal bar | `src/widgets/list.rs` |
| `Table` internal bar | `src/widgets/table.rs` |
| `FilePicker` internal bar | `src/widgets/file_picker.rs` |
| `Viewport3D` orbit | `src/widgets/viewport3d.rs` |

The `Viewport3D` case is the starkest: an entire interaction mode
(orbiting the camera) died under select mode, replaced by a highlight
over mosaic cells nobody would ever want to copy.

## Shipped

**`Element::drag_zone(|rect| -> Option<Rect>)`** — an element declares
the sub-rect of itself that owns pointer drags. A SUB-rect, not a flag,
because `List`/`Table`/`FilePicker` handle their bar on the same element
as their rows: only the strip may stand down, or every row would stop
being selectable. `None` means nothing is grabbable right now (a bar
with no overflow, an auto-hidden strip) — an invisible target must not
swallow a selection any more than it may steer an offset. Pure over
solved geometry, the same contract `Element::measure` carries.

**`UiTree::press_probe_at` → `PressProbe`** — the pane that would clamp
a selection from a point, plus whether a drag zone owns it, resolved in
ONE descent. `pane_rect_at` became a thin wrapper. Zone closures are
user code, so the walk collects handles under the borrow and calls them
after releasing it (the held-borrow contract).

**The selection layer stands down.** A Down whose probe reports
`drag_owner` arms no anchor and passes. Everything downstream then falls
out of the existing code: the later Drag and Up find no armed drag and
take the "not ours" branches, so there is no claim and no cancelled
capture. The ANCHOR decides — a drag that starts in content and crosses
a strip keeps selecting, which is what a user reaching past a scrollbar
means.

Rejected: standing down whenever any tree holds a pointer capture.
`Button` and `List` rows capture on Down too, so that would have
silently killed drag-to-select over buttons — the behavior 0285
deliberately built.

## Evidence

Failing-test-pin first; all five surfaces covered.

- `adv_selection::thumb_drag_scrolls_with_select_mode_on` — the report,
  through the real pipeline (Driver + CaptureTerm + SGR bytes): the
  offset moves, no cell wears the selection ink, nothing reaches the
  clipboard. Failed before the fix with "offset stayed 0".
- `adv_selection::a_selection_that_crosses_the_strip_keeps_selecting` —
  the anchor decides, not the pointer.
- `adv_selection::an_invisible_bar_does_not_eat_a_selection` — a bar
  with nothing to scroll owns nothing.
- `app::selection::tests::a_press_in_a_drag_zone_never_arms_the_selection`
  — the layer rule in isolation: Pass / Pass / Pass, no claim.
- `widgets::scroll::tests::the_strip_is_a_drag_zone_and_the_content_is_not`
  and `an_auto_hidden_strip_is_not_a_drag_zone`.
- `widgets::list::tests::only_the_strip_column_owns_drags`,
  `widgets::table::tests::only_the_strip_column_owns_drags`,
  `widgets::file_picker::tests::only_the_strip_column_owns_drags` — the
  strip owns drags, the rows/cells/filenames beside it stay selectable,
  and a listing that fits declares no zone.
- `widgets::viewport3d::tests::an_orbiting_viewport_owns_its_drags` —
  and a viewport with no orbit callback declares nothing.
- Counter-pins unchanged and green:
  `adv_selection::drag_select_over_a_button_neither_clicks_nor_wedges_it`
  (the `Claim` path is untouched) and the whole 21-test `adv_selection`
  suite.
- Full suite green (80 binaries), clippy clean, and
  `cargo run --example capture -- review` leaves every golden
  byte-identical — this is a hit-test change, so no pixel may move.

## Follow-ups revealed

- `docs/captures` regeneration in a sandbox skips `gallery` and
  `viewer3d` ("never entered the app screen") — an environment limit of
  the capture harness, unrelated to this change, but it means those two
  captures are not covered by a local golden re-run.
- The `drag_zone` seam is the natural home for the split-pane divider
  (app-kits 0580) and the node-graph editor's pan/drag (extensions
  0430) when they land — neither can survive select mode without it.
