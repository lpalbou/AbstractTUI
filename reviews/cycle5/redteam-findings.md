# VERIFY (formerly REDTEAM) cycle-5 findings

Continuity note: the verification seat kept the `redteam-*` review
filenames through cycle 5 for ledger continuity; the RT-numbered finding
series continues unbroken (RT5-*). Cycle 6 onward uses `verify-*`.

Cycle shape: compile-fix first (the tree had to build), then the live
PTY smoke harness (the headline), an independent JPEG pass, the formal
scroll-opt verdict, and the RT ledger (RT3-2 close, RT4-1 authored,
clippy/windows).

## Compile fix (routed breakage)

`tests/adv_splash.rs` and `tests/perf_budgets.rs` called the deleted
`BrandmarkRenderer::new`. Ported both call sites to
`BrandmarkRenderer::with_params(boot::brandmark3d::identity_params())`
(the public compatibility constructor + params adapter). Tree green
after the port: **970 passed / 46 ignored / 0 failed**.

## Headline: live PTY smoke harness

`src/testing/pty.rs` (the ONE sanctioned unsafe outside term FFI —
`openpty`/`dup`/`pre_exec(setsid+TIOCSCTTY)`/`fcntl`, each block with a
safety argument in the header) + `tests/live_smoke.rs`. Every example
built once, then run under a REAL pty (TERM=xterm-256color,
COLORTERM=truecolor, 100x30), given ~1.5-3s to render, scripted keys,
waited to exit. Per-example result:

| Example | Exit | Bytes | Unknown seq | Terminal restored |
| --- | --- | --- | --- | --- |
| hello | 0 | 6,667 | 0 | alt off, paste off, cursor shown, kitty depth 0 |
| themes | 0 | 11,602 | 0 | clean |
| widgets | 0 | 5,745 | 0 | clean |
| effects | 0 | 83,339 | 0 | clean |
| dashboard | 0 | 14,026 | 0 | clean |
| splash (skip) | 0 | 44,651 | 0 | clean |
| splash (unskipped, 2.5s ceiling) | 0 | 76,423 | 0 | clean |
| viewer3d | 0 | 123,999 | 0 | clean |
| images | 0 | 19,879 | 0 | clean |

All twelve live tests pass (`--ignored`, ~12s). Referee gaps closed to
model KERNEL's new verbs: OSC 9 / OSC 99 notifications tracked, kitty
keyboard query `CSI ?u`, XTSMGRAPHICS read `CSI ?…S`, XTWINOPS `CSI …t`
consumed as legal query traffic (not counted unknown).

## RT5-1 (P0, KERNEL): poll(/dev/tty) reports POLLNVAL under a pty on macOS — keystrokes never reach the app on the controlling-terminal path

The strongest bug this cycle, found only because the pty harness runs
the REAL `/dev/tty` path. When the pty is the child's CONTROLLING
terminal (how every real terminal emulator runs an app), input is
silently dropped:

- Characterization (`rt5_1_poll_devtty_characterization` + its child,
  both in `live_smoke.rs`): with a byte already queued, `poll(2)` on the
  `/dev/tty` fd returns `rc=1, revents=0x20` (POLLNVAL) while the SAME
  input polls readable (`revents=0x1`) on the stdin fd (the pty replica).
- `src/term/unix.rs` masks `POLLIN | POLLHUP | POLLERR`, so a POLLNVAL
  wake matches nothing → the byte is never read. `FIONREAD` on the slave
  confirms the queue GROWS (1, then 3 bytes) while the app draws happily
  and ignores every key.
- Impact: on macOS, any app using the `/dev/tty` open path (the default
  when stdin/stdout aren't both the tty, and the preferred path) is
  keyboard-dead in a real terminal. The smoke suite's interactive cases
  only pass because they run with `ctty=false` (the stdin/stdout
  fallback), which is NOT the default path.
- Repro: `RT5_1=1 cargo test --test live_smoke live_ctty_input_reaches_app
  -- --ignored --nocapture` (acceptance test; env-gated so the live
  suite stays green while the finding is open).
- Demand (KERNEL): on macOS, either open `/dev/tty` such that `poll`
  works (some macOS setups need the tty opened without `O_NONBLOCK` at
  open time, or a `select`/`kqueue` path), or detect POLLNVAL and fall
  back to the stdin fd. A pty is the canonical test bed — if poll can't
  see it there, it can't see iTerm2/Terminal.app either.

## Independent JPEG pass (decode-or-reject-never-panic)

GFX3D shipped a truncation ladder + 600-case marker soup + dimension
guard. The independent pass (`tests/adv_jpeg.rs`, 11 tests) attacks the
half those can't reach — STRUCTURALLY VALID but pathological entropy —
via a byte-level flat-JPEG builder (`src/testing/jpeg_build.rs`) whose
Huffman code assignment is fully controlled:

- **Deep single-code Huffman trees** (the named soft spot): a valid
  table whose only symbol sits at each canonical length 1..=16 decodes
  correctly at EVERY length (flat field ≈ 128); no length wedges or
  misdecodes. Truncated deep-tree entropy rejects cleanly.
- Oversubscribed tables (codes overflow their bit length) and
  count/value-mismatch DHTs rejected by name.
- Component-count lies (SOF 3 vs SOS 1), dangling quant/entropy table
  refs, sampling factors > 2, wrong restart-marker index — all named
  errors, no panics, no OOB indexing.
- Restart-interval edges (0, 1, 2, 3, 255) all decode.
- 600 seeded mutations across 4 bases (incl. deep-tree bases): 23
  decoded (sane dims), 577 rejected, **0 panics**.
- Allocation budget (`alloc_budget.rs`): the 65535×65535 dimension bomb
  is rejected having allocated < 64 KB (guard fires BEFORE any plane
  alloc); a hostile-corpus batch stays bounded per attempt.

No JPEG defects found — the decoder's guards are sound. Finding-adjacent
note filed as **RT5-2 (P3, GFX3D, informational)**: the SOS component
selector byte is not validated against SOF component ids (the scan binds
by position). Harmless today (single interleaved scan), but a selector
that names a nonexistent component is silently accepted; worth a named
error if multi-scan is ever revisited.

## Scroll-opt referee verdict (RENDER default flip GATE)

RENDER's `compute_scrolled` + `emit_scrolled` (ScrolledRuns token) run
through the DECSTBM VtScreen across the frames-harness workloads plus 6
seeds of random scroll+mutation. **Property held (bytes-applied == target,
zero unknown sequences, DECSTBM never left set) in every case** — the
flip gate is satisfied.

| Workload | Baseline B/frame | Scroll-opt B/frame | Win | Frames shifted |
| --- | --- | --- | --- | --- |
| log-append (90x28) | 2,318 | 273 | **8.5×** | 27–30/30 |
| list up+down (70x20) | 1,607 | 178 | **9.0×** | 43/45 |
| banded, band-tight damage (70x22) | 1,648 | 202 | **8.2×** | 29/30 |

Damage-shape note for RENDER: the banded (fixed chrome) workload engages
the optimization only when damage is reported TIGHT to the scrolling
band (rows 1..h-1). Under full-frame damage the union's edge rows are
unchanged chrome, the anchor heuristic finds no alignment, and it
declines to plain repaint (still correct, just no win). Real damage
tracking reports the tight rect, so this is the expected path — recorded
so a future "why didn't my full-redraw scroll optimize" question has an
answer.

## RT ledger

| Finding | Status | Evidence |
| --- | --- | --- |
| RT3-1 orient2d i64 overflow | CLOSED (c4) | still green |
| RT3-2 char-based input cursor | **CLOSED (c5)** | REACT adopted cluster indexing via `text::segments`; ignore lifted; re-verified + TORTURED (`input_editing_is_cluster_atomic_under_hard_clusters`): FR/JP regional flags, skin-tone modifiers, MWGB family, laptop-ZWJ, combining acute — cluster-atomic under Backspace/Delete/Left+insert, verified against the engine's own segmentation |
| RT3-3 dead Phase::Target | CLOSED (c3→4) | green |
| RT3-4 wheel clamps target rect | CLOSED (c3→4) | green |
| RT4-1 image lifecycle unrefereed below the widget seam | **CLOSED (c5)** | `ImageSession` landed; the reserved `unreachable!()` placeholder is now the REAL test (`tests/adv_image.rs`, 4 new): transmit-once, no-retransmit-on-move (id transmit_count stays 1 across 3 moves), version-bump frees the old id + mints a new one, `release`/`release_all` free every upload, full lifecycle under tmux wrap — all refereed by `KittyModel` id accounting, zero leaks, zero violations |
| RT4-2 clippy tally | UPDATED | see below |
| RT4-3 windows target | GREEN | `cargo check --target x86_64-pc-windows-msvc` clean this cycle |

## RT4-2 clippy delta (per owner)

Whole-crate `cargo clippy --all-targets`: **79 → 29 warnings** (cycle 4
→ cycle 5). Zero in VERIFY-owned files (fixed this cycle: two
`unnecessary_cast` in adv_overlay, two `unusual_byte_groupings` in
adv_jpeg/adv_scroll). Remaining top sites, all P3/non-behavioral, by
owner:

- DESIGN: `boot/identity.rs` 6 (`field_reassign_with_default` — Default-
  then-assign constructor style), `examples/dashboard` 2.
- GFX3D: `gfx/png.rs` 2, `gfx/proto/sixel.rs` 2 (`unused_mut` — real,
  one-line), `gfx/quantize.rs` 1, `gfx/jpeg_entropy.rs` 1.
- Cross-crate: 6 `assertion has a constant value`, 5 `unusual byte
  groupings`, 2 `manual is_multiple_of`, 2 `type_complexity`.

One item worth a look (not a VERIFY finding, flagged for the owner):
clippy reports one `error`-level lint (`eq_op`, "equal expressions as
operands to ==") in a builder file — with `-D warnings` that would fail
CI. Owner should confirm it's an intentional self-comparison test.

## Perf re-baseline — see final report table (all budgets green).
