# 0180 — Platform-claim accuracy + CI gates (Linux pty, perf/fuzz/soak, MSRV)

## Metadata
- Created: 2026-07-21
- Status: Planned
- Track: app-widgets (API/claims honesty lane)
- Completed: N/A

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
- [ ] Linux live-pty CI job (with example prebuild) or README reword
- [ ] Scheduled perf/fuzz/soak workflows
- [ ] MSRV established + declared + CI job + policy line
- [ ] README perf-claim nuance sentence
