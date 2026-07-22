# Proposed: disposal safety engine-wide — the 0250 ruling stopped at List/Table

## Metadata
- Created: 2026-07-22
- Status: Proposed (footgun/API — FIELD study-2 consumer-tensions
  finding §3.1, filed by the convergence pass; the report ranked it
  top-tension #2)
- Completed: N/A

## ADR status
- Governing ADRs: None. ADR impact: none — widget callback contract;
  worth one line in the eventual API-stability pass (0170) as a stated
  law ("widget bookkeeping completes before user callbacks run").

## Context
The 0250 ruling ("bookkeeping-before-callbacks") fixed the
dispose-from-a-callback crash on `List` AND `Table`
(completed/first-app/0250, shipped 2026-07-22). It was applied to the
two widgets that crashed, not stated as an engine-wide law — and the
first consumer is still paying for the difference: every modal close in
`abstractcode-tui` defers scope disposal one tick (`UiCtx::retire`,
consumer src/ui/mod.rs:67-94, via `after(Duration::ZERO, …)`) because
"`Button`'s mouse path still writes its own `pressed` signal AFTER
`on_click` returns … a synchronous modal close from a mouse-clicked
approve/deny button would still die with 'handle used after its node
was disposed'". Their comment names the deletion criterion verbatim:
"Delete only when EVERY widget callback that can close a modal is
disposal-safe."

## Current code reality
- **Button is the confirmed offender** (verified at source, convergence
  cycle 2): the mouse-Up arm runs `fire()` then `pressed.set(false)`
  (src/widgets/button.rs:193-197) — a click callback that disposes the
  button's scope (the natural modal approve/deny) leaves `pressed`
  dangling for the post-callback write. The keyboard arm is clean
  (fire, then only `ctx.stop_propagation()`, button.rs:178-183).
- **Audited clean this cycle** (set-before-callback, no post-write):
  `Checkbox` (`toggle`: `checked.set` THEN `on_change`,
  src/widgets/checkbox.rs:96-102), `Tabs` (`active.set` THEN
  `on_change`, src/widgets/tabs.rs:102-103), `RadioGroup`
  (`selection.set` THEN `on_change`, src/widgets/radio.rs:108-109).
- **Not yet audited** (the item's real deliverable): every remaining
  user-callback site — TextInput/TextArea `on_change`/submit paths,
  the `app::select` faces' commit callbacks, Scroll/Feed callbacks if
  any, and every future widget. A single missed site re-creates the
  0250 crash class in whichever consumer composes it with a close.
- The consumer's deferral is not free: it is one tick of a dead-looking
  modal plus two paragraphs of justification in every modal-using app
  (the report: "the most item-worthy unfiled finding").

## Problem or opportunity
Disposal safety is currently a per-widget accident, not a contract.
Consumers cannot know which callbacks may synchronously dispose the
widget's scope, so the only safe posture is deferring EVERY close —
re-introducing the exact latency/ghost-input class 0250 was fixed to
end.

## Proposed direction (engine's call)
1. Fix Button: complete the widget's own state writes BEFORE invoking
   `on_click` in the mouse-Up path (`pressed.set(false); fire();` —
   the 0250 move applied verbatim), with a regression test that a
   click callback disposing the button's scope does not panic.
2. Audit every widget callback site against the same law; fix any
   further offenders found.
3. State the law once in the widget docs (and cite it from 0170's
   stability pass): "all widget bookkeeping completes before user
   callbacks run; a callback may dispose the widget's scope."
4. Acceptance is consumer deletion: `UiCtx::retire`'s one-tick deferral
   (and its justification block) becomes a synchronous close.

## App-side state
`abstractcode-tui` ships the one-tick retire deferral described above;
it is deletable exactly when the law holds engine-wide — their comment
already says so.
