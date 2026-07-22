# 0001 — Roadmap: general capabilities for any terminal application

## Metadata
- Created: 2026-07-21
- Updated: 2026-07-22 (cycle-3 synthesis: era statuses set to shipped
  reality; the Media/AV and Interaction-depth eras added from the study-2
  evidence; the next-wave recommendation rewritten from consumer evidence.
  Evidence base: the six study-2 reports in `reviews/study2/`.)
- Status: Planned (standing document — the canonical roadmap; updated as bands close)
- Track: roadmap (cross-track)
- Completed: N/A

## Mission

The maintainer's rule, verbatim: "best in class TUI — we must not design for a
single app but for any future apps, so we must think in terms of general
needs." AbstractTUI is a published, test-pinned engine (0.2.1 on crates.io;
the 0.2.2 patch in flight — rendering, damage, input, layout, reactivity,
images, 3D, themes — audited clause by clause in
`reviews/cycle11/completeness-and-code-port.md` §1). This roadmap maps the
road from "a proven engine" to "a foundation any application class builds
on": every capability below is justified by the **class of apps** it serves,
never by one app. The port epics (0200, 0210), the watcher milestone (0060),
and the in-repo example vehicles appear only as **validation vehicles** —
real programs that prove the general capabilities hold under real workloads.
The first such vehicle has now reported: `abstractcode-tui` shipped on the
engine, migrated onto the Content-era widgets, and its tension report
(`reviews/study2/field-consumer-tensions.md`) is the strongest evidence
stream this roadmap has ever had.

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
   repository's transport ADR (0050), with evidence, never by drift.
4. **Honest degradation, honest claims.** Every fallback is labeled, never
   silent (bounded ingestion counts its drops; capability gaps surface as
   notices). Every public claim is backed by executed evidence or worded down
   to what ran (0180). "Done" includes the docs.
5. **Zero idle cost is inviolable.** Idle = 0 bytes, 0 allocations, 0 wakeups
   — test-pinned today through the whole app layer (`tests/adv_app.rs`,
   `tests/alloc_budget.rs`, including a mounted Feed, an armed interval, a
   parked popup, and a parked protocol image). Every addition (timers, feeds,
   reconnect, selection, meters, game ticks) must preserve it and extend the
   pins. A feature that costs something while nothing happens is wrong.

## The app classes served

| Class | What it demands beyond the shipped engine | Items |
| --- | --- | --- |
| Dashboards & monitors | timed refresh ✓, follow-tail logs ✓, bounded floods ✓; time-axis history charts, severity-tinted lines | 0070✓, 0130✓, 0100✓, 0010/0020✓, **0190, 0102** |
| Chat & feeds | rich feeds ✓, composer ✓, reconnect, rich/multi-ink lines, slice sync, image attachments | 0100✓, 0120✓, 0130✓, 0150 (clipboard ✓), **0040, 0102, 0104, 0660** |
| Editors & consoles | multiline editing ✓, streaming ✓, diff fidelity ✓; stateful lexers, disposal-safe callbacks | 0120✓, 0110✓, 0140 (diff ✓), **0297** |
| Viewers | rich documents, take-text-out (screen ✓ / logical open), activate references | 0100✓, 0270✓, **0160, 0165, 0142–0148** |
| Games & toys | fixed ticks, held-key input, sprites/tiles, grid math (shaders/particles shipped) | 0070✓, **0700, 0710, 0720, 0730** |
| Voice & AV apps | meters/scopes, push-to-talk, speaking highlight, trustworthy pixel images | **0610–0650, 0660s, 0700** |

(The gateway-console, workflow-editor, coordination-UI, and
entity-monitoring classes were gap-checked in
`reviews/study2/field-app-classes.md`: all COVERED by the app-kits 0500s,
extensions 0400s, control-plane 0300s, and ports bands — the full band map
lives in `docs/backlog/overview.md`. The two gaps that check named — 0190
and the 0040 promotion below — are filed.)

## Milestone bands

Version numbers are capability bands, not dates: a band ships when its "done"
bar is met. Breaking changes batch into their band's release under the
written breaking budget — never a trickle. The 0.3 budget exists
(`planned/0002_the_0_3_breaking_budget.md`, Accepted-pending-maintainer) and
the semver CI gate enforces additive-only between windows.

### v0.2 — Content era — **COMPLETE (field-validated)**

**General need: apps whose content grows and moves.** Delivered 2026-07-21/22:
**0100** Feed (keyed, windowed, streaming items), **0110** `md::StreamSession`
(open-block-only re-parse), **0120** TextArea + anchored completion, **0130**
`follow_tail` + measured extent, and the first-app defect wave
(0220/0230/0240/0250 fixed; 0270 shipped selection + OSC 52 copy, which also
delivered 0150's clipboard leg — the notify/bell/title legs remain on the
planned ledger).

- Done bar met and measured: a streamed token costs one open-block re-typeset
  (median 73 B/frame vs a 9,670 B full paint — 0.8 %, `reviews/study2/
  quality-perf.md`); the composer is engine-supplied; follow-tail is one
  verb.
- **Field-validated by the abstractcode-tui migration**: the consumer deleted
  its hand-rolled transcript column, height math, autoscroll effects, and
  focus bookkeeping, and its module doc now calls the transcript "a
  PROJECTION" (`reviews/study2/field-consumer-tensions.md` §1 — the adoption
  scorecard: everything shipped as a widget with owned state was adopted
  wholesale). The 0200 console phase-1 fixture leg was superseded by this
  real consumer; it remains available as a second validator.
- What the migration's residue earned: the Interaction-depth era's Feed-block
  family below (0102/0104/0280), the disposal law (0297), and the modal
  ergonomics findings (content sizing, slot semantics — §3.2/§3.3, not yet
  itemized; they ride the app-kits stacked-dialog story).

### v0.2 → v0.3 — Live-data era — **foundation COMPLETE; lifecycle half open**

**General need: apps fed by the world.** The foundation shipped 2026-07-21:
**0010** source→signal binding, **0020** bounded/coalescing ingestion,
**0030** the documented pattern (`examples/feed.rs` + `docs/live-data.md`),
**0070** `reactive::interval`.

- Lifecycle (evidence-gated): **0040** connection model + jittered backoff —
  **its promotion trigger has fired** (dated evidence section in-item,
  2026-07-22): the first consumer hand-rolled SSE + reconnect/backoff
  WITHOUT jitter (`field-consumer-tensions.md` §3.5), and entity-class
  monitors multiply the cost (`field-app-classes.md` class 4, "this study's
  strongest cross-track recommendation"). Promote at the next single-writer
  pass; declare the API surface stable only after a real disconnect cycle.
  **0050** the transport ADR stays gated on watcher/port evidence — never
  the armchair.
- One field note to fold into docs (convergence hand-off): the consumer
  correctly REFUSED `channel_source`/`bounded_source` for ordered state
  deltas (a dropped record is corruption) and used posted fold closures —
  `docs/live-data.md` should bless that third shape explicitly.

### v0.3 — Depth era — **IN PROGRESS**

**General need: apps where users read closely and act on what they see.**

- Shipped: selection v1 (**0270**: bypass docs, mouse-capture suspend verb,
  screen-text selection + OSC 52 — 0160's screen-level tier) and the diff
  lexer (**0140**'s line-oriented slice, `text::DiffLexer` — the consumer
  gets diff tinting with zero app code).
- Open: **0140-stateful** (cross-line python/js/toml lexers; the
  stateful-seam design note gates python), **0160-logical** (widget-content
  mapping: copy markdown source, unwrap soft-wraps — shares the text↔cells
  substrate with 0148 and 0630), **0165** hyperlink/reference hit-testing
  (0480's `register_link` seam may merge into it).
- Validation vehicle: console 0200 phases 3–4 and chat 0210 (copy message,
  open link) — unchanged.

### v1.0 — Trust era — **gates SHIPPED; audit + scheduled jobs open**

**General need: apps that bet years on the engine.**

- Shipped 2026-07-22: MSRV 1.87 declared; **semver / msrv / live-pty CI
  gates** wired (enforcement caught a live mid-enum `Role::Select` insertion
  on day one via a local semver-checks run — `planned/0002` records the
  catch and its resolution); ADRs 0001–0003
  landed; `Capabilities`/`GraphicsCaps` are `#[non_exhaustive]`; the 0.3
  breaking budget is WRITTEN before its window (0002,
  Accepted-pending-maintainer) — the honest repair after 0.2.0 shipped
  without a list.
- Open: **scheduled perf/fuzz/soak** (0180's remaining leg — see risk #1 in
  `reviews/study2/ACTION-PLAN.md`: both perf suites are explicitly-run
  today, so an emission regression is invisible until someone runs them by
  hand), the **Windows interactive session** (operator act, never proven
  end-to-end), the full 0170 audit (prelude criteria, public-api gate), and
  the external-consumer feedback loop before any 1.0 freeze
  (abstractcode-tui is in-family; 1.0 still requires a consumer not written
  by the engine's authors).

### Media/AV era (band 0600s) — **NEW: image truth fixed; voice plumbing itemized**

**General need: apps that show real pixels and speak.** Every class grows a
media surface eventually — chat feeds carry image attachments, monitors and
assistants need level meters, readers want inline images, voice apps need
push-to-talk and karaoke highlights. The boundary ruling
(`reviews/study2/media-voice-plumbing.md` §2): **the engine never does audio
I/O, HTTP, codecs, VAD, or synthesis** — it owes the UI primitives only.

- **The image lifecycle is now trustworthy.** The maintainer's doubt ("I am
  unsure the 'view image' part ever worked") was well-founded: the protocol
  emitters were byte-correct, but the lifecycle around them carried five bug
  classes, all fixed with tests (`reviews/study2/media-images-truth.md`,
  adversarially re-reviewed in `quality-on-media.md`; shipping in the 0.2.2
  patch): (1) kitty move ghosts — pid-less `a=p` accumulates placements,
  fixed with placement id `p=1`; (2) mosaic blit double-offset + vacated-cell
  corpses — blit origin zeroed, `Driver::pre_image_pass` repairs vacated
  rects; (3) iTerm2/sixel corpses invisible to the diff — prev-poison forces
  re-emission; (4) tmux's 1 MiB single-sequence discard — per-escape
  wrapping; (5) scroll-shift desync with live byte-channel images — plain-diff
  guard (measured 10.2× scroll bytes; the byte-win restore is 0675). Silent
  ladder degradations now surface as notices, and the review's own find
  (parked mosaic decay under beneath-repaints) is fixed with a
  pre-fix-failing test. Remaining honest gap: kitty/iTerm2/sixel end-to-end
  on real terminals is maintainer-verifiable in 10 lines (recipe in the
  report).
- Items: **voice plumbing 0610–0650** (push-to-talk over 0700's key state;
  `Meter`/`AudioScope` with ballistics and an idle-fixpoint acceptance test;
  speaking highlight over the shared text↔cells mapping; the
  external-process lifecycle as a verified docs pattern — data plumbing is
  covered, process lifetime is ~12 lines of app code by design;
  `examples/voice-mock.rs` as the no-audio validation vehicle) and
  **images-in-content 0660s** (0660 images in Feed/Markdown, 0665 animated
  sessions, 0670 cell-size refresh, 0675 scroll re-place, 0680 sixel bottom
  row, 0685 probed-caps signal — converging with first-app 0295, 0688
  detection/transport robustness).
- Done means: a feed can carry image attachments without silent corruption on
  any channel; a voice app builds meters, PTT, and highlights from engine
  primitives with zero audio code in the engine; a silent meter is an idle
  loop; every degradation is labeled.
- Validation vehicles: `examples/voice-mock.rs` (0650), the images example's
  maintainer recipe on real terminals, the first feed-with-attachments
  consumer.

### Interaction-depth era (band 0700s + the Feed-block family) — **NEW**

**General need: apps that treat input as physical facts and feed lines as
structured content.** Two halves, both earned by study-2 field evidence.

- **The key-state chain 0293 → 0700 → 0610** (convergence cycle 2, wired in
  all three items): first-app **0293** — kitty enter-flags never follow the
  probe, so Shift+Enter (and release visibility) is dead on iTerm2/VS Code/
  Warp, exactly the terminals that DO speak the protocol; it ships in the
  0.2.2 patch and is the fidelity prerequisite. **games/0700** owns the
  primitive: held keys as a first-class input fact (down-set, edge
  callbacks, capability-honest fidelity flag, legacy repeat-timeout
  degradation, FocusLost hygiene). **media-av/0610** consumes it (push-to-
  talk with latch fallback). One primitive, two app classes — the
  general-needs-first law working as designed.
- **Games toolkit 0710–0730** (`reviews/study2/field-games.md`): the verdict
  is honest — roguelike/RPG buildable TODAY, real-time action blocked on
  0700, hex tactics taxed by hand-rolled grid math. **0710** public frame
  tasks + fixed-timestep helper (the pacing already exists; the sanctioned
  lane is private), **0720** sprite/tile toolkit (masked blit, sheets,
  palette swap), **0730** board-grid math (square + hex, aspect-corrected —
  placement routes through 0400's classification). Audio defers to media-av;
  saves to control-plane 0340; strokes to extensions 0420.
- **The Feed-block family 0102/0104/0280/0660**: the consumer's #1 tension
  (`field-consumer-tensions.md` §4.1) is **0102 `FeedBlock::Rich`** — a
  ~137-line Card subsystem exists only because feed lines cannot carry
  spans, while the engine already owns `RichText` ("one renderer, three
  faces" — Feed is the missing fourth). **0104 `FeedState::sync`** deletes
  the ~180-line fingerprint/mirror sync machinery every fold-shaped consumer
  will re-implement slightly wrong (§3.6). 0280 (widget-hosting blocks) and
  0660 (image blocks) press on the same enum: **one block-vocabulary design
  pass, first-to-execute owns it, the enum grows once.**
- Done means: move-while-held with clean stop on protocol terminals and
  honest degradation elsewhere; a game tick is one helper with real dt; feed
  lines mix inks without custom blocks; a slice-of-truth feed syncs without
  app-side fingerprint machinery.
- Validation vehicles: the first real-time game example; 0650's PTT leg; the
  abstractcode-tui deletion list (Card ~137 lines, wire_feed ~180 lines, the
  retire deferral).

## Validation vehicles (dogfooding)

| Vehicle | Proves | Status |
| --- | --- | --- |
| abstractcode-tui (shipped) | Content era field-validated; the standing tension reporter (six upstream reports + 0297/0298 earned this cycle) | **Migration DONE 2026-07-22**; next deletion list: Card (0102), sync machinery (0104), retire deferral (0297), reconnect loop (0040) |
| 0060 watcher | Live-data era under a real network: 0010/0020/0040 carrying real traffic; produces 0050's evidence | Proposed; explicitly not-now (maintainer-gated) |
| 0200 coding console | Content era + streaming + the subprocess lane | Proposed epic; widget deps DONE — unblocked on that side |
| 0210 chat TUI | Both eras end-to-end incl. 0040/0050 + 0150; phase 1 adopts the 0060 shape | Proposed epic; widget deps DONE; lifecycle 0040/0050 remain — **the named second-consumer candidate** |
| voice-mock example (0650) | Media/AV era: 0610/0620/0640 (0630's leg follows it) with no audio, no network | Proposed; the band's validation vehicle |
| first real-time game example | Interaction-depth era: 0700/0710/0720 under a real tick | Not yet filed as an example; rides 0700's landing |
| 0590 reference validators | app-kits band slices (admin console, wizard, triage shell) | Proposed; grows with each app-kits landing |

## Sequencing that must not be violated

- **The 0.3 breaking budget (0002) gates every breaking shape**: `Role`/
  `TokenKind` `non_exhaustive`, `content_size` deprecation, 0570's Tree
  variants — batched into the one 0.3 window on maintainer sign-off; the
  semver CI job enforces additive-only meanwhile. (The 0170-before-0100/0130
  edge is discharged: the Content era shipped additively; what remains
  breaking lives in 0002.)
- **The key-state chain is ordered**: 0293 (wire enablement, in the 0.2.2
  patch) → 0700 (the service; lands independently but runs
  repeat-approximated on iTerm2/VS Code/Warp until 0293) → 0610 (consumes
  0700, adds no key machinery of its own).
- **The Feed-block family is one design pass**: 0102 (`Rich`), 0280
  (widgets), 0660 (images) extend the same `FeedBlock` enum — first-to-
  execute owns the vocabulary, the other two review, the enum grows once.
  0104 rides the same review (its sync adapter shapes the item model's
  identity story).
- **Live-data stays one-directional**: 0040 promotes on the filed evidence;
  its API is declared stable only after a real disconnect cycle (0060 or
  0210 phase 1); **0060 before 0050 closes** — the transport ADR waits on
  real experience.
- **0300 before everything in its band** (0310/0320/0340/0350 consume the
  lifecycle surface); **0420 before 0430/0440/0450**, **0440 before 0430**
  (extensions); **0340 ↔ 0520** (the wizard is the persist registry's first
  consumer). The full cross-band edge list lives in
  `docs/backlog/overview.md`.
- **0295 and 0685 are one accessor** (probed-capabilities signal) — converge
  at design time, build once.
- Ports start only when their dependency lists land; whichever of 0060 /
  0210-phase-1 lands first, the other adopts it. 0180's scheduled jobs are
  independent and may land any time; they must land before 1.0.

## The next wave (cycle-3 recommendation, 2026-07-22)

Argued from consumer evidence — the abstractcode-tui tension report is the
strongest signal this roadmap has: a real app, a well-executed adoption, and
an honest residue list. Ranked:

1. **The 0.2.2 patch (shipping now)**: the image-lifecycle fixes (the five
   bug classes above) + first-app 0290/0293/0295/0296/0298. 0290 has NO
   app-side workaround (the selection layer eats `c`/Enter before dispatch);
   0293 is the key-state chain's prerequisite and turns Shift+Enter on for
   the majority macOS terminals with zero app changes.
2. **`FeedBlock::Rich` (0102) + `FeedState::sync` (0104)**: the consumer's
   #1 tension and its §3.6 twin — additive, engine-owned models already
   exist, and they unblock the log/chat/entity app classes
   (`field-app-classes.md` classes 3 and 5). Run the block-vocabulary pass
   with 0280/0660 in the room.
3. **0040 promotion (jittered reconnect)**: the trigger fired with two
   studies' evidence; every networked consumer between now and the helper
   hand-rolls a divergent, jitterless copy.
4. **0297 disposal law engine-wide**: tension #2 — Button's post-callback
   write forces a one-tick retire deferral in every modal-closing consumer;
   acceptance is the consumer deleting it.
5. **0700 key-state**: the games+voice shared primitive, unblocked by 0293;
   real-time games stay blocked until it lands.
6. **The 0.3 budget execution** when the maintainer signs 0002.

The full three-horizon plan with efforts and validation targets is
`reviews/study2/ACTION-PLAN.md`.

## What "best in class" means, measured

For each general need, the measure is **deletion**: the app-side machinery an
author must write for a class's defining surfaces approaches zero, verified
concretely by the dogfood builds. **The first deletion receipts are in**: the
abstractcode-tui migration deleted its hand-rolled transcript, height math,
autoscroll effects, and focus bookkeeping; the next named deletions are the
Card system (0102), the sync machinery (0104), the retire deferral (0297),
and the reconnect loop (0040). And the measure is **preservation**: every
addition keeps the audited invariants — zero-idle-cost (now pinned with a
parked protocol image mounted), damage-proportional repaint, labeled
degradation, the five-crate footprint, claims never ahead of evidence.
