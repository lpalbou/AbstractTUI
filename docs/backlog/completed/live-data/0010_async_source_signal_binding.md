# Planned: Async data-source → Signal binding (named helper + ownership rule)

## Metadata
- Created: 2026-07-21
- Status: Completed (build wave, LIVEDATA seat, cycles 1-2)
- Completed: 2026-07-21

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None — this names
  and documents an existing, test-pinned mechanism; it changes no policy.

## Context
Both cycle-11 reviews (reviews/cycle11/completeness-and-code-port.md §2a,
reviews/cycle11/robustness-and-chat-port.md §R4 and "The live-data path") reach the same
conclusion: the cross-thread ingress mechanism is the engine's strongest asset for networked
applications — and it is invisible. The standing external critique is that AbstractTUI "ships no
async/HTTP/WebSocket story"; the honest half of that critique is not missing machinery but a
missing name, helper, and contract statement. Every live-data item in this track builds on this
one.

## Current code reality
- `src/reactive/scheduler.rs:61-82` — `WakeHandle` is `Clone + Send + Sync`; `post(f)` pushes a
  `Box<dyn FnOnce() + Send>` into `RemoteShared.posted: Mutex<Vec<PostedJob>>` and fires the
  waker (`notify()`, scheduler.rs:45-54). `wake_handle()` (scheduler.rs:85-89) binds to the
  calling thread's runtime.
- `src/reactive/scheduler.rs:111-123` — `drain_posted()` runs all queued closures on the UI
  thread, FIFO in post order (single `Vec`, taken whole). Per-producer order is therefore
  preserved; cross-producer order is lock-acquisition order.
- `src/reactive/scheduler.rs:163-179` — `spawn_worker(label, f)` catches worker panics and posts
  a labeled failure; `Driver::turn` converts it to an app error (src/app/driver.rs:249-252).
- `src/app/driver.rs:156-159` — `Driver::new` installs the `TerminalWaker` (src/term/waker.rs:46)
  as the wake callback; a post interrupts the blocking read (unix self-pipe, src/term/unix.rs:595;
  windows auto-reset event, src/term/windows.rs:461).
- `docs/design/01-damage-contract.md` §1-2 — posted jobs run only in phase U; the damage set
  seals at phase L; a post landing mid-frame lands in the NEXT frame exactly once. Pinned by
  `tests/adv_app.rs:96` (`cross_thread_post_lands_exactly_one_frame_later`) and
  `tests/adv_app.rs:299` (`spawned_worker_panic_surfaces_as_app_error`).
- `src/reactive/runtime.rs:158-162` — `check_thread` makes wrong-thread `Signal` use a named
  panic, never silent aliasing. This is the enforcement side of the ownership rule.
- `src/prelude.rs` — exports `Signal`, `batch`, widgets, `App`; `WakeHandle` and `spawn_worker`
  are NOT in the prelude and appear in zero examples (grepped `examples/`: only `reactive::after`
  timers, e.g. examples/dashboard/main.rs:100-133 fakes its feed).

## Problem
The load-bearing pattern for any networked or long-lived app — background thread reads I/O,
posts a closure, the closure writes a `Signal` on the UI thread — exists only as raw primitives
discovered by reading `src/reactive/scheduler.rs`. There is no named binding from "a stream of
values produced off-thread" to "a Signal the UI reads", no stated ordering guarantee, and no
single sentence an application author can cite for the ownership rule (all Signal writes happen
on the UI thread, inside posted closures). Consumers will each reinvent the three lines — or
worse, try to write signals from the worker and hit the thread panic without knowing the
sanctioned alternative.

## What we want to do
1. Ship a thin, named helper over the existing primitives — indicative shape:
   `reactive::source(cx, label, |emit| { ... worker loop ... }) -> Signal<T>` (or a
   `feed_signal`-style free function pairing `spawn_worker` + `WakeHandle::post` + a
   `Signal<T>`/`Signal<Vec<T>>` fold). The worker gets an `emit(value)` handle; each emit posts a
   closure that applies the value to the signal on the UI thread. No new mechanism: composition
   of `spawn_worker`, `WakeHandle`, `Signal`.
2. Document the two guarantees the primitives already provide, as contract text on the helper
   and on `WakeHandle::post`: (a) ordered delivery — values from one producer apply in emit
   order; (b) frame semantics — a burst of emits coalesces into one wake and one frame, and an
   emit landing mid-frame lands next frame (damage contract §2).
3. State the ownership rule in one place, in plain words: the reactive graph is single-threaded;
   background threads never touch signals; the only sanctioned crossing is a posted closure.
   Cross-reference the named panic (`runtime.rs:158`) as the enforcement.
4. Re-export `WakeHandle` (and the new helper) from the prelude, per the cycle-11 suggestion.

## Scope / Non-goals
Scope: one helper (≤ ~100 lines) in `src/reactive/`, rustdoc contract text, prelude re-export,
tests. Non-goals: bounded queues and drop policies (0020 builds on this); any transport or I/O
code (0050 owns that decision); connection state modeling (0040); changing `WakeHandle::post`
semantics — the raw primitive stays as-is for callers that need it.

## Expected outcomes
An application author finds one named function for "background data into the UI", with ordering
and threading rules stated where the cursor hovers, and never needs to read scheduler internals
to write their first networked app.

## Validation
- Unit: N values emitted from one worker arrive in order in the signal; emits from two workers
  each preserve per-worker order.
- Integration (CaptureTerm + `Driver::turn`): a burst of emits between turns renders exactly one
  frame; a worker panic surfaces as an app error (extends the existing `tests/adv_app.rs` pins).
- Doctest on the helper compiles and runs (not `ignore`-fenced).
- Idle discipline: with the worker asleep, turns stay idle — extend
  `tests/adv_app.rs:55` (`idle_app_emits_zero_bytes_across_idle_turns`) shape to the helper.

## Progress checklist
- [ ] Helper implemented in `src/reactive/`
- [ ] Ordering + ownership contract text on helper and `WakeHandle::post`
- [ ] Prelude re-export (`WakeHandle` + helper)
- [ ] Unit + integration + doctest coverage

## Completion report
- Final path: docs/backlog/completed/live-data/0010_async_source_signal_binding.md
- Date: 2026-07-21
- Shipped API (`src/reactive/source.rs`):
  `channel_source(cx) -> (SourceSender<T>, Signal<Vec<T>>)` (append
  buffer: every value, per-sender order) and
  `latest_source(cx, initial) -> (SourceSender<T>, Signal<T>)`
  (newest wins, coalesces at source, one posted apply per drain cycle).
  `SourceSender<T>` is `Clone + Send`, never blocks/fails, and is
  INERT + COUNTED (`dead_sends()`) after the owning scope's disposal.
  Contract text (ownership rule, ordered delivery, one-frame-per-burst,
  control-vs-data lane) landed on `WakeHandle::post` and both helpers;
  running doctests on both.
- Deliberate drift from the item's indicative shape: the sender-shaped
  API replaced `source(cx, label, |emit| ...)` — strictly more general
  (N producers, foreign threads); worker composition via `spawn_worker`
  is the documented and example shape. Prelude re-export is filed to
  the integrator (`reviews/wave/livedata-to-integrator.md`) — the file
  is outside the LIVEDATA seat's paths.
- Tests: `reactive::source::tests::{channel_delivers_every_value_in_send_order,
  channel_preserves_per_sender_order_across_concurrent_senders,
  latest_coalesces_bursts_to_the_newest_value,
  latest_reschedules_after_each_drain,
  sends_after_scope_disposal_are_inert_and_counted,
  latest_sends_after_disposal_are_inert_and_counted_once_per_cycle}`;
  integration `tests/wave_livedata.rs::{concurrent_senders_each_keep_emit_order,
  sender_outliving_its_scope_is_inert_and_counted,
  feed_burst_renders_one_frame_and_quiet_source_is_byte_free,
  worker_quits_cleanly_without_surfacing_a_failure}`.
- Measured: burst of 500 posts = 1 waker invocation (see 0020's dedup);
  burst renders exactly one frame; 16 idle turns with a live-but-quiet
  sender emit 0 bytes, 0 flushes (asserted through Driver+CaptureTerm).
