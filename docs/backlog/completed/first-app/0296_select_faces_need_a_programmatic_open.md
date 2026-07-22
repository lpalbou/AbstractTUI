# Completed: select faces need a programmatic open — command-summoned pickers cannot adopt them

## Metadata
- Created: 2026-07-22
- Status: Completed (was: Proposed — API gap report, first-app finding, 0.2.1 adoption wave)
- Completed: 2026-07-22 (first-app fix wave, cycle 3)

## ADR status
- Governing ADRs: None. ADR impact: none — additive API on the 0500 family.

## Context
The 0.2.1 upgrade prompt told `abstractcode-tui` its `/model` and `/theme`
pickers "stop being List-in-Modal" now that `app::select` ships. Adoption
was attempted and does not fit: every picker in this app is COMMAND-
SUMMONED (`/theme`, `/model`, `/sessions`, `/workflow` typed into the
composer) — there is no persistent form surface where a closed trigger row
would live. The select faces open their popup ONLY from trigger-row events
(Enter/Space/click on the focused row; select.rs wires `open` to those
paths exclusively) — no public `open()` exists.

## Current code reality
- Wrapping a `Select` in the existing command modal costs the user an
  EXTRA keystroke (open modal → Enter to open the popup → browse → Enter)
  and deletes no app code — the modal scaffold, Esc handling, and (for
  the theme picker) the revert logic all stay, now duplicated across the
  modal layer and the popup layer (`commit_on_move`'s Escape restores the
  pre-open value, and the modal's own Escape must ALSO revert).
- Building a caret-anchored filterable picker directly on `Popup` (the
  claude-code-style command palette — arguably what "stop being
  List-in-Modal" wants) would mean re-implementing `select_core`'s
  filter/highlight/type-ahead/option-rows machinery app-side: it is
  deliberately private (`mod core`), and hand-rolling it is exactly the
  machinery-duplication this wave deletes.
- What the app DID adopt from 0250/0.2.1: `List::on_activate` in all four
  single-select pickers (root-Enter shortcuts deleted; activation is now
  the engine's, disposal-safe by clause 4).

## Problem or opportunity
Command-driven pickers are the dominant picker shape in terminal agent
tools (claude-code, codex-cli: `/theme`-style commands opening a
filterable list at the composer). The select family's filtering,
type-ahead, highlight/commit split, and popup placement are exactly right
for it — only the summoning is missing.

## Proposed direction (engine's call)
- A programmatic open on the faces (e.g. `Select::open_at(cx, overlays,
  anchor: PanelAnchor) -> PopupHandle`, same for `Combobox`), or
- a standalone `app::select::pick(cx, overlays, anchor, options, opts) ->
  on_commit` one-shot (the command-palette primitive: no trigger row at
  all), or
- promote the `option_rows_view`/filter/type-ahead core to a public
  building block so apps can compose their own summoned pickers.

## App-side state
`abstractcode-tui` keeps its four pickers as `List`-in-`Modal` with
`on_activate` (one keystroke to choose, browse-never-commits, live theme
preview via the selection signal). Conversion is queued on this item.

## Completion report

- Completed: 2026-07-22 (first-app fix wave, cycle 3). Shipped the first
  proposed direction (a programmatic open on the faces), reshaped to fit
  the builder pattern: after `.view(cx)` the app holds no face object,
  so the verb rides a cloneable handle.
- Shipped API (additive, prelude-exported):
  - `app::select::SelectHandle` — `SelectHandle::new()`, `.open() ->
    bool`; attach with the new `.handle(&h)` builder on `Select`,
    `Combobox`, AND `MultiSelect`. `open()` returns true when the popup
    is open after the call (already-open counts); false for
    unmounted/never-painted/disabled/empty faces — honest feedback,
    never a panic.
  - The anchor source, solved honestly: popups anchor at the trigger's
    LAST-PAINTED rect, recorded by the face's outer element at draw
    time (`EventCtx::current_rect` does not exist outside dispatch).
    The one-frame-after-mount caveat is DOCUMENTED on the handle
    (open on the frame after the face first renders); same-turn layout
    moves anchor one frame stale, and the existing `DismissReason::
    Resize` covers the viewport-change case.
  - Disposal safety (the 0297 discipline applied at birth): the wire is
    severed by the face scope's cleanup; dyn_view regenerations rewire,
    and a generation guard keeps a stale cleanup from severing the
    newer wire in either disposal order.
- NOT shipped, deliberately: the standalone `pick(..)` one-shot and the
  public `select_core` promotion (the item's options 2/3). The handle
  makes command-summoned pickers adoptable with the family's
  filter/type-ahead/highlight machinery intact; a trigger-less
  caret-anchored one-shot remains a separate ask if the field still
  wants it after adopting the handle — file fresh with the consumer's
  shape.
- Tests (`src/app/select_tests_handle.rs`, through the real
  dispatch/overlay rig):
  `handle_refuses_before_first_paint_then_opens_at_the_trigger`,
  `handle_open_commit_flows_like_a_gesture_open`,
  `handle_refuses_disabled_and_empty_faces`,
  `handle_dies_with_the_face_and_rewires_on_regeneration`,
  `handle_works_on_combobox_and_multiselect` (combobox keeps its
  anchor-row-included geometry under programmatic opens).
- Docs: `docs/api.md` select section ("Programmatic open —
  `SelectHandle`" + example); CHANGELOG under Unreleased. App-side:
  `abstractcode-tui`'s four List-in-Modal pickers can now convert per
  this item's queue note.
