# Convergence cycle 2 (2026-07-22) — games-band adversarial review + cross-band folds

Single-writer convergence pass over the cycle-1 study output (media-av
0610–0688, games 0700–0730, the three FIELD/MEDIA reports) plus the four
fresh first-app filings (0293/0295/0296/0298). Every verdict below was
checked AT SOURCE (file:line read this pass, engine HEAD of 2026-07-22);
"fixed in item" means the item file was edited this cycle. No code was
touched — items/docs/ledger only.

---

## 1. Games band adversarial review (4 items + README)

Method: every engine citation opened and read; claims checked against the
cited lines, not against the filing report. Verdict per item, findings
ranked by severity.

### 0700 (key press/release state) — VERDICT: sound design, ONE WRONG
citation + one over-claim, both load-bearing. FIXED in item.

- **WRONG (fixed)**: "wired as the full-screen default at
  options.rs:179". src/term/options.rs:179 is a TEST literal (the
  `enter_and_leave_mirror_each_other` fixture constructs
  `EnterOptions { kitty_keyboard: KittyFlags::standard(), .. }`).
  `EnterOptions::default()` actually carries `KittyFlags(0)`
  (options.rs:114). The real wiring is `Driver::new`
  (src/app/driver.rs:163-171): standard flags only when ENV detection
  claims the protocol.
- **OVER-CLAIM (fixed)**: "the engine REQUESTS release visibility on
  every kitty-capable terminal". The env claim covers
  kitty/WezTerm/Ghostty/foot only (src/term/caps.rs:235). Probe-PROVEN
  terminals (iTerm2 ≥ 3.5, VS Code/Cursor, Warp — `on_reply` flips
  `caps.kitty_keyboard = true`, src/term/probe.rs:132-139) never get
  the flags push, because nothing re-emits enter flags after the probe:
  `apply_caps_upgrade` (driver.rs:705-721) refreshes PRESENTATION state
  only (poison prev, damage layers, dirty images) — it writes no bytes
  to the terminal and never touches kitty keyboard flags. This is
  exactly first-app/0293's finding; the two items had not been joined.
  0700 now names 0293 as its fidelity prerequisite (see §2).
- VERIFIED correct: options.rs:54-73 (`KittyFlags::standard()` =
  DISAMBIGUATE|REPORT_EVENT_TYPES); input/mod.rs:227-239 (three kinds);
  kitty.rs:139-152 release decode tests (item said 144-151 — within the
  test, fine); is_down at input/mod.rs:349-353; the routing drop at
  events.rs:80-82 with the documented-drops list at events.rs:70; the
  kindless `ui::KeyEvent` construction at events.rs:83-87; lock
  stripping events.rs:17-32; legacy press-only input/mod.rs:227-229;
  shortcut-release rule pinned by the test at input/mod.rs:503-514;
  FocusGained/FocusLost arriving (and being dropped) at
  events.rs:120-124 — consistent with the item's design placing the
  key-state tap PRE-conversion; grep confirmed zero key-state tracking
  in src/ui/ + src/app/.

### 0710 (game tick / fixed timestep) — VERDICT: strongest item of the
band; every architectural claim true; TWO imprecise line ranges. FIXED.

- VERIFIED: pacing at src/app/mod.rs:363-377 (FRAME_INTERVAL = 16 ms,
  only while `frame_tasks_pending() > 0`) and the zero-idle block at
  378-389 — exact. `register_frame_task` PRIVATE with the "Internal to
  the reactive layer; `animate` is the public consumer" comment at
  src/reactive/animate.rs:107-111 — exact. `request_frame` public
  (src/reactive/mod.rs:72-75) — exact. `run_frame_tasks` at
  animate.rs:113-130 — exact. interval's fixed-delay/coalescing
  contract + "period is a MINIMUM… reads its own clock inside `f`" at
  src/reactive/interval.rs:13-20 — exact. `ParticleField::step(dt)`
  "fixed-timestep-friendly" (particles.rs:5-6, step at 130-146).
  `anim::Clock` real/virtual (anim/mod.rs:63-118). `set_shader_t`
  (overlays.rs:626-631). The frame-aligned-Instant rule the helper must
  follow (animate.rs:30-36, the `Flight.started` doc). Shader
  determinism contract (shaders.rs:8-13). The effects example's pause
  key demonstrating idle-restoration (effects.rs:9-12).
- **IMPRECISE (fixed)**: (a) "run_frame_tasks runs every turn in phase
  U (driver.rs:260-271)" — the pump is at driver.rs:267-279
  (`run_due_timers` 274, `run_frame_tasks` 278); 260-271 spans `caps()`
  and the turn doc comment. (b) "advances time by `clock_ms +=
  FRAME.as_millis()` (effects.rs:27, 88-90)" — the `+=` is at
  effects.rs:86; 88-90 are the destructure/`set_shader_t` lines. Both
  ranges corrected in the item (the same drift exists in
  field-games.md §1 — left standing there, reports are historical
  records; this file is the correction of record).

### 0720 (sprite/tile toolkit) — VERDICT: claims all true at source; one
citation pointed at the constructor instead of the carried state. FIXED.

- VERIFIED: `Surface::blit` copies EVERY source cell — the per-cell
  loop at src/render/surface.rs:439-447 has no transparency/empty test
  (adopt + assign unconditionally), with clipping (426-437), pair
  repair (448-457), and the damage rule at 458 — the item's
  "surface.rs:439-458 is the template" reading is exact. Cross-layer
  transparency under `Blend::Normal` (`Glyph::EMPTY` see-through) is
  the compositor module doc's blending model (compositor.rs:11-19).
  Layer verb set at overlays.rs:602-641 (set_offset 602, transform
  618-620, shader clock 626-631). Mosaic least-squares fit
  (mosaic_fit.rs:5-14). Alpha-0 passthrough rule (shaders.rs:15-18).
  Particle aspect correction (particles.rs:120-122). Blit test corpus
  at surface_tests.rs:146+ (`blit_clips_and_repairs_pairs`,
  `blit_adopts_pool_and_links`). Logo + `three::brandmark` exist
  (src/widgets/logo.rs, src/three/brandmark.rs).
- **LOOSE (fixed)**: "each `LayerHandle` carries its own surface,
  damage tracking, and z entry (overlays.rs:158-166)" — those lines are
  the `layer`/`layer_draw` CONSTRUCTORS; the carried state is
  `render::layer::Layer` (surface, origin, z, opacity, blend,
  transform, shader, frame_damage — layer.rs:160-172). Item now cites
  both ends. The heavyweight-per-sprite argument itself stands.

### 0730 (board-grid math) — VERDICT: citations exact; ONE architecture
gap — the item presumed a core home without running the 0400 table.
FIXED (scoping, not placement).

- VERIFIED: the only Bresenham is `BrailleGrid::line` at
  src/widgets/chart.rs:82-105 (private, dot-space) — exact to the line.
  `src/widgets/grid.rs` is a LAYOUT container (true, unrelated).
  Aspect-correction precedent: particles.rs:120-122 (velocity), 138
  (gravity `* 0.5`), burst roundness 105-108; the CONSUMER really does
  re-derive `cols / (2*rows)` (abstractcode-tui
  src/ui/transcript_view.rs:222-230, read this pass — "a cell is ~1
  wide x 2 tall, so displayed aspect = cols / (2*rows)"). Blit/offset
  cites exact. No-libm discipline (shaders.rs:8-13).
- **SCOPING GAP (fixed)**: the ADR section asserted "one new module,
  e.g. `base::grid`" — a CORE placement — without engaging extensions
  0400, whose decision table exists precisely for this question ("does
  a minimal app pay for it in-tree, and does it have its own release
  cadence? both = sibling; cost-but-no-cadence = feature; neither =
  core"). The two live precedents pull opposite ways: 0420's dot canvas
  went CORE because "a minimal app drawing a sparkline already contains
  most of it" — an argument that does NOT transfer (zero grid code
  exists; no minimal app contains hex math); 0440 keeps its pure layout
  math (`GraphDesc -> Layout`) in the SIBLING crate `abstracttui-graph`
  — pure math rides its domain's crate there. Honest counter-arguments
  for core: a few hundred lines of dependency-free integer math,
  dashboard-class consumers (map viewers, seat plans), no independent
  cadence, and no games sibling crate exists to ride. RESOLUTION
  APPLIED: 0730's ADR section now routes placement through 0400's
  classification explicitly (promotes only with a recorded
  classification; both precedents argued in-item). The games README
  sequencing carries the same note. This review deliberately does NOT
  pre-decide the answer — that is 0400's ruling to make.

### games/README.md — VERDICT: accurate table + non-items; sequencing
updated (0700's 0293 chain; 0730's 0400 routing; overview-fold note
marked DONE). The "audio → MEDIA band" and "saves → 0340" deferrals
check out against both bands (bell/notify verified at
src/term/mod.rs:254-259 + verbs.rs ladder; 0340 exists in
control-plane).

---

## 2. The key-state chain: 0293 → 0700 → 0610 (WIRED)

Finding: three items in three bands described one subsystem without a
dependency edge among the first two. Verified at source that 0293's
claim is REAL: the enter-time push happens once in `Driver::new` from
env caps (driver.rs:163-171); the probe proves the protocol later
(`on_reply` → `caps.kitty_keyboard = true`, probe.rs:132-139);
`apply_caps_upgrade` (driver.rs:705-721) is presentation-only — it
emits no terminal bytes and no kitty flags push exists anywhere on the
post-probe path. Consequence chain, now recorded in all three items:

1. **first-app/0293** — enable flags post-probe (and gate the WezTerm
   env claim on probe evidence — the inverse defect, caps.rs:235 vs
   WezTerm's default-off config). Added: a "Downstream consumers"
   section naming the chain, and the exit-restore note (a probe-time
   push needs its own pop bookkeeping — `leave_bytes` emits `CSI < u`
   only when ENTER pushed, options.rs:149-151). Citation corrected
   (driver.rs:158-166 → 163-171).
2. **games/0700** — OWNS the primitive (down-set service + edge
   callbacks + capability-honest fidelity flag + legacy repeat-timeout
   degradation + FocusLost hygiene + opt-in release routing). Now
   names 0293 as fidelity prerequisite: without it the service runs
   repeat-approximated on the majority macOS terminals — exactly where
   the protocol exists.
3. **media-av/0610** — CONSUMES 0700 (already did; unchanged design —
   it adds no second key-state machinery, only the PTT policy: latch
   fallback, Stop(FocusLost), mode labeling). Now carries the full
   chain note + the env-claimed-vs-probe-proven precision in its code
   reality.

Division of labor is clean and stands: 0293 = wire enablement,
0700 = engine service + honesty, 0610 = app-facing voice policy.

---

## 3. Feed-block family (CONVERGED)

FIELD's consumer report names `FeedBlock::Rich` the single
highest-leverage addition (field-consumer-tensions.md §4.1, tension
#1); media-av/0660 wants image blocks in Feed; first-app/0280 wants
widget-hosting blocks. All three press on the same enum
(`FeedBlock`: Text/Markdown/Code/Custom, feed.rs:74-94 — verified).
Actions:

- **Filed app-widgets/0102 (`FeedBlock::Rich`)** — no existing item
  covered it (checked proposed/app-widgets/: 0140-0190 ids only, all
  mdpad/stability seeds). Engine side verified: `render::rich::
  RichText` + `RichTextView`'s shared span walk ("one renderer, three
  faces", richtext.rs:1-20) — Feed is the missing fourth face. Filed
  in app-widgets per the convergence instruction; noted in-item that
  field-app-classes suggested first-app-next-to-0280 as an alternate
  home — the 0660 cross-ref makes the three-way design pass explicit
  either way.
- **0660 cross-ref updated**: Rich (0102) and images (0660) are
  siblings; 0280 is the third pressure; one design pass settles the
  block vocabulary, first-to-execute owns it, the enum grows once.

---

## 4. FIELD's earned items — filed where absent (checked first)

Checked proposed/app-widgets/ + proposed/first-app/ for prior filings;
none existed. Filed:

| ID | Title | Band | Evidence base |
| --- | --- | --- | --- |
| 0102 | `FeedBlock::Rich` span-model feed lines | app-widgets | consumer-tensions §4.1 (Card system ~137 lines, transcript_view.rs:41-177); engine feed.rs:74-94 + richtext.rs:1-20 verified |
| 0104 | `FeedState::sync` slice-diff adapter | app-widgets | consumer-tensions §3.6 (~180 lines: wire_feed 502-584, FNV fingerprint 389-483, mirror predicate 591-599 + byte-exactness test 726-784); engine push/update at feed.rs:190-236 verified |
| 0190 | Time-axis charts + history windows (`TimeSeries`) | app-widgets | field-app-classes class 5 (the named NEEDS-ITEM, "0190 is free"); verified: no time-axis code in chart.rs, `finite_range` value-only (chart.rs:117-121), dashboard hand-rolled ring (examples/dashboard/main.rs:40-42 — report said 42-43, actual 40-42), gap contract chart.rs:20 |
| 0297 | Disposal safety engine-wide (Button et al.) | first-app | consumer-tensions §3.1 (UiCtx::retire one-tick deferral, consumer ui/mod.rs:67-94, read this pass); Button offender VERIFIED (`fire(); pressed.set(false)`, button.rs:193-197); Checkbox/Tabs/Radio audited CLEAN this cycle (set-before-callback: checkbox.rs:96-102, tabs.rs:102-103, radio.rs:108-109) — the remaining audit is the item's deliverable |

Also per FIELD's hand-off: **0040 promotion evidence added** — a dated
section on proposed/live-data/0040 recording that the first consumer
hand-rolled SSE + reconnect/backoff/poll-fallback WITHOUT jitter
(gateway/sse.rs 125 lines; runner.rs:923-1012, linear 500ms×n cap 5s —
the item's named thundering-herd risk verbatim), that field-app-classes
class 4 multiplies the cost (entity monitors: per-entity streams), and
recommending promotion to planned/ at the next single-writer pass
(0050 stays evidence-gated; API-stability still validates against a
real disconnect cycle).

NOT filed, deliberately (FIELD's own scoping honored): the radial gauge
(waits on 0420's public dot canvas — filing now inverts the
dependency); the alert-state recipe (a docs/validator-journey
paragraph, not an item); the route-editor default-vs-override fold-in
(belongs INSIDE 0510/0590's text — an integrator edit to app-kits items
this pass did not own; recorded here as an open hand-off); the
fold-closure blessing in docs/live-data.md (a docs edit, not an item —
open hand-off #2); the embedded CheckList widget (consumer-tensions
§3.4 — REAL, but app-kits-band territory adjacent to 0550/0530; left
to the app-kits owner with this pointer rather than cross-band-filed;
open hand-off #3).

---

## 5. Ledger fold (single-writer, this pass)

overview.md updated: games track row added (media-av's already
existed); games 0700/0710/0720/0730 rows added; first-app
0293/0295/0296/0297/0298 rows added; new app-widgets rows 0102/0104/
0190 added; counts recomputed FROM THE FILESYSTEM (planned 4 /
proposed 67 / completed 14 — 63 pre-existing proposed files + the 4
filed this pass); band note extended (media-av 0600–0690, games
0700–0790; first-app parenthetical corrected to 0220–0298). The 0040
row now carries the promotion recommendation. Cross-track sequencing
gained the 0293→0700→0610 chain and the Feed-block family edge.

## 6. Standing corrections of record

- field-games.md carries the same three citation drifts fixed in the
  items (options.rs:179 test-literal as "wired default";
  driver.rs:260-271 for the pump; effects.rs:88-90 for the `+=`).
  Reports are period documents — corrections live in the items + here.
- field-games.md's "the engine already REQUESTS release visibility"
  framing (§2) inherits the env-claimed-only precision from §1/§2
  above.
- field-app-classes.md's dashboard citation (main.rs:42-43) is off by
  one (40-42); the 0190 item cites the verified lines.
