# LIVEDATA cycle 1 — live-data track (0010, 0020, 0030, 0070)

Status: all four items implemented and green in one cycle. Suites: full
`cargo test` green (931 lib + all integration incl. peers' wave file +
35 doctests, 0 failures), `cargo clippy --all-targets` zero warnings,
`cargo fmt --check` clean. No file over 600 lines; std only.

## Shipped API (all in `reactive::`, no prelude edits — seam filed)

- `channel_source(cx) -> (SourceSender<T>, Signal<Vec<T>>)` — 0010
  append-buffer flavor: every value, in per-sender order, unbounded
  (control lane).
- `latest_source(cx, initial) -> (SourceSender<T>, Signal<T>)` — 0010
  latest-value flavor: newest wins, intermediates coalesce at the
  source, one posted apply per drain cycle.
- `SourceSender<T>`: `Clone + Send`, `send(v)` never blocks/fails,
  inert after scope disposal (counted: `dead_sends()`).
- `bounded_source(cx, capacity, policy) -> (BoundedSender<T>,
  Signal<Vec<T>>, Signal<IngestStats>)` — 0020 data lane: capacity
  bounds transit AND the retained window (≤ 2×capacity total);
  `OverflowPolicy::{DropOldest, DropNewest, Coalesce(CoalesceFn<T>)}`
  (+ `OverflowPolicy::coalesce(f)` sugar); `IngestStats { delivered,
  dropped, coalesced }` with the exactness invariant delivered +
  dropped + coalesced = sent.
- `interval(cx, period, f) -> IntervalHandle` — 0070: fires in phase U
  off the timer heap, fixed-delay drift (missed ticks coalesce, no
  catch-up storms), `cancel()` idempotent + usable from inside `f`,
  scope disposal cancels, handle drop does NOT cancel (the scope owns
  the lifetime — the dead-pane leak is impossible either way). Cancel
  physically removes the heap entry (new internal timer ids), so a
  cancelled interval never bounds the idle sleep.
- Engine micro-fix (scheduler.rs): `RemoteShared::notify` dedups the
  waker via `swap` — N posts between drains = 1 waker invocation; plus
  the 0010/0020 contract text on `WakeHandle::post` (ownership rule,
  ordering, frame semantics, control-vs-data lane).

## Design in 6 lines

1. Producers never touch the graph: senders own `Arc` shared state + a
   `WakeHandle`; every crossing is a posted closure that writes signals
   ON the UI thread in phase U (drain_posted), so the damage contract's
   epoch rule holds unchanged — a burst lands next frame, exactly once.
2. Disposal safety is the arena's stale-handle discipline: apply
   closures check `Signal::is_alive` and count instead of applying.
3. The bounded lane moves values through a mutexed transit deque with
   the policy applied at push (bounded even if never drained), and ONE
   posted drain per cycle (an `AtomicBool` scheduled flag, cleared
   before take — the drain_posted discipline) folds the batch + stats
   into two signal writes inside one `batch()`.
4. Window-stage accounting is arithmetic, not per-item: drops = values
   that never became observable; ring aging of already-shown items is
   deliberately uncounted (retention churn ≠ loss).
5. `interval` rides the existing timer heap as a self-re-arming
   one-shot; re-arm deadline = fire-clock + period where the fire clock
   is `run_due_timers`' `now` (published as `timer_now`), so injected
   test clocks are authoritative end to end.
6. Waker dedup is level-triggered-safe: the flag is set after the push
   and cleared before the take, so a racing post costs at most one
   spurious wake, never a lost one.

## Measured (release, this machine)

- Flood: 100,000 sends from 4 threads through
  `bounded_source(cap 1024, DropOldest)` in **5.49 ms ≈ 18.2 M
  sends/s**; the single drain took **39 µs**; window exactly 1024;
  dropped exactly 98,976 (counted + labeled). CI asserts a loose
  50k/s floor only.
- Wake dedup ratio: **500 posts → 1 waker invocation** (engine dedup,
  asserted); bounded lane additionally **1000 sends → 1 posted job**
  (helper dedup, asserted).
- Burst → exactly 1 frame; quiet source → 16 turns, 0 bytes, 0 flushes
  (both asserted through the real Driver + CaptureTerm).

## Tests

In-module: `reactive::source::tests` (order, concurrent senders,
latest-coalescing, inert+counted disposal ×2, reschedule),
`reactive::ingest::tests` (per-policy exactness ×3, one-drain-per-burst,
bounded+counted disposal, zero-capacity panic, under-capacity order),
`reactive::interval::tests` (steady cadence, missed-tick coalescing,
cancel-between-fires removes the heap entry, cancel-inside-callback,
scope-disposal cancel, drop-does-not-cancel, zero-period panic),
`reactive::scheduler::tests::waker_invoked_once_per_drain_cycle`.
Doctests on `channel_source`, `latest_source`, `interval` (run, not
ignored). Integration (`tests/wave_livedata.rs`, 9):
`concurrent_senders_each_keep_emit_order`,
`sender_outliving_its_scope_is_inert_and_counted`,
`each_policy_accounts_exactly`, `burst_costs_one_wake_and_one_drain`,
`feed_burst_renders_one_frame_and_quiet_source_is_byte_free`,
`worker_quits_cleanly_without_surfacing_a_failure`,
`interval_ticks_render_and_missed_ticks_coalesce_through_the_driver`,
`interval_rearm_uses_the_fire_clock_not_wall_time`,
`flood_100k_posts_stays_bounded_with_exact_accounting`.

## Example + docs

`examples/feed.rs`: bursty worker (xorshift bursts 3–34 + 150–950 ms
gaps) → bounded lane → Scroll view with the hand-rolled follow-tail
idiom (noted as such; header points at CONTENT's Feed for cycle 2),
status line with events/sec (sampled by `interval`) and an
honest dropped counter, space=pause (true idle), clean worker join on
quit, headless exit-0 guard (verified). `docs/live-data.md`: ownership
rule, bindings table, policies, why-no-Block, worker lifecycle,
copy-paste snippet, testing section; linked from docs/README.md (my one
line).

## Spec drift, reported as instructed

- 0010 proposed a `source(cx, label, |emit| …)` worker-spawning shape;
  shipped the sender-shaped API (strictly more general: N producers,
  foreign threads) — composition with `spawn_worker` is documented and
  is the example's shape. Prelude re-export could not be done here
  (file not mine) — seam filed.
- 0020 lists a `Block` policy; per wave instruction Block is NOT
  offered and `DropNewest` ships instead; the refusal rationale
  (UI-stall inheritance, priority inversion, cancellation deadlock) is
  documented on the enum and in docs/live-data.md.
- 0020's "waker dedupe" verified against scheduler.rs as cited: posts
  did invoke the waker unconditionally per post; frame requests already
  deduped. Fixed as the item prescribes (`swap`), no semantic change.
- 0070 signature gained `cx` (scope-disposal cancellation — the wave
  brief's requirement); the item's open design ruling (cancel-on-drop
  vs forget) resolved as: drop does NOT cancel, scope disposal + explicit
  cancel do. Dashboard adoption not performed (examples/dashboard is
  not my path) — mechanical migration filed to the integrator.
- 0030's cross-file checkboxes (SUMMARY.md, examples/README.md,
  dashboard header pointer, api/architecture cross-links) filed to the
  integrator; backlog checklist state recorded here, backlog files left
  untouched (not my paths).

## Seams filed

- `reviews/wave/livedata-to-integrator.md` — prelude re-exports,
  SUMMARY/examples-README lines, dashboard interval migration,
  CHANGELOG entry text.
- `reviews/wave/livedata-to-content.md` — Feed widget pairing contract
  for the cycle-2 example switch (data-in shape, follow signal,
  resize re-pin).
- Nothing needed from STABILITY this cycle (no Driver/App seam: the
  existing phase-U drain + `set_clock` + timer deadline plumbing were
  sufficient as-is).

## Risks / cycle-2 watch list

1. **Feed switch**: my example's follow-tail + status line must become
   deletions, not adapters — depends on CONTENT's Feed API (seam filed;
   I adapt regardless).
2. **Prelude/SUMMARY wiring** is integrator-gated; until it lands, the
   docs page teaches full paths (`abstracttui::reactive::…`) which stay
   correct either way.
3. **Stats write cadence**: the bounded drain writes `stats` on every
   drain; a UI tracking only `dropped` re-renders per drain because
   `delivered` changes too. If that shows up in a real profile, split
   the stats signal (delivered vs dropped) in cycle 2 — cheap, additive.
4. **Coalesce fold panics** (producer thread) poison the transit mutex
   → later sends panic with the lock message. Documented ("cheap and
   panic-free"); if a real consumer hits it, wrap folds in
   catch_unwind + a labeled degradation in cycle 2.
5. **`timer_now` is pass-scoped**: an interval callback that spawns
   nested `run_due_timers` re-entry would see the outer pass's clock —
   not reachable today (timers don't recurse into the pump), noted for
   anyone adding one.
