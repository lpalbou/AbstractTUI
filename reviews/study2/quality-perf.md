# QUALITY — performance audit (study 2)

Date: 2026-07-22 · tree: 0.2.1 post-release working tree · owner: QUALITY

Scope: verify the engine's core performance claims (zero idle cost,
damage-proportional repaint) on the CURRENT tree with the three
0.2.x waves' widgets in play (Feed/StreamSession, TextArea/Completion,
selection, Select family, diff CodeView), and extend the explicit perf
suites to cover the new surfaces.

Method note (load honesty): this host carried load averages between ~20
and ~75 during the audit (many concurrent agents). Timing medians below
name the run's approximate load; allocation counts and byte counts are
load-independent facts. The perf doctrine already encodes this (debug
refuses to assert; budgets carry slack; CONTRIBUTING now says a red
timing on a loaded host is a re-run signal).

## 1. Existing suite: `perf_budgets` (release, serial)

First run at load ≈ 75 (worst case), re-run of the three reds at load ≈ 37.

| test | metric | measured (median) | budget | verdict |
| --- | --- | --- | --- | --- |
| diff+present 200x60 full-change | wall/frame | 415 µs | 2 ms | PASS |
| parser 1 MB hostile soup | wall | 51.8 ms @load75 → **13.5–20.1 ms** @load37 | 50 ms | PASS (load artifact) |
| pool churn 100k unique clusters | behavior | cap 4096 holds, refusals stable | n/a | PASS |
| link churn past u16 space | behavior | 4,465 refusals, no wrap | n/a | PASS |
| flatten+diff+present 200x60 + Shimmer | wall/frame | 1.15 ms | 3 ms | PASS |
| splash 2D fallback frame 100x30 | wall/frame | 104 µs | 2 ms | PASS |
| brandmark 3D frame 100x30 | wall/frame | 452 µs | 8 ms | PASS |
| keystroke→frame via Driver::turn | wall | 376 µs | 3 ms | PASS |
| VT model referee 200x60 | wall/frame | 4.9 ms @load75 → **1.3–2.3 ms** @load37 | 3 ms | PASS (load artifact) |
| grid solve 12 cols × 480 children | wall | 110 µs | 3 ms | PASS |
| markdown parse+rich 1000-line doc | wall | 830 µs | 20 ms | PASS |
| richtext wrap 800-para @60 cols | wall | 24.8 ms @load75 → **8.8–13.5 ms** @load37 | 20 ms | PASS (load artifact) |

Verdict: **no engine-primitive regression.** The three first-run reds
(parser soup, VT referee, richtext wrap) all cleared their budgets with
30–70 % headroom once host load halved, and their load-75 medians track
the RT6-4 precedent (the cycle-6 "3.11 ms referee" finding that resolved
as host contention). The parser's 50 ms budget has the thinnest quiet
headroom of the suite (~13.5 ms best vs 50 ms — fine; the worst case at
load 75 sat at 118 ms, which is why perf stays a quiet-runner suite).

## 2. New suite: `perf_app_surfaces` (release, serial) — the wave surfaces

Added `tests/perf_app_surfaces.rs` (sibling of `perf_budgets`, same
doctrine: `#[ignore]`d, release-only asserts, debug prints-only). Every
measurement drives the REAL loop (`Driver::turn` against `CaptureTerm`)
and asserts byte-emission proportionality beside the timing budget.
Run: `cargo test --test perf_app_surfaces --release -- --ignored
--test-threads=1 --nocapture`. Measured at load ≈ 25–35:

| surface | metric | measured | budget | verdict |
| --- | --- | --- | --- | --- |
| Feed streaming: one ~6-char token → frame, 90x30, 40-item history, follow-tail pinned | wall/frame | 732 µs | 3 ms | PASS |
| ─ same, emission | bytes/frame | median 73 B, max 427 B vs 9,670 B first paint | < ⅓ first paint | PASS (0.8 % of a full paint) |
| Select popup: open+close cycle (2 frames), 100x30 | wall/cycle | 154 µs | 6 ms | PASS |
| ─ open emission | bytes | 301 B vs 3,720 B first paint | < ⅓ first paint | PASS |
| ─ close emission (vacated region repaint) | bytes | 254 B | < ⅓ first paint | PASS |
| Selection drag: full-screen region, head ±1 row, 200x60 | wall/frame | 2.04 ms | 5 ms | PASS |
| ─ same, emission | bytes/frame | 260 B (one changed row) | print-only | damage-proportional |
| TextArea keystroke with completion dropdown open, 4-row composer, 90x30 | wall/frame | 262 µs | 3 ms | PASS |
| ─ same, emission | bytes/frame | 465 B | print-only | bounded |
| Diff-tinted CodeView scroll 1 line, 400-line patch, 100x40 | wall/frame | 984 µs | 5 ms | PASS |
| Startup: spawn → first painted frame (pty, splash off), release `hello` | wall | 597 ms cold / **56 ms warm** | 1500 ms (warm) | PASS |
| Startup: release `dashboard` | wall | 682 ms cold / **49 ms warm** | 1500 ms (warm) | PASS |
| Startup: debug `hello` / `dashboard` | wall | 855/797 ms cold, 57/60 ms warm | print-only | recorded |

Notes on the numbers:

- **Feed streaming is damage-honest end to end**: the median token frame
  emits 73 bytes — the open block's changed rows plus cursor economy —
  against a 9.7 KB full paint. The 427 B max frames are paragraph-seal
  ticks (a `\n\n` token closes a block and the scroll advances). The
  claim "a token costs one open-block re-typeset" survives composition
  with `Scroll::follow_tail` through the real loop.
- **Selection drag is the most expensive per-frame surface (2.04 ms)**
  and honestly so: a full-screen region damages old ∪ new row rects, so
  the compositor recomposes ~12,000 cells before the diff finds the one
  changed row (260 B emitted). It stays under half the 5 ms budget and
  under the 16 ms/60 fps bar with 8× headroom, but it is the surface
  where a future 300x80 terminal would feel cost first — see
  investment 2 below.
- **Startup cold vs warm split**: the ~600–850 ms cold numbers are
  first-exec OS costs on this box (macOS code-signature validation +
  cold page cache under load — the same binary run again lands at
  ~50–60 ms, debug included). The warm number is the honest engine
  claim (enter + probe write + first frame ≈ 50 ms, dominated by pty
  round-trips); the assert is intentionally a catastrophic-regression
  ceiling on warm release runs, not a millisecond pin.
- The suite prints byte medians for every surface so future runs can
  ratchet emission, not just wall time.

## 3. Idle honesty with the new mounts (allocation pin, always-on)

`alloc_budget.rs` gained
`idle_turns_with_feed_interval_and_parked_popup_allocate_nothing`:
a 60x16 app mounting a `Feed` with history AND an open (quiet)
streaming item, an armed-but-not-due `interval` on the driver's
injected clock, and a PARKED open `Select` popup. Sixteen idle turns
through the real driver measure **0 allocs / 0 reallocs / 0 bytes
requested / 0 bytes written** on the UI thread.

This runs in the DEFAULT suite (allocation counts are
optimization-independent), so the "an idle app allocates nothing" claim
is now CI-pinned through the whole app layer with the new widgets
mounted — previously it was pinned only at the render layer
(diff/present) and as zero-bytes (not zero-allocs) at the app layer.
`docs/architecture.md`'s damage-promise section now names this pin.

## 4. Regressions found

None in the engine. Two findings that are not regressions:

1. The three first-run perf failures were host-contention artifacts
   (§1), reproducing the documented RT6-4 class. No budget was changed;
   CONTRIBUTING now states the quiet-host rule explicitly next to the
   suite invocations.
2. `perf_budgets::perf_richtext_wrap_large_doc` sits closest to its
   budget under load (24.8 ms vs 20 ms at load 75; 8.8 ms quiet). If the
   scheduled quiet-runner job (0180's open leg) lands, this is the
   first test that would benefit from run-isolation.

## 5. Files touched

- `tests/perf_app_surfaces.rs` — NEW: 6 release-mode budget tests for
  the wave surfaces (feed streaming, select popup, selection drag,
  composer keystroke, diff scroll, pty startup).
- `tests/alloc_budget.rs` — NEW test: zero-allocation idle with
  Feed + interval + parked popup.
- `CONTRIBUTING.md` — the third explicit suite named beside the other
  two; quiet-host guidance.
- `docs/architecture.md` — the damage-promise test list gained the
  app-layer idle-allocation pin.
- `CHANGELOG.md` — Unreleased section records the new suites.
