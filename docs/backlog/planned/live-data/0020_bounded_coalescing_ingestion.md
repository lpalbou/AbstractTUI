# Planned: Bounded, coalescing event ingestion (back-pressure for flooding producers)

## Metadata
- Created: 2026-07-21
- Status: Planned
- Completed: N/A

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None — engine
  behavior for the raw primitive is unchanged; the bounded path is an additive helper plus one
  internal micro-fix.

## Context
Both cycle-11 reviews flag the same hazard independently
(reviews/cycle11/completeness-and-code-port.md P1-5,
reviews/cycle11/robustness-and-chat-port.md "The live-data path" → Backpressure): rendering
coalesces by construction, but the posted-jobs queue does not. A flooding backend — rapid
tool-result chunks, a bursty message hub — grows the queue without limit between turns. The
reference domain already solves this shape on its own side: the agora client's inbox is a
bounded deque (`~/projects/a2a/src/agora/client/inbox.py:24`, `maxsize=1000`) that drops with
cursor-based recovery on the hub (inbox.py:34). The engine-side recipe must exist once, or every
consumer rediscovers it under load.

## Current code reality
- `src/reactive/scheduler.rs:30-34` — `RemoteShared.posted` is `Mutex<Vec<PostedJob>>`: no bound,
  no drop policy, no growth visibility.
- `src/reactive/scheduler.rs:74-81` — `WakeHandle::post` pushes then calls `notify()`
  unconditionally; `notify()` (scheduler.rs:45-54) stores the wake flag and invokes the waker
  callback (a self-pipe write) **per post**, even when the flag is already set. Frame requests
  dedupe (`request_frame`, scheduler.rs:134-146, only the first request between
  `take_frame_request` calls reaches the `FrameRequester`); wake notifications do not.
- `src/reactive/scheduler.rs:111-123` — `drain_posted()` takes the whole `Vec` each turn; memory
  between turns is whatever producers managed to queue.
- Rendering already coalesces: N posts = one wake = one frame
  (`tests/adv_app.rs:96`, damage contract §2). The exposure is queue memory and per-post
  lock/pipe churn, not repaint cost.
- Labeled-degradation conventions exist for surfacing this honestly:
  `App::push_startup_notice`/`use_startup_notices` (src/app/mod.rs:196-208) and the crate-wide
  labeled-fallback posture (docs/design/00-vision.md).

## Problem
Under a flooding producer the engine has (a) unbounded memory growth in the posted queue,
(b) one waker invocation and one mutex round-trip per post, and (c) no signal anywhere that
pressure occurred — a slow terminal plus a fast feed silently degrades into growth. The raw
primitive is correct for its job (low-rate control messages); what is missing is the bounded
lane for high-rate data and the guidance that producers should batch.

## What we want to do
1. **Waker dedupe (engine micro-fix):** in `RemoteShared::notify`, skip invoking the waker when
   the wake flag was already set (`swap` instead of `store`). Cuts per-post pipe writes to one
   per drain cycle. No semantic change: the loop already treats wakes as level-triggered
   (`drain_posted` clears the flag before running jobs, so a mid-drain post re-flags).
2. **Bounded ingestion helper**, layered on 0010's binding: a producer-side handle owning a
   bounded buffer (capacity chosen by the app), which posts **one** drain closure per wake
   carrying the batch, with an explicit overflow policy chosen at construction:
   - `DropOldest` — ring semantics; count what was dropped (the agora-inbox shape);
   - `Coalesce(fn)` — merge superseded values (progress updates, presence refreshes);
   - `Block` — apply producer-thread back-pressure (only for producers that may stall).
3. **Labeled back-pressure signal:** the helper exposes a `Signal<u64>` dropped-count (or a
   small stats struct) so the UI can render "N events dropped" honestly; document that a
   consumer surfacing it should follow the labeled-degradation convention. Silent dropping is
   not acceptable; unlabeled truncation is the failure mode this exists to prevent.
4. **Producer-side batching guidance:** contract text (here and in the 0030 docs page): read
   loops should drain everything available per read and emit batches, not per-item posts —
   one closure per batch is the intended cadence for high-rate sources.

## Scope / Non-goals
Scope: the `notify` dedupe, one bounded helper type + policies, the drop-count signal, tests,
contract text. Non-goals: bounding `WakeHandle::post` itself (it stays unbounded by contract —
low-rate control lane; bounding it would silently break posted-closure semantics for existing
callers); any global queue cap or engine-wide policy knob; transport work (0050); changing the
one-frame-per-drain render behavior (already correct).

## Expected outcomes
A flooding producer costs bounded memory and one wake per drain cycle; overflow is a counted,
render-able fact instead of silent growth; the raw primitive's contract ("unbounded, low-rate
control lane — batch or use the bounded helper for data") is written where callers will see it.

## Validation
- Flood test: M producer threads × N posts through the bounded helper — memory bounded by
  capacity, kept-item order preserved, drop counter exact under `DropOldest`.
- Coalesce test: superseded values merge; final applied state equals last-writer state.
- Waker-dedupe test: K posts between drains invoke the waker callback once (extend the counting
  pattern in scheduler.rs tests / src/term/waker.rs:78).
- Integration: burst through the helper renders one frame (CaptureTerm + `Driver::turn`);
  idle-zero-cost pins (tests/adv_app.rs:55, tests/alloc_budget.rs) stay green.

## Progress checklist
- [ ] `notify` waker dedupe + test
- [ ] Bounded helper with DropOldest / Coalesce / Block policies
- [ ] Dropped-count signal + labeled-degradation guidance
- [ ] Contract text on `WakeHandle::post` (control lane vs data lane)
- [ ] Flood/coalesce/ordering/integration tests
