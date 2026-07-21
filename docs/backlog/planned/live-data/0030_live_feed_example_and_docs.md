# Planned: Live-feed example + docs page (make the background-feed pattern visible)

## Metadata
- Created: 2026-07-21
- Status: Planned
- Completed: N/A

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None.

## Context
The robustness review's finding R4 is blunt: `WakeHandle`, `spawn_worker` and the post-drain
contract — "the single load-bearing API for any real application with I/O" — appear in **zero
examples and zero docs/*.md pages** (reviews/cycle11/robustness-and-chat-port.md §R4, ranked P1;
independently filed as P2-9 in reviews/cycle11/completeness-and-code-port.md). Today you find
the pattern only by reading `src/reactive/scheduler.rs`. The completeness review adds that an
in-repo example also *pins the pattern against regressions*, not just teaches it.

## Current code reality
- `examples/` holds 11 product examples + `capture.rs` + `examples/dashboard/`; a grep for
  `wake_handle|WakeHandle|spawn_worker` across `examples/` returns nothing. The dashboard fakes
  its live feed with `reactive::after` one-shot timers (examples/dashboard/main.rs:100-133) —
  the exact place a reader would look for the real pattern and learn the wrong one.
- `docs/*.md` (api, architecture, getting-started, faq…): architecture.md carries one conceptual
  sentence; no page shows the worker→post→signal shape. `docs/SUMMARY.md` is the mdBook index a
  new page must join.
- The pattern itself is proven and cheap to demonstrate: `spawn_worker`
  (src/reactive/scheduler.rs:163), `WakeHandle::post` (scheduler.rs:74), phase-U drain
  (src/app/driver.rs:224), one-frame-per-burst (tests/adv_app.rs:96), zero-idle-cost while the
  worker sleeps (tests/adv_app.rs:55; App::drive_loop blocks with no deadline when nothing is
  pending, src/app/mod.rs:369-380).
- Headless harness for pinning the example's behavior exists: `testing::CaptureTerm` +
  `Driver::turn` (the canonical harness documented on `App`, src/app/mod.rs:85-118).

## Problem
The one API every networked, long-lived app starts from is undiscoverable. The consequence is
not hypothetical: the standing external critique ("no async story") reads absence of docs as
absence of capability, and the only in-repo "live data" example teaches timer-faked feeds. An
engine claiming zero-idle-cost live UIs must show one.

## What we want to do
1. `examples/feed.rs`: a minimal app that spawns a worker (via `spawn_worker` or the 0010
   helper once it lands) posting synthetic events on a bursty cadence (bursts + quiet gaps, so
   both coalescing and idle are visible) into a scrolling view (List + Scroll, stick-to-bottom
   unless the user scrolled up — the hand-rolled follow-tail idiom, noted honestly as such),
   with a status line showing events/sec and dropped count once 0020 lands. Quit teardown
   included (worker exits cleanly on quit).
2. A docs page (`docs/live-data.md`, wired into docs/SUMMARY.md and cross-linked from
   architecture.md and api.md): the ownership rule (writes on the UI thread only, posted
   closures are the crossing), the ordering and one-frame-per-burst guarantees, producer-side
   batching guidance (0020), worker-death surfacing (`spawn_worker` → app error), and the
   feed example walked through.
3. A headless integration test that drives the example's core (worker + post + scroll view)
   through CaptureTerm and asserts: tear-free frames, one frame per burst, zero bytes across
   idle turns while the worker sleeps.

## Scope / Non-goals
Scope: one example, one docs page, one pinning test, SUMMARY/API cross-links. Non-goals: real
network I/O (synthetic generator only — transports are 0050's decision); the reconnect state
machine (0040); a packaged Transcript/Feed widget (app-widgets track, band 0100+ — this example
uses today's List/Scroll and says so); rewriting the dashboard example (a pointer from its
header comment to the real pattern is enough).

## Expected outcomes
A newcomer searching "background thread", "async", or "live data" finds a runnable example and a
docs page teaching the sanctioned pattern in under a minute; the pattern is regression-pinned by
a test that exercises the example's shape, not just the primitives.

## Validation
- `cargo build --examples` includes feed.rs (CI already builds examples).
- New integration test green: burst → one frame; idle turns emit zero bytes with the worker
  quiet; quit tears the worker down without a worker-failure error.
- docs page present in docs/SUMMARY.md; example listed in examples/README.md.
- Doc snippets compile (no `ignore` fences for the core pattern).

## Progress checklist
- [ ] `examples/feed.rs`
- [ ] `docs/live-data.md` + SUMMARY/api/architecture cross-links
- [ ] Headless pinning test (burst/idle/teardown)
- [ ] examples/README.md entry; dashboard header pointer
