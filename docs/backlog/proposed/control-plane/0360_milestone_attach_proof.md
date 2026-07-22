# 0360 — Milestone: attach/detach proof (headless serve + attach client, one session)

## Metadata
- Created: 2026-07-22
- Status: Proposed (milestone — validation vehicle for 0350, the
  watcher-0060 pattern: a small real program proving the lane before
  the design freezes)
- Track: control-plane
- Depends on: 0350 (design), 0320 (socket seam); 0300 events optional
  for the proof
- Completed: N/A

## ADR status
- Governing ADRs: none yet; this milestone deliberately PRECEDES the
  attach-wire ADR — its experience report is that ADR's evidence (the
  same evidence-before-decision rule the live-data track pins for its
  transport ADR: `docs/backlog/overview.md:99-100`).

## Context
0350 grades the attach/detach design; this milestone is the ~2-4 day
executable slice that proves the graded-v1able core against reality
before any protocol freezes: ONE headless app, ONE attach client, unix
only, fixed caps, no images. The roadmap's validation-vehicle rule
applies verbatim (`planned/0001_roadmap.md` principle 2: apps validate,
they never design — gaps found here are filed back into 0350/0320, not
patched around).

## Current code reality
Everything the proof composes was verified for 0350; the shortest
path through it:
- `App::run_on(term, cfg)` over a to-be-written `VirtualTerm`
  (`src/app/mod.rs:348-352`; blocking-read gap named at
  `src/testing/capture.rs:180-216`; waker from a closure,
  `src/term/waker.rs:46-63`).
- Reattach repaint via the resize law (`Driver::apply_resize` +
  `poison_prev`, `src/app/driver.rs:508-533,560-565`) — the proof
  drives it by always sending the client size on attach.
- The client is a byte pump against a raw tty; the pty test harness can
  spawn and script IT (`src/testing/pty.rs:58-185` — controlling-tty
  child, keystroke injection, `TtyState` cooked-mode assertions,
  pty.rs:266-295).
- A served app to mirror: `examples/dashboard` (already
  capture-scripted at fixed sizes, `examples/capture.rs:66-83`) or the
  simpler `examples/hello` for the first loop.
- Socket: `std::os::unix::net`, 0600 under a 0700 dir (0320's
  posture).

## Problem
Until a real terminal attaches to a real headless session, 0350's
grading is paper: the framing choice (JSONL-adjacent vs length-prefixed
binary), the enter-posture ownership, and the restore-on-server-death
behavior all have plausible designs that only a running proof can rank.

## What we want
1. `VirtualTerm` (minimal): blocking read (condvar), waker, byte tap,
   size state. Lives where the design review says (testing-adjacent or
   a new module — the proof may start under `examples/` support code
   and migrate).
2. A `serve` example: `cargo run --example serve_dashboard -- <socket>`
   — runs the dashboard headless at fixed caps (256-color, no
   graphics), no probe, listening on the socket.
3. An `attach` example/binary: raw-mode the real tty, hello with size,
   pump presenter bytes ↔ input bytes, detach on a reserved chord
   (client-swallowed), restore the tty on every exit path.
4. The proof script (documented, repeatable): serve → attach → observe
   live dashboard → type a theme-switch key → detach → reattach →
   state persisted across the gap → kill server mid-attach → client
   restores cooked mode.
5. **The experience report** (the actual deliverable): what the framing
   measured (bytes/latency of full-repaint on attach at 120×35), which
   design questions from 0350 got answered, which engine gaps were
   filed. Lands beside this file as a completion report.

## Scope / Non-goals
Scope: the two example programs, the minimal VirtualTerm, the proof
script, the report.
Non-goals: everything 0350 defers (multi-client, caps re-negotiation,
autospawn/daemonize, windows) plus: images (fixed no-graphics caps),
production session registry (a fixed socket path + refuse-second-client
is enough to prove the lane), packaging/naming of the client (ruling
comes with 0350's build).

## Feasibility
**v1-able as scoped.** Zero new dependencies, no engine surface
changes required beyond what the proof itself justifies (the poison/
repaint lever may need a deliberate public hook if same-size attach
turns out common — that finding is exactly what the proof exists to
produce). The one genuine unknown worth measuring, not debating:
whether full-frame-on-attach at realistic sizes is comfortably under
perceptual latency on a unix socket (expectation: yes by orders of
magnitude — the perf ledger has full-change 200×60 diff+present at
~435 µs on the reference box, `reviews/cycle11/completeness-and-code-
port.md` §0 — but the report should print the number, not assume it).

## Expected outcomes
0350's needs-design questions get evidence; the attach-wire ADR gets
its data; the maintainer gets a runnable "minimize to background /
maximize to reattach" demo on a real app.

## Validation
- The proof script executed on macOS (dev box) and Linux (pty CI job
  when 0180 lands it), driven by the pty harness end-to-end: mirrored
  screen text asserted, detach/reattach cycle asserted, cooked-mode
  restore asserted after server kill.
- Byte-stream cleanliness: the client-received stream replayed through
  `VtScreen` with `unknown_seq_count() == 0`
  (`src/testing/vt.rs:12-14`).
- Idle: served-but-detached idle dashboard emits zero bytes between
  animation frames; with its clock paused, zero wakeups.

## Progress checklist
- [ ] Minimal VirtualTerm (blocking read + waker + tap)
- [ ] serve_dashboard example (fixed caps, socket)
- [ ] attach client (posture, pumps, reserved detach chord, restore)
- [ ] Proof script green on macOS + Linux
- [ ] Kill-safety assertions (both directions)
- [ ] Experience report filed; findings folded into 0350/0320
