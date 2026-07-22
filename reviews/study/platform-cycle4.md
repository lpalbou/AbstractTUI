# Platform track — cycle 4 (final pass)

Two artifacts settled here: (1) the reconciliation of the extensions
seat's "7 one-line corrections" (`reviews/study/extensions-cycle3.md`
§2), which were filed against a snapshot of my files taken at ~03:10 —
BEFORE the cycle-3 fold wave landed (03:11–03:22); (2) the final
newcomer read of the band and the integrator handoff block.

## 1. Reconciliation table (their 7 corrections vs current files)

Method: each correction re-checked against the CURRENT item text this
session (grep + full re-reads), not against my own cycle-3 report.

| # | Their correction (short) | Verdict | Where it lives now |
| --- | --- | --- | --- |
| 1 | 0350 lacks ImageSession-identity needs-design + conservative-serve-caps rule | **Already covered** (folded 03:11) | 0350 "The hard parts": "Rule adopted (extensions review P1-1)" (conservative caps, upgrade-direction rationale) + the full "ImageSession identity across attach (needs-design)" entry citing `src/gfx/session.rs:122-139` and the channel-change-reset precedent (session.rs:138-139); Feasibility regraded (graphics-enabled serve blocked on the reset design); the misleading "same lever" line in Current code reality corrected to name its honest limit |
| 2 | 0310:92-95 + 0320:80-83 still say mpsc drop-oldest | **Already covered** (folded 03:13/03:12) | 0310 verb list `subscribe` + 0310 Feasibility (names the replaced mpsc sketch) + 0320 `subscribe/event` verb: mutex-owned ring with `OverflowPolicy::DropOldest` (`src/reactive/ingest.rs:55-63`), with the wire-visible starvation rationale in both |
| 3 | gltf_json still a checklist step, 0410 coordination absent | **Already covered** (folded 03:12) | 0320 §4 "JSON promotion is a NAMED PRECONDITION, not a checklist step" (before-or-with ordering vs 0410; `three` re-export cfg'd under the `three` feature); also in 0320's Depends-on metadata, Feasibility risk (b), and checklist line 1 |
| 4 | stdio predicate "refuses when a session is entered" wrong | **Already covered** (folded 03:12) + one residual fixed THIS cycle | 0320 §2 "The stdio predicate (corrected…)": free iff the terminal did not take the stdin/stdout fallback, plumbed at `run_prepared`; cycle-4 residual: the "Current code reality" stdio bullet still told the old too-strict story — now rewritten to the narrower fact |
| 5 | inject(Resize) unguarded; 0330 resize mapping too | **Already covered** (folded 03:13/03:14) | 0310 `inject` "Resize guard (extensions review P2-1, verified)" — accepted only when `Terminal::is_tty` is false, structured refusal otherwise; 0330 mapping line: "headless/serve sessions only", bridge surfaces the bus error verbatim |
| 6 | 0300 post-enter() obligations / 0340 multi-instance / 0340 crc32 | **Already covered** (folded 03:14-03:15) | 0300 §4 "What 'residual' means, exactly" (closed list: damage-all+poison, size re-query, latched verbs at unix.rs:636-667; do-not-double-apply); 0340 §4 "Multi-instance rule" (pid-bearing exclusive lock; live-pid = refuse, dead-pid = crash; N instances = N paths); 0340 §3 checksum = `gfx::png::crc32` (png.rs:388-390, ungated under 0410) |
| 7 | README extensions edge still "nothing here assumes their design" | **Already covered** (folded 03:16) | README "Cross-track edges → extensions 0400–0490: converged (cycle 3)" — placement (bus core / server default-OFF feature / bridges external), JSON precondition both sides, dotted-name actions, `invoke_with` reserved seam, canvas opacity |

**Outcome: 7/7 already covered** — the parallel-read artifact was a
timestamp race, not a disagreement: their §2 header itself records
"as of 03:10" and correctly credits the cycle-2 wave (02:57-02:58)
for the folds that HAD landed. Nothing in their corrections
contradicted my folds; correction 4 exposed one genuine residual (an
un-updated Current-code-reality bullet), fixed this cycle. The
lesson worth keeping for the process: **verification snapshots in
parallel study cycles must state their read-time against the peer's
write-time** — theirs did, which is why this reconciliation took
minutes instead of an argument.

## 2. Final newcomer read — fixes applied

- **0310**: the CaptureTerm validation bullet asserted the SemanticTree
  query "matches `accessibility_tree_text()` byte-for-byte" — 
  self-contradictory once the composed root+overlay snapshot rule
  (cycle 3) landed. Rewritten: byte-match holds with no overlays; with
  a modal open the reply must additionally contain the modal tree's
  entries. Added the Resize-guard acceptance case (refused on `is_tty`,
  accepted on CaptureTerm). Checklist lines updated to name the ring,
  the guard, the composed tree, and the dotted-namespace doc.
- **0320**: the "Current code reality" stdio bullet still carried the
  pre-correction "only coherent when headless" story, contradicting
  the corrected §2 predicate two screens later — rewritten to the
  narrower fact (pipes are the terminal wire only on the fallback
  path; normally-launched apps leave them free).
- **0340**: the first-consumer section updated to the ACCEPTED shape —
  app-kits' 0520 §7 settled on ONE key per wizard (`wizard.<id>`)
  sampling step values + current + visited; my text had said "keys
  derived from wizard/step ids" (plural). Heading now records
  "named cycle 2; accepted by app-kits cycle 3".
- **Stale-reference sweep**: no control-plane item references 0165 or
  needs a 0480 pointer (extensions' new link-seam item) — the bands
  never coupled on links; `run_with` (the pre-harmonization verb name)
  is gone; no mpsc/headless-only residue outside quoted corrections.
  0300/0330/0350/0360 read standalone-executable with explicit
  dependency lines; all feasibility verdicts checked current against
  the cycle-3 folds.

## 3. INTEGRATOR HANDOFF BLOCK (single-writer overview fold)

### Topic-tracks table row (one line)

| control-plane | `proposed/control-plane/` | Proposed | Making running apps observable and drivable from outside their own keyboard: lifecycle events, an automation bus + opt-in JSONL control server, declared-keys persistence with crash-resume, and headless serve with terminal attach/detach. |

### Proposed-ledger rows (final wording)

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0300 | App lifecycle events: named transitions (boot/ready/resize/caps/focus/suspend/resume/quit/shutdown) + custom app events, subscribable | control-plane | Scheduling any of 0310/0340/0350 (it sequences first in the band), or the first app needing suspend wiring / a flush hook. |
| 0310 | Automation bus: inject input / query semantic state / invoke actions / subscribe to events (in-process, core) | control-plane | 0300 landing + a driving consumer: a port harness (0200/0210), an embedding host, or 0320's scheduling. |
| 0320 | Control wire protocol + serve seam: JSONL over unix socket / free-pipes stdio / fd-pair, default-OFF `control-server` feature | control-plane | 0310 landing; PRECONDITION: JSON promotion to a neutral home coordinated with extensions 0410; closes only with the protocol+security ADR. |
| 0330 | MCP bridge note: agent access as an out-of-crate client of the frozen protocol | control-plane | 0320's protocol ADR freezing + a kickoff ruling naming the bridge's home and language. |
| 0340 | Persist: declared state keys, atomic snapshots, restore-on-start, pid-bearing crash marker | control-plane | 0300 landing, or the app-kits wizard (0520 — its accepted first consumer) starting; closes only with the container-format ADR. |
| 0350 | Background mode + attach/detach: headless sessions a terminal can connect to (design) | control-plane | Maintainer security + session-ownership review; graduates to build only after 0360's experience report folds back. |
| 0360 | Milestone: attach/detach proof — headless serve + attach client, one session, no graphics | control-plane | 0350's design review done + 0320's socket seam available; deliberately ~2-4 days, report-first. |

### Cross-track dependency edges to record in overview sequencing

- **0300 before 0310/0320/0340/0350** (band-internal foundation; the
  event vocabulary freezes with 0320's ADR).
- **0320 ↔ extensions 0410**: the `gltf_json` promotion to a neutral
  home lands BEFORE or WITH whichever ships first; the `three`
  re-export is cfg'd under the `three` feature (recorded in both
  items).
- **0340 ↔ app-kits 0520**: 0520 is 0340's accepted first consumer
  (one `wizard.<id>` key); 0520's kill-mid-wizard/restart acceptance
  journey doubles as 0340's acceptance evidence and pressure-tests its
  two needs-design edges.
- **0360 → {0350, 0320-ADR}**: the proof's experience report is the
  evidence input to both, before any wire freeze (the 0060→0050
  evidence-before-decision pattern).
- **0250 ruling**: the movement-vs-activation ruling text lives in
  `reviews/study/platform-on-appkits.md`; the List/Table engine fixes
  should cite it; app-kits 0500/0530/0550/0570 are born to it.
- **Two engine deltas homed OUTSIDE this band** (0170-flagged): the
  `app::Overlays` top-z query (home: app-kits 0500) and the
  subtree-scoped focus step (home: app-kits 0510).
- **ADR queue contributed by this band**: 0320 protocol + trust
  boundary; 0340 container format; plus the liftable a11y-completeness
  / redaction-at-source clause (`reviews/study/platform-cycle3.md`
  question b) for the 0170 pass. `Role` non-exhaustiveness settles
  both app-kits' Tree/TreeItem additions and 0320's protocol-enum
  concern in one 0170 line.
- **Band count for the overview header**: +7 proposed items
  (0300–0360), band 0300–0390, gaps left for insertion.
