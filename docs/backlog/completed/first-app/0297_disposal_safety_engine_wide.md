# Completed: disposal safety engine-wide — the 0250 ruling stopped at List/Table

## Metadata
- Created: 2026-07-22
- Status: Completed (fix wave 3, FIXNET)
- Completed: 2026-07-23

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

## The law (stated once, engine-wide)

**Every widget completes its own bookkeeping — every write to its
scope-owned signals — BEFORE user callbacks run; a callback may
dispose the widget's scope synchronously.** `EventCtx` calls
(`stop_propagation`, focus/capture requests) are dispatch-owned flags,
exempt on either side of a callback (the 0250 List/Table tests already
prove dispatch tolerates mid-dispatch disposal). One knowable
consequence, accepted by the 0250 precedent: bookkeeping uses the
state as the WIDGET left it — a callback that mutates the widget's own
state (submit-and-clear) sees that mutation rendered by the next
event, never retroactively applied to the event that fired it. Stated
for consumers in `docs/api.md` § "The widget disposal-safety law";
0170's stability pass should cite it from there.

## Completion report (2026-07-23, fix wave 3 — FIXNET)

Final path: `docs/backlog/completed/first-app/0297_disposal_safety_engine_wide.md`.

**Audit of every user-callback site in `src/widgets` + `src/app`**
(criterion: any write to scope-owned signals after user code runs):

| Site | Order found | Verdict | Action |
| --- | --- | --- | --- |
| `Button::on_click`, mouse-Up arm (button.rs) | `fire()` THEN `pressed.set(false)` | **OFFENDER** (the filing) | FIXED: `pressed.set(false); stop_propagation(); fire()` |
| `Button::on_click`, keyboard arm | fire, then only `stop_propagation` | clean (EventCtx exempt) | stop_propagation moved ahead of fire for uniformity; pinned |
| `TextArea` `on_change`/`on_submit` (textarea.rs handler) | `handle_key` notified INSIDE, then the handler re-published the caret cell (`caret.get_untracked` + `publish_caret_cell` + `caret.set`) AFTER the callback | **OFFENDER** (found by this audit; the filing's "not yet audited" list was right to exist — a submit-and-close composer panicked on the dead caret signal) | FIXED: `handle_key` now returns the OWED callback (`Owed::{Nothing,Change,Submit}`) and runs no user code; the handler publishes the caret cell, stops propagation, THEN notifies over a pre-read snapshot |
| `Checkbox::on_change` (`toggle`) | `checked.set` then callback, nothing after | clean | pinned |
| `RadioGroup::on_change` (`pick`) | `selection.set` then callback | clean | pinned |
| `Tabs::on_change` (`switch`) | `active.set` then callback | clean | pinned |
| `TextInput` `on_change`/`on_submit` (edit_key/notify) | all value/caret writes before `notify`; only `stop_propagation` after | clean | pinned (both arms) |
| `List::on_select`/`on_activate` | bookkeeping-first since 0250 | clean | already pinned by 0250 |
| `Table::on_select` | bookkeeping-first since 0250 | clean | already pinned by 0250 |
| `Table::on_sort_requested` (s-key + header-click arms) | callback last in both arms | clean | pinned |
| `Select`/`Combobox`/`MultiSelect` commit `on_change` (`write_value`/`commit_and_close`) | `value.set` then callback; post-callback code touches only Rc session state + the IDEMPOTENT `popup.dismiss` — under a disposing callback the popup's content scope (a child of the face's) dies first and the AnchorGone cleanup ends the popup, so the later dismiss no-ops | clean, but it hangs on the cascade, not ordering | pinned (`select_commit_on_change_may_dispose_the_faces_scope`) |
| `Popup::on_dismiss` (anchored_owned.rs `end`) | layer removed + scope disposed BEFORE the callback | clean by construction | covered by the select pin |
| `Overlays::on_outside_press` (`fire_outside_press`) | take → run → put-back into the app-owned store, `index_of`-guarded | clean (no scope-owned writes after) | — |
| `PushToTalk::on_start`/`on_stop` | `state.set` first; post-callback writes touch only the Rc slot | clean | — |
| `Viewport3D::on_orbit`/`on_zoom` | post-callback writes touch only `Rc<RefCell<Option<Point>>>` + EventCtx | clean | — |
| `Actions::run` | take → run → put-back into the Rc registry | clean for the LAW; adjacent nit noted below | — |
| Mapping fns (Feed `height`/`key`/`fingerprint`/`render`, Completion `provider`, Tabs panel builders) | value-mapping callbacks, not event callbacks | out of scope (a mapping fn disposing its own widget mid-build is not the modal-close class) | — |

**Fixes shipped** (both semver-invisible: pure ordering):

1. `src/widgets/button.rs` — mouse-Up: `pressed` cleared before
   `fire()` (the 0250 move verbatim); keyboard arm reordered for
   uniformity.
2. `src/widgets/textarea.rs` — `handle_key` returns `(consumed, Owed)`
   and runs NO user code; the handler completes the caret-cell publish
   + `stop_propagation` and fires the owed callback LAST. Deliberate
   divergence documented in-code: a buffer-mutating callback
   (submit-and-clear) leaves the published caret cell one event stale;
   the next event re-publishes (anchor consumers key off caret
   movement).

**Representative disposal tests** (each disposes the widget's scope
from inside the callback; green):

- `widgets::button::tests::on_click_may_dispose_the_buttons_scope`
  (mouse + keyboard arms)
- `widgets::textarea::disposal_tests::on_submit_may_dispose_the_textareas_scope`
- `widgets::textarea::disposal_tests::on_change_may_dispose_the_textareas_scope`
- `widgets::input::tests::callbacks_may_dispose_the_inputs_scope`
- `widgets::checkbox::tests::on_change_may_dispose_the_checkboxes_scope`
- `widgets::radio::tests::on_change_may_dispose_the_radio_groups_scope`
- `widgets::tabs::tests::on_change_may_dispose_the_tabs_scope`
- `widgets::table::tests::on_sort_requested_may_dispose_the_tables_scope`
- `app::select::tests::select_commit_on_change_may_dispose_the_faces_scope`

**Law statement**: in this item (above) and in `docs/api.md` § "The
widget disposal-safety law" (consumer-facing; names every pinned site).

**Acceptance handoff**: the consumer deletion criterion is now met on
the engine side — every widget callback that can close a modal is
disposal-safe and pinned. Deleting `UiCtx::retire`'s one-tick deferral
(consumer `src/ui/mod.rs:67-94`) is `abstractcode-tui`'s move on its
next engine bump.

**Follow-ups revealed** (recorded, not invented):

- `Actions::run` puts the taken callback back BY STALE INDEX: an
  action that unregisters an EARLIER entry while running shifts the
  vector, so the put-back can land on a different entry (clobbering
  its callback) or fall off the end (the running action loses its own
  closure — it goes dead after one self-modifying run). Not a
  disposal/UB issue (Rc registry, no scope-owned state) and not this
  item's law; worth a by-name put-back when the actions surface next
  opens.
- The select faces' `commit_and_close` fires `on_dismiss` with
  `AnchorGone` (not `Commit`) when the commit callback disposes the
  opener — semantics footnote only; both reasons mean "ended", and
  the face's own session bookkeeping handles either.
