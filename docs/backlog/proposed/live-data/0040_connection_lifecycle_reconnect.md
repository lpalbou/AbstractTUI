# Proposed: Connection lifecycle model + jittered reconnect/backoff helper

## Metadata
- Created: 2026-07-21
- Status: Proposed
- Completed: N/A

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None by itself —
  but the API shape should be reviewed together with the 0050 transport decision so the state
  model does not accidentally encode one transport's semantics.

## Context
The standing external critique names exactly one "real unknown" in AbstractTUI's networking
story: **network-driven reactivity + reconnect under the frame loop**. Items 0010/0020 settle
the reactivity half mechanically; reconnect is the half with no in-repo precedent at all. The
reference domain shows the target shape: the agora client survives drops with an exponential
backoff loop — 0.5 s doubling to a 30 s cap, reset on clean EOF, resubscribing its desired
channel set on every reconnect (~/projects/a2a/src/agora/client/client.py:423-433, :53), with a
REST catch-up path beside the push plane. Every long-lived networked app on this engine will
need precisely this machinery, and it interacts with the frame loop's idle discipline — which is
why it deserves a designed answer rather than N hand-rolled ones.

## Current code reality
- Nothing connection-shaped exists in-crate (no state enum, no backoff helper; verified by
  reading src/reactive/, src/app/, and grepping for reconnect/backoff).
- The idle machinery a disconnected app must cooperate with:
  `App::drive_loop` (src/app/mod.rs:354-387) blocks in `wait_for_activity` with **no deadline**
  when nothing is pending, and in `wait_until(next_timer_deadline())` when one-shot timers are
  armed (mod.rs:369-380). `reactive::after` (src/reactive/animate.rs:141-146) registers a
  one-shot timer that costs **zero wakeups until due** — the natural backoff timer.
- Worker-side waiting is equally free: a background thread sleeping between reconnect attempts
  costs the UI loop nothing; it reports transitions via `WakeHandle::post`
  (src/reactive/scheduler.rs:74) and its death is surfaced loudly (`spawn_worker`,
  scheduler.rs:163; src/app/driver.rs:249-252).
- Honest degradation surfaces exist for the offline state: `use_startup_notices`
  (src/app/mod.rs:196-208), Badge/status-line widgets, and Toast for transient "reconnecting"
  notices (src/app/popups.rs).

## Problem or opportunity
Without a shared model, each app invents its own connection enum, its own backoff (usually
without jitter — thundering-herd on hub restart), and its own answer to "what does the frame
loop do while offline". The wrong hand-rolled answer is a busy-wait or a poll timer that
destroys the engine's zero-idle-cost property; the right answer (worker sleeps; UI blocks; a
state signal drives the offline rendering) is non-obvious enough that the reviews call it the
port's one real unknown.

## Proposed direction
1. A reusable connection-state model as a plain signal-friendly enum — indicative:
   `ConnState { Offline, Connecting, Online, Reconnecting { attempt: u32, retry_at: Instant } }`
   — owned by the app's UI thread, written only via posted transitions from the connection
   worker (the 0010 ownership rule applied). The UI renders it like any signal (badge, status
   line, dimmed panes).
2. A jittered exponential backoff helper (pure, no I/O): `Backoff::next() -> Duration` with
   base/cap/reset — the agora parameters (0.5 s, ×2, cap 30 s, reset on clean close) as
   defaults, plus full jitter to avoid synchronized retry storms. Usable from either side:
   worker-thread sleeps, or UI-side scheduling via `reactive::after`.
3. A documented answer to "the frame loop while disconnected": the worker owns the retry clock
   and sleeps; the UI thread stays blocked in `wait_for_activity` (idle, zero wakeups) and
   repaints only on posted state transitions. If an app wants a visible countdown, that is an
   ordinary one-shot timer, billed as such. Never a poll loop.
4. Catch-up belongs to the app/transport (per-channel cursors in the reference domain), not to
   this model — the state enum must not grow transport-specific fields.

## Why it might matter
This is the named de-risking gap between "the primitives are proven" and "someone can leave a
networked app running overnight". It is also the item the 0060 watcher milestone exercises
under real failure (hub restart mid-session).

## Promotion criteria
Promote to planned/ when the 0060 watcher build starts (it needs this on day one), or when any
second real consumer appears. The API shape should be validated against a real disconnect/
reconnect cycle before the helper's surface is declared stable — which is exactly what 0060
provides; designing the full surface before that evidence exists is the risk that keeps this
item in proposed/.

## Validation ideas
- Unit: backoff sequence (base→cap, reset, jitter bounds); state-transition legality.
- Integration (CaptureTerm + injected clock via `Driver::set_clock`, src/app/driver.rs:204): a
  scripted worker posts Offline→Reconnecting→Online; assert rendered states and that idle turns
  between transitions emit zero bytes.
- Soak-shaped check under 0060: kill and restart the hub; the app reconnects, catches up, and
  the session survives without manual action.

## Non-goals
No transport code, no TLS, no socket ownership (0050 owns that decision); no automatic catch-up/
replay logic; no engine-loop changes — the existing idle machinery is sufficient by design and
this item must prove that, not work around it.

## Guidance for future agents
Re-read `App::drive_loop` and the damage contract before designing: the model must ride the
existing timer/wake machinery. Check whether 0010 landed with a helper — the connection worker
should be expressible as an ordinary 0010 source whose emitted values are state transitions.
