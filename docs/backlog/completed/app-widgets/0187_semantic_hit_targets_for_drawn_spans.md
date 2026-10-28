# 0187 — Semantic hit targets for custom-drawn spans (action ids, engine hit-test)

## Metadata
- Created: 2026-10-28
- Status: Completed (2026-10-28, chrome wave; released in 0.7.0)
- Track: app-widgets
- Completed: 2026-10-28
- Related: 0165 (link hit-testing in rendered content), 1210 (OSC 8
  through the draw path). This item is the chrome half.
- Root link: AbstractFramework backlog 1055; consumer: AbstractCode
  (clickable workflow / model / session id in its chrome).

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (additive draw API).

## Inventory (what exists)
Element-level mouse handlers and `EventCtx::click_count` (src/ui/click.rs);
`List::row_accessory`/`on_accessory_click`; `PageHost`'s bar uses ONE
pure plan for draw and hit-test (`plan_bar`/`hit_bar`,
src/widgets/page_host_bar.rs) — the right pattern, but private to that
widget. 0165 covers links inside content. Nothing lets an app tag a
span it DRAWS in a custom draw closure (a status line, a header) with an
action and receive the click without mirroring its draw arithmetic.

## What we want
A draw-time API to attach an action id to a drawn span
(`canvas.text_with_action(x, y, "model", style, ActionId("pick-model"))`
or a span attribute), recorded per frame by the engine; the engine
hit-tests presses against the recorded rects and dispatches
`on_action(ActionId)` to the owning element (hover and focus-ring
optional). No hard-coded cell coordinates in apps.

## Validation
Headless: a draw closure with two tagged spans; clicks land on the
right ids after a resize and a re-layout; a click between spans fires
nothing; red without the recorder.

## Completion report (2026-10-28)

Shipped in `src/ui/action.rs` (+ wiring in `ui/draw.rs`, `ui/tree_dispatch.rs`,
`ui/mount.rs`, `ui/canvas.rs`, `ui/tree.rs`):

- `ui::ActionId`; `StyledCanvas::print_action(p, text, style, &id)` and
  `StyledCanvas::mark_action(rect, &id)` (provided methods — no implementor
  changes); `Element::on_action(f)`; `UiTree::action_rect(id)` /
  `UiTree::action_targets()`.
- Recording: one thread-local frame per drawing element's draw closure; the
  record replaces the drawer's previous one, is stored relative to its rect
  (follows layout moves without a repaint), clipped to it, dropped on unmount.
- Mouse: press + release on the same span = one activation; the deepest drawer
  on the hit path wins. Keyboard (host focused): Left/Right over spans in reading
  order, Enter/Space activate; the cursor span is REVERSED (the focus ring).
- Deviation, stated: hover styling is not provided (the item made it optional).

Evidence (`src/widgets/status_bar_tests.rs`):
`raw_draw_closure_spans_hit_after_relayout` (two tagged spans, a click between
them fires nothing, a sibling grows and the record follows without a repaint),
`clicks_reach_each_action_at_several_widths`,
`press_and_release_on_different_spans_fire_nothing`,
`keyboard_reaches_every_action_in_reading_order`,
`the_keyboard_cursor_draws_reversed_while_focused`,
`spans_follow_a_resize_and_a_relayout`; the `Element::on_action` doctest.
Red without the recorder: with the store call removed from `ui/draw.rs`,
6 of the 11 tests fail (checked 2026-10-28).
