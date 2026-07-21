# Port epics backlog track

## Status
Proposed (both epics blocked on dependencies; neither started).
Numbering band: 0200–0290 within the 0100–0290 planning band.

## Purpose
Two real applications, evaluated in depth by the cycle-11 reviews
(`reviews/cycle11/completeness-and-code-port.md` §2,
`reviews/cycle11/robustness-and-chat-port.md` Part 2), serve as the
validation vehicles for AbstractTUI's application story — they prove the
general capabilities hold under real workloads; they do not define them
(see `../../planned/0001_roadmap.md`):

- `0200_epic_coding_console_port.md` — a coding-agent console driven by
  the `abstractcode serve` JSONL backend (transcript with streaming
  markdown, tool-call cards, approval flow, composer, meters, permission
  modes, session tabs, detail panel).
- `0210_epic_a2a_chat_tui_port.md` — a chat/coordination client for the
  agora hub (channel/DM sidebar, live message list, obligations-honest
  inbox, composer, members/votes/owed panels).

Both reviews reached the same verdict from independent angles: the
engine's async-ingestion, damage, and security substrate is ready and
test-pinned; the missing layer is content widgets — and the two
applications need the *same* ones. These epics exist to consume those
widgets, prove their shapes against real workloads, and turn the engine's
"a large app can be built on this" claim into running programs.

## Dependency edges
Both epics depend on:
- The app-widgets track (`../../planned/app-widgets/`,
  `../app-widgets/`): 0100 Feed (both), 0120 TextArea (both),
  0130 follow-tail + size query (both), 0150 terminal verbs (both,
  polish phases), 0110 streaming markdown (0200 only — chat envelopes
  arrive whole), 0140 lexers (0200 mainly — diff tinting; proposed).
- The live-data track (band 0010–0090, separately authored): 0010
  async-source binding + 0020 bounded ingestion + 0030 feed
  example/docs (both epics); 0040 connection lifecycle + 0050
  transport/TLS decision (0210 only — 0200's backend is a local
  subprocess pipe). The live-data 0060 milestone (read-only multi-room
  watcher) is a slice of 0210 phase 1; whichever lands first, the other
  adopts it. `docs/backlog/overview.md` (integrator-owned) is the
  cross-track ledger.

Neither epic modifies the engine directly: gaps found while building are
filed back into the widget or live-data tracks.

## Sequencing recommendation
Land 0100 → 0110 → 0130 → 0120 first (the shared transcript stack), then
start 0200 phase 1 and 0210 phase 1 — both are read-only viewers by
design, the cheapest way to validate the Feed under real event streams
from two very different producers (a subprocess JSONL pipe vs. a network
push feed). Interactive phases follow the composer (0120).

## Promotion criteria (proposed → planned)
An epic promotes when: its Phase-0 dependency list is landed or
scheduled, a kickoff ruling names the crate's home and owner, and the
phase-1 fixture strategy exists. Until then they are direction, not
commitment.

## Non-goals for the track
- Re-implementing backend logic (agent engine, hub server) — both epics
  are pure frontends against existing protocol boundaries.
- The paused `../abstractcoder` charter is prior art for 0200, not a
  started codebase (charter + two source files, no buildable crate; on
  hold by operator decision). Neither epic reports progress it does not
  have.
