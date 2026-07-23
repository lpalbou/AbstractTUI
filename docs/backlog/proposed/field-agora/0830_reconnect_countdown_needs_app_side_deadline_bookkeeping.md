# Proposed: rendering the reconnect countdown honestly requires app-side deadline bookkeeping the engine already owns

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — ~20min once understood; every reconnect UI will re-derive it
- Class: API gap

## Context
The watcher's charter says the header renders "reconnecting (attempt +
countdown) — never a lie". `ConnState::Reconnecting { attempt, next_in }`
carries the DRAWN DELAY at transition time — perfect for a static
"retry #2 in 1.4s" (the documented rendering), but a static number on
screen for up to 30s reads stale immediately: by the time a human looks,
"in 12.3s" is false by however long the badge has been up.

A live countdown needs the DEADLINE, and the app must reconstruct it:
an effect that catches the transition and stores
`Instant::now() + next_in`, plus a ticker scoped to the Reconnecting
branch (a `dyn_view_scoped` region owning a 500ms `interval`, so the
clock dies the moment the state changes — otherwise the standing timer
violates the app's own zero-idle posture). It works — the live drill
rendered a ticking countdown through 7 attempts — but the deadline is a
fact the engine already holds precisely (it armed the retry one-shot at
exactly that instant, connection.rs:372), and the app-side reconstruction
is off by the post/drain latency between the timer arm and the effect
run. Every consumer of `reactive::connection` that takes "honest
countdown" seriously will write this same ~25-line dance.

## Current code reality (0.2.8)
- `src/reactive/connection.rs:83` — `Reconnecting { attempt: u32,
  next_in: Duration }`; `next_in` is the jitter draw, not a deadline.
- `:372` — `arm_timer_at(now + next_in, …)`: the engine computes the
  true deadline and drops it into the timer heap; nothing exposes it.
- `ConnState` is a closed vocabulary by design — but a deadline is
  transport-agnostic, and an ACCESSOR (below) needs no enum change.

## Repro
Render `ConnState::Reconnecting` per the docs ("retry #2 in 1.4s") and
leave the hub down with the 30s cap: the badge shows a frozen number
for tens of seconds. Now try to render it ticking: you need the
deadline-capture effect + scoped interval described above.

## Workaround in the field (delete when fixed)
`src/ui/header.rs` in agora-tui: the `reconnect-deadline` effect
(Instant reconstruction) + the scoped 500ms ticker inside
`status_line`. An engine-side `Connection::retry_deadline() ->
Option<Instant>` (or `Reconnecting` carrying the deadline in a future
0.3 vocabulary) would delete the reconstruction; the scoped ticker
stays (rendering cadence is the app's business).
