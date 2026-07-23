# 0180 — Platform-claim accuracy + CI gates (Linux pty, perf/fuzz/soak, MSRV)

## Metadata
- Created: 2026-07-21
- Status: Completed — all four legs executed (three on 2026-07-22, the
  scheduled deep-gate leg on 2026-07-23; see status notes)
- Track: app-widgets (API/claims honesty lane)
- Completed: 2026-07-23 (REVIEWER, wave 3 cycle 2)

## Status note — 2026-07-23: scheduled deep gates shipped (want #2, the last leg)

- **`.github/workflows/perf.yml`**: weekly cron (Mon 03:17 UTC, off the
  :00 rush) + `workflow_dispatch`; ubuntu; prebuilds release tests and
  both example profiles (so the startup test never pays a cold cargo
  build inside a measurement window); runs `perf_budgets` and
  `perf_app_surfaces` in release, serially (`--ignored
  --test-threads=1`), then `fuzz_big` and the soak. The printed
  measurements upload as a run artifact (90-day retention) for trend
  eyeballing.
- **Load-sensitivity policy** (the quality-perf.md risk note, encoded):
  the two TIMING suites retry ONCE on failure (30 s settle between
  attempts) to absorb runner scheduler noise — the budgets carry
  30–70 % quiet-host headroom, so two consecutive breaches are a real
  regression and fail the run honestly. Fuzz and soak are
  deterministic and never retry (their failures repeat by
  construction). Failures open visibly (red scheduled run), never
  block PRs — ci.yml's omission comment now points here.
- **Byte ratchets added** (the "prints byte medians so future runs can
  ratchet emission" note in quality-perf.md §2, now cashed in):
  `perf_app_surfaces` asserts absolute byte budgets (quiet-host
  baseline × 1.5) where it previously only printed — feed stream token
  frame ≤ 110 B (measured 73), select popup open/close ≤ 452/381 B
  (301/254), selection drag ≤ 390 B (260), composer keystroke ≤ 698 B
  (465), codeview scroll ≤ 383 B (255 — measurement added: the test
  previously discarded its bytes; the vertical-shift detection engages
  for app-managed scroll too), feed-scroll shift/guard phases ≤
  258/2,637 B (172/1,758). Byte counts are load-independent (fixed
  caps, deterministic content — verified identical debug↔release), so
  ratchets assert in EVERY profile; a breach means the damage contract
  regressed, and the retry cannot mask it.
- **Deliberate red-budget dry run executed locally** (the validation
  bullet): a ratchet temporarily broken to 10 B failed attempt 1,
  failed the retry, and exited non-zero through the same
  `run_timing_suite` shell function the workflow uses; restored and
  re-verified green. Suite runtimes on this host (release): fuzz_big
  0.04 s, soak 3.7 s, perf_app_surfaces ~10 s, perf_budgets ~10 s —
  comfortably inside the 60-minute job timeout with cold compiles.
- **Docs aligned**: README's Performance paragraph now states the
  timing budgets + ratchets gate on the scheduled job (the want #4
  sentence is now simply true); CONTRIBUTING names the workflow beside
  the manual invocations; SETUP.md's follow-ups list records both the
  MSRV job (was stale — it predated the 2026-07-22 leg) and the
  scheduled gates.
- **Residual, named honestly**: "run at least once green" on a hosted
  runner is a push-time event — this repo has never been pushed, so
  EVERY workflow here (ci.yml included) awaits its first hosted run.
  The local dry run covers the falsifiability half of the validation;
  the first green scheduled run lands with the publication push
  (SETUP.md step 1). If the hosted runner proves noisier than the
  retry-once policy absorbs, the recorded quiet-host medians in the
  ratchet comments are the recalibration baseline.

## Status note — 2026-07-22: MSRV + Linux pty job + claim rewording shipped

- **MSRV declared and verified**: `rust-version = "1.87"` in Cargo.toml.
  The floor is the library's own std usage — `is_multiple_of`
  (stabilized 1.87; gfx/png, gfx/base64, three/validate, jpeg), above
  `is_none_or` (1.82) and the windows-sys 0.61 Windows floor (1.71) the
  audit recorded. Verified locally: `cargo +1.87.0 check --all-targets
  --locked` compiles the whole tree (one inference-sensitive call in
  `testing/fuzzish.rs` needed a turbofish — an inference accident, not
  a feature need). CI `msrv` job added (pinned 1.87.0, ubuntu,
  `--locked`; note lockfile v4 needs cargo ≥1.78, so 1.87 reads it
  natively). Bump policy written into CONTRIBUTING (minor-version
  event, declared in CHANGELOG) and echoed in README.
- **Linux pty truth (want #1)**: CI job `live pty (ubuntu)` runs the
  ignored live_smoke suite serially per CONTRIBUTING, with an explicit
  `cargo build --examples` prebuild step (the R5 flake fix; the suite's
  internal `ensure_examples_built` Once-guard then finds warm
  binaries). README Linux row reworded to name its actual evidence
  (default suite in CI + the dedicated pty job) instead of the bare
  "pty coverage" claim; final validation is the job's first green run
  on the hosted runner — if it proves un-runnable there, fall back to
  the item's soften-the-row outcome.
- **README perf nuance (want #4)**: the Performance paragraph now
  states the split honestly — allocation budgets gate every CI run;
  release-mode timing budgets are explicit manual suites until want #2
  lands.
- **Remaining (want #2)**: the scheduled/nightly perf + fuzz_big + soak
  workflows. Untouched this cycle — they need the quiet-runner class
  and a deliberate red-budget dry run, which is its own verification
  exercise.
- Rider shipped the same day (0170's remainder, re-anchored by the
  audit): the `semver` CI gate (cargo-semver-checks vs latest published
  release, verified locally: 196 checks pass vs 0.2.0) and the written
  0.3 budget (`planned/0002_the_0_3_breaking_budget.md`).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: None. This item changes CI and one README row, not APIs.

## Context
Both cycle-11 reviews audited claims against evidence. The code held up
everywhere — except one README row that implies test coverage that never
ran, and a set of real, passing suites that nothing executes
automatically. For a crate asking applications to build on it, the claims
ledger and the regression gates are part of the product.

## Current code reality
- `README.md:139-143` — the platform table's Linux row: "Verified — same
  unix code paths and pty coverage." The robustness review (R1) verified
  this overstates: the live-pty suites (`tests/live_smoke.rs`,
  `src/term/rt5_live_tests.rs`, `src/term/tmux_live_tests.rs`) are
  `#[ignore]`d and `.github/workflows/ci.yml` never passes `--ignored` —
  there is **no record anywhere of the pty suite executing on Linux**.
  The shared-`unix.rs` argument is real, but RT5-1 itself (Darwin's
  poll(2) rejecting the `/dev/tty` alias) proves intra-unix quirks exist;
  the symmetric Linux-only quirk class is exactly what an un-run live
  suite misses. The macOS and Windows rows are accurate (Windows even
  errs conservative — CI runs 897 lib tests on a real Windows runner).
- `.github/workflows/ci.yml:5` — "Live/perf suites are #[ignore]d in the
  tree and deliberately not run here." So the 12 release-mode timing
  budgets (tests/perf_budgets.rs), the 5 fuzz campaigns
  (tests/fuzz_big.rs), and the 10k-frame soak pass when run by hand and
  gate nothing. README's "enforced by in-tree perf budgets" is true for
  the allocation budgets (default suite) but the timing budgets are
  manual (completeness review, marketing-deltas #1).
- `.github/workflows/ci.yml:7-8` — "no MSRV job: Cargo.toml declares no
  `rust-version` yet." Verified: no `rust-version` key in Cargo.toml.
  Named as a follow-up in .github/SETUP.md since publish.
- Rig flake with a known cause (robustness R5): a cold parallel
  `live_smoke -- --ignored` run builds example binaries inside test
  bodies while 14 pty cases race an 8 s deadline — 13/14 on first run,
  14/14 warm. A prebuild step in the harness closes it.

## Problem
One public claim is ahead of its evidence, and three real suites are
regression-blind: a timing regression, a parser panic reintroduction, or
an MSRV break would ship silently until someone runs the manual batteries.

## What we want
1. **Linux pty truth**: add a CI job (ubuntu) running the ignored live
   suite headlessly under a real pty (the suite already self-hosts via
   its pty helper; prebuild the example binaries first — fixes R5 in the
   same stroke). If the suite proves un-runnable on hosted runners,
   soften the README row to what is true ("same unix code paths;
   live-pty suite executed on macOS") — either outcome ends the
   claim/evidence gap; the job is the preferred one.
2. **Scheduled deep gates** (scheduled/nightly + manual dispatch, not
   per-PR): release-mode `perf_budgets -- --ignored` with generous
   budgets on the quiet runner class (the suites are load-sensitive and
   say so), `fuzz_big -- --ignored`, and the soak test. Failures open
   visibly (workflow failure), never block unrelated PRs.
3. **MSRV**: declare `rust-version` in Cargo.toml (establish it with
   `cargo-msrv` or by building against the oldest claimed toolchain),
   add the pinned-toolchain CI job, and state the bump policy in
   CONTRIBUTING (MSRV bump = minor version, declared in CHANGELOG).
4. **README nuance line** for the perf claim: allocation budgets gate
   every run; timing budgets gate on the scheduled job (once #2 lands
   this sentence becomes simply true).

## Scope / Non-goals
Scope: ci.yml/docs.yml-adjacent workflow additions, Cargo.toml
`rust-version`, README/CONTRIBUTING wording, the live-smoke prebuild.
Non-goals: Windows interactive verification (a session on a real console
is an operator act, not a CI job — the README already frames the first
Windows run as a beta event); converting the 23 `ignore`-fenced doctests
(worthwhile, but it is API-shape work that belongs with 0170's audit);
new test content.

## Expected outcomes
Every public platform/perf claim is backed by a job or worded to its
evidence; timing/fuzz/soak drift surfaces within a day instead of never;
downstream users get an MSRV contract.

## Validation
- The Linux pty job runs green in CI (or the README row is reworded and
  the review's R1 finding is closed as "claim corrected").
- Scheduled jobs exist, have run at least once green, and a deliberately
  broken budget fails the scheduled job in a dry run.
- `cargo +<msrv> test` green in CI; Cargo.toml declares the same version.

## Progress checklist
- [x] Linux live-pty CI job (with example prebuild) or README reword
      (2026-07-22: BOTH — job wired + row reworded to its evidence;
      green first run pending push)
- [x] Scheduled perf/fuzz/soak workflows (2026-07-23: perf.yml — weekly
      cron + dispatch, retry-once timing policy, byte ratchets added to
      perf_app_surfaces, measurements artifact; red-budget dry run
      executed locally; green first hosted run pending push, like every
      workflow in this repo)
- [x] MSRV established + declared + CI job + policy line (2026-07-22:
      1.87, verified with the pinned toolchain locally)
- [x] README perf-claim nuance sentence (2026-07-22; tightened
      2026-07-23 to name the scheduled job — the sentence is now
      simply true)
