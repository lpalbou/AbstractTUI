# REDTEAM cycle-2 findings

Method: the cycle-2 attack suites (`tests/adv_{input,render,reactive,
gfx,theme}.rs`, `tests/alloc_budget.rs`, `tests/perf_budgets.rs`) run
against the tree as of this filing. Every finding has a repro test in
the tree; acceptance = the named test passing (un-ignored where noted).
Transient mid-cycle build breaks were retested before filing and are
not findings.

## RT2-1 (P1, RENDER): steady-state diff+present allocated on every
## frame — FOUND, then FIXED same-cycle; acceptance test now permanent

First measurement (2026-07-20, warmed 200x60 full-change frame,
truecolor caps, `tests/alloc_budget.rs`): **3,643 allocations +
68 reallocs, ~10.5 KB per frame** — vs the charter's ZERO. At 60 fps
that is ~216k allocs/sec in the engine's tightest loop.

RENDER's same-cycle rework brought the full-change steady state to
**0 allocs / 0 reallocs / 0 bytes** for both stages (attribution
ratchet output at close). `diff_present_steady_state_allocates_nothing`
is now un-ignored and permanent: a regression re-opens the finding.
This is the adversarial process working as designed — measured, filed,
fixed, pinned inside one cycle.

## RT2-8 (P2, RENDER): the NO-CHANGE frame path still allocates

Residual of RT2-1: two IDENTICAL frames through diff+present allocate
**~385 allocs + 8 reallocs on 80x24** (≈16 per row) even though zero
runs are produced and zero bytes emitted. No-change frames are the
idle-adjacent common case (a parked app repainting after a wake, a
cursor-blink frame with terminal-native cursor). Acceptance:
`presenter_no_change_frame_emits_and_allocates_nothing`
(`#[ignore = "RT2-8..."]`), un-ignore on fix.

## RT2-2 (P2, GFX3D): `Doc::parse` accepts out-of-range buffer/view indices

Mutants `json_bufferview_index_oob` (accessor -> bufferView 7 of 2) and
`json_buffer_index_oob` (view -> buffer 3 of 1) parse successfully.
Index validation needs NO binary data — it is pure metadata arithmetic
available at parse time. Deferring it to extraction means every
consumer between parse and extraction handles dangling indices.

Repro: `tests/adv_gfx.rs::glb_mutant_campaign...` tolerated-list
entries. Demand: validate index ranges in `Doc::parse`; remove the two
entries from the tolerated list (the test then enforces forever).

## RT2-3 (P2, GFX3D): sparse accessors accepted at parse

`json_sparse_accessor` parses. `docs/design/gfx-three.md` promises
"sparse rejected loudly", and sparse-ness is visible in pure metadata.
An accepted-then-ignored sparse accessor would silently render wrong
geometry (the sparse substitutions dropped) if extraction forgets to
re-check — reject at the door instead.

Repro/acceptance: same ratchet mechanism as RT2-2.

## RT2-4 (P3, REACT): one Dyn remount emits three identical damage rects

`dyn_remount_damages_exactly_its_region` observed
`[Rect{13,0,7,4}; 3]` for a single signal write: dispose-damage,
remount-damage and (apparently) a duplicate. Correctness is unaffected
(union is right; compositor coalesces) but the damage feed is 3x the
translation work per remount, and the pattern will multiply across
nested Dyns. Test currently tolerates ≤ 4 rects — tighten to ≤ 2 when
fixed.

## RT2-5 (P2, KERNEL): the `CSI 1;5R` collision is pinned, with a tripwire

Decoded as Ctrl+F3 (documented choice; CPR wins for param0 != 1).
`tests/adv_input.rs::csi_1_5_r_ambiguity_pinned_as_f3` pins BOTH sides
and panics with a named message if the resolution ever flips silently.
The hazard goes live the day ANY module emits DSR 6 (`CSI 6 n`) — the
cursor-position probe habit from other codebases. Demand: grep-level
review rule — no `\x1b[6n` emission anywhere without revisiting this
decode (the probe module currently uses `CSI 16 t` and DA1, which are
safe).

## RT2-6 (P3, DESIGN, verified-good): the audit exception mechanism held

`everforest-light` text/surface_raised measures 4.25:1 (< 4.5 floor).
It is DECLARED in `AUDIT_EXCEPTIONS` with an inline justification, and
the adversarial test now enforces both directions: undeclared
violations fail, and STALE exceptions (that no longer fire) also fail.
The valve is capped at 2 entries in the test — growth is a finding.

## RT2-7 (verified-good, REACT): draw-read guard landed mid-cycle and
## passed acceptance

`RT1-2`'s guard landed while this suite ran (`report_draw_read`:
debug panic naming the offending node; release counts + bounded
samples). The adversarial test un-ignored and passed the same day:
a tracked `signal.get()` in a draw closure panics in debug builds
(premise-checked: the closure really ran), and `get_untracked` — the
sanctioned peek at captured data — stays silent. The stale-pixel bug
class is now structurally loud.

## RT2-9 (P3, REACT): `App::viewport()` lies after a driver-handled resize

`Driver::apply_resize` resizes its surfaces and calls
`app.tree().set_viewport(..)` directly, bypassing `App::set_viewport` —
`App::viewport()` then reports the pre-resize size for the rest of the
session. Anything consulting it (an overlay placing itself by viewport,
a debug HUD) works with stale geometry. One-line fix; acceptance test
`tests/adv_app.rs::app_viewport_tracks_driver_resize` is `#[ignore]`d
with this id.

## Verified-good this cycle (attack survived, no finding)

For the record, the claims that HELD under attack — each now has a
permanent regression test:

- **THE diff/present property** (RENDER): 10 seeds x 25 cumulative
  frames truecolor + 4x15 xterm256 + 3x15 ansi16 + 20 baseline-caps
  frames, sizes to 120x40, content incl. CJK, VS16, ZWJ families,
  combining marks, links, all 9 attrs: model-exact, zero unknown
  sequences, balanced 2026 brackets. (`tests/adv_render.rs`)
- **Downlevel contrast preservation** (RENDER): 800 random fg/bg pairs
  with contrast ≥ 2:1 stayed distinct through BOTH 256 and 16-color
  quantization, verified through the public pipeline.
- **Wide-pair + pool invariants under hostile blits** (RENDER): 300
  seeded blit rounds with mid-pair clip edges, `debug_validate` plus an
  independent sweep, then a full present-property check each round;
  ZWJ pool adoption across surfaces resolves through the destination
  pool.
- **external_write custody §6** (RENDER): payload appears exactly once,
  SGR/link closed before it, cursor moved absolutely, full re-sync
  after — model-verified.
- **Risky-cluster CUP invalidation** (RENDER, the RT1-7 demand): a ZWJ
  family forces an absolute CUP before the next glyph. Landed and
  verified.
- **Parser fuzz posture** (KERNEL): 1,200-chunk hostile corpus one-shot
  + streamed, split-invariance across 5 chunk sizes on 66 streams, all
  single bytes + ESC pairs, cap-abort with correct consume-to-final
  resync, bounded paste chunks (byte-exact reassembly), zero
  text leakage from swallowed sequences.
- **Probe discipline** (KERNEL): dumb terminals get zero query bytes;
  mute terminals end at the deadline; keystrokes survive mid-probe;
  LATE replies surface as CapsReply events, never text (`RT1-6` all
  three demands verified).
- **Enter/leave balance** (KERNEL): every DECSET has its DECRST and
  kitty push has pop across 5 option shapes, VT-model-checked.
- **Reactive core** (REACT): diamond + nested-pull dedupe (their §12
  risk 1), mid-flush disposal of queued effects, self-disposing
  ancestor mid-run, 10k nested-Dyn churn with flat node/slot counts,
  batch coalescing with read-after-write coherence, cleanup LIFO
  ordering, dispose-during-dispatch (modal close) without panics or
  dead-handler fires.
- **THE frame loop** (REACT `Driver`, landed late cycle — attacked the
  same evening, `tests/adv_app.rs`): idle turns emit ZERO bytes and
  zero renders across 16 turns; a cross-thread posted signal write
  lands exactly one frame later, once (epoch rule §2); resize between
  key events re-layouts + repaints with prev poisoned; the whole
  session (enter -> frames -> leave) is modeled traffic with balanced
  modes and no open 2026 bracket; raw-mode Ctrl+C quits; a panicking
  `spawn_worker` surfaces as a NAMED app error (RT1-15b delivered);
  first paint never waits for the probe and late DA1 replies upgrade
  caps mid-session (RT1-6a); 1000 turns on an exhausted script never
  block.
- **Theme registration audit** (DESIGN, the RT1-9 demand): strict
  refuses text==bg with a structured violation list; labeled admits
  with surfaced warnings; reserved ids refuse in both modes; invalid
  ids named; polarity lies caught; indecisive grounds caught.
  Derivation walks held for 2,000 random palettes (never worse than
  the raw ink / the raw ground). Chart ramps pairwise-distinguishable
  on every built-in.
- **glTF JSON parser** (GFX3D): number grammar corners, surrogate
  pairs (combine + 6 rejection shapes), 200-deep nesting rejected
  without stack overflow, dup-keys-first-wins, 4 MB escape-free string
  fast path, invalid UTF-8 rejection.
- **PNG decoder** (GFX3D): dimension bombs rejected by the cap before
  allocation, CRC corruption rejected (documented stance verified),
  chunk-length lies + full truncation ladder + 400 seeded mutations
  panic-free, IDAT size lies rejected both directions, bad filter
  byte rejected, type-3-without-PLTE rejected, out-of-scope features
  named in errors.
- **Licensing** (integrator): unchanged from cycle 1 — no new deps.
