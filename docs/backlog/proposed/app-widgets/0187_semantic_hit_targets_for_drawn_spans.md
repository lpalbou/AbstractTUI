# 0187 — Semantic hit targets for custom-drawn spans (action ids, engine hit-test)

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: app-widgets
- Completed: N/A
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
