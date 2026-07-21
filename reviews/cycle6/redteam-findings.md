# VERIFY (redteam series) cycle-6 findings

Continuity: the verification seat keeps the `redteam-*` filenames and the
RT-numbered series. Cycle 6 opened with the whole `cargo test` suite
UNVERIFIABLE end-to-end — integration binaries lagged public-API changes
made by owners. That gap (lib unit tests green while the integration
suite won't compile) is the cycle's process lesson and the first section
below.

## RE-GREEN: public-API drift that blocked all verification

Owners evolved public APIs without updating the dependent test literals
in the same change. `cargo test --lib` stayed green (unit tests live
beside the code), so the breakage was invisible to the owners — it only
surfaced in the VERIFY-owned integration binaries, which are where
cross-module contracts are actually exercised.

| # | Owner | Drift | Fix (VERIFY-owned) |
| --- | --- | --- | --- |
| RT-DRIFT-1 | KERNEL | `input::KeyEvent` gained a `keypad: bool` field (keypad/media distinction) | `tests/adv_splash.rs` now builds via functional-update from a constructor — `KeyEvent { kind: Release, ..KeyEvent::plain(Enter) }` — so future field additions fill in automatically (adopting KERNEL's stated intent for `KeyEvent::char`/`::key`/`::plain`) |
| RT-DRIFT-2 | VERIFY (self) | `testing::fuzzish::hostile_corpus()` → `hostile_corpus(seed: u64, count: usize)` | `tests/adv_text.rs` two call sites pass explicit seed+count. Filed against myself: a rig-signature change must green EVERY consumer in the same edit |
| RT-DRIFT-3 | REACT | `layout::Track` gained `Percent` + `Auto` variants (non-exhaustive match broke `adv_grid.rs`) | matched the new variants; also confirmed REACT now EXPORTS `Track`/`Display`/`Overflow` from `layout` (they were unreachable in cycle 5 — grid was uncallable from outside the crate; that gap is now closed) |

Process request to all owners (on the record): run `cargo test --no-run`
(NOT just `cargo test --lib`) before declaring a cycle green. The
lib-tests-pass-but-integration-won't-compile gap is invisible to `--lib`.
For struct literals across test code, prefer constructors / functional
update (`..Default`) or `#[non_exhaustive]` so a field addition doesn't
break every construction site.

RE-GREEN CONFIRMED: `cargo test --no-run` clean across all binaries; full
`cargo test` = **1140 passed / 0 failed / 59 ignored**.

## New findings

### RT6-1 (P2, GFX3D): animation sampler underflow-panics on a NaN sample time

`three::animation::locate(times, t)` panics on `t = NaN`:

```
if t <= times[0] ... { return (0,0,0.0); }       // NaN <= x is FALSE
if t >= times[last] { return (last,last,0.0); }   // NaN >= x is FALSE
let i = times.partition_point(|&x| x <= t);        // NaN: nothing <= NaN => i = 0
let (i0, i1) = (i - 1, i);                          // 0usize - 1 => PANIC
```

- Debug: "attempt to subtract with overflow" (confirmed live,
  `animation.rs:123`). Release: `usize` wraps to `usize::MAX`, the next
  line indexes `times[usize::MAX]` → bounds-check panic (or worse if a
  future refactor drops the check).
- Reachability: NaN `t` is not exotic — a caller looping with `t =
  elapsed % duration` produces NaN whenever `duration == 0.0` (an
  animation whose keyframes all share one time; parseable glTF). A
  sampler must be TOTAL over floats, like the decoders are total over
  bytes.
- Repro: `cargo test --test adv_anim_gltf nan_sample_time_must_not_panic
  -- --ignored` (acceptance test, ignored until fixed).
- Demand (GFX3D): one line — `if !t.is_finite() { return (0,0,0.0); }` at
  the top of `locate` (clamp-to-first, consistent with the before-range
  branch), or guard `i == 0`.

### RT6-2 (P3, GFX3D, informational): no animated GLB asset in the verification set

The Model-level animation determinism/clamp tests (`adv_gltf_anim.rs`)
are asset-guarded and currently SKIP — neither workspace GLB tried
(`x-wing/scene.glb`, `machine.glb`) reports a non-empty `animations()`
through `Model::load`. Not a defect (the sampler itself is covered
directly in `adv_anim_gltf.rs`, and the load path is fuzzed), but the
deepest end-to-end animation check (load → rig → hierarchy walk → pose)
has no live subject. Demand (GFX3D/DESIGN): drop one small animated GLB
into a known test-assets path (or point me at one) and the guarded tests
light up with zero code change.

## Cycle-6 attack suites added (all green)

- **Layout** (`tests/adv_layout.rs`, 6 tests): flex-grow exact main-axis
  tiling + no-overlap + cross-axis containment over 400 random rows;
  pure-grow fills the container to the last cell (contiguous, gap-free);
  wrap greedy line-break no-overlap/no-escape over 400 random shapes;
  one-line wrap == plain row; percent dims; determinism. Main-axis
  OVERFLOW asserted legal (fixed non-shrink children extend past a
  too-small container — standard flex); containment asserted only in the
  fit case.
- **Grid** (`tests/adv_grid.rs`, 5 tests): fr columns tile the content
  width exactly (sum + gaps == width) with contiguous gap-separated
  tracks over 400 random specs; `col_span` covers tracks + internal
  gaps; zero-track spec == one full-width column with disjoint stacked
  rows; over-wide span clamps safely; determinism. Containment asserted
  only when tracks fit (fixed tracks wider than the container legitimately
  overflow — grid does not shrink fixed tracks).
- **glTF animation sampler** (`tests/adv_anim_gltf.rs`, 7 + 1 ignored):
  LINEAR matches hand-computed lerp at arbitrary fractions; correct
  segment selection in multi-key tracks; STEP hold-then-jump; end
  clamping; rotation output always unit-length; antipodal midpoint
  finite; 500-seed property that well-formed tracks never yield
  non-finite output; + the RT6-1 NaN acceptance test (ignored).
- **glTF animation, Model level** (`tests/adv_gltf_anim.rs`, 3 tests):
  load → sample_pose determinism (byte-exact pose hash) + world-matrix
  finiteness (asset-guarded); before/after-range clamps to endpoints
  (asset-guarded); 300-mutant hostile-GLB corpus through `Model::load`
  never panics now that animation parsing is in the load path (9 loaded /
  350 rejected / 0 panics).
- **Component/Callback** (`tests/adv_compose.rs`, 5 tests): clones share
  one closure; noop/default inert; a callback OUTLIVES its creating scope
  (captures only its own state, no dangle); touching a DISPOSED signal is
  a CONTROLLED panic caught via `catch_unwind` with the runtime proven
  still consistent afterward (not UB — the reactive "use-after-dispose is
  a caught bug, not corruption" contract); dropping all clones drops
  captured state exactly once (no leak / double-drop).
- **Text/markdown/highlighter** (`tests/adv_text.rs`, 9 tests): markdown
  parser totality over the hostile corpus + markdown-shaped soup + 500
  seeds (never panics, always degrades to literal text, block count
  bounded — no quadratic blowup); unclosed fence captures remaining
  lines; RichText wrap preserves plain text + width bound; span styles
  never cross-contaminate after wrap (disjoint-alphabet colors); wide-
  glyph wrap stays cluster-atomic; highlighter determinism + totality
  (ascending, non-overlapping, char-boundary token ranges) over the
  hostile corpus + 500-seed code fuzz; `from_highlighted` preserves
  source text.
- **JPEG no-per-frame-alloc + hostile** (carried from cycle 5, still
  green) and **animation no-per-frame-alloc** (`alloc_budget.rs`): a
  multi-track animation driving 4 nodes sampled 240 times with a
  pre-grown pose buffer allocates ZERO on the hot path.

## LIVE PTY SMOKE — the headline (all 10 examples, real pseudo-terminal)

`cargo test --test live_smoke -- --ignored` (TERM=xterm-256color,
COLORTERM=truecolor, 100x30, ~1.5-3 s each, scripted keys, waited to
exit). Every example: exits 0 within deadline, VtScreen sees ZERO unknown
sequences, terminal fully restored, no panic text.

| Example | Exit | Bytes | Unknown seq | alt off | paste off | cursor shown | kitty depth |
| --- | --- | --- | --- | --- | --- | --- | --- |
| hello | 0 | 6,667 | 0 | ✓ | ✓ | ✓ | 0 |
| themes | 0 | 11,602 | 0 | ✓ | ✓ | ✓ | 0 |
| widgets | 0 | 5,745 | 0 | ✓ | ✓ | ✓ | 0 |
| effects | 0 | 81,160 | 0 | ✓ | ✓ | ✓ | 0 |
| dashboard | 0 | 14,001 | 0 | ✓ | ✓ | ✓ | 0 |
| components | 0 | 8,669 | 0 | ✓ | ✓ | ✓ | 0 |
| grid | 0 | 7,928 | 0 | ✓ | ✓ | ✓ | 0 |
| splash (skip) | 0 | 40,867 | 0 | ✓ | ✓ | ✓ | 0 |
| splash (unskipped, 2.5s ceiling) | 0 | 72,491 | 0 | ✓ | ✓ | ✓ | 0 |
| viewer3d | 0 | 111,448 | 0 | ✓ | ✓ | ✓ | 0 |
| images | 0 | 19,879 | 0 | ✓ | ✓ | ✓ | 0 |

14 live tests pass (10 examples + splash-unskipped + the RT5-1
characterization + child). The pty harness (`src/testing/pty.rs`) is
solid after the cycle-5 debugging — the CLOEXEC-on-master + drain-then-
close-master kill path (documented there) is what makes hung children
reapable on macOS.

## RT status table (RT1–RT6)

| Finding | Sev | Owner | Status |
| --- | --- | --- | --- |
| RT3-1 orient2d i64 overflow | P1 | GFX3D | CLOSED (c4) — still green |
| RT3-2 char-based input cursor | P2 | REACT | CLOSED (c5) — cluster-atomic editing; re-tortured c6 with flag/skin-tone/ZWJ families, still green |
| RT3-3 dead `Phase::Target` | P2 | REACT | CLOSED (c3→4) |
| RT3-4 wheel clamps target rect | P2 | REACT | CLOSED (c3→4) |
| RT4-1 image lifecycle unrefereed | P3 | GFX3D+REACT | CLOSED (c5) — real ImageSession lifecycle vs KittyModel; still green |
| RT4-2 clippy tally | P3 | all | CARRIED — whole-crate `--all-targets` = **31** warnings (was 29 c5, 43 c4); ZERO in VERIFY files. Concentrations: DESIGN `boot/identity.rs` field-reassign, GFX3D `png.rs`/`sixel.rs`, cross-crate const-value asserts. No `error`-level lint this cycle (the c5 `eq_op` cleared). |
| RT4-3 windows target | P2 | KERNEL | GREEN — `cargo check --target x86_64-pc-windows-msvc` clean this cycle |
| RT5-1 poll(/dev/tty) POLLNVAL on macOS | **P0** | KERNEL | **OPEN** — keystrokes never reach the app on the controlling-terminal path (the default `/dev/tty` open). Acceptance `RT5_1=1 … live_ctty_input_reaches_app` still FAILS. Interactive live-smoke cases run with `ctty=false` (the stdin/stdout fallback) to prove the app logic; the ctty path is keyboard-dead in any real macOS terminal until this is fixed. This is the single most important open finding. |
| RT5-2 JPEG SOS selector not validated | P3 | GFX3D | OPEN (informational) — scan binds by position; a selector naming a nonexistent component is silently accepted (harmless with one interleaved scan) |
| RT6-1 animation NaN-time panic | P2 | GFX3D | OPEN — acceptance test ignored; one-line fix demanded |
| RT6-2 no animated GLB in test set | P3 | GFX3D/DESIGN | OPEN (informational) — Model-level animation tests skip for lack of a subject |

## Perf re-baseline (release: `cargo test --release --test perf_budgets -- --ignored`)

| Budget | Measured (median) | Budget | Verdict |
| --- | --- | --- | --- |
| diff+present 200x60 full-change | 449 µs | 2 ms | ✓ |
| keystroke→frame via Driver::turn | 38 µs | 3 ms | ✓ |
| splash 2D fallback frame 100x30 | 99 µs | 2 ms | ✓ |
| brandmark 3D frame 100x30 | 417 µs | 8 ms | ✓ |
| parser 1MB hostile soup | 23 ms | 50 ms | ✓ |
| **grid solve 12 cols × 480 children** (new) | 738 µs | 3 ms | ✓ |
| **markdown parse+rich 1000-line doc** (new) | 1.42 ms | 20 ms | ✓ |
| **richtext wrap 800-para doc @ 60 cols** (new) | 15.9 ms | 20 ms | ✓ (tight) |
| flatten+diff+present 200x60 + Shimmer shader | 3.57 ms | 3 ms | **OVER** — see RT6-3 |
| VT model referee 200x60 styled frame | 3.11 ms | 2 ms | **OVER** — see RT6-4 |

### RT6-3 (P2, RENDER): shader-pipeline frame budget exceeded (3.57 ms vs 3 ms)

`flatten+diff+present` for a 200x60 frame with an active Shimmer cell
shader now medians 3.57 ms (was 421 µs at cycle-4 close). The richer
effects landing this cycle (gradient fills, drop-shadow, blur-ish dim
ramps) plausibly raised per-cell shader cost. The measurement is noisy
(best 1.16 ms, worst 7.6 ms — GC/thermal), so this is a WATCH not a hard
regression call, but the median crossed the 3 ms line. Demand (RENDER):
confirm whether the new effect passes run per-cell every frame even when
static, and whether the shimmer path can early-out on unchanged params.

### RT6-4 (P3, VERIFY→note): VT referee overhead 3.11 ms vs its own 2 ms budget

The referee model (`VtScreen`) feeding a full 200x60 styled frame medians
3.11 ms — over the 2 ms self-budget the rig sets for itself (the rig must
stay much faster than the engine it referees; the engine's own
diff+present is 449 µs). This is VERIFY's own budget to keep honest. The
model allocates a String per printed cell today (documented in
`alloc_budget`); the fix is a cell-string interner in `VtScreen`,
deferred — it does not affect correctness, only the referee's speed
margin. Recording it rather than silently widening the budget.

Both OVER items are measurement-of-record, not blockers; the correctness
property suites (which use these paths) all pass.
