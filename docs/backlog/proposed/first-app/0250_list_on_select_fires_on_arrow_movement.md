# Proposed: `List::on_select` fires on arrow movement — no activation concept

## Metadata
- Created: 2026-07-21
- Status: Proposed (API footgun report — first-app finding)
- Completed: N/A

## ADR status
- Governing ADRs: None. ADR impact: none — widget API semantics, worth a line
  in the eventual API-stability pass (0170).

## Context
`abstractcode-tui` built its `/model` and `/theme` pickers as the obvious
shape: a `List` in a `Modal` with `on_select` applying the choice and closing
the modal. Live result: merely ARROWING through the list applied every row it
passed (the user's provider/model preference was silently rewritten to
whatever they browsed past) and the close-on-select disposed the List's scope
mid-keystroke — the List's own `select()` continues after the callback
(ensure-visible `offset.update`, src/widgets/list.rs:232-243) and panicked
with "handle used after its node was disposed" (src/reactive/signal.rs:82).
This crashed the app for the maintainer on first contact with the picker.

## Current code reality
- `List::on_select` (src/widgets/list.rs:139-141) is invoked from `select()`
  whenever the selection index CHANGES (list.rs:220-229) — Up/Down/PageUp/
  PageDown/Home/End (list.rs:266-280) and mouse row clicks all route through
  it. There is no separate "activate" event; Enter/Space on a focused List do
  nothing (no such arm in the key handler).
- The doc comment says only "`on_select`" with no movement-vs-activation
  distinction; every consumer must discover that "select" means "highlight".
- After the callback returns, `select()` keeps using the List's internal
  signals (`offset.update`) — so a callback that disposes the List's scope
  (the natural modal-picker close) crashes. Deferring the close is the
  workaround, but nothing warns about it.

## Problem or opportunity
Choose-from-a-list-in-a-modal is the single most common modal pattern in any
TUI. With the current surface it takes three non-obvious moves to build
safely: ignore `on_select`, add your own Enter shortcut that reads the
selection signal, and defer any scope disposal out of widget callbacks.
Table has the same shape (`selection` + callbacks) and likely the same
hazard.

## Proposed direction
1. Add an activation event: `List::on_activate(FnMut(usize))` fired on
   Enter/Space while focused and on mouse double-click (or single-click,
   design call) — the "user chose this row" semantic. Keep `on_select` as the
   highlight-changed notification it actually is, and document both.
2. Make widget callbacks disposal-safe: run them after the widget's own
   post-callback work (move ensure-visible BEFORE the callback), or document
   loudly that callbacks must not dispose the widget's scope synchronously.
3. Mirror the same activation concept on `Table`.

## Why it might matter
First-contact crash + silent preference corruption from the most natural
composition of two flagship widgets (List + Modal). The fix is additive API.

## Workaround in the field (delete when fixed)
abstractcode-tui's pickers bind no `on_select`; a root-level Enter shortcut
reads the selection signal and confirms; `UiCtx::close_modal` defers the
actual `Modal::close` by one tick (`reactive::after(ZERO, …)`) so no widget
callback can race its own disposal (src/ui/modals.rs + src/ui/mod.rs in that
repo, with headless regression tests reproducing the original crash).

## Promotion criteria
Fold into 0170 (API stability pass) or promote standalone with the next
engine cycle; the activation event should exist before 0100's Feed widget
ships (same interaction family).

## Validation ideas
- Widget test: arrows change selection and fire `on_select` only; Enter fires
  `on_activate` once with the current index.
- Regression: an `on_activate` that closes/disposes the surrounding modal
  scope must not panic.

## Non-goals
No change to selection semantics or keyboard bindings; no double-click
gesture machinery beyond what mouse support already provides.
