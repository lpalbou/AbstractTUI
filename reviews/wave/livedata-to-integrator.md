# LIVEDATA → integrator: seams outside my paths (cycle 1)

Everything below is shipped and tested on my side; these are the
one-line-each wirings that live in files I do not own.

## src/prelude.rs (0010 checklist item "prelude re-export")

Add to the `crate::reactive` re-export line (app-code surface, all
documented):

```rust
pub use crate::reactive::{
    batch, untrack, Memo, Scope, Signal,
    // live-data wave:
    bounded_source, channel_source, interval, latest_source,
    IngestStats, IntervalHandle, OverflowPolicy, SourceSender, WakeHandle,
};
```

(`BoundedSender` can stay behind the explicit path — it appears in type
position rarely; include it if you prefer symmetry. `WakeHandle` is the
0010-cited cycle-11 suggestion.)

## docs/SUMMARY.md (mdBook index)

One line, next to the other guides:

```md
- [Live data](live-data.md)
```

docs/README.md already links it (my line). Optional cross-links the
0030 item names: architecture.md's scheduler paragraph and api.md's
reactive section can point at `live-data.md`.

## examples/README.md

Catalog entry for the new example:

```md
| feed | Live background data: bursty worker → bounded ingestion →
scrolling view; drop counter, events/sec via `interval`, zero-idle
proof. No special terminal requirements. Keys: space pause, f follow,
q quit. |
```

## examples/dashboard/main.rs (DESIGN-era file — 0070 adoption)

`tick_loop`/`clock_loop` (main.rs:97-112) are the hand-rolled
self-rescheduling recursion 0070 replaces. Migration is mechanical:

```rust
// before: fn tick_loop(tick: Signal<u64>) { after(TICK, move || { ...; tick_loop(tick) }) }
interval(cx, TICK, move || tick.update(|t| *t += 1));
interval(cx, Duration::from_secs(1), move || clock.set(clock_text()));
```

Behavior deltas, both improvements: cancellation exists (scope disposal
already covers the dashboard), and a suspended terminal no longer
replays missed ticks. Also 0030 asks for a header pointer in the
dashboard toward the real live-data pattern (`examples/feed.rs` /
`docs/live-data.md`).

## CHANGELOG.md

Proposed entry (I do not own the file; wording ready to paste; updated
cycle 2 for the fold firewall + the Feed example switch):

```md
- reactive: async source→signal bindings (`channel_source`,
  `latest_source`), bounded coalescing ingestion (`bounded_source` with
  `DropOldest`/`DropNewest`/`Coalesce`, an honest stats signal incl.
  drop + fold-panic counters — a panicking coalesce fold degrades
  labeled instead of poisoning the lane), cancellable `interval` timer,
  and waker dedup (one wake per burst). New example `examples/feed.rs`
  (renders through `widgets::Feed`) + guide `docs/live-data.md`.
```

## Backlog state (updated cycle 2)

CLOSED BY LIVEDATA: 0010/0020/0030/0070 carry `## Completion report`
sections and were MOVED to `docs/backlog/completed/live-data/`
(2026-07-21); the two track READMEs were updated to match. Remaining
integrator surface is exactly the file list above (prelude, SUMMARY,
examples/README, dashboard migration + header pointer, api/architecture
cross-links, CHANGELOG) — all one-liners except the dashboard
migration, which is written out verbatim in this note.
