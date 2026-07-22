# LIVEDATA cycle 2 — Feed pairing, coalesce firewall, endurance soak, backlog closure

Status: all five cycle-2 tasks done. Whole-tree `cargo test` green
(958 lib + every integration binary + 40 doctests, 0 failures), clippy
zero across all targets, MY files fmt-clean, no new deps, all my files
< 600 lines (ingest tests split to a sibling file, the soak to its own
binary). Foreign note: `cargo fmt --check` currently flags two diffs in
`src/widgets/scroll_tests.rs` — CONTENT's file, landed mid-write during
my gate run; per shared-tree etiquette I did not touch it (their
formatting pass will clear it).

## 1. Example switch (examples/feed.rs → widgets::Feed)

DELETED: the cycle-1 hand-rolled follow-tail — both effects
(`feed-follow`, `feed-follow-watch`), the derived follow bookkeeping,
the manual `List`-column construction, the per-rebuild content column,
and (after CONTENT's mid-cycle landing) the interim `content_size`
hint + the rebuild-per-extent outer dyn. REPLACED WITH: `FeedState` +
slot-keyed window sync (`slot-0..slot-399` keyed replace — CONTENT's
stated equivalent until `FeedState::clear()` lands), markdown ALERT
items (Feed's rich blocks earn their place), and the ENGINE's
`Scroll::follow_tail(Signal<bool>)` over the MEASURED content extent —
the view now rebuilds on theme switch only; appends never remount.
The `f` key survives but is one line (`follow.set(true)` — the
app-visible half of the engine signal), and the status line reads the
same signal for "following / scrolled". ONE interim remains, marked in
the example header: the slot-keyed sync body becomes
`FeedState::clear()` + re-push when clear() lands. Headless exit-0
guard re-verified after the swap.

## 2. Coalesce-poison fix (the cycle-1 risk item, closed)

A panicking `Coalesce` fold used to unwind through the transit mutex
guard — every later `send` then died on a poisoned-lock panic (the
worst failure shape: one bad fold kills the whole lane opaquely). Now
(`run_fold` in ingest.rs): the fold runs under
`catch_unwind(AssertUnwindSafe(..))` at BOTH fold sites (producer-side
transit overflow, UI-side window overflow); on panic the consumed value
counts as `dropped`, the event increments the new
`IngestStats::fold_panics` (labeled degradation — render it like
`dropped`), the merge target keeps the fold's partial state (it is user
data mid-user-code; synthesizing a clean state would be dishonest), and
the lane keeps working. Why not "retry as DropOldest": the fold OWNS
the incoming value — after the panic it no longer exists; a true retry
would need `T: Clone`. Documented on the enum, the stats struct, and
docs/live-data.md. AssertUnwindSafe soundness argument is in the code
comment (our invariants live entirely outside the closure; the target
goes back to user code either way).

Tests: `transit_fold_panic_degrades_labeled_never_poisons`,
`window_fold_panic_degrades_labeled_and_drain_survives` (both assert
later sends work + exact stats). `IngestStats` gained the field; all
literal constructions in tests updated.

## 3. Hardening

- **Endurance soak**
  (`tests/wave_livedata_soak.rs::soak_60_virtual_seconds_bursty_producer_through_feed`):
  60 virtual seconds on the driver's injected clock; per-cycle producer
  THREAD sends alternating 250/650-event bursts through
  `bounded_source(400, DropOldest)` → slot-keyed `FeedState` → `Feed`
  in `Scroll` with `follow_tail` over the MEASURED extent (the
  example's exact final shape) + a 1 s `interval` (the rate sampler).
  A counting global allocator (the alloc_budget.rs per-thread pattern,
  confined to the soak binary) measures the UI thread. Results:
  allocation plateau DEAD FLAT (7,006 allocs/cycle, ~807.1 KB/cycle in
  both steady-state halves; gate ≤1.5× growth), live reactive nodes
  constant at 26 across all steady cycles (exact plateau — the leak
  detector), Feed and window bounded ≤400 every cycle, one rendered
  frame per burst then byte-free idle (4 idle turns × 60 cycles,
  0 bytes), FOLLOW-TAIL HOLDS every cycle (the newest event is on
  screen — including the full-window steady state where a drain
  replaces content without changing the extent, the edge I filed to
  CONTENT: their landing covers it, now test-pinned from my side),
  stats exact (19,500 delivered + 7,500 dropped = 27,000 sent;
  fold_panics 0), interval cadence exact (60 fires/60 virtual seconds;
  the half-period clock offset in the test names the first-deadline
  epsilon it absorbs).
- **Test-binary split**: the measured half (soak + flood + the
  counting allocator) moved to `tests/wave_livedata_soak.rs`;
  `tests/wave_livedata.rs` keeps the 8 functional pins allocator-free.
  In-module ingest tests moved to the sibling
  `src/reactive/ingest_tests.rs` (the widgets `feed_tests.rs` pattern)
  to keep ingest.rs under the 600-line bar.
- **Re-measured after the tree moved** (release, cycle 2): flood =
  100k posts / 4 threads in 5.71 ms ≈ **17.5M sends/s**, drain 45 µs,
  window exactly 1024, dropped exactly 98,976. Cycle-1 baseline was
  18.2M/s — run-to-run noise (the DropOldest hot path allocates nothing
  per send, and the fold firewall sits outside it). Wake-dedup ratio
  unchanged and asserted: 500 posts → 1 waker invocation; bounded lane
  1000 sends → 1 posted drain.
- **Docs**: live-data.md's copy-paste snippet is now the Feed-based
  pairing, COMPILE-CHECKED against the freshly built rlib (the check
  caught a stale-rlib pitfall: `target/debug/libabstracttui.rlib` can
  be older than `deps/` — pick the newest deps artifact). Back-pressure
  honesty section sharpened: exactness invariant spelled out,
  fold_panics documented, retention-churn-vs-drop distinction stated.

## 4. Backlog closure

0010, 0020, 0030 (planned/live-data) and 0070 (proposed/live-data) each
gained a dated `## Completion report` (shipped API, drift, test names,
measured numbers) and were MOVED to `docs/backlog/completed/live-data/`.
Both track READMEs updated to point at the moves (planned README now
records the wave completion; proposed README's status + 0070 line
updated). No git commands used.

## Seams / for the integrator (final restated list)

Unchanged from cycle 1 plus one wording update — all in
`reviews/wave/livedata-to-integrator.md`: prelude re-exports
(`WakeHandle` + the six live-data names), docs/SUMMARY.md line,
examples/README.md entry, dashboard `tick_loop`/`clock_loop` →
`interval` migration + header pointer, optional api/architecture
cross-links, CHANGELOG entry (text updated for fold_panics + Feed).

## Cycle-3 watch list

1. `follow_tail` + measured extent LANDED mid-cycle and the swap is
   EXECUTED (example, soak, docs snippet — all re-verified). Remaining:
   the `FeedState::clear()` swap in the window-sync body when it lands
   (one marked spot in examples/feed.rs + the docs snippet note).
2. The slot-keyed sync is O(window) pushes per drain with O(window²)
   prefix-sum churn inside FeedState (trivial at 400; measured flat in
   the soak) — clear()+push drops it to O(window); revisit only if a
   port uses windows ≥10k.
3. Follow-tail full-window-replacement edge: now test-pinned from my
   side every soak cycle (their implementation holds it). Nothing owed.
4. Stats-signal granularity (cycle-1 risk 3) stays open-by-evidence:
   the soak showed no cost worth a split yet.
