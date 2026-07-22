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
| Planned | 3 |
| Proposed | 41 |
| Completed | 12 |
| Deprecated | 0 |
| Recurrent | 0 |

## Topic tracks

| Track | Dir | State | Purpose |
| --- | --- | --- | --- |
| live-data | `planned/live-data/`, `proposed/live-data/` | Mixed | Network-driven reactivity: async-source→signal binding, bounded ingestion, reconnect, the transport decision, and the read-only watcher milestone. |
| app-widgets | `planned/app-widgets/`, `proposed/app-widgets/` | Mixed | The content-widget layer real apps need (feed/transcript, streaming markdown, multiline composer, follow-tail scroll, lexers) + the API-stability and platform-accuracy passes. |
| ports | `proposed/ports/` | Proposed | The two application epics that consume both tracks: a coding-agent console and an a2a chat TUI. |
| first-app | `proposed/first-app/` | Proposed | Bug/footgun reports from the first shipped application (`abstractcode-tui`, 2026-07-21): reproduced engine defects with field workarounds to delete. |
| control-plane | `proposed/control-plane/` | Proposed | Making running apps observable and drivable from outside their own keyboard: lifecycle events, an automation bus + opt-in JSONL control server (MCP-bridgeable), declared-keys persistence with crash-resume, and headless serve with terminal attach/detach. |
| extensions | `proposed/extensions/` | Proposed | Modularity architecture (two feature classes + the `abstracttui-*` sibling family, ADR-ready) and the diagram-class capability lane: core vector canvas + link-registration seam, node-graph widgets, mermaid subset, mdpad-reader enablement, and the standing web-rendering verdict. |
| app-kits | `proposed/app-kits/` | Proposed | The application-kit layer over the content widgets: anchored-popup substrate + choice controls, form kit + wizard, rich data tables, chip/count vocabulary, navigation (sidebar + filter tabs), header/banners, tree view, split panes + panel rail — proven by three in-repo reference validators (admin console, setup wizard, triage shell). |

## Planned ledger

| ID | Title | Track |
| --- | --- | --- |
| 0001 | Roadmap: general capability classes, milestone bands, validation vehicles (canonical) | roadmap |
| 0150 | Terminal verbs (notify/bell/title/clipboard) reachable from components | app-widgets |
| 0180 | Platform-claim accuracy (Linux pty CI) + perf/fuzz/soak gates + MSRV | app-widgets |

## Completed ledger

Each file carries a dated completion report with test names and measured
numbers (2026-07-21: the Content + Live-data wave; 2026-07-22: the
composer wave).

| ID | Title | Final path |
| --- | --- | --- |
| 0010 | Async data-source → Signal binding (`channel_source`/`latest_source`) | completed/live-data/ |
| 0020 | Bounded coalescing ingestion (`bounded_source`, stats, fold-panic firewall) | completed/live-data/ |
| 0030 | Live-feed example + `docs/live-data.md` | completed/live-data/ |
| 0070 | `reactive::interval` (cancellable, coalescing) | completed/live-data/ |
| 0100 | `widgets::Feed` (keyed, windowed, streaming items) | completed/app-widgets/ |
| 0110 | `md::StreamSession` (open-block-only re-parse, equivalence-pinned) | completed/app-widgets/ |
| 0270 | Text selection + clipboard copy (all three tiers: bypass docs, mouse-capture suspend verb, screen-text selection + OSC 52) — completed 2026-07-22 | completed/first-app/ |
| 0120 | `widgets::TextArea` + `app::anchored` completion dropdown (0500's passive slice + `Overlays::top_z`) | completed/app-widgets/ |
| 0130 | `Scroll::follow_tail` + measured content extent | completed/app-widgets/ |
| 0220 | BUG fixed: autofocus in dyn_view regeneration panicked | completed/first-app/ |
| 0230 | BUG fixed: modal shortcuts dead until focus entered the modal | completed/first-app/ |
| 0240 | Footgun fixed: modal overflow crushed fixed rows (defaults + debug notice) | completed/first-app/ |

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
| 0170 | 1.0-track API stability pass — PARTIALLY EXECUTED: ADRs 0001-0003 landed + `Capabilities`/`GraphicsCaps` now `#[non_exhaustive]`; the full 1.0 audit (prelude criteria, public-api gate, breaking budget enforcement) stays open | app-widgets | The remaining audit rides the 0.2 release prep. |
| 0200 | EPIC: coding-agent console over `abstractcode serve` JSONL | ports | Its widget + live-data dependencies land (Feed/stream/follow-tail + TextArea 0120 DONE — widget deps complete). |
| 0210 | EPIC: a2a chat TUI over the agora hub | ports | Its widget + live-data dependencies land (Feed + TextArea 0120 DONE; lifecycle 0040/0050 remain). |
| 0250 | Footgun: `List::on_select` fires on arrow movement (no activation event) | first-app | Fix per the 0250 ruling (selection follows movement; activation = Enter/click-when-selected — recorded in reviews/study/platform-on-appkits.md and encoded by app-kits items). |
| 0260 | Disclosure widget: per-item fold/unfold for transcripts (maintainer ask) | first-app | Fold into Feed's item model (0100 shipped — extend), or standalone on a second consumer. |
| 0142 | Markdown tables (GFM subset) — shares `solve_columns` with the Table widget | app-widgets | mdpad-class reader green-light, or chat messages needing tables. |
| 0144 | Markdown images — in-flow mosaic rendering, lazy decode | app-widgets | mdpad-class reader green-light; protocol-images-in-flow deferred by design. |
| 0146 | Heading anchors + TOC extraction (`md::outline`) | app-widgets | mdpad-class reader; 0165 consumes the anchor ids. |
| 0148 | Search-highlight overlay (shares the text↔cells mapping with 0160) | app-widgets | A reader/console reaching its find-in-content phase. |
| 0300 | App lifecycle events (boot/ready/resize/caps/focus/suspend/resume/quit + custom) — the band foundation | control-plane | Scheduling any of 0310/0340/0350, or the first app needing suspend/flush hooks. |
| 0310 | Automation bus: inject input, query semantic tree + screen text, invoke named actions, subscribe to events | control-plane | 0300 + a driving consumer (port harness, embedder, or 0320). |
| 0320 | JSONL control protocol + opt-in serve seam (default-OFF `control-server` feature; socket perms = auth) | control-plane | 0310 + the JSON-promotion precondition (with extensions 0410); closes only with the protocol ADR. |
| 0330 | MCP bridge — out-of-crate client of the frozen 0320 protocol | control-plane | 0320's ADR freezing + a kickoff ruling on home/language. |
| 0340 | Persist registry: declared keys, atomic phase-boundary snapshots, crash marker, restore-on-start | control-plane | 0300, or app-kits 0520 starting (its accepted first consumer). |
| 0350 | Background serve + attach/detach design (VirtualTerm, conservative serve caps, attach = caps upgrade) | control-plane | Maintainer security/ownership review; builds only after 0360's report folds back. |
| 0360 | Milestone: attach proof — one headless app, one client, fixed caps (~2-4 days, report-first) | control-plane | 0350 review + 0320 socket seam. |
| 0400 | Extension architecture: two feature classes (default-ON trim / default-OFF opt-in) + sibling-crate family; ADR skeleton ready | extensions | Maintainer sign-off; ADR lands before/with the first 04xx packaging execution. |
| 0410 | Feature-gate `three`/`jpeg`/`proto` (default-on trim; gltf_json promotion coordinated with 0320) | extensions | 0400's ADR + integrator Cargo.toml sign-off; batch with the 0.2 window (0170). |
| 0420 | Canvas/vector layer in core: dot canvas, bezier/arc, styled blit; chart refactor gated on byte-identical goldens | extensions | First diagram consumer scheduled (0440/0450) — or standalone on the chart-dedup merit. |
| 0430 | `abstracttui-graph`: interactive node-graph editor (cards/ports/edges/pan/drag/tooltips), staged M1-M3, keyboard-first | extensions | 0420 + 0440 landed; a named dataflow-editor consumer; family launch gate (0170) holds. |
| 0440 | `abstracttui-graph`: read-only auto-layout view — layered v1 (DAG-class), designed force v1.5 (KG-class) | extensions | 0420 + a named DAG-view consumer; v1.5 on the first knowledge-graph consumer. |
| 0450 | `abstracttui-mermaid`: spelling-exact flowchart/sequence subset, atomic per-diagram fallback | extensions | 0420 + 0440 landed; the mdpad rebuild reaching its diagram phase. |
| 0460 | mdpad-class reader enablement: parity dashboard + four core-gap seeds (0142-0148) | extensions | Maintainer green-light on the rebuild; seeds promote individually. |
| 0470 | Web/HTML feasibility — verdict: full web NEVER; readable-subset slice gated on four criteria | extensions | All four criteria met — else the verdict stands. |
| 0480 | Core seam: `StyledCanvas::register_link` (producer half of the link channel; OSC 8 works pre-0165) | extensions | Any canvas-link consumer (0430 M3, 0450) or 0165's scheduling; may merge into 0165. |
| 0500 | Anchored-popup substrate (owned/passive/tooltip modes) + Select/Combobox/MultiSelect family — passive slice + `Overlays::top_z` SHIPPED 2026-07-22 via 0120 (`app::anchored`) | app-kits | First config surface or 0510; owned/tooltip modes + the three faces remain. |
| 0510 | Form kit: field rows, form state signals, validation, submit gating, masked input | app-kits | 0520 or a second settings form; engine deltas: subtree focus step, `TextInput::masked`. |
| 0520 | Wizard flow: multi-step container on the form kit; crash-resume via 0340 (its first consumer) | app-kits | 0510 landing. |
| 0530 | Table upgrades: rich cells, badges, row actions, activation event, row identity | app-kits | Admin-console validator scheduling. |
| 0540 | Chips, counts, and tag-input vocabulary | app-kits | First consumer among 0500/0550/smart-note-class apps. |
| 0550 | Navigation kit: NavList (sidebar + unread badges) + FilterTabs | app-kits | Validators or 0210's room list. |
| 0560 | Header bar + persistent banners (existing tokens only; banner-ground = theme-lane follow-up) | app-kits | Admin-console validator. |
| 0570 | Tree view (outline/file-tree; Role variants ride the 0.2 batch) | app-kits | Triage-shell outline or a file-manager consumer. |
| 0580 | Split panes + collapsible panel rail | app-kits | Triage-shell validator. |
| 0590 | Reference validators: admin console, setup wizard, triage shell (in-repo; no item completes unvalidated) | app-kits | Grows a slice with each landing app-kits item. |

## Next recommended work

(Updated 2026-07-21 after the Content + Live-data wave and the three-track
study. The former #1/#2 recommendations — 0100 and the live-data chain —
are DONE.)

1. ~~**0120 (TextArea)**~~ — DONE 2026-07-22 (with 0500's passive-panel
   substrate slice + `Overlays::top_z`). Both port epics now have their
   full widget dependency set.
2. **The 0.2 budget batch** — remaining 0170 audit + the riders the
   study queued (`Role` variants or `#[non_exhaustive]`, subtree focus
   step, `TextInput::masked`; `Overlays::top_z` landed additively with
   0120) + 0250's ruling fix + 0180's platform-claim honesty: one
   budgeted breaking window, not a trickle.
3. **0300 (lifecycle events)** — the control-plane band's foundation;
   cheap, additive, and everything agent-facing (0310-0360) consumes it.
4. **0500 (popup substrate + selects)** — the app-kits trunk; unblocks
   forms/wizards and the config-console app class.

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

### Cross-track edges from the 2026-07-21 study (load-bearing)

- **0300 before everything in its band** — 0310/0320/0340/0350 all consume
  the lifecycle surface.
- **0320 ↔ 0410**: whichever ships first must promote `gltf_json` to a
  neutral home (with a `three`-feature re-export) or the second is stranded.
- **0340 ↔ 0520**: the wizard is the persist registry's accepted first
  consumer; 0520's crash-resume journey is 0340's restore-ordering evidence.
- **0360 → 0350/0320-ADR**: the attach proof's experience report folds back
  before the attach design or the protocol ADR freezes (the 0060→0050
  evidence-first pattern).
- **0500's popup substrate before its consumers**: 0120's completion
  dropdown (passive-panel mode), 0530's action menus, extensions 0430's
  tooltips all consume it; the `Overlays::top_z` engine delta rides the
  0.2 window.
- **0420 before 0430/0440/0450**; **0440 before 0430**; the link seam
  (0480, mergeable into 0165) before 0430's activation milestone.
- **The 0250 ruling** (selection follows movement; activation = Enter /
  click-when-selected; commit-on-move per-widget opt-in, default off) is
  recorded in `reviews/study/platform-on-appkits.md` and encoded by
  0530/0550/0570; the List/Table engine fixes cite it.
- **Sibling extension crates inherit the dependency posture** (std +
  abstracttui + hand-rolled parsing); the TLS-class exception is not
  granted here — it rides live-data 0050's transport ADR.

## ADR state

`docs/adr/` exists: **0001** (API stability policy toward 0.2/1.0),
**0002** (two-`Style` ruling), **0003** (struct extensibility) landed
2026-07-21. Still owed: the **extension-architecture ADR** (skeleton ready
in `reviews/study/extensions-cycle3.md` §1c — lands before/with the first
04xx packaging execution), the **0320 control-protocol ADR**, the **0340
persistence-container ADR**, and the **0050 transport ADR** (waits on
0060's evidence). The a11y-completeness + redaction-at-source clause
(drafted in `reviews/study/platform-cycle3.md`) joins the next ADR pass.

## Process

- New item: scan every lifecycle dir + topic folder for the next unused global
  `NNNN`, add it under the right state, and update this overview's counts,
  ledgers, and sequencing in the same pass.
- Completion: append a `## Completion report` (final path, date, outcome, key
  validation), move to `completed/`, update the ledgers here.
- Deprecation: append a `## Deprecation report` with the reason, move to
  `deprecated/`, update this overview.
- Bands: live-data owns 0010–0090, app-widgets owns 0100–0190, ports own
  0200–0290 (0200/0210 = port epics; 0220–0260 = first-app findings),
  control-plane owns 0300–0390, extensions owns 0400–0490, app-kits owns
  0500–0590. Leave gaps for insertion.
