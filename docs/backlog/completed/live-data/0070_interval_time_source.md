# 0070 — Recurring time source: `interval` beside `reactive::after`

## Metadata
- Created: 2026-07-21
- Status: Completed (build wave, LIVEDATA seat, cycles 1-2 — promoted
  and executed with the live-data foundation; dashboard adoption filed
  to the integrator, see the completion report)
- Track: live-data
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None — this repo has no ADR system yet (see 0170).
  ADR impact: None expected (additive helper over the existing timer
  heap; no loop or policy change).

## Context
Time is the zeroth data source: dashboards and monitors refresh on a
cadence, clocks tick, pollers re-arm, presence and meters decay, games
step fixed ticks. Every app class has at least one periodic job, and the
engine ships only the one-shot: `reactive::after`. The robustness review
names the gap directly in its smaller-items list — "an `interval` helper
beside `reactive::after` (the dashboard hand-rolls re-arming)"
(`reviews/cycle11/robustness-and-chat-port.md`, Part 2 P2). The flagship
example is the evidence: it defines recursive self-rescheduling loops for
its tick and its clock (`examples/dashboard/main.rs:97-110`,
`tick_loop`/`clock_loop`) — the exact boilerplate every consumer will
re-derive, each with its own cancellation bug surface.

## Current code reality
- `src/reactive/animate.rs:141-146` — `after(delay, f)` pushes onto the
  runtime's timer heap and wakes the loop to recompute its sleep
  deadline. Contract text on it is explicit: timers do NOT frame-pace;
  a pending timer costs zero wakeups until due.
- `src/app/mod.rs:369-380` — the idle branch blocks in
  `wait_until(next_timer_deadline())` when timers are armed and in
  `wait_for_activity` (no deadline) otherwise: the machinery an interval
  must ride is already zero-idle-cost by construction.
- `src/reactive/animate.rs:154-174` — `run_due_timers` fires in phase U
  and tolerates timers registering new timers mid-run, which is exactly
  what a re-arming interval does.
- `examples/dashboard/main.rs:97-110` — the hand-rolled shape: a named
  `fn` that calls `after(TICK, …)` and re-invokes itself. It has no
  cancellation story (the example never needs one; real apps do — a
  poller must stop when its pane closes).
- One-shot cancellation does not exist either: `after` returns `()`; the
  only way to "cancel" is a flag checked inside the closure.

## Problem
Every periodic behavior costs a recursive helper function and an ad-hoc
cancellation flag, both easy to get subtly wrong (a forgotten flag keeps
a dead pane's poller re-arming forever — a silent wakeup leak in an
engine whose brand is zero idle cost). The engine owns the timer heap and
the idle discipline; it should own the three lines that make them safely
repeatable.

## What we want (proposed shape)
1. `reactive::interval(period, f) -> IntervalHandle`: runs `f` on the UI
   thread every `period`, riding the existing timer heap (one pending
   timer per interval; re-armed after each fire; never frame-paced).
2. **Cancellation as a first-class handle**: `IntervalHandle::cancel()`
   (idempotent), plus cancel-on-drop or explicit `forget()` — the design
   ruling this item needs before planning; either way the dead-pane
   leak must be impossible to write by accident.
3. **Drift policy stated honestly** in the rustdoc: fixed-delay
   (next = fire-time + period; late fires do not burst to catch up) is
   the proposed default — periodic UI work wants steadiness, not
   catch-up storms; whichever is chosen, the contract text says so.
4. The dashboard's `tick_loop`/`clock_loop` rewritten onto it, so the
   flagship example teaches the sanctioned shape.

## Scope / Non-goals
Scope: the helper + handle, contract text, dashboard adoption, tests.
Non-goals: frame-paced animation (frame tasks / `animate` own the
per-frame lane, animate.rs:107-135); wall-clock/cron scheduling
("every day at 9" is app policy); background-thread timers (workers own
their own clocks and sleep on them — the 0040 reconnect posture);
changing `after` (it stays the primitive; `interval` composes it or the
heap directly).

## Expected outcomes
Periodic refresh is one cancellable line; between ticks an app is
byte-for-byte idle exactly as if the interval did not exist; no consumer
ever writes a self-rescheduling recursion or a cancellation flag again.

## Validation
- Injected-clock test (`Driver::set_clock`, src/app/driver.rs:204): N
  simulated periods → exactly N fires, monotone spacing under the stated
  drift policy.
- Cancellation: cancel between fires → no further fires; cancel inside
  the callback → no re-arm; handle drop behaves per the ruling.
- Idle pin: with one armed interval and no fire due, turns emit zero
  bytes (extend `tests/adv_app.rs:55`'s shape with the timer-bounded
  wait); the allocation budget stays green.
- Dashboard example still renders its tick/clock identically
  (docs/captures/ regeneration unchanged).

## Progress checklist
- [ ] Design ruling: handle semantics (cancel-on-drop vs forget) + drift policy
- [ ] `interval` + `IntervalHandle` over the timer heap
- [ ] Contract text (no frame pacing; zero wakeups between fires)
- [ ] Dashboard `tick_loop`/`clock_loop` adoption
- [ ] Injected-clock + cancellation + idle tests

## Completion report
- Final path: docs/backlog/completed/live-data/0070_interval_time_source.md
- Date: 2026-07-21
- Shipped API (`src/reactive/interval.rs`):
  `interval(cx, period, f) -> IntervalHandle` over the existing timer
  heap (one pending one-shot, re-armed after each fire; timers never
  frame-pace; zero wakeups between fires). Internals: timer entries
  gained cancellation ids (`runtime.rs` TimerEntry) and the fire pass
  publishes its clock (`timer_now`), so re-arms ride the LOOP's
  injected clock deterministically. `after` is byte-compatible.
- Design rulings the item left open, resolved: drift policy =
  FIXED-DELAY (next = fire time + period; a suspend of N periods fires
  ONCE — missed ticks coalesce, no catch-up storms; documented in the
  rustdoc contract). Handle semantics = drop does NOT cancel; explicit
  `cancel()` (idempotent, callable from inside `f`) and scope disposal
  do — the dead-pane leak is impossible either way, and cancel
  physically removes the heap entry so a cancelled interval never
  bounds the idle sleep. Signature gained `cx` for the disposal tie.
- Deliberate drift: dashboard `tick_loop`/`clock_loop` adoption is
  filed to the integrator (examples/dashboard is outside the LIVEDATA
  seat's paths; the migration is mechanical and written out in
  reviews/wave/livedata-to-integrator.md). Proof-of-consumption lives
  in examples/feed.rs (events/sec sampler) and the tests below.
- Tests: `reactive::interval::tests::{fires_once_per_elapsed_period_with_steady_clock,
  missed_ticks_coalesce_into_one_fire,
  cancel_between_fires_removes_the_pending_timer_entirely,
  cancel_from_inside_the_callback_stops_the_rearm,
  scope_disposal_cancels_the_interval, dropping_the_handle_does_not_cancel,
  zero_period_panics_loudly}` + doctest; integration
  `tests/wave_livedata.rs::{interval_ticks_render_and_missed_ticks_coalesce_through_the_driver,
  interval_rearm_uses_the_fire_clock_not_wall_time}` and the endurance
  soak (`tests/wave_livedata_soak.rs`) (the soak
  pins 60 fires across 60 virtual seconds on the driver's injected
  clock, with armed-but-not-due turns emitting zero bytes).
