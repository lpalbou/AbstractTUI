# Proposed: Milestone — read-only multi-room watcher over a live hub (dogfood)

## Metadata
- Created: 2026-07-21
- Status: Proposed
- Completed: N/A

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None directly —
  but this milestone's experience report is the designated evidence input for the 0050 transport
  ADR.

## Context
The critique this track exists to answer, verbatim: AbstractTUI "ships no async/HTTP/WebSocket
story and only a single-line text input"; "nobody has ever built a networked, long-lived app on
it"; the one real unknown is "network-driven reactivity + reconnect under abstracttui's frame
loop." The reviewer's proposed de-risking move: a **~2-day read-only multi-room watcher** —
connect to a hub over WebSocket, render several channels plus a presence sidebar in live panes,
quit/switch-focus as the only interactions. This item records that milestone. It is explicitly
**not work to do now**: it is the first networked app, the existence proof, and the validation
gate for this track — nothing in the shipped engine couples to it.

## Current code reality
Engine side — what the watcher composes from (verified in cycle-11 reviews against source):
- Live ingress: `spawn_worker` + `WakeHandle::post` + phase-U drain, tear-free by the damage
  contract's epoch rule, one frame per burst, zero-idle-cost while quiet
  (src/reactive/scheduler.rs; docs/design/01-damage-contract.md §2; tests/adv_app.rs:55,96,299).
- Sidebar + rooms: `List` (virtualized, sticky selection) + `Badge` for unread counts, `Tabs` or
  focused panes for rooms, `Toast` for other-room notices, `use_startup_notices` for degraded
  starts (src/widgets/, src/app/). A read-only, watcher-scale message pane is buildable today by
  windowing the transcript by hand (the robustness review's "port CAN ship today by hand"
  assessment); the packaged Transcript/Feed widget is the app-widgets track's problem
  (see the app-widgets Feed item, band ~0100) and is **not** required at this scope.
- Hostile-content safety needed for rendering strangers' messages is structural: control
  clusters stripped at the draw boundary (src/render/surface.rs:358), hostile-byte-hardened
  input parser, write-only OSC 52 (reviews/cycle11/robustness-and-chat-port.md §R2).

Target side — the hub is real and running today (~/projects/a2a/src/agora/): channels + `dm:`
channels, append-only `Envelope` messages (models.py: status/urgency/to/reply_to, title ≤120
chars, markdown body), member/presence listing, votes; transport = WebSocket push with
exponential-backoff reconnect + REST catch-up + `/inbox` long-poll ≤55 s fallback
(client/client.py:339-443,:47); bounded client inbox with drop + cursor recovery
(client/inbox.py:24,34); CLI entry `agora = agora.cli:main` (pyproject.toml:62).

Gap side — what the watcher exercises that nothing has: 0010 (binding), 0020 (bounded ingestion
under real hub bursts), 0040 (reconnect under the frame loop — the named unknown), 0050
(transport evidence). No example, test, or consumer currently runs the engine against a network
for hours.

## Problem or opportunity
"Nobody has ever built a networked, long-lived app on it" is true and can only be answered by
building one. The watcher is the smallest honest instance: real hub, real WebSocket, real
reconnects, real hostile-ish content, hours-long uptime — while deliberately dodging the two
known heavy gaps (rich transcript widget, multiline composer) that belong to the app-widgets
track. Its scope is small precisely so that failure is diagnostic: anything hard in it is
engine-track work, not app ambition.

## Proposed direction
A standalone binary (own repo or workspace member — **zero coupling to the shipped engine**,
which it consumes as a normal crates.io dependency):
- Connect to an agora hub over WebSocket (client crate chosen app-side per 0050 Option-A
  posture; REST catch-up on reconnect).
- Render 2-4 subscribed channels in live panes + a presence/members sidebar with unread badges.
- Read-only: **no composing, no acking, no input beyond quit and focus/room switching.**
- Survive hub restarts (0040 model + backoff), label dropped/coalesced floods (0020 signal),
  idle at zero cost when the hub is quiet.
- Budget: ~2 days, per the reviewer's sizing — treat significant overrun as a finding about the
  track, not a schedule slip to push through silently.

Completion = the live-data lane is validated: each of 0010/0020/0040 demonstrably carried real
traffic, and the 0050 ADR has its evidence (an experience report is part of this milestone's
definition of done).

## Why it might matter
It converts the track from claims to proof, in the exact shape the critique demands — and it
produces the requirements evidence for the app-widgets Feed/transcript work (band ~0100) that a
full chat client (composing, obligations, votes) would additionally need.

## Promotion criteria
Promote to planned/ when 0010 and 0020 have landed (0030's example proves the pattern headlessly
first), a hub instance is available to run against, and ~2 days are actually allocated. Do not
start it before 0010/0020: hand-rolling their gaps inside the watcher would un-validate the
track.

## Validation ideas
- Live session: several rooms updating concurrently, tear-free, correct per-channel ordering
  (hub `seq` respected after catch-up).
- Kill/restart the hub mid-session: state transitions render (0040), catch-up fills the gap, no
  duplicate or lost rendering.
- Flood a channel: bounded memory, labeled drop count (0020), UI stays responsive.
- Multi-hour soak: no queue growth, zero bytes/allocs while idle (the engine's existing idle
  pins, observed in a real networked posture for the first time).
- Experience report written (input to 0050's ADR; follow-ups filed against this track and
  app-widgets).

## Non-goals
No composing/posting, no ack/obligation handling, no votes, no attachments, no DM authoring —
read-only by design (this also keeps it honest as a watcher: acks are triage-seen in the target
domain and a watcher must not emit them). No engine features built "just for the watcher"; gaps
found go through backlog items. Not a replacement for the a2a CLI.

## Guidance for future agents
Read reviews/cycle11/robustness-and-chat-port.md Part 2 before building — it maps the whole
domain onto engine surfaces. Keep the watcher's transcript rendering deliberately primitive
(windowed, last-N); resist upstreaming it — the real widget is specified in the app-widgets
track with different requirements (streaming re-typeset, keyed reconciliation).


## Status update (2026-07-23)

MAINTAINER GREEN-LIT (validator decision): a read-only a2a/agora
multi-channel watcher is one of the two chosen second-validator apps.
Promoted proposed -> planned. Scope stands as written (read-only,
multi-room, live panes + presence sidebar, quit/switch-focus only);
it validates live-data 0010/0020/0040 in the field and produces the
evidence 0050's transport ADR waits on.
