# Control-plane backlog track

## Status
Proposed (study cycles 1–3 complete, 2026-07-22: drafted, cross-reviewed
by the extensions seat — all findings folded, see
`reviews/study/platform-cycle3.md` — and converged with the app-kits +
extensions tracks; awaiting maintainer reading + the integrator's
overview fold). Numbering band: **0300–0390**.

## Purpose
Four maintainer asks, one shared foundation: make a running AbstractTUI
application **observable and drivable from outside its own keyboard** —
by another process, by a test harness, by an agent through a protocol
bridge, or by a terminal that connects later.

1. **Lifecycle formalization** (0300) — boot/ready/suspend/resume/
   detach/attach/shutdown plus custom app events as a documented,
   subscribable surface. The foundation everything below shares.
2. **Programmatic/agent control** (0310, 0320, 0330) — an in-process
   automation bus (inject input, query the semantic tree, invoke named
   actions, subscribe to events), a transport-agnostic JSONL control
   server (stdio / unix socket, strictly opt-in), and a protocol-bridge
   note that makes an MCP wrapper a thin external client.
3. **State snapshot / resume-after-crash** (0340) — an honest `Persist`
   registry (apps declare keys; the engine never pretends it can
   serialize arbitrary signals) with atomic snapshots, restore-on-start
   and crash-marker semantics.
4. **Background mode + attach/detach** (0350, 0360) — run headless
   against a virtual terminal, connect a real terminal to see live
   rendering, detach, reattach. A feasibility-graded design item plus a
   small proof milestone.

The engine is unusually well prepared for this track: `Driver::turn` is
already a non-blocking, scriptable frame pass (`src/app/driver.rs:222`),
`App::run_on` drives the real loop against any `Terminal` object
(`src/app/mod.rs:348`), `testing::CaptureTerm` + `VtScreen` prove the
in-memory-terminal seam end-to-end (`src/testing/capture.rs`,
`src/testing/vt.rs`), `UiTree::accessibility_tree()` is a machine-readable
UI state snapshot (`src/ui/tree.rs:170`), and `Actions` is a
run-by-name command registry (`src/app/actions.rs:125`). Most of this
track is *naming, composing and exposing* seams the engine already
tests against — plus the genuinely new pieces (wire protocol,
persistence, session brokering), graded honestly below.

## Invariants every item must preserve
Restated from `planned/0001_roadmap.md` and binding here:
- **Single UI thread.** All control-plane ingress crosses via
  `WakeHandle::post` (the one sanctioned crossing —
  `src/reactive/source.rs:5-10`; wrong-thread access is a named panic).
  Replies leave as `Send` snapshots, never live handles.
- **Damage contract** (`docs/design/01-damage-contract.md`): control
  commands are posted jobs, drained in phase U only; nothing in this
  track runs user code past phase U or writes mid-frame.
- **Zero idle cost.** A quiet control server adds zero UI-loop wakeups
  (its threads block on their own fds); a detached headless app idles
  at the same 0 bytes / 0 allocations the pins enforce
  (`tests/adv_app.rs:54`, `tests/alloc_budget.rs:140`). Feature off =
  zero threads, zero cost.
- **Five-dep standalone posture** (`Cargo.toml:19-34`). This entire
  track requires **no new dependencies**: JSON parsing is in-crate
  (`src/three/gltf_json.rs`), unix sockets come from
  `std::os::unix::net`, everything else is std + existing deps.
- **Honest degradation.** Every capability gap is labeled (windows
  socket transport, caps mismatch on attach, refused restores), never
  silent.

## Items

| ID | Title | Feasibility verdict |
| --- | --- | --- |
| 0300 | App lifecycle events — named, subscribable transitions + custom app events | **v1-able** (vocabulary + emission points are design decisions, mechanics all exist) |
| 0310 | Automation bus — inject input, query semantic tree, invoke actions, subscribe | **v1-able**, core unconditional (composes existing seams; DropOldest ring egress; opens one private queue) |
| 0320 | Control wire protocol + serve seam — JSONL over unix socket / free-pipes stdio / fd-pair | **v1-able** unix; ships as default-OFF `control-server` feature; PRECONDITION: JSON promotion with extensions 0410; windows named pipe **needs-design** (deferred) |
| 0330 | MCP bridge note — protocol mapping for an external MCP wrapper | **trivial after 0320**; bridge + productized attach client are out-of-crate protocol consumers |
| 0340 | Persist registry — declared state keys, atomic snapshot, restore, crash marker (pid-bearing) | **v1-able** core; migration hook + restore/mount ergonomics **needs-design** (first consumer: app-kits 0520) |
| 0350 | Background mode + attach/detach — design | core **v1-able-with-design** (conservative serve caps); caps re-negotiation + ImageSession reset **needs-design**; multi-viewer, windows **research** |
| 0360 | Milestone: attach/detach proof (serve example + attach client) | **v1-able** as scoped (single session, single client, fixed no-graphics caps) |

## Sequencing (load-bearing; final, cycle 3)
- **0300 before 0310/0320**: the bus and the wire protocol both carry
  lifecycle events; retrofitting the vocabulary later would break the
  protocol's first consumers. (0300 is also 0340's flush trigger and
  0350's Detached/Attached vocabulary — the track's foundation.)
- **0310 before 0320**: the wire protocol is a serialization of the
  bus, verb for verb. Designing the wire first would invert ownership
  (protocol dictating engine API).
- **0320's precondition**: the JSON promotion to a neutral home lands
  BEFORE or WITH whichever of {extensions 0410's `three` gate, 0320}
  ships first — coordinated edge, recorded on both sides (0320 §4;
  their 0410). 0320 itself ships as the default-OFF `control-server`
  feature (placement settled with extensions' 0400 ruling).
- **0320 before 0330 and 0360**: the MCP bridge and the attach proof
  are both out-of-crate/feature-gated clients of the same transport
  seam and protocol.
- **0340 is independent** after 0300 (it emits lifecycle events but
  shares no machinery with the server items). It may land any time;
  its first consumer is the app-kits wizard (0520), whose
  resume-after-crash journey is 0340's acceptance evidence.
- **0350 (design) before 0360 (proof)**, and 0360's experience report
  feeds back into 0350 AND the 0320 protocol ADR before any freeze —
  the same evidence-before-decision rule the live-data track uses for
  its transport ADR (0050). Graphics-enabled serve stays blocked on
  0350's ImageSession-reset design; 0360 deliberately fixes
  no-graphics caps to dodge that class.

## What we will NOT do (honest list)
- **No remote code execution surface, ever, by default.** The control
  server is opt-in twice over: compiled only under the default-OFF
  `control-server` feature (feature off = the constructor does not
  exist), and constructed only by the app's explicit call. Its verbs
  are a closed set (inject input, read state, run *registered*
  actions). There is no eval, no arbitrary FFI, no file access verb.
- **No TCP listener in v1.** Unix domain sockets (0600, per-user
  runtime dir) and stdio only. Anyone who can open the socket already
  runs code as the same user — the documented trust boundary. Remote
  control, if ever wanted, is a later item with its own threat review.
- **No MCP/JSON-RPC dependency in the core crate.** Bridges are
  external clients of the wire protocol (0330).
- **No automatic serialization of arbitrary signals.** Signals are
  `Box<dyn Any>` cells (`src/reactive/signal.rs:73-89`); without user
  participation a general snapshot is impossible in Rust without
  reflection, and we will not pretend otherwise (0340).
- **No tmux replacement.** Attach/detach hosts ONE AbstractTUI app per
  session — no window management, no arbitrary child processes, no
  scrollback re-implementation (0350 non-goals).
- **No async runtime.** Blocking threads + `WakeHandle::post` is the
  engine's concurrency model; this track adds no tokio/mio.

## Cross-track edges (by band, read-only)
- **live-data 0010–0090**: shares the ingress philosophy
  (`bounded_source`'s counted-drop back-pressure is the model for the
  control server's event egress). 0050's transport ADR is about
  HTTP/WS/TLS for *app data*; this track deliberately needs none of it.
- **app-widgets 0100–0190**: 0170's API-stability pass gates the public
  shapes 0300/0310 add to `App`/`Driver`; sequence their merges the
  same way 0100/0130 are gated.
- **ports 0200–0290**: both port epics are natural control-plane
  validators (a coding-console driven end-to-end by its own test
  harness through 0320 would be the strongest acceptance evidence).
- **extensions 0400–0490**: converged (cycle 3). Placement per their
  0400 ruling: 0310 bus = core unconditional; 0320 server = in-tree
  default-OFF `control-server` feature; 0330 bridge + productized
  attach client = out-of-crate protocol consumers. Shared edge: the
  JSON promotion precondition (their 0410 ↔ our 0320, recorded both
  sides). Shared vocabulary: extension commands register as dotted-name
  actions ("graph.zoom_in") with name+chord+description metadata —
  nullary in v1, `invoke_with(name, payload)` reserved as the v2 seam;
  canvas-drawn content stays OPAQUE to the semantic tree (extensions
  expose intent as actions/events); structured model export is not a
  bus query (reviews/study/platform-cycle3.md, question c).
- **app-kits 0500–0590**: the wizard (0520) is the NAMED first
  `Persist` consumer (draft survival, resume-after-crash — recorded in
  0340 and in reviews/study/platform-on-appkits.md F3), and the
  admin/chat patterns are the first attach/detach beneficiaries. Two
  shared contracts pinned in that review: **redaction at source**
  (masked inputs must mask `access_value` — the bus/wire/bridge
  republish the semantic tree verbatim, F2) and **a11y completeness**
  (every interactive affordance a kit widget renders must appear in
  the accessibility snapshot, or it is invisible to automation, F6).

## ADR state
No ADR system exists yet (`docs/backlog/overview.md` ADR section; 0050
and 0170 will stand it up). Two items in this track join the "needs
ADR before closing" list when scheduled: **0320** (wire-protocol
stability + security posture are durable public contracts) and
**0340** (snapshot format compatibility policy). One ADR candidate
CLAUSE is drafted and queued for whichever wave stands `docs/adr/` up:
the a11y-completeness/redaction-at-source rule
(`reviews/study/platform-cycle3.md`, question b — liftable verbatim).

## Promotion criteria (proposed → planned)
An item promotes when: its dependency chain above is landed or
scheduled, the maintainer confirms the security posture (0320/0350),
and — for 0350 — the 0360 proof exists and its experience report is
folded back. Until then these are direction, not commitment.

## Ledger note
Deliberately **not yet folded into `docs/backlog/overview.md`** (the
integrator-owned cross-track ledger): three planning tracks are being
drafted in parallel (bands 0300/0400/0500) and the fold happens once,
after cross-review, to keep the ledger single-writer.
