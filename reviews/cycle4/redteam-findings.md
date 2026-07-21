# REDTEAM cycle-4 findings

Cycle shape: referee-first (DECSTBM landed and announced within the
first hour so RENDER's scroll-region work was never blocked), then
scroll workloads, the kitty lifecycle model, ledger verification, and
the deferred cross-platform/hygiene audit.

## RT3 fix-verification ledger

| Finding | Status | Evidence |
| --- | --- | --- |
| RT3-1 orient2d i64 overflow | **CLOSED (cycle 4)** | GFX3D clamped snaps to ±2^29 subpixels + screen-space guard-band; owner lifted the tagged ignore per R4-2; REDTEAM re-verified — magnitudes through f32::MAX safe (`adv_raster`) |
| RT3-2 char-based input cursor | OPEN (in progress) | `text::segments` landed (RENDER's helper); REACT's input adoption not yet in-tree at cycle close — acceptance test still `#[ignore = "RT3-2..."]`, still failing when force-run |
| RT3-3 dead `Phase::Target` | **CLOSED (cycle 3→4)** | fires at target now; acceptance green, ignore lifted |
| RT3-4 Scroll clamps against target rect | **CLOSED (cycle 3→4)** | nested-scroll wheel routing green, ignore lifted |

## Referee additions (cycle 4)

- **DECSTBM in `VtScreen`** (`tests/vt_regions.rs`, 13 ground-truth
  tests): margins set/clamp/ignore-invalid/home-on-set; LF/RI scroll
  the REGION at its margins and rows outside provably never move;
  cursor below the region sticks without scrolling; SU/SD region-
  scoped; IL/DL bounded by the bottom margin with column homing and
  outside-region no-op; ED/EL deliberately IGNORE margins (xterm);
  wide pairs move wholesale with scrolled rows; DECSTBM+wrap
  interaction covered. Announced to RENDER in redteam-requests.md.
- **DECSCUSR** tracked (`cursor_style()`), **OSC 52** tracked
  (`clipboard()`), both excluded from unknown-count — KERNEL's cycle-3
  emissions are now assertable, clean traffic.
- **Scroll-shaped workloads** in `testing::frames` (log-append /
  list-scroll / banded fixed-chrome / scroll+selection compound), each
  with full-width per-item-distinct rows so common prefixes cannot hide
  scroll cost. Property held for all shapes (`tests/adv_scroll.rs`).
- **`KittyModel`** (`src/testing/kitty_model.rs`): APC `_G` parsing with
  chunk reassembly (≤4096, non-final 4-aligned), transmit/place/delete
  accounting per id, leak set (`live_data_ids`), violation ledger
  (place-before-transmit, id 0, interleave-while-open, oversize
  chunks), and byte-exact tmux passthrough unwrapping.

## Scroll byte metrics (baseline, full-diff path, published for RENDER)

| Workload | Frames | Bytes/frame (baseline) |
| --- | --- | --- |
| log-append (90x28, 2 lines/frame) | 30 | 2,318 |
| list-scroll (70x20, down+up) | 45 | 1,607 |
| banded fixed-chrome (70x22) | 30 | 1,648 |

These are the numbers the scroll-region optimization exists to shrink
(a region scroll + fresh-rows emission should land well under half).
The property suite runs identically with the optimization on; re-run
`cargo test --test adv_scroll -- --nocapture` after enabling the flag
and publish the delta next to these.

## New findings

### RT4-1 (P3, GFX3D + REACT): image lifecycle surfaces exist only below the widget seam

`widgets::image` documents that protocol placement needs "a
post-present overlay pass owned by the app layer" — at cycle close
neither REACT's post-present seam nor GFX3D's `ImageSession` is
in-tree, so kitty lifecycle guarantees (transmit-once, delete-on-drop)
are currently enforced by NOTHING between the emitters and the app.
The rig is ready (`KittyModel` + `tests/adv_image.rs`, 5 tests green
against the raw emitters; the session lifecycle test is `#[ignore]`d
pending the API). Not a defect yet — a coverage hole that becomes one
the moment a widget ships using the emitters directly.

### RT4-2 (P3, hygiene, per owner): clippy audit snapshot

Whole-crate `cargo clippy --all-targets` at audit time: **79 warnings**
(incl. lib/test duplicates), zero in REDTEAM files after this cycle's
cleanup. Top sites by owner — all P3, none behavioral:

- REACT: `widgets/input.rs` 7 (mostly `type_complexity` on handler
  types), `ui/focus.rs` 3, `reactive/runtime.rs` 3, `widgets/table.rs`
  2, `widgets/button.rs` 2 — the `very complex type` lint (15 crate-
  wide) concentrates here; a couple of `type` aliases would clear it.
- DESIGN: `boot/identity.rs` 6 (`field assignment outside of
  initializer` — Default-then-assign constructor style).
- KERNEL: `input/parser.rs` 4 (loop-index lints, nul-terminated string
  construction in `unix.rs`).
- GFX3D: `gfx/png.rs` 3 (byte-grouping style in test constants),
  `gfx/proto/sixel.rs` 2 (`unused_mut` — real, one-line),
  `gfx/mosaic_fit.rs` 2.
- Cross-crate: 6 `assertion has a constant value` (compile-time
  invariant asserts — consider `const _:` form), 7 `manual
  is_multiple_of`.

Suggestion, not a demand: a once-per-cycle `cargo clippy --fix` pass
per owner keeps this near zero; REDTEAM will re-tally each cycle.

### RT4-3 (P2, KERNEL): Windows target accumulates warnings invisibly

`cargo check --target x86_64-pc-windows-msvc` is GREEN (verified this
cycle) but emits a warning in the windows-only code path that the
default build never surfaces (unused import in `term/windows.rs` at
audit time). Nothing gates on the cross-target build, so windows-only
rot is invisible until someone runs the check by hand. Demand: add the
target check to whatever pre-cycle-close ritual builders run (REDTEAM
runs it each cycle regardless and will file real breakage as P1).

## Verified-good this cycle (attack survived, no finding)

- **Scroll property**: all four scroll-shaped workloads model-exact
  through diff+present with zero unmodeled bytes; the banded shape's
  steady-state per-frame cost stays under one full paint (chrome not
  repainted).
- **Kitty emitters vs the lifecycle model**: transmit+display once,
  re-place on move without re-transmit, delete-by-id frees data, no
  leaks at end; 128x128 chunked transmit reassembles as ONE transmit
  with aligned non-final chunks; two images + interleaved presenter
  traffic account independently; tmux `unwrap(wrap(x))` byte-identical
  with identical accounting; truncated wrappers lose no bytes.
- **Region semantics under fuzz**: wide-pair invariant holds through
  every DECSTBM operation (the vt_regions suite + the standing
  wide-pair fuzz in rig_self, which now exercises region scrolls via
  the hostile corpus).

## Perf (release re-baseline at cycle close — see final report table)

Dashboard scripted-session perf test: PENDING — DESIGN's dashboard
example had not landed at cycle close; the test slot is reserved in
`perf_budgets.rs` commentary and lands with the example (cycle 5).
