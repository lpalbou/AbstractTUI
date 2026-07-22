# Live-data / networking — planned items

Committed items of the live-data track (global number band 0010–0090).

All three planned items COMPLETED in the build wave (LIVEDATA seat,
2026-07-21) and moved to `../../completed/live-data/`:

- `0010_async_source_signal_binding.md` — named helper + ownership rule for background-thread →
  Signal ingress (the track's foundation).
- `0020_bounded_coalescing_ingestion.md` — bounded/coalescing ingestion + labeled back-pressure
  signal; waker dedupe.
- `0030_live_feed_example_and_docs.md` — `examples/feed.rs` + docs page for the background-feed
  pattern.

(0070, the interval time source, was promoted from the proposed arc and
completed in the same wave — it lives in `../../completed/live-data/` too.)

The canonical track README — purpose, full item list including the proposed arc
(0040 reconnect, 0050 transport decision, 0060 watcher milestone), reading order, scope, and
non-goals — lives at `../../proposed/live-data/README.md`.
