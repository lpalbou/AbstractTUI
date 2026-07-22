# 0640 — External audio-process lifecycle: a documented pattern, not engine code

## Metadata
- Created: 2026-07-22
- Status: Proposed (docs + example item — VERIFIED no engine delta needed)
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: nothing shipped-side; 0650 demonstrates the pattern live.
- Promotion trigger: ships WITH 0650 or the first real voice app,
  whichever comes first.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none (documentation + example).

## Context
Terminal voice apps do audio through EXTERNAL processes or audio-thread
libraries: the assistant records via sounddevice threads and plays via
`afplay`/`paplay`/`ffplay` child processes
(`gateway_voice_manager.py:_spawn_player`, verified 2026-07-22), pausing
with SIGSTOP/SIGCONT. A Rust TUI does the same shape: spawn
`rec`/`afrecord`/`ffmpeg`, read stdout chunks on a thread, feed the UI.
The question this item answers: does the ENGINE owe machinery for that,
or is it already covered? **Verified: covered — with one pattern gap
worth documenting, not building.**

## Current code reality (the verification)
- Data plumbing: `bounded_source` (src/reactive/ingest.rs:370) is
  exactly the audio-chunk shape — bounded window, overflow policy,
  coalescing stats, fold-panic firewall; `latest_source` carries levels.
  Senders outliving the scope turn INERT (counted dead sends, never UB)
  — the reader thread can keep reading a dying process harmlessly.
- Teardown hook: `Scope::on_cleanup` (src/reactive/scope.rs:117) runs on
  scope disposal — the place a child-process guard's kill belongs.
- Failure surfacing: worker deaths become app errors via
  `take_worker_failures` (src/app/driver.rs:287-290) when the app uses
  engine workers; plain `std::thread` readers should instead send a
  labeled terminal event through their own source (document this).
- THE GAP (pattern, not API): nothing kills the CHILD PROCESS when the
  scope dies. Scope death makes sends inert, but an orphaned `rec`
  holds the microphone open — a privacy-grade leak. The fix is ~12
  lines of app code: a `KillOnDrop(Child)` guard registered via
  `cx.on_cleanup`, reader thread detects EOF and exits. No engine type
  makes this meaningfully shorter, and owning process management would
  drag platform semantics (process groups, signals) into a UI crate.

## Problem
Every voice app will face the orphaned-recorder trap; without a
documented pattern the first ones will leak microphones on panic/quit
paths and file it as an engine bug.

## What we want
1. A **docs section** (docs/live-data.md or a new docs/voice.md):
   spawn → read-thread → `bounded_source` → signal; `KillOnDrop` guard
   on `cx.on_cleanup`; SIGSTOP pause pattern (unix), and the honest
   Windows note (no SIGSTOP — suspend via the media lib, or stop/restart).
2. The 0650 example exercises the guard with a MOCK process (e.g.
   `/bin/cat` fed by a timer) so the pattern is test-covered without
   audio hardware.
3. An explicit statement in docs: the engine does NOT and will not own
   audio I/O — the boundary is `Vec<f32>`/bytes into live-data sources.

## Scope / Non-goals
Scope: docs, the guard pattern, example wiring.
Non-goals: engine process-management API, audio device enumeration,
sample-format conversion (all app-side; say so).

## Expected outcomes
The first real voice app copies a proven pattern instead of discovering
the orphaned-mic trap in production.

## Validation
- Example-level test (0650's suite): scope disposal reaps the mock
  child (waitpid returns), sends after death are counted dead, UI
  survives.

## Progress checklist
- [ ] docs section (pattern + boundary statement)
- [ ] KillOnDrop guard in the 0650 example + reap test
- [ ] Windows honesty note
