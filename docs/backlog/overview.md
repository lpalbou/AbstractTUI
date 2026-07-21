# AbstractTUI backlog — overview

Planning memory for AbstractTUI (the Rust terminal-UI engine, published as
`abstracttui` 0.1.0). The engine itself is complete and shipped; this backlog
tracks the work that turns it from "a proven engine" into "a foundation people
build long-lived, networked applications on." It is organized around one
honest observation: nobody has yet built a networked, long-lived app on
AbstractTUI, it ships no async/HTTP/WebSocket story, and its text input is
single-line. The two evaluations in `reviews/cycle11/` are the evidence base;
every item cites concrete engine code.

## Design principles

General-needs-first (every capability justified by an app class, never one
app), apps-as-validators, standalone dependency posture, honest degradation,
zero idle cost — codified with the milestone bands and validation vehicles in
`planned/0001_roadmap.md`, the canonical roadmap.

## Counts

| State | Count |
| --- | --- |
| Planned | 10 |
| Proposed | 13 |
| Completed | 0 |
| Deprecated | 0 |
| Recurrent | 0 |

## Topic tracks

| Track | Dir | State | Purpose |
| --- | --- | --- | --- |
| live-data | `planned/live-data/`, `proposed/live-data/` | Mixed | Network-driven reactivity: async-source→signal binding, bounded ingestion, reconnect, the transport decision, and the read-only watcher milestone. |
| app-widgets | `planned/app-widgets/`, `proposed/app-widgets/` | Mixed | The content-widget layer real apps need (feed/transcript, streaming markdown, multiline composer, follow-tail scroll, lexers) + the API-stability and platform-accuracy passes. |
| ports | `proposed/ports/` | Proposed | The two application epics that consume both tracks: a coding-agent console and an a2a chat TUI. |
| first-app | `proposed/first-app/` | Proposed | Bug/footgun reports from the first shipped application (`abstractcode-tui`, 2026-07-21): reproduced engine defects with field workarounds to delete. |

## Planned ledger

| ID | Title | Track |
| --- | --- | --- |
| 0001 | Roadmap: general capability classes, milestone bands, validation vehicles (canonical) | roadmap |
| 0010 | Async data-source → Signal binding (helper + UI-thread ownership rule) | live-data |
| 0020 | Bounded, coalescing event ingestion (back-pressure + waker dedupe) | live-data |
| 0030 | Live-feed example (`examples/feed.rs`) + docs page | live-data |
| 0100 | Feed / Transcript widget (virtualized, append-only, keyed rich blocks) | app-widgets |
| 0110 | `md::StreamSession` — append tokens, re-parse only the open block | app-widgets |
| 0120 | TextArea — multiline composer, history, block paste, completion dropdown | app-widgets |
| 0130 | Scroll follow-tail idiom + optional `content_size` via layout query | app-widgets |
| 0150 | Terminal verbs (notify/bell/title/clipboard) reachable from components | app-widgets |
| 0180 | Platform-claim accuracy (Linux pty CI) + perf/fuzz/soak gates + MSRV | app-widgets |

## Proposed ledger

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0040 | Connection lifecycle model + jittered reconnect/backoff | live-data | Starting the watcher (0060) or either port. |
| 0050 | Transport story: HTTP/WebSocket/TLS dependency decision (first ADR) | live-data | Decide only after the watcher's evidence (0060); do not settle from the armchair. |
| 0060 | Milestone: read-only multi-room watcher over the a2a hub (dogfood) | live-data | Maintainer green-light; validates 0010/0020/0040. Explicitly not-now. |
| 0070 | Recurring time source: `interval` beside `reactive::after` | live-data | With the live-data foundation, or the first consumer hand-rolling re-arming. |
| 0140 | Stateful cross-line lexers (python/js/toml) + diff lexer | app-widgets | The coding-console port (0200) reaching syntax/tool-result previews. |
| 0160 | Content selection + copy (take text out of any view) | app-widgets | Design ruling (per-widget vs screen-layer selection); a dogfood app reaching its copy phase. |
| 0165 | Hyperlink/reference hit-testing through the event path | app-widgets | A dogfood app reaching its "activate a reference" phase. |
| 0170 | 1.0-track API stability pass (non_exhaustive, two-`Style`, prelude, first ADRs) | app-widgets | Before 0100/0130 public shapes merge, so the churn ships as one budgeted 0.2. |
| 0200 | EPIC: coding-agent console over `abstractcode serve` JSONL | ports | Its widget + live-data dependencies land. |
| 0210 | EPIC: a2a chat TUI over the agora hub | ports | Its widget + live-data dependencies land. |
| 0220 | BUG: `autofocus` inside a dyn_view regeneration panics the runtime | first-app | Deterministic repro; next engine work cycle. |
| 0230 | BUG: Modal content shortcuts dead until focus enters the modal tree | first-app | Deterministic repro; pairs with 0220; next engine work cycle. |
| 0240 | Footgun: overflowing modal content shrinks fixed rows to zero | first-app | Next engine cycle, or fold into 0130's Scroll layout contract. |
| 0250 | Footgun: `List::on_select` fires on arrow movement (no activation event) | first-app | Fold into 0170's API pass, or before 0100 ships. |
| 0260 | Disclosure widget: per-item fold/unfold for transcripts (maintainer ask) | first-app | Fold into 0100's item model, or standalone on a second consumer. |

## Next recommended work

1. **0100 (Feed / Transcript)** — both evaluations ranked it the #1 gap; the
   agent transcript and the chat message list are the same widget, and every
   ingredient already exists in-repo. Sequence **0170's** API rulings before
   its public surface merges so the `Scroll`/`List` changes ship as one
   budgeted 0.2, not a trickle of breaks.
2. **0010 → 0020 → 0030 (live-data foundation)** — the one-directional chain
   that makes any network-fed app possible; 0030 is the first example/doc of
   the load-bearing background-feed pattern, which today appears nowhere.
3. **0180 (honesty pass)** — small and independent: correct the Linux
   platform claim and gate the currently-manual perf/fuzz/soak suites.

## Sequencing (load-bearing)

- **live-data is one-directional**: 0010 before 0020/0030; 0010+0020 before
  the watcher (0060) — hand-rolling their gaps inside the watcher would
  un-validate the track. **0060 before closing 0050**: the transport ADR
  waits on the watcher's experience report as its evidence.
- **0100 is the widget trunk**: 0110 feeds its streaming tail, 0130 is how it
  composes with `Scroll` (design together), 0140 tints its blocks. 0170 gates
  the public shapes of 0100/0130.
- **Ports depend on both tracks**: 0200 (console) ← 0100/0110/0120/0130/0140/0150
  + live-data 0010/0020/0030 (subprocess pipe, no network — not 0040/0050).
  0210 (chat) ← 0100/0120/0130/0150 + live-data 0010/0020/0030/0040/0050; its
  read-only phase 1 IS the 0060 milestone (adopt, don't restart).
- The read-only watcher (0060) needs **nothing** from app-widgets (its scope
  is a hand-windowed read-only view); a full chat client is the first thing
  requiring both tracks.

## ADR state

AbstractTUI has no ADR system yet. Two items require the repository's first
ADRs before they can close: **0050** (the transport/dependency decision — it
changes the standalone dependency posture) and **0170** (API-stability policy
toward 1.0). When either is scheduled, stand up `docs/adr/` first and record
the decision there; until then this backlog carries the explicit "needs ADR"
state on those items.

## Process

- New item: scan every lifecycle dir + topic folder for the next unused global
  `NNNN`, add it under the right state, and update this overview's counts,
  ledgers, and sequencing in the same pass.
- Completion: append a `## Completion report` (final path, date, outcome, key
  validation), move to `completed/`, update the ledgers here.
- Deprecation: append a `## Deprecation report` with the reason, move to
  `deprecated/`, update this overview.
- Bands: live-data owns 0010–0090, app-widgets owns 0100–0190, ports own
  0200–0290 (0200/0210 = port epics; 0220–0240 = first-app findings).
  Leave gaps for insertion.
