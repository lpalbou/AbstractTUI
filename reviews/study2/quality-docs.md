# QUALITY — docs freshness audit (study 2)

Date: 2026-07-22 · tree: 0.2.1 post-release working tree · owner: QUALITY

Scope: coherence audit of the documentation set after three waves of
appends by six agents — api.md end to end, getting-started,
examples/README, docs index, CHANGELOG 0.2.1, llms files, stale-claim
hunt.

## 1. docs/api.md (end-to-end read)

Verdict: **coherent**. Section order is sane (prelude → reactive → ui →
layout → widgets with the wave subsections grouped → app → selection →
theme → render → gfx → three → term/input → testing → stability). Every
new public surface is present and accurate: Feed + `StreamSession`,
`follow_tail`, TextArea/`TextAreaState`, Completion + `AnchoredPanel`,
`DismissReason` + the OWNED `Popup`, the Select family, the diff lexer
pair (`DiffLexer`/`DiffKind`/`diff_token_color`), `masked`, and the
selection/mouse-capture/clipboard trio. No duplicated or contradicting
snippets found (the catalog-bullet + detail-section pairs are
summary-then-depth, not drift). Factual claims verified against the
registry: 26 themes, 36 tokens, the theme-family enumeration sums to 26.

One fix applied: the `## app — the runtime` overlay bullet described
only the PASSIVE `AnchoredPanel`; the OWNED `Popup` and `Tooltip` modes
were documented solely inside the widgets section. The bullet now names
all three routing modes with their one-line contracts.

## 2. docs/getting-started.md

The first-app path was TextInput-only. Added one light-touch paragraph
after the interactivity section pointing at the one-import wave
surfaces (`Select`/`Combobox`/`MultiSelect`, `TextArea`+state, `Feed` +
`follow_tail`) with the api.md anchor and the two examples that compose
them. Fixed "twelve runnable programs" → fourteen in "Where next".

## 3. examples/README.md

- The table matches the 14 on-disk examples (13 demos + the capture
  tool) plus `common/`.
- **Gap fixed**: `feed` and `transcript` — precisely the two wave
  examples — were the only ones WITHOUT detail sections (table rows
  only). Both gained house-format sections (description, keys grounded
  in the source including the composer-focus caveat, needs, looks-like).
- Key-claim verification against sources: `images` and `grid` both bind
  `t` (theme cycle) that their Keys lines omitted — added. `widgets`
  F2 / `effects` p,d,m,n / `themes` Enter / `dashboard` claims check
  out. `images` "p protocol toggle" ✓.
- Updated for this pass's changes: gallery blurb (Select/TextArea/diff
  on the board), components blurb (the picker section that 0.2.1's own
  CHANGELOG records), capture blurb (the new `apps` family + the
  byte-deterministic note).

## 4. docs/README.md + SUMMARY.md

SUMMARY.md lists all eight guides — complete. docs/README.md: fixed
"twelve runnable programs" → fourteen; the captures bullet now names
the in-process app stills. Guide table already covered live-data.md.

## 5. CHANGELOG (0.2.1 as ONE release)

The 0.2.1 Added list interleaved four authors' entries (widgets, diff,
project-infra, app). Reordered — facts untouched — into: widgets
(`List::on_activate`, `masked`) → the diff feature pair (text lexer +
`CodeView::lang`) → the app layer (popup substrate → select family) →
project infra (0.3 breaking budget, CI gates). Also removed a stray
mid-list blank line in the 0.2.0 section. Added an Unreleased section
recording this pass's additions (new perf suite, alloc pin, gallery/
capture extensions, stale-claim fixes) so the next release cut doesn't
have to archaeology them.

## 6. llms.txt + llms-full.txt

- llms.txt: "the two explicitly-run suites" → the current three; the
  captures bullet gained the app stills. Example count ("fourteen") was
  already correct.
- llms-full.txt: regenerated mechanically (header + README + the eight
  guides in canonical order + PACKAGE FACTS tail, exact
  `===== FILE: =====` separator format). Two stale facts fixed in the
  tail while regenerating: `miniz_oxide` 0.8 → **0.9** and
  `windows-sys` 0.60 → **0.61** (both drifted from Cargo.toml), and the
  test-invocation list gained the `perf_app_surfaces` suite.

## 7. Stale-claim hunt (whole tree, user-facing surfaces)

| claim | where | fix |
| --- | --- | --- |
| "12 examples" / "twelve runnable programs" | CONTRIBUTING, docs/README.md, getting-started.md | → 14 / fourteen |
| `FeedState::clear` described as NOT YET LANDED ("when that lands…"; "today's equivalent") | docs/live-data.md, examples/feed.rs (module doc + inline comment) | it shipped — reworded to present-tense fact (slot-keyed replace kept as the recommended recipe, with the honest why) |
| "landed this wave" (wave-relative wording in shipped example doc) | examples/feed.rs | removed; backlog id kept as `backlog 0270` |
| "Exactly one exists today" | docs/theming.md | "Exactly one exists" |
| `miniz_oxide` 0.8, `windows-sys` 0.60 | llms-full.txt package facts | 0.9 / 0.61 (Cargo.toml truth) |
| "the two explicitly-run suites" | llms.txt, CONTRIBUTING | three suites, named |

Verified NOT stale: test count "~1,440" (1,442 after this pass — the
"roughly" holds), README's "Fourteen runnable examples", README perf
numbers (200x60 diff+present ~0.5 ms — measured 415 µs this pass;
idle-zero — now allocation-pinned through the app layer), the 26-theme
and 36-token counts everywhere, MSRV 1.87, version strings (0.2.1 in
Cargo.toml = CHANGELOG = llms files). No "tonight/today/yesterday"
time-relative wording remains in user-facing docs; no incident language
found in any public doc (backlog/reviews history untouched — internal
records keep their dated voice).

## 8. Files touched (docs mission)

docs/api.md · docs/getting-started.md · docs/README.md ·
docs/live-data.md · docs/theming.md · docs/architecture.md (perf-pin
list, see quality-perf.md) · examples/README.md · examples/feed.rs
(doc comments) · CONTRIBUTING.md · CHANGELOG.md · llms.txt ·
llms-full.txt (regenerated last, after all doc edits).
