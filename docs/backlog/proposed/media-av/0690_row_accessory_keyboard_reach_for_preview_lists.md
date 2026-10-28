# 0690 — Browse-and-preview lists: reuse List + accessory + after(); add keyboard reach to the row accessory

## Metadata
- Created: 2026-10-28
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: 0640 (external audio-process lifecycle pattern) for the
  playing half.
- Root link: AbstractFramework backlog 1055 (voice library track).
- Promotion trigger: the AbstractGateway console-TUI voice picker
  (`console-tui/src/ui/routes.rs:2861`, a `Combobox` whose Test
  synthesizes but does not play) adopts the recipe below.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (one additive key binding).

## Context
A recurring app shape: go through a list and preview each item in place
(a voice, a sound, a theme), then keep the one you like; the web console
now does it for voices (choosing a voice plays it, a newer choice cancels
the older one, a play/stop toggle at the right of the row).

## Inventory — what abstracttui already offers (verified 2026-10-28)
- `List` (src/widgets/list.rs): `on_select` fires when the highlight
  MOVES (the preview trigger), `on_activate` on Enter/Space/click on the
  selected row (the commit); `row_accessory` + `on_accessory_click`
  (list.rs:22, 411) give a trailing per-row column (`▶` / `■`) that does
  not change selection.
- `reactive::after(delay, f)` (src/reactive/animate.rs:159): the
  one-shot timer for a settle-debounce; a generation counter in the app
  drops stale previews.
- `Spinner` (src/widgets/spinner.rs) for "loading", `Progress`
  (src/widgets/progress.rs) for elapsed/duration, `Meter` / `AudioScope`
  for levels.
- 0640: the external player process lifecycle (kill-on-drop guard on
  `cx.on_cleanup`) so a preview stops when the list goes.
- `Combobox` (src/app/select_combobox.rs:50) fires `on_change` on commit
  only: a browse-and-preview surface should use `List` (in a modal or
  drawer), not `Combobox`.

## What is missing (the only proposal)
The row accessory is mouse-only: there is no key that activates the
selected row's accessory (Space is already Enter's alias for
`on_activate`). Add `List::accessory_key(Key)` (e.g. `p`, or Shift+Space)
that fires `on_accessory_click(selected)` without changing selection,
announced through the row's gesture label (0615).

## Recipe for the console-TUI voice picker (no new widget)
`List` of voices (label = name, grouped by the app) → `on_select` arms
`after(200 ms)` with a generation id → synthesize → play through the 0640
pattern; `row_accessory` shows `▶`/`■` for the selected row and toggles
play/stop (replay from 0 = restart the player); a `Spinner` in the status
line while loading; the error as one line; leaving the list kills the
player.

## Validation
A test that `accessory_key` fires the accessory callback for the
selected row and leaves selection unchanged; red without the binding.
