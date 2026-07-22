# 0150 — Terminal verbs (notify/bell/set_title/clipboard_copy) from components

## Metadata
- Created: 2026-07-21
- Status: Planned
- Track: app-widgets
- Completed: N/A

## 2026-07-22 — clipboard leg landed via 0270

`app::selection::copy_to_clipboard(text)` (prelude-exported) now gives
component code the clipboard verb: texts queue on the app-thread
selection store, the driver drains them each turn and emits OSC 52
through `Presenter::external_write` custody (cell runs first, one flush
— the §6 rule this item is bound by), with the empty-payload refusal
(empty OSC 52 CLEARS the clipboard) and a one-time labeled notice when
`Capabilities::osc52_copy` was not advertised. The same drain also
carries the tier-2 `mouse_capture()` requests — a working precedent for
this item's verb-queue shape. Remaining scope here: notify / bell /
set_title (and their capability gates), which still have no
component-reachable path.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: None expected; the change must preserve the damage
  contract's presenter-custody/one-flush rule (docs/design/
  01-damage-contract.md), which is a design-note constraint today.

## Context
Any long-running app signals beyond its own cells: monitors ring or
notify on alerts, chat and feed clients ping on unread traffic and count
it in the terminal title, consoles put job status in the title and copy
results to the clipboard — attention and egress verbs every app class
reaches for once it runs longer than a glance. The robustness review's
gap table (Part 2) found all four verbs implemented and capability-gated
on the `Terminal` trait — and unreachable from any component, because
`App::run` owns the terminal for the app's lifetime. The two port epics
are the first validators; both hit this on day one of their polish
phases.

## Current code reality
- `src/term/mod.rs:202-243` — the verbs exist with the right postures:
  `set_title` (OSC 0, control bytes stripped, title-stack push/pop best
  effort), `clipboard_copy` (OSC 52, **write-only by design** — the read
  form is an exfiltration vector the engine never emits, term/verbs.rs:82-86),
  `bell` (BEL), `notify(message, channel)` (OSC 9 / OSC 99 / BEL fallback;
  one channel per call — `Capabilities::notify_channel()`, caps.rs:495-500,
  picks it so double-notification terminals don't pop twice).
- No pass-through: nothing in `src/app` or `src/ui` exposes the terminal
  to user code (grep confirms; the robustness review reports the same).
  `Quitter` (app/mod.rs:73-80) and `WakeHandle` show the established
  pattern for handing components a narrow, cloneable capability.
- The one-writer rule this must not break: phases seal damage and the
  presenter emits exactly one flush per turn (app/mod.rs:11-15); overlay
  image bytes already route through presenter custody
  (overlays.rs module doc, "damage contract §6") — terminal writes from
  arbitrary component code would bypass that custody.

## Problem
The verbs are engine-private. Applications that need them would have to
run the driver loop themselves (`Driver::turn` embedding) just to touch
the terminal between turns — abandoning `App::run` and its
panic-restore/idle machinery for a title update.

## What we want
A small, queued capability handle — `TerminalActions` (name open):
1. Cloneable handle obtainable from the app (mirroring `Quitter`),
   carrying `set_title(&str)`, `bell()`, `notify(&str)`,
   `clipboard_copy(&str)`.
2. Calls **enqueue requests**; the driver drains the queue during the
   present phase and writes through the terminal it already owns —
   preserving one-writer and the single flush. Requests coalesce
   sanely (last title wins per turn; notifications don't dedupe).
3. Capability honesty: `notify` resolves its channel via
   `Capabilities::notify_channel()`; `clipboard_copy` reports (or
   surfaces via a labeled result/notice) when `osc52_copy` is absent so
   apps can degrade visibly rather than silently.
4. Thread posture: same as the rest of the reactive world — call from
   the UI thread; cross-thread callers go through `WakeHandle::post`
   (document this; do not build a second cross-thread channel).

## Scope / Non-goals
Scope: the handle, the driver drain, caps gating, prelude export, a docs
paragraph + example usage in one existing example. Non-goals: exposing
the raw `Terminal` (custody stays with the driver); clipboard **read**
(never — the write-only stance is deliberate security posture); the OSC 99
rich-notification protocol (ids/actions/icons — the basic form is
forward-compatible, term/verbs.rs:153-156); suspend/resize verbs.

## Expected outcomes
`actions.notify("build failed")` from a component lands on the wire in the
same turn's flush, on the right dialect, with zero new threading rules;
the chat port's unread ping and the console's failure bell are one-liners.

## Validation
- CaptureTerm byte assertions: title/bell/notify/copy bytes present
  exactly once per request, inside the turn's single flush; OSC 9 vs
  OSC 99 selection follows injected caps; no bytes when caps deny and the
  degradation is surfaced.
- Coalescing: two `set_title` calls in one turn emit one title.
- Idle pin: an app holding the handle but not calling it stays
  zero-byte idle (the existing idle tests must stay green).

## Progress checklist
- [ ] Request queue + driver drain at present
- [ ] Handle type + App accessor + prelude export
- [ ] Caps gating + labeled degradation
- [ ] Byte/coalesce/idle tests
- [ ] Docs + example usage
