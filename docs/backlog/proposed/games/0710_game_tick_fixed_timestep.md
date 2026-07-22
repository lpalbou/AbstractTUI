# 0710 — Game tick: public per-frame tasks + a fixed-timestep helper

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: games (band 0700–0790)
- Completed: N/A
- Depends on: nothing hard — the frame-task pump and pacing exist and
  run today; this item makes the surface public and encodes the
  simulation convention once.
- Cross-band consumers: shader-clock drivers (examples/effects.rs
  hand-rolls one today), particle scenes, demo/splash sequencing,
  physics-flavored dashboards.
- Promotion trigger: the first real-time game example, or the second
  in-tree consumer caught hand-rolling an `after`-recursion clock
  (the effects example is the first).

## ADR status
- Governing ADRs: ADR-0001 (additive: publishing an existing internal
  seam + one new helper; no existing public API changes shape).
  ADR impact: none expected — the frame-task contract is already
  written down in the damage contract (§4, animations are sequences of
  frame requests); this item cites it rather than amending it.

## Context
A cell game is a simulation stepped at a steady dt plus a render of the
current state. The engine's loop already paces frames and already runs
per-frame tasks with the frame's honest clock — but only `animate()`
can register one, so every other time-driven consumer builds its clock
from timers and ASSUMES its cadence held.

## Current code reality
- **Pacing exists**: while `frame_tasks_pending() > 0` the drive loop
  waits at most `FRAME_INTERVAL` = 16 ms per turn (~60 fps,
  src/app/mod.rs:363-377); otherwise it blocks on the earliest timer
  deadline or indefinitely (mod.rs:378-389). Zero-idle is preserved by
  construction: an empty task list means no pacing.
- **The pump exists**: `run_frame_tasks(now)` runs every turn in phase
  U with the frame's clock reading (src/app/driver.rs:267-279 —
  `run_due_timers` at 274, `run_frame_tasks` at 278;
  src/reactive/animate.rs:113-130), and tasks self-retire by returning
  false.
- **The registration is private**: `register_frame_task`
  (src/reactive/animate.rs:107-111) — "Internal to the reactive layer;
  `animate` is the public consumer." `request_frame` IS public
  (src/reactive/mod.rs:72-75) but without task registration it only
  buys one extra turn, not a per-frame callback.
- **The cost of the gap is in-tree**: examples/effects.rs drives three
  shader clocks with an `after(FRAME)` recursion that advances time by
  `clock_ms += FRAME.as_millis()` (`FRAME` at effects.rs:27; the
  recursion at effects.rs:84-96 with the `+=` at effects.rs:86) —
  ASSUMED dt.
  The interval contract itself warns the assumption is false under
  load: "the period is therefore a MINIMUM… a job that must know real
  elapsed time reads its own clock inside `f`"
  (src/reactive/interval.rs:13-20).
- **The simulation primitives already speak dt**:
  `ParticleField::step(dt)` is "fixed-timestep-friendly"
  (src/anim/particles.rs:5-6, 130-147); `anim::Clock` provides
  real/virtual time for tests (src/anim/mod.rs:63-119); shaders take a
  seconds clock via `LayerHandle::set_shader_t`
  (src/app/overlays.rs:629-634). The engine ships simulation pieces
  with no sanctioned tick to drive them.

## Problem
Time-driven consumers must choose between (a) `interval`, whose
coalescing fixed-delay contract is right for UI and wrong for
simulation dt, (b) `after`-recursion with assumed dt (the in-tree
example's drift), or (c) abusing `animate()` on a dummy signal to reach
the real frame lane. Games make the gap acute: a 30 fps game wants the
frame's true `now`, a fixed simulation step decoupled from render
cadence, and a pause that returns the app to true idle.

## What we want
1. **Publish the frame-task lane** (suggested `reactive::frame_task(cx,
   impl FnMut(Instant) -> bool)`): scope-owned like `interval` (dispose
   = task dropped at its next poll; document that a task returning
   `true` keeps pacing the loop and MUST be pause-aware — the standing
   zero-idle discipline). Registration re-requests a frame exactly as
   `animate` does (src/reactive/animate.rs:100-102).
2. **A fixed-timestep helper over it** (suggested `anim::Ticker` or
   `game::Loop`, home per owner's taste):
   - configured `sim_dt` (e.g. 1/30 s) + max catch-up steps per frame
     (the spiral-of-death clamp — a suspended terminal must not replay
     minutes of steps; same coalescing philosophy as interval.rs:16-19);
   - calls `step(dt)` zero-or-more times per frame from the
     accumulator, then `render(alpha)` once (interpolation factor
     optional — cell games mostly ignore it, the API should not force
     it);
   - `pause()`/`resume()`/`speed(f32)`; paused = task returns false =
     the loop is truly idle (the effects example's `p` key already
     demonstrates the requirement, effects.rs:9-11);
   - time from the pump's `Instant`, never `Instant::now()` inside the
     task (test-drivability — the same rule `animate` follows,
     animate.rs:30-36).
3. **Docs**: one "time in AbstractTUI" section placing the three lanes
   — `after`/`interval` (timer heap, coalescing, zero-wakeup), frame
   tasks (paced, per-frame, real now), and shader clocks (`set_shader_t`
   fed FROM either) — so consumers stop guessing which lane they want.

## Scope / Non-goals
Scope: the public registration, the helper, pause/speed, docs, tests.
Non-goals: changing `FRAME_INTERVAL` or the pacing policy (16 ms is
right); a render-interpolation framework; frame-rate GUARANTEES (the
loop shares the thread with input and effects — the helper measures and
clamps, it cannot promise); vsync-style timing.

## Expected outcomes
The effects example's hand-rolled clock is deletable (its `after`
recursion becomes a frame task with true dt); a 30 fps game loop is
~10 lines over the helper; pause restores zero-idle; virtual-clock
tests can drive whole game simulations deterministically.

## Validation
- Frame task: registered task receives monotonically nondecreasing
  `now`s; returning false retires it and `frame_tasks_pending` drops;
  scope disposal retires it without a poll leak.
- Ticker: given scripted frame instants, accumulator emits the exact
  expected `step` counts (including the clamp under a simulated 5 s
  stall — steps == max_catch_up, not 150); pause emits zero steps and
  leaves no pending frame requests; speed(2.0) doubles steps over the
  same span.
- Determinism: same scripted instants + same seed (ParticleField) =
  identical surfaces (golden), riding the existing determinism
  contracts (particles.rs:5-7, shaders.rs:8-13).
- Idle honesty: after pause, `frame_tasks_pending() == 0` (the
  drive-loop blocks) — the zero-idle guarantee test-pinned.

## Progress checklist
- [ ] Design nod from the reactive/frame-pacing owner (public surface
      shape: free fn vs Scope method)
- [ ] `frame_task` public registration + scope ownership + tests
- [ ] Fixed-timestep helper (accumulator, clamp, pause, speed) + tests
- [ ] Effects example migrated (deletes the assumed-dt recursion)
- [ ] "Time in AbstractTUI" docs section
