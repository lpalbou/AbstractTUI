# 0285 — selection layer eats drag-less clicks: buttons unclickable while select mode is on

## Metadata
- Created: 2026-07-23
- Status: Completed 2026-07-23 (click-through + pointer-capture heal)

## ADR status
- Governing ADRs: None directly. The damage contract is untouched; this is
  input routing policy inside the selection layer (0270's feature).

## Context
Live P0 in the first app (2026-07-23, maintainer report with screenshot):
the tool-approval modal renders three engine `Button`s — approve /
approve all / deny — and **none respond to the mouse**. Root cause is not
the buttons, the modal, or overlay routing (overlays dispatch mouse into
modal trees layer-locally, and Button's Down/Up path is disposal-safe
since 0297): it is the screen-text selection layer. While select mode is
on, `Selection::on_input` consumes **every** left Down (arms a drag
anchor, `SelectionAct::Consumed`) and every left Up ("the click's paired
release"), and the driver runs this intercept **ahead of overlay
routing** (`handle_event`: "Deliberately ahead of overlay routing:
select mode is an explicit user mode and may copy from modal content
too"). A drag-less click — Down and Up on the same cell with no Drag in
between — never reaches any widget. With select mode enabled app-wide
(the first app arms it at boot: left-drag has no other meaning there),
**no Button anywhere in the app can ever fire by mouse**.

The two features are individually correct and jointly broken: selection
wants the press (a click clears + re-anchors), widgets want the click.
Every full-screen app that enables 0270 and renders any clickable widget
hits this on day one.

## Current code reality
- Engine: `src/app/selection.rs` `on_input` — `(Down, Left)` →
  `Consumed` (arms `st.drag`, clears any region); `(Up, Left)` with
  `st.drag` armed and no region → `Consumed` ("the click's paired
  release"). `src/app/driver.rs` `handle_event` — selection intercept
  runs before `convert_event` + overlay/tree dispatch.
- First consumer evidence: `abstractcode-tui` approval modal
  (`src/ui/modals.rs` `open_approval`) — three `Button::new(..).on_click`
  children; keyboard works (a/A/d), mouse dead. Maintainer-reported.

## Proposed direction (engine's call)
Click-through for drag-less clicks: the selection layer should own the
gesture only once it becomes a DRAG.
- On left Down: remember the anchor (and clear a visible region, as
  today) but **do not consume** — let the Down route to overlays/tree so
  widgets can arm their pressed state.
- On left Drag with an armed anchor: begin consuming (selection painting
  starts) — from here Drag and the final Up are the layer's, as today. A
  widget that saw the Down but never the Up will need its pressed state
  to be drag-tolerant (Button already only fires on Up-inside).
- On left Up with an anchor but NO drag (a plain click): pass the Up
  through — the widget fires; the selection layer treats the gesture as
  the click-clears case it already implements.
- One cell of drag slop (Down and Up on the same cell vs neighbors) is
  the engine's call; terminals quantize to cells, so exact-cell is
  probably fine.

The alternative — an engine-side "suspend selection while any modal
overlay is open" policy — fixes only modals, not clickable widgets in
main trees; click-through fixes the class.

## App-side workaround to delete when this lands
`abstractcode-tui` suspends select mode while a modal is open
(`UiCtx::open_modal` → `selection().set_enabled(false)`,
`close_modal` → `set_enabled(true)`; single-writer with boot). Trades
drag-copy INSIDE modals (native Shift/Option drag still works) for
working buttons. Pinned by
`approval_buttons_are_clickable_select_mode_yields_to_modals`
(headless_ui.rs — drives a real SGR click on the approve button).
Delete the two toggles when click-through ships; the test's click
assertions should keep passing unchanged.

---

## Completion report (2026-07-23)

### Mechanism verdict

The report's analysis is **confirmed at source** on every cite:
`Selection::on_input` consumed every left Down (arming `st.drag`,
`SelectionAct::Consumed`) and every drag-less Up ("the click's paired
release"), and `Driver::handle_event` runs the intercept deliberately
ahead of `convert_event` + overlay/tree dispatch — a drag-less click
never reached any widget.

One claim needed a correction: *"Button already only fires on
Up-inside"* is true for FIRING but not sufficient for pass-through-Down.
Two additional wedges were found by tracing the capture machinery:

1. **Stuck capture**: `UiTree` captures the pointer on Down and releases
   only on Up. With the Down passed through and the layer consuming
   Drag+Up, the capture never released — the NEXT click anywhere was
   redirected to the stale captured widget (its Down handler consumed
   the press, click-to-focus stole focus back). One drag-select would
   have eaten the following click.
2. **Pre-existing stale-capture defect**: Button's `pressed.set(true)`
   on Down regenerates its own `dyn_view` hit leaf inside that same
   dispatch, so the captured `ViewId` was ALREADY dead by the end of
   every button press — the documented "pointer capture keeps the
   release routed here" contract never held past the first pressed
   re-render (a release outside the button wedged `pressed` visibly,
   selection or no selection).

### What shipped

- **Click-through** (`src/app/selection.rs`): left Down with no visible
  region arms the anchor and PASSES (`DragArm.press_routed` remembers
  the routing); the first Drag off the anchor cell claims the gesture
  (`SelectionAct::Claim` when the press routed, plain `Consumed` for a
  dismissal-anchored drag); a drag-less Up passes (the widget fires);
  a Down on a VISIBLE region clears + consumes BOTH halves (dismissal,
  Esc parity). Same-cell drags stay potential clicks (cell quantization
  is the drag slop). Copy keys / wheel / 0290 release-copy semantics
  unchanged.
- **Gesture-claim release** (`src/app/driver.rs`,
  `src/app/overlays.rs`, `src/ui/tree.rs`): on `Claim` the driver calls
  `cancel_pointer_press` on every overlay tree + the root tree — the
  tree holding a live capture dispatches a synthetic left-Up at
  `(-1,-1)` (outside every rect) through the normal capture routing:
  release-inside-decides widgets un-press without firing, the capture
  drops, the next real click routes fresh. Position-routed synthesis
  was rejected: non-modal overlay dispatch is bounds-gated, so only
  per-tree capture routing reaches the right tree.
- **Pointer-capture heal** (`src/ui/tree.rs`): `capture_pos` records the
  press cell; a capture whose instance was disposed re-points at that
  cell's current occupant (`validated_capture`, used by dispatch AND
  cancel). This fixes the pre-existing wedge and is what makes the
  cancel deliverable at all (the capture is stale by claim time on
  every Button press). When the pressed subtree genuinely died, the
  gesture tail lands on whatever is beneath — which never armed a
  press, so release-inside-decides keeps it harmless.

### Decided click rules (docs/api.md, selection section)

1. Down, no visible region → arm + PASS; drag-less Up → PASS (widget
   fires).
2. First Drag off the anchor cell → CLAIM (+ release-outside to the
   trees); same-cell drags stay clicks.
3. Down on a visible region → clear + CONSUME both halves (dismissal).

### Tests

- `src/app/selection_tests.rs`: `selection_claims_left_drag_only_...`
  re-pinned to the new acts (Down `Pass`, first drag `Claim`, click Up
  `Pass`); NEW `dismissal_click_with_visible_region_consumes_down_and_
  its_paired_up`, `same_cell_wiggle_stays_a_click`,
  `claim_fires_once_per_gesture`.
- `tests/adv_selection.rs` (real driver + SGR bytes): NEW
  `plain_click_fires_buttons_while_select_mode_is_on` (the named
  regression), `same_cell_wiggle_still_clicks_the_button`,
  `drag_select_over_a_button_neither_clicks_nor_wedges_it` (no click,
  no stuck BOLD, capture dropped, copy works, next click on another
  button routes fresh), `click_dismissing_a_visible_selection_never_
  fires_the_widget_beneath`, `modal_buttons_are_clickable_while_select_
  mode_is_on` (the consumer's approval-modal shape + overlay-tree
  cancel). All pre-existing tests in the file pass UNCHANGED (they pin
  outcomes, not the consume-all acts).
- `src/widgets/button.rs`: NEW `cancel_pointer_press_unpresses_without_
  firing`, `outside_release_reaches_the_button_and_clears_pressed`
  (the heal's own pin — the pre-existing wedge).

### Consumer follow-up

`abstractcode-tui` can delete its modal select-mode suspension
workaround; its click assertions keep passing unchanged (verified shape:
`modal_buttons_are_clickable_while_select_mode_is_on` drives the same
SGR click through a real modal with select mode ON). Bonus fix in the
same class: modal outside-press dismissal (and non-modal menu dismissal)
now also works in select mode — those presses were eaten too.
