# Wave 3 — REVIEWER handoff (cycle 2)

Date: 2026-07-23. Owner: REVIEWER. Full findings:
`reviews/wave3/review-cycle2.md` (numbered, severity + file:line).
Probes: `tests/wave_c2_review.rs` (13 tests, public API only).

## Ledger row for FIXNET (the overview.md writer this cycle)

0180 is CLOSED by the scheduled-gates leg and moved to
`docs/backlog/completed/app-widgets/` with its completion report.
Append-ready row, matching the overview table shape:

| item | title | state | note |
| --- | --- | --- | --- |
| app-widgets/0180 | Platform-claim accuracy + CI gates | completed (wave 3, REVIEWER — earlier legs 2026-07-22) | `.github/workflows/perf.yml` (weekly + dispatch: perf suites w/ retry-once load policy, fuzz_big, soak, measurements artifact); byte RATCHETS added to perf_app_surfaces (baseline × 1.5, assert in every profile); red-budget dry run executed locally; first hosted green pending push like every workflow |

## Demands by owner (from review-cycle2.md)

To INTEGRATOR (feed*/markdown* namespaces):

- C-1 (P2): one-writer self-heal for `FeedState::sync` — mutation
  counter → rebuild on foreign writes (probe flips loudly when it
  lands; update it in the same change).
- C-2 (P3): NaN-fingerprint doc line in `SyncSpec::new` + api.md
  ("float fingerprints must use `to_bits`").
- C-3 (P3): rebuild-storm cost sentence in api.md §Feed-sync.
- ~~C-8 (P3): `feed_typeset.rs:216` unreachable-pattern lint break~~ —
  DISCHARGED mid-cycle by INTEGRATOR (`#[allow]` + rationale; verified
  gone in the final battery).
- R-3 (P3): image cache file identity — fold `st_ino` (cfg(unix)) into
  `markdown_image.rs::file_signature` (the JsonFileRunStore mtime
  lesson, verbatim class).

To FIXNET (live-data net files):

- `src/reactive/connection.rs:158` `pub fn next(&mut self)` trips
  clippy `should_implement_trait` (Iterator lookalike) — rename
  (`next_delay`/`advance`) or `#[allow]` with a reason; it fails the
  lint gate.
- Mid-battery, `src/reactive/connection.rs:522` referenced a missing
  `connection_tests.rs` (whole-tree compile error at that moment) —
  presumably in-flight; flagging in case it's a forgotten `#[path]`.

To CONTENT2's owner (chart_time.rs — not in-flight, but not mine to
re-design):

- C-4 (P3): `TimeSeries` restart boundary vs its own gap-honesty claim
  (`missed == capacity` restarts though `[NAN × cap-1, v]` is bounded);
  pad-or-redoc, one of the two.
- C-6 (P4): 32-bit `usize` wrap in the missed count
  (`.min(capacity as u64)` before the cast).

To whoever lands a suspend keybinding (cc INPUTAV):

- I-2 (P2): `Terminal::suspend` must drain the key-state down-set on
  resume (stale holds; PTT keeps "recording" after resume — focus loss
  does NOT cover Ctrl+Z). The drain shape now exists in
  `keys::publish_fidelity`'s downgrade path.

## Fixes made by REVIEWER (small, tested, non-peer files)

- `src/app/keys.rs`: Full→Degraded publish now drains the down-set into
  synthesized release edges (+ unit test) — the stuck-hold/stuck-mic
  class closed structurally (unreachable today; probes only upgrade).
- `src/widgets/chart_time.rs`: CADENCE CHOICE doc paragraph (jitter →
  phantom gaps + sample loss; probe-pinned).
- `tests/perf_app_surfaces.rs`: byte ratchets (see 0180 note) + the
  codeview test now measures its bytes instead of discarding them.
- `.github/workflows/perf.yml` (new), ci.yml omission comment, README
  performance paragraph, CONTRIBUTING pointer, SETUP.md follow-ups
  (MSRV entry was stale), CHANGELOG Unreleased entries.
- `tests/wave_c2_review.rs` (new): the 13 probes.
