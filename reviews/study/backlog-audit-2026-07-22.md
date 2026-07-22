# Backlog audit — 2026-07-22 (post-0.2.0 adversarial pass)

Auditor posture: skeptical staff engineer; every premise checked against the
tree, the git history, the published release, and the sibling consumer.
Findings only — no backlog file was edited (the integrator applies).

## Verified ground truth (the premises, checked)

- **0.2.0 is real and released today**: `Cargo.toml:3` (`version = "0.2.0"`),
  `CHANGELOG.md:8` (`## [0.2.0] - 2026-07-22`), local tag `v0.2.0`
  (commit c793c1f), GitHub release published `2026-07-22T04:01:05Z`
  (`gh release view v0.2.0`, isDraft:false). The release workflow's
  `github-release` job `needs: publish` (`.github/workflows/release.yml`),
  so a published GH release implies the crates.io publish succeeded.
- **The only DECLARED breaking change in 0.2.0**: `term::Capabilities` +
  `GraphicsCaps` `#[non_exhaustive]` (CHANGELOG.md:102-106). One more
  technically-breaking change shipped UNDECLARED: `Role::TextArea` added to
  the public exhaustive `Role` enum (src/ui/access.rs:31-51; CHANGELOG lists
  it under Added, no migration note). No live victim: the one consumer never
  matches `Role` (grep of ../abstractcode-tui: zero `Role::` references).
- **The consumer is mid-upgrade**: `../abstractcode-tui/Cargo.toml:26` already
  reads `abstracttui = "0.2.0"`. It is an **AbstractGateway HTTP/SSE client**
  (ureq + serde_json), NOT the 0200 `abstractcode serve` JSONL console —
  two different consumers; several backlog texts blur them (see C7, S10).
- **Suite size**: 1,393 `#[test]` functions statically (`rg -c '#[test]'`
  across src/tests/benches); llms.txt:68 claims "~1,385 tests" — consistent.
- **Dependabot queue**: exactly 6 open PRs (`gh pr list`), all opened
  2026-07-21 ~11:30Z — i.e. BEFORE the release, so 0.2.0 shipped with the
  old pins (Cargo.lock: miniz_oxide 0.8.9, windows-sys 0.60.2, libc 0.2.186).
- **Backlog counts vs filesystem**: Planned 3 ✓, Completed 12 ✓, Proposed 41
  files ✓ — but the overview's Proposed LEDGER has 42 rows (see S4).

Severity scale: **P1** = actively misdirects the next unit of work;
**P2** = wrong but bounded (contradicted nearby, or historical); **P3** =
pointer rot / cosmetic.

---

## 1. Staleness findings

### S1 (P1) — The "0.2 budget batch" is dead; the window was spent today with only part of the batch

`overview.md:126-130` (Next-recommended #2) still queues "the 0.2 budget
batch" as future work: remaining 0170 audit + riders (`Role` variants or
`#[non_exhaustive]`, subtree focus step, `TextInput::masked`) + 0250's fix +
0180. The 0.2 breaking window closed at 04:01 UTC today. Rider-by-rider fate,
verified in source:

| Rider | Fate | Evidence |
| --- | --- | --- |
| `Overlays::top_z` | **SHIPPED** (additively, with 0120) | `src/app/overlays.rs:239` + `overlay_tests.rs::top_z_tracks_the_live_maximum` |
| `Role` non_exhaustive (or batched variants) | **MISSED** — `Role` is still a plain exhaustive pub enum, and 0.2.0 added `Role::TextArea` anyway (an undeclared break) | `src/ui/access.rs:30-31` (no attribute); CHANGELOG.md:23 |
| Subtree focus step | **NOT SHIPPED** — and it never needed the window: a new `ui::focus` API is additive | no match for any subtree-step API in src/ui/ (grep) |
| `TextInput::masked` | **NOT SHIPPED** — also additive (new builder method) | grep `mask` in src/widgets/input.rs: only the borrow-note comment at input.rs:305-310 |

The honest re-anchoring: only the **Role** rider was ever genuinely breaking;
it now seeds the **0.3** budget. The focus step and `masked` were misfiled
into a breaking batch (both additive under ADR-0001 §1) and can ship in any
0.2.x. The 0250 fix is additive by the item's own text (0250:56 "The fix is
additive API"). 0180 changes CI and wording, not API. Next-recommended #2
should dissolve into (a) an additive queue and (b) a written 0.3 budget.

### S2 (P1) — 0170's trigger names a vehicle that already departed

`overview.md:81`: "The remaining audit rides the 0.2 release prep." The 0.2
release happened WITHOUT the audit: 0170's checklist
(`proposed/app-widgets/0170_api_stability_pass.md:124-132`) still shows
`[ ] Public-surface audit`, `[ ] 0.2 breaking-change budget`, `[ ] Doctest
un-ignore sweep`, `[ ] public-api diff gate in CI`. The item's own status
line (0170:5-6, "should precede a 0.2") is likewise expired. Consequence per
ADR-0001 §2 ("Every intended break is written into a single budget list for
the release … `docs/backlog/` owns the list; item 0170 seeds the 0.2
budget"): **0.2.0 shipped breaking changes without the written budget list
the policy requires** (the CHANGELOG's Changed section carried the
non_exhaustive migration note, but `Role::TextArea` got none — a second,
smaller compliance miss). The remaining 0170 work re-anchors as: the 0.3
budget document + the `cargo public-api` CI gate, whose value is HIGHEST
right now — it is the mechanism that enforces additive-only between windows.

### S3 (P1) — 0570 is silently blocked until 0.3

`overview.md:113` and `proposed/app-kits/0570_tree_view.md:17-23`: "either
`Role` gains `#[non_exhaustive]` in the 0.2 budgeted batch … or these
variants ride that same batch. Named here so the item cannot land
'additively' by accident." That batch is spent and Role is still exhaustive
(S1). Adding `Role::Tree`/`TreeItem` — or adding `#[non_exhaustive]` itself —
is now a 0.3-window change. 0570 cannot land before a 0.3 without violating
its own guard. The same Role fix was wanted by control-plane serializers
(0570:21). The 0.3 budget should open with this line item.

### S4 (P1) — Overview Proposed ledger carries a completed item (0070)

`overview.md:77` lists 0070 (`reactive::interval`) in the **Proposed**
ledger; the same overview lists it in the **Completed** ledger
(`overview.md:60`) and the file lives at
`completed/live-data/0070_interval_time_source.md` (nothing under
proposed/live-data/ has that number). The ledger table has 42 rows against
41 proposed files; the Counts table (overview.md:24, "Proposed 41") is
correct — the stale row is the discrepancy. Delete the row.

### S5 (P1) — `planned/app-widgets/README.md` contradicts itself and the tree

- Status header: "Planned (not started)" — while four of its six listed
  items (0100/0110/0120/0130) are completed and moved to
  `completed/app-widgets/`.
- Its "## Items (planned/)" section still lists those four as residents of
  planned/ (only 0150 and 0180 remain there).
- Its own later text knows better: the Dependency-shape paragraph says
  "0120 (TextArea — completed 2026-07-22, now in
  `../../completed/app-widgets/`)" and the Reading order says "0100 → 0110 →
  0130 → 0120 (all four completed)". One file, three mutually inconsistent
  states.
- "Governing ADRs: None — this repository has no ADR system" — false since
  2026-07-21 (`docs/adr/0001..0003`).
- "…the API-stability and platform-claim honesty work that must land before
  a 0.2 that external applications can trust" — 0.2 shipped; the sentence
  now reads as an unmet precondition asserted in the past tense.

### S6 (P2) — `proposed/live-data/README.md` item rows label completed work "Planned" at dead paths

Rows for 0010/0020/0030 point at `../../planned/live-data/…` (files moved to
`completed/live-data/`) and carry bold "**Planned**" status labels, while
the same file's Status paragraph correctly says the foundation is COMPLETED.
Also "no ADR system yet" (stale). The sibling `planned/live-data/README.md`
is accurate — use it as the model.

### S7 (P2) — Four completed items ship completion reports over fully unchecked checklists

`completed/live-data/0010` (4 unchecked), `0020` (5), `0030` (4), `0070` (5)
each carry a dated completion report AND a Progress checklist with zero boxes
ticked (grep `- [ ]` under completed/). Every box describes work the report
says happened. The eight app-widgets/first-app completed items are clean —
this is specifically the 2026-07-21 live-data wave's hygiene gap. (Task
hint (c) confirmed; no sibling note actually contradicts a ledger row beyond
this.)

### S8 (P2) — "no ADR system in this repo yet" persists in 33 files

Grep count: 33 backlog files carry the phrase (34 hits), all false since
2026-07-21. In completed items it is a harmless historical record; in the
~20 OPEN items it misstates the governing policy — ADR-0001 (stability),
ADR-0002 (two-Style), ADR-0003 (struct extensibility) now govern every
public-surface decision, and the tree is inconsistent about it: 0400, 0410,
0480 (written later in the day) cite the ADRs correctly while their siblings
deny the system exists. Bulk fix at next touch of each item; the ADR-status
section is the first thing a scheduler reads.

### S9 (P2) — Overview preamble still describes the 0.1.0 world

`overview.md:3-9`: "published as `abstracttui` 0.1.0" (it is 0.2.0) and the
organizing observation "…its text input is single-line" (TextArea shipped
in 0.2.0; the single-line critique — the "bluntest line" per 0120:14-15 —
is retired). The async/network half of the observation survives (transport
is still absent; 0040/0050 open). The preamble is the first paragraph every
future agent reads; it should be re-anchored to the post-0.2 state.

### S10 (P2) — 0200/ports framing: "blocked on dependencies" is no longer true, and "the widget items lack the consumer" is false

- `proposed/ports/README.md:4`: "both epics blocked on dependencies; neither
  started." 0200's Phase-0 list (0200:73-91: 0100/0110/0120/0130 +
  live-data 0010/0020/0030) is fully landed — the overview's own trigger
  column says so (`overview.md:82`: "widget deps complete"). 0200's
  checklist line 143 ("Phase 0: dependencies confirmed landed") is
  satisfiable today and unchecked.
- `0200:66-67`: "Without this epic the engine's app-readiness claims stay
  theoretical, and the widget items lack the consumer that proves their
  shapes" — stale: abstractcode-tui (shipped 2026-07-21, upgrading to 0.2.0
  now) proves app-readiness and consumes Feed/TextArea/completion/selection.
  0200 remains valuable as the SECOND consumer and the `serve`-protocol
  validator, but its Problem statement overstates its necessity.
- 0210 is genuinely still blocked (0040/0050), correctly stated.

### S11 (P2) — Roadmap band model vs the actual release; a "must not be violated" line was violated

`planned/0001_roadmap.md:60-62`: "Version numbers are capability bands, not
dates: a band ships when its 'done' bar is met." The v0.2 Content-era band
(lines 64-93) gates on 0150 (only its clipboard leg landed — 0150:9-21) and
on "the three first-app workarounds are deleted from the field" (the
migration is happening today, not done) — yet **0.2.0 the release** shipped.
Line 84 ("their public shapes merge only after the 0.2 breaking budget is
written") and line 179 ("0170's rulings before 0100/0130 public shapes merge
(one budgeted 0.2)") were half-honored: the ADR rulings did land first
(2026-07-21), the written budget never existed (S2). The roadmap needs one
honest paragraph: 0.2.0-the-release ≠ Content-era-band-complete; the band's
residue (0150 remainder, migration evidence) rolls forward, and the next
breaking window is 0.3.

### S12 (P3) — 0500's shipped-delta framing lags its own status note

`proposed/app-kits/0500:9-11,24` still say `Overlays::top_z` "merges under
0170's budget window" and checklist item 1 (line 297-298) says it "rides the
0.2 budget window" — it shipped 2026-07-22, additively, and never needed a
window. The spec header (line 140) says the substrate "lands in
`app::popups`" while it landed in `app::anchored` (the status note at
265-294 acknowledges the naming drift and is otherwise exemplary). Also the
promotion-trigger line (0500:16-17) attributes the `/model`//`/theme`
pickers to "the 0200 console" — they belong to abstractcode-tui (correctly
named at 0500:37-39); the two consumers are different programs (see C7).

### S13 (P3) — 0260's preferred path died when 0100 completed

`proposed/first-app/0260:63-66`: "Fold into 0100's design (preferred — the
item model should be born with collapse semantics), or promote standalone if
a second app needs it before 0100 starts." 0100 started AND completed; Feed
shipped with no collapse/disclosure support (grep `collaps|disclos|fold` in
src/widgets/feed.rs: zero hits). The overview row (line 85) was re-anchored
("0100 shipped — extend"); the item text was not. Also 0260:31 points at
"planned/app-widgets" for 0100 (moved). Note: a SECOND consumer still does
not exist — the trigger is not yet true (contrary to a tempting reading;
abstractcode-tui is the first and only consumer).

### S14 (P3) — Pointer rot cluster

- `proposed/extensions/README.md:123` — item-grammar pointer at
  `docs/backlog/planned/app-widgets/0120_…` (moved to completed/).
- `proposed/app-kits/README.md:9-12` — "The overview's counts/ledgers are
  NOT updated by this study … folding this band into overview.md is a named
  follow-up" — the fold happened (overview rows 0500-0590 + track row
  exist, overview.md:39,106-115). The follow-up note should close.
- `overview.md:46` + roadmap:76 — 0150's title still enumerates
  "notify/bell/title/clipboard"; the clipboard leg shipped via 0270
  (0150:9-21 records it; remaining scope is notify/bell/title).

### S15 (P2) — 0250's stated sequencing was breached, and its value peaked today

`proposed/first-app/0250:66-68`: "the activation event should exist before
0100's Feed widget ships (same interaction family)." Feed shipped without
it; no `on_activate` exists anywhere (grep src/widgets/: only `on_select`,
list.rs:139, table.rs:95). With the consumer live TODAY carrying the
documented workaround (0250:58-63: pickers bind no on_select + deferred
modal close), the fix is at maximum field-evidence value and is additive.
The overview's "Promotion trigger" cell for 0250 (line 84) is actually a fix
DIRECTIVE, not a trigger — the trigger condition (an active consumer whose
workaround the fix deletes) is satisfied.

---

## 2. Contradiction hunt (cross-track, post-wave)

### C1 (P1) — Three documents assume Feed selection-by-key; 0100's completion report deferred it

0100's completion report (completed/app-widgets/0100, "Deferred, still
honest" paragraph): "optional selection by key (item 6) — neither port needs
it for v1." But:

- `planned/0001_roadmap.md:183`: "0160 builds on 0100's selection-by-key."
- `proposed/app-widgets/0160:88-89` (layer 1, the command-copy recipe):
  "selected feed item (0100's selection-by-key) … → 0150's clipboard_copy."
- `proposed/first-app/0260:50-51`: the disclosure keyboard story "meets
  0100's selection model (j/k between items, Enter toggles)."

The capability is deferred-and-unowned: no backlog item owns "Feed item
selection." 0160's cheapest layer and 0260's keyboard story both dangle from
it. The integrator should either add it to 0260/0160's scope explicitly or
file it as the Feed extension it is — otherwise the first of those items to
start re-discovers the gap mid-build.

### C2 (P2) — 0410 vs overview: additive by its own analysis, breaking-window-gated by the ledger

`proposed/extensions/0410:11-15` (ADR status): "features must stay additive;
default-on gating is NOT a breaking change for default builds, and
`default-features = false` is a new, documented opt-in surface." The
overview row (`overview.md:98`) says "batch with the 0.2 window (0170)."
The item's analysis is correct under Cargo semver practice (introducing
default-on features preserves every existing build). Post-0.2, the overview
phrasing would needlessly park 0410 until 0.3; in truth it waits only on
0400's ADR + integrator Cargo.toml sign-off and could ship in a 0.2.x.

### C3 (P2) — 0148 and 0160 name different "shared substrates," and half of one now exists

`proposed/app-widgets/0148:24-25`: "The text↔cells mapping is the shared
substrate with 0160 selection; whichever lands first builds it and the other
consumes." 0160's 2026-07-22 scope note (0160:29-32) instead names 0270's
post-flatten RECOLOR patch as "the candidate to generalize when search
lands." Both are real, and they are different substrates: (a) the
recolor-a-region machinery — SHIPPED via 0270 (src/app/selection.rs paint
path); (b) the typeset-text↔cells mapping (source offsets → cell rects) —
unbuilt, still shared between 0148 and 0160's remaining logical-selection
scope. 0148 also specifies its highlight as "a style patch at draw time"
(0148:18-19), which conflicts with the shipped post-flatten approach; the
two need one design note before either proceeds. 0148 predates 0270 and has
no scope-sync note — it should get one mirroring 0160's.

### C4 (P2) — 0160's metadata contradicts its own scope note

`0160:5-6` Status: "needs a design ruling on the selection model's home —
per-widget vs a screen-level layer — before it is planned" — but the scope
note above it records that 0270 TOOK the screen-level side for v1. The open
ruling is now only about the logical/per-widget remainder. The checklist
(0160:131-136) is likewise all-unchecked while layer 3 v1 shipped, the
ruling was taken for v1, and layer 1's clipboard verb landed (partial).

### C5 (P2) — 0140's token-vocabulary options are no longer semver-equal

`proposed/app-widgets/0140:68-73` frames the diff-lexer vocabulary ruling as
theme-churn vs extra-mapping: "add `TokenKind::Inserted/Deleted/Meta`
(touches every theme + the contrast audit) vs. a separate `DiffKind`."
`TokenKind` is a public exhaustive enum (src/text/highlight.rs:22-37 — no
`#[non_exhaustive]`; only KeyEvent/MouseEvent/Capabilities/GraphicsCaps
carry the attribute crate-wide). Post-0.2, option (a) is a 0.3-window
breaking change; option (b) is additive and shippable now. This is exactly
the trap 0570 documented for `Role` ("cannot land additively by accident"),
unnamed in 0140 because it predates the ADRs. The ruling should either take
the DiffKind path or put TokenKind-non_exhaustive on the 0.3 budget beside
Role.

### C6 (P3) — 0165/0480: not a contradiction, but a decision the overview leaves dangling

`overview.md:105` "may merge into 0165"; 0480's Placement-decision section
(0480:17-36) argues standalone well (producer half works today via
terminal-side OSC 8; either half lands first) and explicitly offers the
merge ("this file merges into 0165 verbatim as its producer section — that
choice is theirs"). Two facts tip it to MERGE: (1) 0480 sits in the
extensions band (0400-0490) while specifying CORE render/canvas work — a
band anomaly it itself concedes (0480:27-29, "this track cannot author
inside 0100-0190"); (2) both halves are unscheduled and describe one
channel whose id/cap/URI semantics must not drift apart. Nothing is lost:
the Option-A spec is complete and merges verbatim. Recommendation in §4.

### C7 (P3) — Two different consumers blur under "the console"

`0500:16-17` ("the 0200 console's /model//theme pickers"), and 0200's
overall framing, invite conflating **abstractcode-tui** (gateway HTTP/SSE
client, shipped, upgrading now) with **0200** (future `abstractcode serve`
JSONL console, not started — ../abstractcoder remains a paused charter per
0200:57-63). Trigger evaluation depends on which consumer a sentence means:
0140's trigger names 0200, but the consumer rendering diffs/code today is
abstractcode-tui. Findings in this audit treat "a live console-class
consumer exists" as the trigger-satisfying fact and say which program.

### C8 (P3) — 0200's non-goal wording predates 0270

`0200:125-126` non-goal: "mouse text selection (command-copy first, per the
review's P1-6)" — written when selection didn't exist. Engine drag-selection
+ OSC 52 copy shipped (0270, `app::selection`). The port still needn't build
anything, but the sentence now under-sells a free capability; phase-4
"copy-message (OSC 52)" (0200:115-116) is one line over the shipped verb.

---

## 3. Dependabot queue — risk read per PR

Baselines verified in Cargo.lock (miniz_oxide 0.8.9, windows-sys 0.60.2,
libc 0.2.186); all six PRs opened 2026-07-21 ~11:30Z, before the release.

### PR #6 — miniz_oxide 0.8.9 → 0.9.1 (cargo)

**Posture: MERGE, as its own commit, full suite as the gate. Low risk after
inspection — but this is the one PR that deserved the inspection.**

- 0.x minor = semver-major-capable under Cargo, and the manifest pin is
  `miniz_oxide = "0.8"` (Cargo.toml:22), so this PR rewrites the PUBLISHED
  requirement to "0.9" — downstream resolvers (abstractcode-tui) get 0.9
  transitively at their next update. This is a library-contract change, not
  just a lock bump.
- What 0.9.0 actually changed (upstream CHANGELOG, 2026-01-12): "minor API
  break for having to add enum variants. Many enums have been made
  non_exhaustive… minimum rust version is now 1.60." The break class is
  enum-matching (TINFLStatus/MZError/TDEFLFlush/CompressionStrategy).
- This crate's usage surface (audited): `deflate::compress_to_vec_zlib`
  (src/gfx/proto/kitty.rs:95, src/gfx/png_encode.rs:50,
  src/gfx/png_test_encoder.rs:74), `inflate::decompress_to_vec_zlib_with_limit`
  (src/gfx/png.rs:171 — the PNG decode critical path), one test-only
  `decompress_to_vec_zlib` (kitty.rs:249). **No enum is ever matched** —
  the one error is Debug-formatted into `Error::Parse` (png.rs:172). The
  high-level function API is unchanged in 0.9.
- MSRV 1.60 is a no-op here (no `rust-version` declared; CI runs stable;
  the consumer declares 1.71).
- Residual risks, both covered: (1) deflate internals changed ("simplify
  stored compression"), so compressed BYTES may differ — no test pins exact
  compressed output (the kitty test round-trips, png encoders feed the
  decoder); (2) decode behavior — 12 unit tests in src/gfx/png.rs + 9 in
  tests/adv_image.rs run in the default suite on three OSes. Optional
  belt: run `fuzz_big -- --ignored` once by hand (the 5k-case hostile
  campaign covers the decoder).
- Ecosystem note: dual-version trees (a downstream also holding 0.8 via
  e.g. flate2) mean two compiled copies, not breakage. Cosmetic.

### PR #5 — windows-sys 0.60.2 → 0.61.2 (cargo)

**Posture: MERGE; the Windows CI job is the real gate. One consequence to
record for 0180.**

- 0.61's substance: windows-sys now links via `windows-link`/raw-dylib
  unconditionally (dropping the windows-targets import-lib crates) and
  **raises MSRV to 1.71** — on the Windows target only. Feature names used
  here (Win32_Foundation, Win32_System_Console, Win32_Storage_FileSystem,
  Win32_System_Threading, Win32_Security — Cargo.toml:28-34) are unchanged.
- The platform is compile-check-only in this repo's claims ledger, but NOT
  CI-blind: `.github/workflows/ci.yml` runs `cargo build --lib` + `cargo
  test --lib` on `windows-latest` — a real runner compiles and runs the lib
  suite. If 0.61's linking change breaks anything, the PR goes red before
  merge. That is exactly the right gate for this crate's honesty posture.
- **0180 interaction (record it)**: when 0180 declares `rust-version`, the
  Windows floor is now 1.71 (unix floor unaffected: libc/miniz_oxide are
  1.60-class). Declaring anything below 1.71 would be false on Windows.
- Build-time side effect: dropping the windows-targets import libs
  meaningfully shrinks Windows CI download/build time. Free win.

### PR #4 — libc 0.2.186 → 0.2.189 (cargo)

**Posture: MERGE, trivially.** Patch-level within the same requirement
(`libc = "0.2"` — Cargo.toml:25 already covers .189), so this is a
Cargo.lock-only change; the published contract is untouched. libc's
never-breaking 0.2 line + unix CI on two OSes covers it. Zero review
attention warranted beyond the green check.

### PR #3 — softprops/action-gh-release 2 → 3 (actions)

**Posture: MERGE.** v3.0.0 (2026-04-12) is a Node 20 → Node 24 runtime
bump, explicitly a drop-in with no input/API changes. This repo's usage is
minimal — one step with `generate_release_notes: true` (release.yml,
`github-release` job). Context that makes staying on v2 pointless: GitHub
forced Node 24 as the default on June 2, 2026 (past), so v2 already runs on
Node 24 with deprecation warnings. Verification is deferred by nature: the
job only runs on the next tag — worth remembering at the next release, not
worth a rehearsal now (the `workflow_dispatch` path exercises `verify`
only; the release job itself is tag-gated).

### PRs #2 + #1 — actions/upload-pages-artifact 3 → 5 and actions/deploy-pages 4 → 5 (actions)

**Posture: MERGE BOTH TOGETHER, then dispatch docs.yml once
(`workflow_dispatch` exists) and eyeball the deployed site.** These two are
one pipeline (docs.yml `build` uploads, `deploy` consumes); the tested pair
is latest/latest, and merging them separately leaves an intermediate
mixed-pair deploy for no benefit.

- upload-pages-artifact v5 = internal actions/upload-artifact v7 (Node 24)
  + new `include-hidden-files` input (default false; .git/.github always
  excluded). Crossing v3→v5 also crosses the v4 boundary (upload-artifact
  v4-class hidden-file exclusion).
- Repo-specific hidden-file exposure, checked: the artifact is `book/`
  (mdBook output; rustdoc copied to book/api/ via `cp -r target/doc/.`).
  Plausible dotfiles: mdBook's `.nojekyll`, rustdoc's `.lock`. Both are
  inert under Actions-based Pages deployment (the artifact is served
  directly; no Jekyll pass, so `.nojekyll` is vestigial; `.lock` is junk).
  If a future site needs a dotfile (`.well-known` etc.), the new input
  exists. Verdict: the exclusion change cannot break THIS site, but the
  one-dispatch verification after merge is the honest close.
- deploy-pages v5 = Node 24 line of the same migration; no contract change
  relevant here (`environment: github-pages` + `id-token: write` already
  configured correctly in docs.yml).

**Suggested merge order**: #4 (libc) → #3 (gh-release) → #1+#2 together +
docs dispatch → #5 (windows-sys, watch the Windows job) → #6 (miniz_oxide,
own commit, full suite). All six should be in before the next crate release
so 0.2.1 ships current pins.

---

## 4. THE RECOMMENDATION

Weighing: the live consumer feedback loop (abstractcode-tui exercising
Feed/TextArea/completion/selection TODAY is the cheapest real-world signal
this crate will ever get), the trigger states verified above, ADR-0001's
additive-only regime until a budgeted 0.3, CI/platform honesty, and the
three study tracks.

### Promotion triggers that are TRUE right now (the objective inputs)

| Item | Trigger text | Verdict |
| --- | --- | --- |
| 0250 | active consumer whose workaround the fix deletes | **TRUE** — consumer live, workaround documented (0250:58-63), fix additive |
| 0200 | "Its widget + live-data dependencies land" | **TRUE** — overview.md:82 says so itself; Phase-0 checklist satisfiable |
| 0140 | "coding-console port reaching syntax/tool-result previews" | **TRUE in substance** — a console-class consumer renders diffs/code today (abstractcode-tui, not 0200; see C7); diff lexer is the named strongest want |
| 0500 | "any dogfood app reaching a settings/config surface" | **TRUE since birth** — the consumer's /model//theme pickers ARE that surface (0500:37-39); substrate risk retired by the shipped passive slice |
| 0160 (layer 2) | "a dogfood app reaching its copy phase" | **TRUE** — selection shipped engine-side (0270), consumer exercising it now; the public rect→text API is the missing half (extract_text is pub(crate), selection.rs:435) |
| 0260 | second consumer | **NOT true** — one consumer exists; don't promote on vibes |
| 0060 / 0210-live / 0300 | maintainer green-light / 0040+0050 / a consumer needing lifecycle hooks | **NOT true** — defer honestly |

### Ranked moves

1. **Ship the 0250 fix (List/Table `on_activate` + disposal-safe callbacks) and cut 0.2.1.**
   WHY: additive, small, and it deletes a first-contact crash + silent
   preference corruption from the one live consumer while that consumer is
   actively rebuilding on 0.2.0 — the highest-value feedback loop hour.
2. **Merge the dependabot queue in the §3 order (all six), before 0.2.1.**
   WHY: every PR is merge-grade after inspection; batching them under the
   0.2.1 release means the next published crate carries current pins and
   the pages/release pipelines get verified once, deliberately.
3. **Write the 0.3 breaking budget + wire the `cargo public-api` CI gate (0170's remainder, re-anchored).**
   WHY: ADR-0001 §2 was breached today for lack of the written list (S2);
   the gate is what makes "additive-only until 0.3" enforced instead of
   aspirational. Seed the budget: `Role` non_exhaustive (+Tree/TreeItem),
   `TokenKind` non_exhaustive-or-DiffKind ruling (C5), `content_size`
   deferral fate (0130:65-66), any List multi-row reshape.
4. **Integrator staleness pass applying §1** (overview preamble + 0070 row +
   Next-recommended rewrite; the three READMEs; item-level scope notes for
   0148/0160/0260/0570).
   WHY: the planning surface currently recommends a spent breaking window
   as the #2 move — every future agent reads that first.
5. **Pull 0500 owned-mode + Select as THE study item.**
   WHY (and why not 0300, the overview's current #3): by the backlog's own
   trigger discipline 0500 is triggered (consumer settings surface exists;
   the 0250 crash was born in exactly that workaround) and 0300 is not (its
   consumers 0310-0360 are unscheduled; no app needs suspend/flush hooks
   yet). The substrate's riskiest half (placement engine, anchor-unmount
   safety, top_z stacking) already shipped and is production-validated via
   0120's completion dropdown — the owned mode is an extension of
   `app::anchored`, not a fresh build. It unblocks the whole app-kits chain
   (0510 forms → 0520 wizard → 0530 tables) and replaces the consumer's
   modal-List pickers with the engine control they should have had.
6. **0140 diff-lexer first slice, DiffKind (additive) path.**
   WHY: the trigger is live (consumer renders diffs today) and the
   line-oriented diff lexer fits even the stateless trait (0140:68-70); the
   TokenKind-widening question must not block it — park TokenKind in the
   0.3 budget (move 3) and ship the additive mapping now.
7. **0180 platform honesty + MSRV declaration.**
   WHY: the release + dependency churn make CI truth timely; the
   windows-sys bump just set a hard fact for it (Windows floor 1.71), and
   the Linux-pty claim/evidence gap is the one public claim ahead of its
   evidence. The public-api gate (move 3) rides the same CI pass.
8. **0160 layer 2: promote `extract_text` to the public rect→text API.**
   WHY: one visibility change + contract text away (selection.rs:435 is
   pub(crate)); it is the engine half of the consumer's copy phase and
   useful headlessly in tests either way.

### Explicit DEFERs (on the record, with the reason)

- **0060 watcher** — maintainer's "explicitly not-now" stands; nothing
  today changes it.
- **0210 live phases + 0040/0050** — transport ADR still waits on 0060's
  evidence by design (evidence-first pattern, overview:139-141); do not
  armchair it because the widget deps landed.
- **0300-0360 (control-plane band)** — 0300's trigger is not true (no
  consumer); revisit when 0310/0320 get a driving consumer or an app needs
  suspend/flush. The overview's Next-recommended #3 should demote
  accordingly.
- **04xx execution** — 0400's ADR is cheap (skeleton exists,
  reviews/study/extensions-cycle3.md §1c) and MAY land opportunistically,
  but 0410's gating waits for the ADR + a consumer who wants the trim;
  0420-0470 wait for their named consumers per their own triggers.
- **0570 tree view** — hard-blocked on the 0.3 Role window (S3); do not
  land variants "additively by accident."
- **0260 disclosure** — one consumer; rewrite the trigger (S13) and note
  the Feed-selection dependency (C1); build when the second consumer or the
  Feed-selection work materializes.

### MERGE / KILL

- **MERGE 0480 into 0165** as its producer section, spec preserved verbatim
  (0480 offers exactly this). Rationale: band anomaly (core work filed in
  the extensions band, self-acknowledged 0480:27-29) + one link channel =
  one item; nothing is scheduled that needs the halves separated. If a
  canvas consumer (0430/0450) is ever scheduled BEFORE 0165, the producer
  half can still ship first from within the merged item — the merge is
  bookkeeping, not sequencing.
- **KILL the 0070 row in the overview's Proposed ledger** (S4) — the item
  is completed; the row is a duplicate, not a file to move.
- **Do NOT merge 0148 into 0160** — the substrates only half-overlap (C3);
  instead give 0148 a scope-sync note naming which half exists.

---

## Appendix — evidence commands (all read-only)

- `gh pr list --limit 20` → 6 PRs, numbers/branches as cited.
- `gh release view v0.2.0 --json tagName,publishedAt,isDraft` →
  published 2026-07-22T04:01:05Z.
- `git log --oneline -15`, `git tag` → c793c1f = 0.2.0 wave commit; tags
  v0.1.0, v0.2.0; working tree clean.
- `rg -c '#\[test\]' src tests benches` → 1,393.
- `grep -A2 'name = "miniz_oxide"' Cargo.lock` (and windows-sys, libc) →
  0.8.9 / 0.60.2 / 0.2.186.
- Role/TokenKind exhaustiveness: `src/ui/access.rs:30-31`,
  `src/text/highlight.rs:22-37`; crate-wide `rg -n 'non_exhaustive' src/`.
- Rider absence: `rg -n 'mask' src/widgets/input.rs`;
  `rg 'focus_step|subtree focus' src/` (empty);
  `rg -n 'on_activate' src/widgets/` (empty).
- Completed-item checklist state: `grep -c '^- \[ \]'` across
  `docs/backlog/completed/**` → 0010:4, 0020:5, 0030:4, 0070:5, others 0.
- Consumer state: `../abstractcode-tui/Cargo.toml:26` (`abstracttui =
  "0.2.0"`); `rg -n 'Role::' ../abstractcode-tui/src` (empty).
- Upstream facts: miniz_oxide CHANGELOG 0.9.0 (2026-01-12), windows-sys
  0.61 crates.io metadata (MSRV 1.71, windows-link switch),
  softprops/action-gh-release v3.0.0 release notes (Node 24 drop-in),
  actions/upload-pages-artifact v4→v5 compare (upload-artifact v7 +
  include-hidden-files).
