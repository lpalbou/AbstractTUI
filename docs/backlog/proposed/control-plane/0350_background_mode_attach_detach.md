# 0350 — Background mode + attach/detach: headless sessions a terminal can connect to

## Metadata
- Created: 2026-07-22
- Status: Proposed (feasibility-graded design item; the buildable proof
  is 0360)
- Track: control-plane
- Depends on: 0300 (Detached/Attached events), 0320 (socket + security
  posture); composes with 0340 (survive even server crashes)
- Completed: N/A

## ADR status
- Governing ADRs: none yet. The attach wire (frame stream framing +
  re-negotiation rules) joins 0320's protocol ADR when this graduates
  from design to build.

## Context
The maintainer's "minimize to background / maximize to reattach": run
the app WITHOUT a terminal, keep it alive and stateful, then connect a
terminal and see live rendering — detach again and it keeps running.
tmux-shaped, but for exactly ONE AbstractTUI app per session, which
changes the problem completely: tmux must own ptys because it hosts
arbitrary child processes; an AbstractTUI session needs **no pty
anywhere** — the app renders into an in-process virtual terminal, and
the only real tty in the system is the one the attach client sits on.
App classes served: long-running dashboards/monitors (check in, walk
away), chat clients (presence continuity), agent consoles (band 0200 —
kick off a long job, reattach later; the app-kit admin patterns in band
0500 inherit the same need).

## Current code reality
The engine is closer to this than it looks; the load-bearing seams all
exist:
- **The loop already runs against any terminal object**: `App::run_on`
  (`src/app/mod.rs:348-352`) drives the real blocking loop over `dyn
  Terminal`; only `App::run` demands a tty (mod.rs:306-313 — and its
  error message names the alternative). `testing::CaptureTerm`
  (`src/testing/capture.rs:48-63`) proves the whole pipeline against an
  in-memory terminal; what it lacks for production is only a BLOCKING
  read (its reads are scripted, capture.rs:180-216) — a `VirtualTerm`
  with a condvar-blocked `read` plus a waker minted from any closure
  (`TerminalWaker::new`, `src/term/waker.rs:46-63`) is a small, honest
  build.
- **Reattach repaint is one existing mechanism away**: on resize the
  driver already poisons `prev` so "the diff re-emits every cell of the
  next frame instead of trusting a model of a screen that no longer
  exists" (`Driver::apply_resize` + `poison_prev`,
  `src/app/driver.rs:508-533,560-565`). An attaching terminal IS a
  screen whose content we must not trust — attach = deliver the
  client's size as `TermRead::Resize` and the engine full-repaints by
  its own resize law. Same-size attach needs the poison exposed
  deliberately (today it is private).
- **Capability change mid-session exists**: `apply_caps_upgrade`
  (driver.rs:538-554) recomputes presenter caps, poisons, damages all
  layers, marks every image dirty, requests a frame — written for the
  startup probe's conservative→probed direction, and it is the right
  lever for first-attach-as-upgrade. HONEST LIMIT (extensions review
  P1-1): the image re-dirty (`img.dirty = true`, driver.rs:517-521 and
  547-550) re-RENDERS placements — it does not reset `ImageSession`'s
  belief about what the terminal already holds
  (`src/gfx/session.rs:122-139` dedupes against that belief); a
  terminal-identity swap needs a session reset — see the hard parts.
- **The frame stream is already bytes**: the presenter emits the full
  terminal dialect the VT model can interpret (`src/testing/vt.rs:1-27`
  scopes it exactly); `examples/capture.rs:279-322` already captures a
  live app's bytes and reconstructs its screen — a detached session's
  virtual screen + byte log is proven machinery.
- **Input backhaul is the engine's own parser**: client tty bytes →
  socket → `TermRead::Input` → `EventReader` (driver.rs:271-272) — the
  hostile-input armor and kitty/legacy decoding all sit engine-side
  already; the attach client never parses anything.
- **Session enter/leave**: `EnterOptions::enter_bytes()/leave_bytes()`
  flow through the ordinary write path (capture.rs:157-174) — the
  attach client must apply an equivalent posture to the REAL tty and
  restore on detach (its own panic hook duty: `install_panic_hook`,
  mod.rs:446-456, is per-process and belongs to the client too).
- **Suspend is prior art for "the screen goes away and comes back"**:
  `Terminal::suspend`'s caller obligations (`src/term/mod.rs:176-190`)
  — blank altscreen, size unknown, verbs reset — are precisely the
  attach-time obligations, already written down.

## Problem
No headless entry exists (an app author must hand-build a Terminal impl
and a loop today); no session broker (naming, discovery, single-writer
enforcement, stale cleanup); no frame/input channel; no client. The
"minimize to background" story is currently: quit and lose everything —
or keep a terminal window hostage.

## What we want
A three-part design (build order = 0360 proof first, then harden):
1. **`VirtualTerm` + headless serve entry** (engine): an in-memory
   `Terminal` with a real blocking read (condvar + waker), a VtScreen-
   style model of the current screen, and a subscriber tap on the byte
   stream. A serve-mode constructor that runs `App::run_on` against it
   with configured caps — env detection is wrong headless
   (`Capabilities::detect_env` reads the SERVER's env; the future
   client's terminal is unknown), so serve caps come from config with a
   conservative default, and the probe is off (RunConfig.probe,
   driver.rs:56,166 — probing a virtual terminal is meaningless).
2. **The attach wire** (rides 0320's socket + security posture): a
   negotiated stream mode beside the JSONL control lane — hello carries
   the client's size + detected caps; server applies
   Resize(+caps change) through the existing levers above; full frame
   re-presents; then raw presenter bytes stream one way, raw input
   bytes + resize notices stream back. Framing needs-design
   (length-prefixed binary beside JSONL vs a second socket — decide in
   0360 with measurements).
3. **The attach client** (small standalone binary): put the real tty in
   the app's session posture, pump bytes both ways, restore on
   detach/exit/panic. Detach chord handled CLIENT-side (a reserved
   chord the client swallows, tmux-prefix-style — the server never
   sees it).
4. **Session semantics**: one live client at a time in v1 (second
   attach refuses politely or evicts per config — pick one, document
   it); detach keeps the app running with `Detached` emitted (0300) so
   apps can pause animations voluntarily (idle cost while detached is
   otherwise unchanged: idle apps already cost zero; animations keep
   billing — honest, app's choice); session registry = socket path
   convention + a liveness lock file so stale sockets from crashes are
   detected and reclaimed, never silently double-served.

## The hard parts, named honestly
- **Caps mismatch between virtual and real terminal**: serve renders
  for configured caps; the attaching terminal may be poorer (16-color,
  no kitty) or richer. `apply_caps_upgrade` handles the presenter
  switch, BUT enter-time postures (kitty keyboard flags pushed at
  enter, `src/app/driver.rs:138-145`) are session-scoped, and
  cell-pixel-size-dependent image placements
  (`Capabilities::cell_pixel_size`) can differ per client — first
  attach fixing session caps is the honest v1; live per-attach
  re-negotiation is the needs-design follow-up.
  **Rule adopted (extensions review P1-1)**: serve-mode default caps
  are CONSERVATIVE (256-color, no graphics) so the first attach is an
  UPGRADE through the proven probe lever (`apply_caps_upgrade` was
  written for exactly the conservative→probed direction,
  driver.rs:538-554); a poorer-than-configured attach
  (refuse vs downgrade) joins the needs-design list below.
- **ImageSession identity across attach (needs-design; extensions
  review P1-1, verified)**: `ImageSession` models uploads living in
  TERMINAL memory — `sync` dedupes against what it believes the
  terminal holds ("same version + same rect = nothing to do; same
  version + new rect = kitty re-places WITHOUT retransmit",
  `src/gfx/session.rs:122-139`), and `Driver::finish` exists because
  uploads outlive cells (driver.rs:434-450). Attach swaps the terminal
  identity out from under that model: the new terminal holds none of
  the uploads the session believes resident (a re-place by kitty id
  addresses pixels that were never transmitted THERE), and deletes
  owed to the old side went to a virtual screen that held nothing.
  The `img.dirty = true` lever re-RENDERS placements; it does not
  reset session belief. Note the engine already resets slot state on
  CHANNEL change ("caps upgraded mid-session: drop the old state
  honestly, start over", session.rs:138-139) — the gap is a
  same-channel terminal swap. Likely shape: attach = image-session
  reset (treat the new terminal as a fresh session: drop slot
  bookkeeping, damage-all — cheap and honest); to be designed WITH the
  caps re-negotiation story. 0360 dodges this whole class correctly by
  fixing no-graphics caps.
- **Session ownership**: who daemonizes (the app with a detach flag?
  a launcher? attach-with-autospawn?), SIGHUP posture for a process
  whose launching terminal closes, and crash-vs-detach
  distinguishability (the 0340 crash marker composes here).
- **Socket security**: same trust boundary as 0320 (same-uid), but an
  attach stream carries EVERYTHING the user sees — the threat note
  must say so explicitly.
- **Who owns restore of the real tty**: the client, always — including
  on client crash (client-side panic hook + emergency restore
  equivalent) and on server death mid-attach (client must notice EOF
  and restore, not hang raw).

## Scope / Non-goals
Scope (of the eventual build; 0360 proves a slice first): VirtualTerm,
serve entry, attach wire, client binary, session lock/registry,
Detached/Attached events, docs + threat note.
Non-goals: multiple simultaneous viewers (research: size/caps
arbitration is the tmux mirror problem — not v1); scrollback
reconstruction (altscreen apps have none; the virtual screen IS the
state); hosting non-AbstractTUI processes (never — that is tmux's
job); windows transport (research until the named-pipe/ConPTY story is
studied); remote (non-localhost) attach (out, per track posture).

## Feasibility
**Graded.**
- **v1-able**: VirtualTerm + blocking read + waker; headless serve with
  conservative-default caps (256-color, no graphics — first attach
  upgrades through the probe lever); single-client attach with
  resize-driven full repaint; byte backhaul; detach/reattach cycle;
  lock-file liveness. Every engine-side lever already exists (run_on,
  poison_prev, apply_resize, apply_caps_upgrade) — the new code is the
  terminal object, the broker, and the client.
- **needs-design**: attach-stream framing; caps re-negotiation beyond
  first-attach-wins (incl. poorer-attach refuse-vs-downgrade);
  **ImageSession identity across attach** (session reset on terminal
  swap — see the hard-parts entry; graphics-enabled serve is blocked
  on this design, which is WHY conservative caps are the v1 default);
  enter-option ownership between server session and client tty;
  autospawn/daemonization ergonomics; eviction vs refusal.
- **research**: multi-viewer mirroring; windows; attach over anything
  but a local socket.
Idle honesty: a detached idle app = the same zero (the loop blocks in
the virtual read; `tests/adv_app.rs:54` shape extends to VirtualTerm);
a detached ANIMATING app keeps paying for frames nobody sees unless it
listens to `Detached` — documented, app-owned choice.

## Expected outcomes
"Close the terminal, keep the app" becomes a supported pattern with a
documented security posture; the console port (0200) gains
kick-off-and-reattach; combined with 0340, even a server crash resumes
to the last snapshot — the full resilience story this track exists for.

## Validation
(For the build; 0360 carries the first executable slice.)
- VirtualTerm unit: blocking read + waker semantics, byte-tap ordering,
  screen model equivalence against CaptureTerm on identical byte
  streams.
- End-to-end under the pty harness (`src/testing/pty.rs:58-185`): spawn
  the CLIENT in a real pty, attach to a headless session, assert the
  mirrored screen text, type through the pty, detach, reattach,
  re-assert — the whole loop CI-runnable on unix dev boxes.
- Kill tests: client SIGKILL mid-attach → server survives, session
  reattachable; server SIGKILL mid-attach → client restores the tty
  (pty harness asserts cooked mode back, `TtyState`, pty.rs:266-295).
- Caps mismatch: attach with poorer PresentCaps → downlevel emission
  verified by the VT model with zero unknown sequences (vt.rs:12-14).

## Progress checklist
- [ ] Design review of this item (maintainer security + ownership calls)
- [ ] VirtualTerm (blocking read, waker, tap, screen model)
- [ ] Headless serve entry + configured caps
- [ ] Attach wire framing decision (with 0360 measurements)
- [ ] Client binary (posture, pumps, restore-on-everything)
- [ ] Session lock/registry + stale reclaim
- [ ] Detached/Attached events + docs + threat note
