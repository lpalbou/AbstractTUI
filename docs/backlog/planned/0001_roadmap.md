# 0001 — Roadmap: general capabilities for any terminal application

## Metadata
- Created: 2026-07-21
- Status: Planned (standing document — the canonical roadmap; updated as bands close)
- Track: roadmap (cross-track)
- Completed: N/A

## Mission

The maintainer's rule, verbatim: "best in class TUI — we must not design for a
single app but for any future apps, so we must think in terms of general
needs." AbstractTUI 0.1.0 is a published, test-pinned engine (rendering,
damage, input, layout, reactivity, images, 3D, themes — audited clause by
clause in `reviews/cycle11/completeness-and-code-port.md` §1). This roadmap
maps the road from "a proven engine" to "a foundation any application class
builds on": every capability below is justified by the **class of apps** it
serves, never by one app. The two port epics (0200, 0210) and the watcher
milestone (0060) appear only as **validation vehicles** — real programs that
prove the general capabilities hold under real workloads.

## Design principles

1. **General needs first.** A capability enters the backlog only with a
   class-level justification: which of the app classes below needs it, and
   what do they all share? "App X needs it" is evidence, never the design
   target. When one app needs something no class needs, it stays app-side.
2. **Apps validate; they never design.** The dogfood apps consume the engine
   as a normal crates.io dependency with zero engine coupling. Gaps found
   while building them are filed as backlog items — nothing ships "just for
   the app". App-side sugar (tool-call cards, approval bars) upstreams only
   when a second consumer proves it general. A budget overrun in a dogfood
   build is a finding about the engine, not a schedule slip.
3. **Standalone dependency posture.** Five tiny crates
   (`unicode-width`, `unicode-segmentation`, `miniz_oxide`, `libc`/
   `windows-sys`) and everything else in-crate. No capability may import the
   world; the one open dependency question (transport/TLS) is decided by the
   repository's first ADR (0050), with evidence, never by drift.
4. **Honest degradation, honest claims.** Every fallback is labeled, never
   silent (bounded ingestion counts its drops; capability gaps surface as
   notices). Every public claim is backed by executed evidence or worded down
   to what ran (0180). "Done" includes the docs.
5. **Zero idle cost is inviolable.** Idle = 0 bytes, 0 allocations, 0 wakeups
   — test-pinned today (`tests/adv_app.rs`, `tests/alloc_budget.rs`). Every
   addition (timers, feeds, reconnect, selection) must preserve it and extend
   the pins. A feature that costs something while nothing happens is wrong.

## The app classes served

| Class | What it demands beyond the shipped engine | Items |
| --- | --- | --- |
| Dashboards & monitors | timed refresh, follow-tail logs, bounded floods from probes/pipes | 0070, 0130, 0100, 0010/0020 |
| Chat & feeds | append-only rich feeds, multiline composer, attention signals, reconnect | 0100, 0120, 0130, 0150, 0010–0050 |
| Editors & consoles | multiline editing, streaming output, code/diff fidelity, subprocess feeds | 0120, 0110, 0140, 0100, 0010/0020 |
| Viewers | rich documents, take-text-out, activate references (images/3D shipped) | 0100, 0160, 0165 |
| Games & toys | fixed ticks, low input latency, shaders (largely shipped) | 0070 (+ shipped engine) |

## Milestone bands

Version numbers are capability bands, not dates: a band ships when its "done"
bar is met. Breaking changes batch into their band's release under 0170's
budget — never a trickle.

### v0.2 — Content era

**General need: apps whose content grows and moves.** Every class has a
surface that appends, streams, wraps, and scrolls — chat rooms, log tails,
transcripts, REPL histories, live documents. Today that surface must be
hand-rolled (the first shipped app did, and every item below cites its
workarounds as field evidence).

- Items: **0100** feed/transcript widget (append-only, keyed, virtualized
  rich items), **0110** streaming text (`md::StreamSession` — re-parse only
  the open tail block), **0120** multiline composer (history, block paste,
  caret-anchored completion), **0130** follow-tail idiom + measured content
  size, **0150** terminal verbs (notify/bell/title/copy) from components.
- Correctness first: the first-app defect wave — **0220** (autofocus panic in
  dyn regeneration), **0230** (modal shortcuts dead until focus), **0240**
  (overflow collapses fixed rows) — fixes before or alongside this band; its
  bugs live exactly on the surfaces this band extends.
- Gate: **0170's API rulings front-run this band** (its full pass completes
  in the Trust era). 0100/0130 land on the crate's own named churn points
  (`List` multi-row content, `Scroll::content_size`), so their public shapes
  merge only after the 0.2 breaking budget is written.
- Done means: appending to a 10k-item feed costs one item's typeset and the
  damaged rows; streaming costs O(open block) per delta, never O(document);
  the composer is engine-supplied; follow-tail has zero app-side edge cases;
  a component can ring/notify/title/copy inside the one-flush contract; the
  three first-app workarounds are deleted from the field.
- Validation vehicle: **console 0200 phase 1** (read-only viewer over
  recorded JSONL fixtures) plus the **abstractcode-tui migration** — the
  shipped first app's hand-rolled transcript, autoscroll effects, and modal
  workarounds are the measurable deletion list.

### v0.2 → v0.3 — Live-data era

**General need: apps fed by the world.** Files, subprocesses, pipes, sockets,
timers — every class except pure viewers is IO-fed. The ingress mechanism
(`WakeHandle::post` + waker + phase-U drain) is the engine's strongest
audited asset and is currently invisible: no name, no bound, no docs, no
example (both cycle-11 reviews, independently).

- Foundation (lands with v0.2): **0010** source→signal binding (named helper
  + the ownership rule), **0020** bounded/coalescing ingestion (labeled
  back-pressure, waker dedupe), **0030** the documented pattern
  (`examples/feed.rs` + docs page). **0070** recurring timers (`interval`
  beside `after` — time is the zeroth data source).
- Lifecycle (v0.3, evidence-gated): **0040** connection model + jittered
  backoff (offline must stay zero-wakeup idle), **0050** the transport ADR —
  decided from the watcher's experience report, not the armchair.
- Done means: the background-feed pattern is teachable in one docs page and
  one example; a flooding producer costs bounded memory, one wake per drain,
  and a counted, renderable drop signal; periodic work is one cancellable
  line; a hub restart mid-session is survived and honestly rendered; the
  dependency posture decision is recorded as the first ADR.
- Validation vehicle: **watcher 0060** — a ~2-day read-only multi-room
  watcher over a live hub: real sockets, real reconnects, real floods,
  hours-long soak, zero engine coupling. It needs nothing from the Content
  era (hand-windowed view, deliberately) so it validates this lane in
  isolation. The console proves the same lane over a subprocess pipe — two
  producers, one contract.

### v0.3 — Depth era

**General need: apps where users read closely and act on what they see.**
Rendering content is half the job; fidelity and interaction are the depth
half: code that tints correctly, text you can take out, references you can
activate. Lexers were already itemized (0140); the other two were named by
the evaluations (completeness P1-6, P2-7) but sat un-itemized until this
roadmap — 0160 and 0165 are new.

- Items: **0140** stateful cross-line lexers (python/js/toml) + the diff
  lexer (line-oriented; tool-result patches are the first validator's
  strongest want), **0160** content selection + copy (command-copy recipe →
  extraction API → opt-in drag selection; OSC 52 stays write-only), **0165**
  hyperlink/reference hit-testing (click/hover on rendered links reaches the
  app; URI vocabulary stays app-side).
- Done means: multi-line strings and block comments stop mis-tinting; diffs
  read red/green; every class can offer copy-message/copy-region without
  forking widgets; a rendered URL or file:line reference is activatable in
  one line of app code.
- Validation vehicle: **console 0200 phases 3–4** (diff tinting, copy,
  file:line jump-to-panel) and **chat 0210** (copy message, open link).

### v1.0 — Trust era

**General need: apps that bet years on the engine.** Stability contracts,
claims backed by gates, and external validation — the difference between "it
works" and "you can build on it".

- Items: **0170** API stability pass (public-surface audit, `non_exhaustive`
  policy, the two-`Style` ruling, deprecation convention, the batched
  breaking budget, the repository's first ADRs, public-api CI gate), **0180**
  platform truth + CI gates (Linux pty job or reworded claim, scheduled
  perf/fuzz/soak, MSRV declaration).
- Beyond items: the Windows interactive session (an operator act — the one
  mandate clause never proven end-to-end); an external-user feedback loop —
  both ports daily-driven plus at least one consumer not written by the
  engine's authors before any 1.0 freeze (the 0.1.0 freeze was validated by
  nobody external; 1.0 must not repeat that).
- Done means: no public claim ahead of its evidence; every intended break
  budgeted, batched, and migration-noted; ADRs record the durable decisions
  (transport posture, stability policy, write-only clipboard); MSRV declared
  and gated; regression drift surfaces within a day, not never.
- Validation vehicle: all three dogfood apps in daily use across macOS and
  Linux, plus the first external application's experience report.

## Validation vehicles (dogfooding)

| Vehicle | Proves | Status |
| --- | --- | --- |
| 0060 watcher | Live-data era under a real network: 0010/0020/0040 carrying real traffic; produces 0050's evidence | Proposed; explicitly not-now |
| 0200 coding console | Content era + streaming (0110/0140) + the subprocess lane (no network) | Proposed epic; blocked on dependencies |
| 0210 chat TUI | Both eras end-to-end: content + live-data incl. 0040/0050 + 0150; phase 1 adopts 0060 | Proposed epic; blocked on dependencies |
| abstractcode-tui (shipped) | Field evidence today; the Content era's migration/deletion test | Shipped; first migration target |

## Sequencing that must not be violated

- 0170's rulings before 0100/0130 public shapes merge (one budgeted 0.2).
- 0010 before 0020/0030; 0010+0020 before the watcher 0060 (hand-rolling
  their gaps inside it would un-validate the lane); 0060 before 0050 closes.
- 0100 is the widget trunk: 0110 feeds its tail, 0130 is designed with it,
  0140 tints its blocks later; 0160 builds on 0100's selection-by-key.
- Ports start only when their dependency lists land; whichever of 0060 /
  0210-phase-1 lands first, the other adopts it.
- 0180 is independent and may land any time; it must land before 1.0.

## What "best in class" means, measured

For each general need, the measure is **deletion**: the app-side machinery an
author must write for a class's defining surfaces approaches zero (a chat
room becomes `Scroll::new(feed).follow_tail(pinned)`; a live pane becomes one
named binding), verified concretely by the abstractcode-tui migration and the
dogfood builds. And the measure is **preservation**: every addition keeps the
audited invariants — zero-idle-cost, damage-proportional repaint, labeled
degradation, the five-crate footprint, claims never ahead of evidence.
