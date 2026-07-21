# Verification doctrine (REDTEAM)

Owner: REDTEAM. Scope: `src/testing/**`, `tests/**`, `benches/**`. This
document is the contract every other module is measured against. If a
verdict from the rig and a claim from a module disagree, the burden of
proof is on the module — and if the rig itself is wrong, that is a P0
against REDTEAM (the rig is self-tested for exactly this reason, see
`tests/rig_self.rs`).

## 0. The rig, one paragraph

`src/testing` ships in the library (usable from any module's unit tests):
`VtScreen` — a VT100/xterm interpreter over a model cell grid (the ground
truth for what emitted bytes DO; palette = a re-export of the canonical
`base::palette`); `CaptureTerm` — an in-memory `term::Terminal`
implementation with scripted reads (input/resize/wake/idle), virtual
deadlines (never sleeps; `EventReader` deadline tests set its pub
timeouts to zero), write-failure injection and a `VtScreen` mirror of
everything written; `assert_snapshot` — golden snapshots under
`tests/goldens/`; `fuzzish` — a seeded xorshift64* PRNG plus hostile
byte-soup generators; `glb_mutate` — a byte-exact minimal GLB plus a
deterministic + seeded mutation battery with per-mutant expectations
(`MustLoad`/`MustReject`/`NoPanic`); `bench` — a std-only median timing
harness. The counting allocator lives in `tests/alloc_budget.rs` (per
test binary, §4). No dev-dependencies anywhere: everything is
hand-rolled per the charter's dependency policy.

## 1. The diff/present correctness property

The single most important property in the engine:

> Take the previous frame as a `VtScreen`. Apply exactly the bytes the
> presenter emitted for the new frame. The resulting screen must equal the
> intended new frame — cell for cell, color for color, attribute for
> attribute — and `unknown_seq_count()` must be 0.

Notes for RENDER (cycle 2, when diff/present lands):

- Equality is judged on the model's `to_styled_dump()` (or cell-by-cell
  for pinpoint messages). The dump includes cursor position and mode
  flags, so "correct pixels, cursor left in the wrong place" still fails.
- `unknown_seq_count() == 0` means the presenter emitted only traffic the
  model understands. The modeled set (CUP/CUU/CUD/CUF/CUB, CNL/CPL, CHA,
  VPA, CR/LF, ED/EL/ECH 0/1/2, SU/SD, DECSC/DECRC, RI/IND/NEL, SGR
  0/1/2/3/4/7/9/22/23/24/27/29/30-37/39/40-47/49/90-97/100-107/38;5/48;5/
  38;2/48;2 (semicolon and colon forms), DECSET/DECRST, OSC 8/0/2, queries)
  was chosen FROM the render contract. If the presenter needs a sequence
  outside this set, that is a request to REDTEAM to extend the model — in
  the same PR-equivalent, with tests — not a license to emit unmodeled
  bytes.
- The property is exercised three ways: hand-picked frame pairs (goldens),
  randomized frame pairs from `fuzzish` (seeded, reproducible), and — the
  case that finds real bugs — randomized SEQUENCES of frames applied
  cumulatively, because diff bugs are usually stateful (stale SGR cache,
  wrong cursor assumption after a wide glyph, BCE misuse).
- Wide-glyph invariant: after any frame, no orphan continuation cells, no
  torn leaders. The model repairs pairs the way real terminals do, so a
  presenter that emits bytes tearing a pair produces a model screen that
  differs from the intent — the property test catches it without any
  special-casing.
- Synchronized output: `sync_begins == sync_ends` after every present, and
  a frame's bytes must not leave 2026 set (the model tracks it as a mode).
- Kitty keyboard: `kitty_push_depth == 0` after leave (enter/leave balance).

### One width, one palette

Two conventions must be shared engine-wide or the property lies:

- **Width**: cell width comes from `unicode-width` (the dependency both
  the model and `render`/`text` use). If RENDER ever caches widths or
  special-cases ambiguous-width characters, the model must be updated in
  the same change. AMBIGUOUS-WIDTH characters are East Asian context
  dependent; the engine treats them as width 1 (unicode-width's default)
  everywhere, and any deviation is a cross-module decision, not a local
  fix.
- **Palette**: indexed colors resolve through `testing::palette::xterm_256`
  (embedded xterm defaults; real terminals theme 0-15, so this is a
  convention, not physics). The presenter's 256/16 downlevel must
  round-trip through this same table in tests.

## 2. Input fuzz

Property: **the input parser never panics and never loses frame sync on
any byte sequence.** (Charter quality bar; KERNEL's parser is the target,
the rig's own `VtScreen` obeys the same rule and is fuzzed the same way.)

- Corpus: `fuzzish::hostile_corpus(seed, n)` — deterministic edge cases
  first (lone ESC, unterminated CSI/OSC/DCS, giant params, 300-digit
  params, truncated UTF-8 at every stage, stray continuations, surrogate
  halves, overlong encodings, CAN/SUB aborts, 4 KiB printable runs), then
  seeded random chunks biased toward sequence shapes. Uniform random bytes
  almost never reach deep parser states; the structured generators are the
  point.
- Split invariance: for any byte stream, feeding it in one call must
  equal feeding it in arbitrary chunks (`fuzzish::random_splits`) — chunk
  boundaries land mid-UTF-8, mid-param, mid-terminator by construction.
- Post-conditions asserted after every hostile input: no panic (implicit),
  cursor within bounds, dumps renderable, wide-pair invariant intact,
  bounded memory (string-frame caps hold).
- Failures reproduce by seed. A failing seed gets PROMOTED into the
  deterministic prefix of the corpus (a regression is never left to
  probability again).
- This is not libFuzzer: no coverage feedback, but it runs in plain
  `cargo test` on every machine, every time, in milliseconds. If we ever
  want coverage-guided runs, that is a separate opt-in binary — the
  in-tree corpus stays the gate.

## 3. Performance budgets and how they are measured

Charter budgets (the numbers REDTEAM enforces):

| Budget | Charter figure |
| --- | --- |
| Full-screen 200x60 animated redraw, diff+present | < 2 ms |
| Idle | zero wakeups |
| Steady-state frame heap allocation in diff/present | zero |
| Input event -> presented damage (small damage) | < 5 ms |
| 3D 80x24-cell viewport shaded mesh | ≥ 30 fps |

Measurement method (no criterion, dependency policy):

- `testing::bench::time_median(name, warmup, runs, iters, f)` —
  `std::time::Instant` (monotonic) around K iterations, repeated N runs,
  report the **median** run (one scheduler hiccup must not fail a build);
  best/worst are reported for eyeballs. `testing::bench::sink`
  (`std::hint::black_box`) defeats dead-code elimination.
- Perf tests are `#[ignore]`d and run explicitly:
  `cargo test --release -- --ignored perf_`. They never run in the default
  test pass (debug-build numbers are noise; CI machines vary). In debug
  builds the exemplar tests print but refuse to assert.
- Budgets in tests carry the charter number in the assert message, so a
  drifting budget is visible in the failure text, and slack vs the charter
  is an explicit, reviewable choice per test.
- Idle = zero wakeups is NOT a timing test: it is an event-loop design
  property. It will be tested by instrumentation — a scripted terminal
  whose `read` counts calls, an app with no animations, assert the loop
  parks in one blocking read and the frame counter stays frozen.
- The 30 fps 3D budget is measured as "median frame time ≤ 33 ms" over a
  rotating reference GLB at 160x96 half-block pixels, same harness.

## 4. Allocation counting (designed now, lands with the hot path)

Strategy for "no heap allocation in the diff/present hot path":

- A counting global allocator in the TEST binary only (IMPLEMENTED,
  cycle 2: `tests/alloc_budget.rs`): `#[global_allocator]` is
  per-binary, so the dedicated integration test target installs
  `CountingAlloc(System)` — `AtomicU64`s (allocs, reallocs, bytes)
  bumped with `Relaxed` ordering. No `#[cfg(test)]` tricks in the
  library, no feature flags, no cost to any other build. The one
  `unsafe impl GlobalAlloc` is confined to that test binary and only
  forwards to `System`. Companion pattern: an always-on ATTRIBUTION
  RATCHET test prints the per-stage split and fails on regression past
  the filing numbers, while the zero-budget acceptance test stays
  `#[ignore]`d with the finding id until the owner lands the fix.
- Usage pattern: run the loop once to warm caches/pools, snapshot the
  counter, run N frames, assert the counter is unchanged. Warmup
  allocations are legal; steady-state allocations are the defect.
- Threads: the counter is process-global. Alloc-budget tests must run
  single-threaded over the measured region (the harness runs the frame
  loop on one thread; other tests in other binaries are unaffected).
- Diagnostics: on failure, re-run with a `track_caller`-style scope guard
  (`AllocScope::named("present")`) that prints the delta per scope, to
  bisect which stage allocated. No backtraces (std-only), but stage-level
  attribution has been enough in practice everywhere else.
- This lands in the cycle where RENDER's diff/present exists; the design
  is recorded now so RENDER shapes the hot path with "zero allocs at
  steady state" as a testable contract, not an aspiration (pre-sized
  buffers owned by the presenter, byte scratch reused across frames,
  no `format!` in the emit path — `itoa`-style hand-rolled integer
  writes into the scratch buffer).

## 5. Golden snapshot policy

- Goldens live in `tests/goldens/<name>.txt`; names are `[a-z0-9_-]`
  (enforced — a name is a filename, no path escapes, no case-collisions
  across filesystems).
- `UPDATE_GOLDENS=1 cargo test` (re)mints goldens. A MISSING golden fails
  with instructions rather than self-minting: CI must never create truth
  nobody reviewed.
- The golden format is `VtScreen::to_styled_dump()`: header (size, cursor,
  pending-wrap marker, mode flags), bordered text rows, then non-default
  style runs. Deterministic by construction — iteration order is row-major,
  colors print as hex, no timestamps, no hash-ordered anything.
- Review rule: a golden change in a diff is a SEMANTIC claim ("the screen
  now looks like this on purpose"). Owners update goldens only for their
  own behavior changes and say so; a golden that changes as a side effect
  of an unrelated edit is a finding.
- Keep goldens small (a 12x4 screen proves as much as a 200x60 one) and
  one-concern-per-golden, so diffs read like statements.

## 6. What the rig deliberately does NOT model (yet)

Recorded so nobody mistakes silence for coverage:

- Scroll regions (DECSTBM), insert/delete line/char (IL/DL/ICH/DCH). The
  presenter contract does not use them in cycle 2 (full-row rewrites are
  planned); if RENDER adopts them for scroll optimization, the model
  grows them FIRST.
- Kitty graphics / iTerm2 / sixel payload SEMANTICS: APC/DCS/OSC frames
  are consumed and counted (`string_frames`), not pixel-decoded. What
  cycle 2 DID add: structural byte-shape validators in
  `tests/adv_gfx.rs` (kitty chunking rules — ≤4096, non-final chunks
  4-aligned + m=1, continuation frames carry only m/q, whole-stream
  base64 validity; sixel framing/raster-attr/register-range/channel
  ≤100/RLE well-formedness; iTerm2 OSC 1337 framing + inline=1 + PNG
  signature + size= consistency). A full sixel pixel decoder remains a
  likely cycle-4 build if visual regressions demand it.
- Terminal resize mid-stream (`VtScreen` is fixed-size; `CaptureTerm`
  scripts resize events but the model does not yet reflow). Lands with
  the app-loop resize story.
- ZWJ/VS16/skin-tone cluster merging IS modeled as of cycle 2 (one
  cell, ≤2 wide — render.md §2.5's convention), so the diff/present
  property judges presenter and model within one width policy. The
  RESIDUE stands: real terminals disagree with any policy (RT1-7), so
  the presenter's defensive cursor invalidation after risky clusters is
  the shipping-world protection; the rig verifies the convention, the
  invalidation protects against the world.
- The 16 system colors as THEMED values: the model pins xterm defaults
  (see "one width, one palette").

## 6b. Cycle-3 rig additions (recorded)

- `glb_mutate::float_payload_mutants()` — NaN/Inf/denormal/huge vertex
  payloads in valid containers; pinned contract: loaders MAY accept,
  the render pipeline must SURVIVE (finite framebuffer, no panic).
- Suites: `adv_splash` (virtual-clock pacing through `SplashIo`),
  `adv_raster` (watertightness, overflow hunt, perspective-correct
  texturing), `adv_widgets` (focus fuzz, editing property vs model,
  windowing cost via a counting canvas, hit-test grid, capture, hover),
  `adv_anim` (shader determinism goldens, hostile shaders vs
  `debug_validate`, additive blend, frame billing through Driver).
- Shader golden: `tests/goldens/shader_determinism.txt` pins the four
  built-ins at fixed (x, y, t) samples — the CellShader purity contract
  made mechanical.

## 7. How each area gets judged (cycle 2 preview)

- KERNEL: fuzz corpus against `input::Parser` (no panic, split
  invariance, bounded buffers, zero text leakage from swallowed
  sequences); enter/leave byte pairing asserted on `CaptureTerm` via mode
  flags + kitty depth; probe never hangs on a mute scripted terminal.
- RENDER: the §1 property, per-frame and cumulative; SGR run minimization
  measured as emitted-byte counts against goldens (economy is a budget,
  not a vibe); BCE/EL/ED choices must match the model's semantics; zero
  unknown sequences.
- REACT: effects fire exactly once per dependency change (counting
  effects); disposal during event routing (unmount mid-bubble) must not
  fire dead effects or panic; damage produced by a signal write is exactly
  the region owned by the affected computations.
- GFX3D: PNG decoder against hand-built vectors (incl. malformed chunks —
  fuzz applies to file parsers too); GLB loader against truncated/hostile
  buffers; mosaic renderer goldens (half/quadrant cells over gradients).
- DESIGN: contrast floors (`Rgba::luminance`-based ratio) for every theme
  token pair the charter names; theme switching leaves no stale colors
  (property: repaint after switch == fresh render with new theme).
