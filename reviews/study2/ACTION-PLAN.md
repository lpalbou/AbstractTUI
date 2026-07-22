# Action plan — cycle-3 synthesis (2026-07-22)

The concrete plan the maintainer asked for. Inputs: the six study-2 reports
in this directory, the amended canonical roadmap
(`docs/backlog/planned/0001_roadmap.md`), the fresh first-app items
0290–0298, and the backlog overview. Effort scale: **S** = hours–1 day,
**M** = 2–5 days, **L** = 1–3 weeks. Backlog counts/ledgers are FIX-INPUT's
this cycle; this file changes none of them.

Ordering law (unchanged): general needs first, apps as validators, every
item below is justified by an app class and validated by a named consumer.

---

## Horizon 1 — this week

### 1. The 0.2.2 patch (ship it) — M overall

The image-lifecycle fixes are already in-tree (CHANGELOG Unreleased; 1,452
tests green, clippy/fmt clean, `cargo semver-checks` says no version bump
required — a compatible patch). The five first-app items below complete it.

| Item | Effort | What / why now / evidence / validates on |
| --- | --- | --- |
| image lifecycle (in-tree) | done | WHAT: kitty `p=1` replace, blit-origin + vacated-rect repair, iTerm2/sixel prev-poison, tmux per-escape wrap, scroll guard, warnings surfaced, parked-mosaic repair. WHY NOW: the maintainer's "did view-image ever work?" doubt was well-founded — anything dynamic corrupted. EVIDENCE: `media-images-truth.md` (5 bug classes) + `quality-on-media.md` (adversarial verdicts, +3 tests, 10.2× guard cost measured). VALIDATES ON: the 10-line maintainer recipe on a real kitty/iTerm2/sixel terminal (the one remaining unverified leg). |
| 0290 selection linger | S | WHAT: key-copy becomes one-shot (copy clears), or first non-selection key clears+passes. WHY NOW: NO app-side workaround exists — the selection layer eats `c`/Enter BEFORE tree dispatch; typing "check" after a drag loses every leading `c`. EVIDENCE: item 0290 (honesty correction: the consumer's `on_change` mitigation is ineffective). VALIDATES ON: abstractcode-tui deletes its composer clear-on-change workaround. |
| 0293 kitty flags post-probe | S–M | WHAT: emit the standard flags push on the probe's false→true transition (+ its own pop bookkeeping); gate WezTerm's env claim on probe evidence. WHY NOW: Shift+Enter is dead on iTerm2/VS Code/Warp — the terminals that DO speak the protocol — and this is the PREREQUISITE of the key-state chain 0293→0700→0610. EVIDENCE: item 0293; convergence-cycle2 §2 (verified at source: `apply_caps_upgrade` is presentation-only). VALIDATES ON: Shift+Enter works on iTerm2/VS Code/Warp with zero app changes; 0700 later runs full-fidelity there. |
| 0295 public caps accessor | S | WHAT: `app::current_caps()`/`use_caps` (post-probe snapshot). WHY NOW: apps ship static, capability-neutral key hints (over- or under-claiming); SAME primitive as media-av 0685 — design once, serve both. EVIDENCE: items 0295 + 0685 (cross-ref recorded both ends). VALIDATES ON: the consumer's placeholder//help swap to per-terminal truth; the images example's channel label goes probe-honest. |
| 0296 programmatic select open | M | WHAT: `open_at`/one-shot `pick` (or a public option-rows core) so command-summoned pickers adopt the select family. WHY NOW: command-driven pickers are the dominant picker shape in terminal agent tools; the 0.2.1 faces only open from trigger rows. EVIDENCE: item 0296 (adoption attempted, does not fit). VALIDATES ON: abstractcode-tui converts its four List-in-Modal pickers. |
| 0298 stale frame band | M | WHAT: reproduce (resize while modal open + on_activate close; CaptureTerm + VtScreen at two sizes), then fix the damage source. WHY NOW: a half-stale screen violates the damage contract's truth promise; live maintainer screenshot. EVIDENCE: item 0298. VALIDATES ON: the repro test + the maintainer's tall-tabbed-terminal scenario. |

### 2. 0102 `FeedBlock::Rich` — M

- WHAT: a span-model feed block; the engine already owns `RichText` ("one
  renderer, three faces" — Feed is the missing fourth). Run the
  block-vocabulary design pass with 0280 (widget blocks) + 0660 (image
  blocks) in the room: the enum grows once.
- WHY NOW: the first consumer's **#1 tension** — a ~137-line Card subsystem
  exists only because feed lines cannot mix inks; every transcript and every
  log viewer re-pays it. Additive.
- EVIDENCE: `field-consumer-tensions.md` §4.1 (tension #1);
  `field-app-classes.md` class 5 (severity-tinted log lines = the same gap);
  convergence-cycle2 §3.
- VALIDATES ON: abstractcode-tui deletes the Card system; a severity-tinted
  log tail needs zero custom blocks.

### 3. 0104 `FeedState::sync` — M

- WHAT: a diffing adapter from a slice source of truth (key fn +
  fingerprint fn + render fn; rebuild-on-shrink policy inside the engine).
- WHY NOW: ~180 lines of fingerprint/mirror machinery every fold-shaped
  consumer will re-implement slightly wrong; the consumer carries a
  byte-exactness test just to keep its hide-predicate honest.
- EVIDENCE: `field-consumer-tensions.md` §3.6 (tension #3).
- VALIDATES ON: the consumer's `wire_feed` + FNV fingerprint + mirror
  predicate are deleted; the fast path becomes the default path.

### 4. 0297 disposal law engine-wide — S–M

- WHAT: fix Button (`pressed.set(false); fire();` — the 0250 move verbatim),
  audit every remaining callback site (TextInput/TextArea, select commits,
  Scroll/Feed), state the law once: "widget bookkeeping completes before
  user callbacks run; a callback may dispose the widget's scope."
- WHY NOW: tension #2 — every modal-closing consumer must defer disposal one
  tick to dodge a crash class 0250 already ruled on; Checkbox/Tabs/Radio
  audited clean this cycle, so the remaining surface is small.
- EVIDENCE: `field-consumer-tensions.md` §3.1; item 0297 (Button offender
  verified at source).
- VALIDATES ON: acceptance is consumer deletion — abstractcode-tui's
  `UiCtx::retire` one-tick deferral becomes a synchronous close.

### 5. 0040 promotion + the jittered backoff helper — M

- WHAT: promote to planned/ (single-writer pass), then build the
  signal-friendly `ConnState` enum + pure jittered `Backoff` helper + the
  documented "frame loop while offline" answer (worker sleeps; UI blocks;
  zero wakeups).
- WHY NOW: the trigger fired — the first consumer hand-rolled SSE +
  linear-backoff reconnect WITHOUT jitter (the item's named thundering-herd
  risk verbatim), and the entity-monitoring class multiplies it per stream.
- EVIDENCE: 0040's dated promotion-evidence section;
  `field-consumer-tensions.md` §3.5; `field-app-classes.md` class 4 ("this
  study's strongest cross-track recommendation").
- VALIDATES ON: the consumer's `runner.rs` reconnect loop swaps to the
  helper (gaining jitter); the API is declared STABLE only after a real
  disconnect cycle (0060 or 0210 phase 1) — build now, freeze later.

---

## Horizon 2 — this month

| Item | Effort | What / why now / evidence / validates on |
| --- | --- | --- |
| 0700 key press/release state | M | WHAT: the held-key service (down-set snapshot + edge callbacks, capability-honest fidelity flag, legacy repeat-timeout degradation, FocusLost hygiene, opt-in release routing). WHY NOW: unblocked by 0293 in the patch; the ONE primitive both real-time games and voice PTT need — general-needs law in action. EVIDENCE: `field-games.md` §2 (releases decoded then discarded one seam later); `media-voice-plumbing.md` §2; convergence §2. VALIDATES ON: move-while-held + chorded diagonals in the first game example; 0610's hold-to-talk. |
| 0710 game tick | S–M | WHAT: public per-frame tasks (the private `register_frame_task` lane, honest `now`) + a fixed-timestep helper (accumulator, spiral-of-death clamp, pause). WHY NOW: the pacing already exists and is adequate — only the sanctioned surface is missing; even the in-tree effects example drifts on assumed dt. EVIDENCE: `field-games.md` §1; convergence §1 (0710 called the band's strongest item). VALIDATES ON: the game example + the effects example's assumed-dt deletion. |
| 0620 `Meter` + `AudioScope` | M | WHAT: level bars with real ballistics (instant attack, frame-clocked decay, peak-hold, dB, theme zones) + rolling waveform over the chart substrate. WHY NOW: every voice app and level-bar dashboard re-derives ballistics; data shapes verified against the real gateway/assistant stack. EVIDENCE: `media-voice-plumbing.md` §1–2. VALIDATES ON: 0650; acceptance MUST pin "a silent meter reaches a fixpoint and stops requesting frames" (the report's open question 2 — zero-idle is inviolable). |
| 0650 voice-mock example (+ 0640 docs pattern) | S–M | WHAT: `examples/voice-mock.rs` — timer-driven fake synth + fake mic (levels, bands, word timings, PTT), no audio, no network; plus the 0640 external-process docs pattern (verified: data plumbing covered, process lifetime is ~12 lines of app code — a `KillOnDrop` guard on `cx.on_cleanup`, deliberately NOT engine code; an orphaned recorder holds the mic open). WHY NOW: the band's validation vehicle; ships the 0610/0620/0640 legs now, the 0630 karaoke leg follows in horizon 3. EVIDENCE: `media-voice-plumbing.md` §2–3. VALIDATES ON: itself (live-smoke case) — it IS the validator. |
| 0142–0148 markdown seeds | L (each M) | WHAT: GFM tables (0142, shares `solve_columns`), in-flow images (0144), heading anchors/TOC (0146), search-highlight overlay (0148 — builds the shared text↔cells mapping). WHY NOW: the mdpad-class reader enabler (0460's four named core gaps); 0148's mapping is the substrate 0160-logical and 0630 both ride — build it once here. EVIDENCE: extensions 0460; `field-app-classes.md` (viewer class); the 0148↔0160↔0630 substrate edge recorded in the items. VALIDATES ON: the mdpad parity dashboard; chat messages with tables. |
| 0190 time-axis charts | M | WHAT: `TimeSeries` bounded buffer (push (t,v), drop by age/count) + time-axis tick labels + gap honesty when samples pause. WHY NOW: every monitor hand-rolls the history ring — the flagship dashboard example included. EVIDENCE: `field-app-classes.md` class 5 (the named NEEDS-ITEM); item 0190 (verified: no time-axis code in chart.rs). VALIDATES ON: the dashboard example graduates from its hand-rolled ring; the first production-monitor consumer. |
| Scheduled perf/fuzz/soak (0180 leg) | S–M | WHAT: the quiet-runner scheduled CI job running the two release perf suites + fuzz/soak; ratchet asserts on the byte medians the suites already print. WHY NOW: risk #1 below — both perf suites are explicitly-run/`#[ignore]`d today, so an emission regression is invisible until someone runs them by hand. EVIDENCE: `quality-perf.md` (suite exists, medians printed "so future runs can ratchet"; richtext-wrap named the first run-isolation beneficiary); 0180's open leg. VALIDATES ON: a seeded regression turning the job red within a day. |

---

## Horizon 3 — next quarter

| Item | Effort | What / why now / evidence / validates on |
| --- | --- | --- |
| The 0.3 budget window | M (batch) | WHAT: execute `planned/0002` when the maintainer signs — `Role` `non_exhaustive` + end-appended variants (incl. the parked `Role::Select` and 0570's `Tree`/`TreeItem`), `TokenKind` `non_exhaustive`, `content_size` deprecation; 0570 tree view rides the same window. WHY: the budget is written and CI-enforced; a day-one local semver-checks run already caught a live mid-enum `Role::Select` insertion (resolved: parked into the batch). EVIDENCE: `planned/0002` (the live catch + enforcement). VALIDATES ON: one batched release, one migration note per entry, semver job green after the bump. |
| Extension architecture execution | L | WHAT: 0400 ADR (skeleton ready) → 0420 core vector canvas (dot canvas, bezier/arc, chart refactor gated on byte-identical goldens) → 0440 `abstracttui-graph` read-only auto-layout (layered v1 for DAG-class). WHY: the abstractflow-class enabler — workflow visualization is COVERED by exactly this chain, and 0730's placement ruling also waits on 0400's classification. EVIDENCE: `field-app-classes.md` class 2; overview sequencing (0420 before 0430/0440/0450; 0440 before 0430). VALIDATES ON: a named DAG-view consumer; the 0450 mermaid subset later. |
| Control-plane 0300 → 0310 → 0320 | L | WHAT: lifecycle events → automation bus (inject input, query semantic tree, invoke actions) → opt-in JSONL control server; 0330 MCP bridge after the protocol ADR freezes. WHY: the agent-controllable-apps bet — the band everything agent-facing consumes; cheap foundation (0300) first. EVIDENCE: control-plane band charter; overview cross-track edges (0300 before its band; 0320 ↔ 0410 gltf_json promotion). VALIDATES ON: a port harness or embedder driving a real app through the bus. |
| 0210 / 0200 port epics | L each | WHAT: the a2a chat TUI (0210; phase 1 = the 0060 watcher shape) and the coding-agent console (0200). WHY: both eras end-to-end under real workloads — and 0210 is the named SECOND CONSUMER (risk #3): it exercises 0040/0102/0550/0560 from a non-composer-centric angle. EVIDENCE: ports epics; `field-app-classes.md` class 3 (coordination UI maps 1:1 onto 0210 + app-kits). VALIDATES ON: daily use; the experience reports feed 0050's ADR and the 1.0 bar. |
| 0630 speaking highlight + 0160 logical selection | M each | WHAT: karaoke emphasis from `Signal<Range>` over rich/markdown text; logical widget-content copy (markdown source, unwrap soft-wraps). WHY: both are consumers of the text↔cells mapping 0148 builds in horizon 2 — sequence them after the substrate exists. EVIDENCE: `media-voice-plumbing.md` §2 (timing honesty: TTS paces by estimation, STT has real word timestamps — app policy, engine renders ranges); 0160's remaining scope. VALIDATES ON: 0650's karaoke leg; copy-message in the chat port. |

---

## Deliberately deferred (with reasons)

- **0060 watcher** — maintainer-gated, explicitly not-now (unchanged);
  0210 phase 1 may absorb it (whichever lands first, the other adopts it).
- **0050 transport ADR** — stays evidence-gated on the watcher/port
  experience report; the consumer proved ureq-class blocking HTTP in a
  worker workable meanwhile. Never settle it from the armchair.
- **Web/HTML (0470)** — the standing verdict holds: full web NEVER; the
  readable-subset slice only if all four gate criteria are met.
- **0350/0360 attach research** — awaits the maintainer's security/ownership
  review; the attach design builds only after 0360's report folds back.
- **0665 animated image sessions** — the decoder dependency needs a ruling
  first (dependency-posture law).
- **iTerm2/sixel beneath-repaint decay (quality-on-media F2)** — a full
  payload re-emission per damaged frame needs a design decision (throttle /
  hole-punching / placement discipline), which is exactly 0660's design
  space; deciding it reflexively now would pre-empt the Feed image block.
- **Radial gauge, alert-state recipe, embedded CheckList, fold-closure docs
  blessing** — the convergence pass's open hand-offs: the gauge waits on
  0420's public canvas (filing now inverts the dependency); the others are
  docs edits or app-kits-owner calls, not roadmap items.

---

## Top-3 risks

1. **Perf ratchets missing — emission regressions are invisible.** Both
   perf suites (`perf_budgets`, `perf_app_surfaces`) are explicitly-run and
   `#[ignore]`d by design; no scheduled job runs them, and the byte medians
   they print are not yet ratcheted. A 2× emission regression on the feed
   token path would ship silently today. Mitigation is scheduled (horizon 2:
   the 0180 leg + ratchet asserts); until it lands, run the suites by hand
   before every release cut.
2. **Band sprawl = curation debt.** 67 proposed items across 8 bands after
   three study waves. The promotion discipline that keeps this a map rather
   than a wish list is evidence-triggered promotion (0040 is the model: a
   dated evidence section, then promote) plus a single-writer convergence
   pass at each wave's end. Without it, the backlog's signal-to-noise decays
   and the next synthesizer re-derives everything.
3. **Single-consumer evidence bias.** Nearly every field signal traces to
   abstractcode-tui — a composer-centric agent console. Its shape
   over-weights transcript/composer needs and under-weights tables,
   navigation, graphs, and multi-stream reconnect. **The named
   second-consumer need**: the a2a chat TUI (0210 phase 1, the 0060 watcher
   shape) or an entity monitor (`field-app-classes.md` class 4) as the next
   validator — both exercise 0040, 0102, 0190, and the app-kits chrome from
   a different angle, and either would convert this roadmap's strongest
   assumptions into evidence or corrections.
