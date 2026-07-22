# Live-data / networking backlog track

## Status
Mixed — the foundation (0010/0020/0030, plus 0070 promoted from here) is COMPLETED (build wave
2026-07-21) and lives under `docs/backlog/completed/live-data/`; three speculative items remain
here. Item numbers in this track use the global band **0010–0090** (gaps left for insertions);
the app-widgets track occupies **0100+**.

## Purpose
Make AbstractTUI honestly capable of hosting networked, long-lived applications. The standing
critique this track answers, encoded verbatim in item 0060: the engine "ships no
async/HTTP/WebSocket story and only a single-line text input"; "nobody has ever built a
networked, long-lived app on it"; the one real unknown is "network-driven reactivity + reconnect
under abstracttui's frame loop." The cycle-11 reviews
(reviews/cycle11/completeness-and-code-port.md, reviews/cycle11/robustness-and-chat-port.md)
found the ingress mechanism proven and test-pinned but unnamed, undocumented, unbounded under
flood, and never exercised against a real network. This track closes that gap in order: name the
pattern, bound it, teach it, model reconnect, decide the transport posture, then prove the lane
with a small real app.

## Items
- `../../planned/live-data/0010_async_source_signal_binding.md` — named helper + ownership rule
  for background-thread → Signal ingress. **Planned** (low-risk; the foundation everything else
  builds on).
- `../../planned/live-data/0020_bounded_coalescing_ingestion.md` — bounded/coalescing ingestion
  with a labeled back-pressure signal; waker dedupe. **Planned.**
- `../../planned/live-data/0030_live_feed_example_and_docs.md` — `examples/feed.rs` + docs page;
  the pattern appears in zero examples/docs today. **Planned.**
- `0040_connection_lifecycle_reconnect.md` — connection-state signal + jittered backoff helper;
  frame-loop behavior while disconnected. **Proposed** (API shape needs 0060's evidence).
- `0050_transport_story_decision.md` — HTTP/WebSocket/TLS posture: transport-agnostic engine vs
  optional helper crate. **Proposed** (decision item; needs the repository's first ADR).
- `0060_milestone_multi_room_watcher.md` — milestone: ~2-day read-only multi-room watcher over
  the a2a hub; the validation gate for the lane. **Proposed** (explicitly not-now).
- `0070_interval_time_source.md` — recurring `interval` helper beside `reactive::after` (time as
  the zeroth data source). **Completed** (build wave 2026-07-21, promoted with the foundation;
  moved to `../../completed/live-data/`).

## Reading order
0010 → 0020 → 0030 (the committed foundation, in dependency order), then 0040 → 0050 → 0060
(the speculative arc: reconnect model, transport decision, proof). 0060's "Current code reality"
doubles as the track's map of engine-ready vs missing surfaces.

## Governing ADRs
None identified after review — this repository has **no ADR system yet** (dependency policy
lives in docs/design/00-vision.md as a hard rule amended via reviews/). Item 0050 requires the
first ADR: the transport/dependency decision is durable cross-cutting policy and must not be
settled by backlog prose or a vision-doc edit.

## Scope
Ingress plumbing (binding, bounding, docs/example), connection lifecycle modeling, the transport
posture decision, and the watcher milestone that validates them. Everything here rides the
existing frame loop and damage contract (docs/design/01-damage-contract.md) — no engine-loop
redesign is authorized or needed.

## Non-goals
- No composer/input work and no rich transcript/feed widget — those live in the app-widgets
  track (band 0100+). A full chat-client port depends on **both** tracks (see the app-widgets
  Feed item, ~0100, for the message-list gap both cycle-11 reviews rank P0); the 0060 watcher is
  deliberately scoped read-only so it needs only this track.
- No transport implementation or new dependency before the 0050 decision is recorded as an ADR.
- No fd-level embedding / unified-external-reactor work — a documented engine non-goal
  (reviews/cycle11/robustness-and-chat-port.md §R4).

## Notes for future agents
Sequencing is load-bearing: 0010 before 0020/0030 (they build on the helper), 0010+0020 before
0060 (hand-rolling their gaps inside the watcher would un-validate the track), 0060 before
closing 0050 (the ADR wants the watcher's experience report as evidence). Re-verify the cited
code lines before implementing — src/reactive/scheduler.rs and src/app/driver.rs are small and
the citations here are to v0.1.0.
