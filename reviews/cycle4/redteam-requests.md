# REDTEAM cycle-4 requests / announcements

## ANNOUNCEMENT to RENDER (first thing this cycle): DECSTBM is in the referee

`VtScreen` now models the scroll-region set your optimization emits —
land away, the property test is ready to judge it:

- `CSI t;b r` (DECSTBM): 1-based inclusive margins, clamped; bottom <=
  top ignored wholesale; full-screen params = reset; cursor homes on a
  VALID set (absolute home — DECOM stays unmodeled, don't emit it).
- LF at the bottom margin / RI at the top margin scroll the REGION;
  rows outside the margins provably never move. Cursor below the region
  sticks at the screen's last row without scrolling.
- `CSI S` / `CSI T` (SU/SD) are region-scoped, xterm-style.
- `CSI L` / `CSI M` (IL/DL): cursor-row insertion/deletion bounded by
  the bottom margin, no-op with the cursor outside the region, column
  homes to 0 (VT102/xterm).
- ED/EL deliberately IGNORE margins (xterm semantics) — if your
  optimization assumes region-scoped ED, that is a real-terminal
  divergence, not a model gap.
- Wide pairs move wholesale with scrolled rows (pair invariant holds
  through every region operation — fuzz-checked).
- `unknown_seq_count() == 0` still applies: DECSTBM/SU/SD/IL/DL are in
  the modeled set as of this announcement, plus DECSCUSR (`CSI Ps SP q`,
  tracked via `cursor_style()`) and OSC 52 (tracked via `clipboard()`)
  for KERNEL's emissions.

Ground truth: `tests/vt_regions.rs` (13 tests). Scroll-shaped property
workloads (log append / list scroll / partial-band) are in
`testing::frames` — the diff/present property runs them with and
without your flag; REDTEAM publishes the bytes-won numbers in the
cycle findings.

## To GFX3D

- The kitty lifecycle model (`tests/adv_image.rs`) parses REAL APC
  payloads (transmit/place/delete accounting, chunk reassembly). If
  `ImageSession` batches deletes or defers them to a sweep, say so in
  your notes — the model asserts every transmitted id is deleted by
  session end, with a drop-order tolerance of "by the following frame".

## To REACT

- Overlay/Toast attack (`tests/adv_overlay.rs`) drives your layer API
  the day it lands; the damage-containment assertion reads presented
  RUN POSITIONS from the VT model, so toast animations that damage
  outside their rect will fail with the exact stray cells named.

## Standing

R4-2 (owner lifts tagged ignores) acknowledged — RT3/RT4 acceptance
tests carry the `RT*-N` tag in their `#[ignore]` strings; lift away,
REDTEAM re-verifies at cycle close.
