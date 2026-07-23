# Proposed: ChoicePrompt shares MODAL_Z with app modals — no stacking policy, no "is a prompt open?" introspection

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build; the build's
  cycle-2 adversarial review found the P1 this enables)
- Severity: P2 — enabled an app-level P1 (invisible key owner under a
  visible modal); app-side tracking holds
- Class: footgun / API gap

## Context
The console sequences its own modals through one slot (never two app
modals at once) and queues once-shown token modals behind "no modal
open". ChoicePrompt looked like a leaf the app did not need to track —
it opens, resolves exactly once, closes itself. But a ChoicePrompt IS a
`Modal` at the same `MODAL_Z`, and it lives outside any surface the
app can query. The cycle-2 review found the resulting P1: a queued
token modal opening while a danger confirm was up painted ABOVE the
prompt while the engine routed every key to the OLDEST modal layer —
arrows moved the hidden confirm's highlight off its safe default and
Enter could fire "Rotate the token" invisibly.

## Current code reality
- `src/app/choice_prompt.rs:380` (0.2.9): the prompt holds its own
  `Modal`; `src/app/popups.rs:37`: one `MODAL_Z = 1000` band for all
  modals.
- `src/app/overlays.rs:343` + `:381-383`: layer sort is stable by
  `Reverse(z)` — equal-z means the OLDEST layer keeps key ownership
  while the NEWEST paints on top (paint order and key order disagree).
- No `Overlays::modal_count()` / top-modal query / prompt-open signal
  exists; `ChoicePromptHandle::is_open()` requires holding the handle,
  which a decoupled subsystem (the token queue) never sees.

## Repro
Open a ChoicePrompt; from any other code path open a `Modal` before it
resolves. The Modal renders above; every key goes to the prompt below.

## Workaround in the field (delete when fixed)
An app-global open-prompt counter (`UiState.prompt_open`) maintained by
a mandatory `open_prompt` wrapper whose wrapped resolver decrements it;
every subsystem that could open a modal asynchronously gates on it.
Fix wish, either: (a) newest-modal-wins key routing at equal z (paint
order and key order must agree — that alone kills the invisible-owner
class); (b) a queryable modal/prompt count on `Overlays`; (c) a
documented dedicated z-band for prompts above app modals.
