# QUALITY — adversarial review of the study-2 image fixes

Date: 2026-07-22 · tree: 0.2.1 post-release working tree (MEDIA fixes
applied) · owner: QUALITY (image-fix review)

Scope: the five bug classes fixed in `reviews/study2/media-images-truth.md`
— (1) kitty move ghosts / `p=1`, (2) mosaic blit double-offset +
`Driver::pre_image_pass`, (3) iTerm2/sixel prev-poison, (4) tmux
per-escape wrapping, (5) the scroll-optimization guard — checked against
the damage contract, multi-image correctness, cost honesty, idle
honesty, and framing correctness. Small fixes were allowed with tests;
one behavioral gap was fixed in this review (F1), one is demand-filed
(F2).

## Verdict summary (per fix)

| # | Fix | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | kitty `p=1` replace semantics | **SOUND, hardened** | `p=1` rides `i=<id>` on both `a=T` (kitty.rs:113) and `a=p` (kitty.rs:144); placements key on the (image id, pid) PAIR, so two images cannot collide — model scopes pids per image id (kitty_model.rs:56-60, 278-292). New driver-level test `kitty_two_images_move_and_remove_independently`: both images move 3 rounds, each keeps exactly 1 placement / 1 transmit; removing one leaves the other placed. |
| 2 | blit origin + `pre_image_pass` | **SOUND for vacated rects; gap found + fixed (F1)** | Vacated-rect chain verified closed end to end (below). The same corpse class fires WITHOUT a placement change — content repainting BENEATH a parked image erased mosaic patches with no repair. Fixed this review (channel-aware repair scan); test failed pre-fix. |
| 3 | iTerm2/sixel prev-poison | **SOUND** | Poison + damage fold are coupled for the same rect (driver_images.rs:122-132); the diff only scans flatten damage, and the chain damage.push → `fill_rect` damage_span (surface.rs:314-338) → flatten → diff-over-poisoned-prev is closed. Poison lives exactly one frame (phase-S whole-frame blit, driver.rs:538-539) — no double emission, no lingering state. |
| 4 | tmux per-escape wrap | **SOUND, hardened** | Wrap applies only under `Some(WrapKind::Tmux)` (pipeline.rs:313-317) — non-tmux bytes untouched; the existing test proves byte-exact unwrap round-trip against REDTEAM's independent `unwrap_tmux`. Added a per-wrapper byte bound (≤16 KiB) so a chunking regression cannot hide behind a passing wrapper census (F6). |
| 5 | scroll guard | **SOUND, scoped tightly, cost measured** | Guard reads `live_byte_slots()` (driver.rs:505), which counts only non-mosaic slots (session.rs:264-269) — mosaic apps keep the optimization. Measured cost: **10.2x bytes/frame** with one parked kitty image (below). Byte-win restoration is 0675's re-place-by-id upgrade. |

## Task 1 — damage-contract compliance of `pre_image_pass`

**Where it runs**: inside `render_frame`, after phase L
(`app.tree().layout()` / `overlays.layout_all()`, driver.rs:414-416),
immediately after `take_damage()` (driver.rs:429), before
`coalesce_damage` (driver.rs:436) and the phase-D clear+redraw. The
"damage set seals HERE" marker sits at the frame decision
(driver.rs:377).

**Epoch-rule verdict: compliant in spirit, letter needs one amendment
line.** The contract (§2) says the damage set is sealed when phase L
begins; `pre_image_pass` adds rects after that. But the rule's
mechanism is "no user code past phase U ⇒ no re-entrant damage", and
the pass runs zero user code: its inputs (`ImageEntry::dirty`,
`retired_images`, entry rects) are written exclusively through
phase-U paths (`ImageHandle::set_rect/set_bitmap/remove`,
`apply_caps_upgrade` — all overlays.rs:713-763 / driver.rs:705-721,
reached from event dispatch or posted jobs). It is the same class of
driver-owned translation as phase L's own geometry-damage fold. Every
consequence lands inside this frame: pushed rects are clipped by
`coalesce_damage`, painted in D, flattened in C, diffed in P.

**Prev-poison double-emission/flicker analysis**: `poison_prev_rect`
mutates only `self.prev`, which is read once (phase-P diff) and then
overwritten whole by the phase-S blit (driver.rs:538-539) — the poison
cannot survive into a second frame, so a poisoned cell re-emits exactly
once. There is no exit path between the poison and the diff (no `?`
between driver.rs:435 and driver.rs:515), so no orphaned poison. All of
it — erasing cell runs, kitty deletes, re-placements — rides ONE
`term.write` + ONE flush (driver.rs:531-535), so there is no
multi-write flicker window.

**One pre-existing atomicity note (F3, not introduced by this wave)**:
protocol payloads are appended AFTER `emit_scrolled` closes the DEC
2026 bracket (`?2026l` at present.rs:244-246; the
`pending_image_bytes` drain is driver.rs:520-522). One wire burst, but
the sync bracket does not cover the image bytes — a 2026-honoring
terminal may present the cell frame and the image placement in two
paints (one-frame image lag on a move). Cosmetic; recorded for a
future presenter-bracket change, no action now.

## Task 2 — multi-image correctness of `p=1`

The kitty spec keys placements on the (image id, placement id) pair.
The emitter always sends `p=1` **together with `i=<image id>`**
(kitty.rs:113, kitty.rs:144), and each overlay image gets a distinct
image id (`ImageRenderer::next_kitty_id`, pipeline.rs:200-203; session
invariant 3 pins id uniqueness across slots, session.rs:324-359). The
KittyModel referee stores pids **per image id**
(`placement_pids: BTreeMap<image_id, BTreeSet<pid>>`,
kitty_model.rs:56-60) and replaces on the same pair
(kitty_model.rs:278-292), which is ghostty's storage shape.

No two-image MOVE test existed (the session accounting test moves one
of two slots but never asserts per-image placement counts). Added
`kitty_two_images_move_and_remove_independently`
(tests/adv_image_lifecycle.rs): two overlay images through the real
driver, three simultaneous-move rounds — each image id must hold
exactly ONE live placement and ONE transmit after every round; removing
image A must leave B's upload and placement intact; zero model
violations. Passes; a cross-image pid collision or a pid-less
regression would fail the placement census immediately.

## Task 3 — scroll-guard cost honesty

**Scoping (verified tight)**: the guard is
`self.image_session.live_byte_slots() > 0` (driver.rs:505);
`live_byte_slots` filters `channel != Channel::Mosaic`
(session.rs:264-269). Mosaic placements live in the cell model and
scroll correctly through the ordinary diff, so a mosaic-only app keeps
the DECSTBM+SU win — pinned at driver level by the new
`scroll_guard_scopes_to_byte_channel_images_only` (three legs: no
image → shift sequence present; parked kitty → no shift, no APC bytes,
plain frames; parked mosaic → shift still present).

**Measurement** (`perf_feed_scroll_with_parked_protocol_image_90x30`,
tests/perf_app_surfaces.rs, `#[ignore]`d; byte counts identical in
debug and release, run 2026-07-22):

| scenario | bytes/frame (median of 24) | note |
| --- | --- | --- |
| 90x30 log scroll, no image | **172 B** | shift engaged all 24 frames |
| 90x30 log scroll, parked kitty image | **1,758 B** | plain diff all 24 frames, zero APC bytes |
| ratio | **10.2x** | first paint 3,006 B — the plain frame is ~58 % of a full paint |
| 40x12 small screen (lifecycle test print) | 106 B → 126 B | 1.2x — small screens barely pay |

Lines are deliberately distinct (real transcript rows); repeating tails
had understated the cost at 1.8x. The guard's price is real for feed
apps with one parked protocol image — backlog 0675 (re-place by id
after a shift) is worth building; until then the guard buys pixel
correctness at ~10x scroll bytes, which is the right trade.

(The 40x12 lifecycle test also prints a parked-MOSAIC leg at 231 B/frame
with the shift still engaged — larger than both others because a static
mosaic image over shifting rows makes its columns residual repaints
every frame AND the F1 repair re-blits it; that is the honest cost of a
cell-model image parked over scrolling content, unrelated to the guard.)

## Task 4 — idle honesty with a parked protocol image

Extended the idle-alloc pin (the prior wave had NOT added an image):
`idle_turns_with_feed_interval_parked_popup_and_parked_image_allocate_nothing`
(tests/alloc_budget.rs, renamed from
`…_and_parked_popup_allocate_nothing`; the two living-doc references in
docs/architecture.md and llms-full.txt updated; the old name in
reviews/study2/quality-perf.md is a historical record and stays).
Setup now runs kitty caps + one parked overlay image, with a
precondition that the placement really went through the byte channel
(`ESC _G` in setup bytes). The pin holds: **16 idle turns = 0 allocs /
0 reallocs / 0 bytes**.

`pre_image_pass` early-out honesty: on idle turns it never runs (no
frame). On rendered frames with a clean placement, the retired drain
and the `moved` collect are empty-iterator collects (no allocation),
and the pass returns at `damage.is_empty()` (driver_images.rs:133)
before the repair scan; with damage but no repair-eligible slots the
scan's `collect` is also empty → no allocation. The restructure for F1
(the early-out moved BELOW the vacated fold so the repair scan can see
plain tree damage) preserves the zero-work steady state.

## Task 5 — tmux framing

- **Non-tmux byte identity**: wrapping happens only under
  `caps.wrap == Some(WrapKind::Tmux)` (pipeline.rs:313-317 for the
  renderer; session.rs `wrap_for` for session-authored place/delete);
  `WrapKind` has exactly one variant, so `None` is the entire non-tmux
  space and those bytes flow untouched. The intended non-tmux delta of
  the wave is the `p=1` key itself, verified separately (task 2).
- **Doubled-ESC per wrapped escape**: `term::tmux_wrap` doubles every
  inner ESC (verbs.rs:167-178) and each APC chunk gets its own wrapper
  (`tmux_wrap_per_escape`, pipeline.rs:331-347). Pinned by byte-exact
  `unwrap(wrap(x)) == x` against REDTEAM's independently implemented
  `unwrap_tmux` (two implementations agreeing), wrapper count == inner
  frame count, and doubling visible as `ESC ESC _G`
  (pipeline.rs tests; tests/adv_image.rs lifecycle-under-tmux; session
  test `tmux_wrap_covers_session_authored_escapes`).
- **Hardening added (F6)**: the census alone could pass while a
  chunking regression regrew individual wrappers toward the 1 MiB
  discard cap. The per-escape test now also asserts every wrapper stays
  ≤ 16 KiB (wrappers are emitted back to back, so consecutive header
  offsets delimit them). Base64 payloads cannot contain ESC, so
  splitting at `ESC \` boundaries can never cut a payload — checked
  against the emitters: iTerm2's OSC is BEL-terminated (single frame,
  wraps whole), sixel's DCS is one ST-terminated frame (split is a
  no-op on it). The >1 MiB single-frame protocols remain 0688's filed
  limit, as the MEDIA report already said.

## Numbered findings

**F1 — parked mosaic image decays under beneath-repaints (P2, FIXED
this review, with a pre-fix-failing test).** The five fixes repaired
vacated-rect corpses, but the same corpse class fires with NO placement
change: tree damage under a parked image clears+redraws those cells
(driver.rs:440-447), erasing the blitted patches, and nothing re-marked
the image dirty — only rects vacated by the pass itself triggered the
old overlap re-emit. A parked mosaic image corroded row by row over any
live content (ticking status line, streaming feed). The
`Overlays::image` doc even promised re-render "(or the frame damages
its rect)" — an unimplemented claim. Fix: `pre_image_pass` step 4 is
now a channel-aware repair scan (driver_images.rs:139-165): mosaic
slots invalidate + re-dirty against ANY damage rect (the re-blit is
wire-free — the diff suppresses byte-identical cells), iTerm2/sixel
keep the vacated-rects-only trigger (see F2), kitty needs nothing
(floats above cells). Test
`mosaic_image_survives_content_repaint_beneath_it` — verified failing
against the pre-review code (patches replaced by red-on-red text
cells), passes with the fix; the doc comment now tells the truth.

**F2 — iTerm2/sixel parked images decay under beneath-repaints (P2,
DEMAND filed).** Same trigger as F1, but the repair for cursor-paint
byte channels is a FULL payload re-emission per damaged frame — a
parked sixel photograph over a streaming feed would re-transmit
hundreds of KB per token. That needs a design decision (throttle,
damage-hole punching, or placement discipline), not a reflex fix; it is
exactly the design space of backlog 0660 (images in content widgets),
whose Feed image block will hit this immediately. Documented honestly
on `Overlays::image` and in the pass docs; behavior unchanged.

**F3 — protocol payloads ride outside the DEC 2026 bracket (P3,
observation, pre-existing).** See task 1. One write/one flush holds;
presentation atomicity of cells+image does not. Not introduced by the
wave; candidate future presenter change.

**F4 — kitty vacated rects are folded into tree damage though kitty
needs no cell repair (P4, no action).** The repaint diff-suppresses to
zero bytes (cells under a kitty placement are never modified in the
model), so the cost is a bounded phase-D redraw. Uniform code beats a
third channel branch; recorded so nobody "optimizes" the fold away for
iTerm2/sixel, which DO need it.

**F5 — the damage contract's phase table predates the image passes
(P4, docs).** §1 lists U/L/D/C/P/S; the driver now runs a pre-D image
pass, a D2 image reconcile, and post-P external writes. The epoch rule
survives (task 1), but the contract should name the image sub-phases so
the next reader doesn't have to re-derive the compliance argument. The
contract accepts amendments via reviews/ — this file is that note.

**F6 — tmux wrapper census lacked a size bound (P3 hardening, FIXED).**
See task 5.

**F7 — measurement/test additions (this review).**
`kitty_two_images_move_and_remove_independently`,
`scroll_guard_scopes_to_byte_channel_images_only`,
`mosaic_image_survives_content_repaint_beneath_it`
(tests/adv_image_lifecycle.rs);
`perf_feed_scroll_with_parked_protocol_image_90x30`
(tests/perf_app_surfaces.rs, `#[ignore]`d); idle-alloc pin extension +
rename (tests/alloc_budget.rs); per-wrapper bound
(src/gfx/pipeline.rs test). CHANGELOG updated under Unreleased.

**F8 — `ImageHandle` silently no-ops when the overlay store borrow is
held (P4, latent, no action).** `with_entry`/`remove` return on
`try_borrow_mut` failure (overlays.rs:723, 754). Unreachable
today — user code runs in phase U where the driver holds no store
borrow, and `dispatch` snapshots targets before running handlers — but
the silent-refusal pattern is a trap for a future caller inside a
driver phase. Recorded only.

## Battery + semver (final tree state)

| gate | result |
| --- | --- |
| `cargo test` (whole tree, debug) | **1,452 passed / 0 failed / 82 ignored** (baseline 1,449 + 3 new; 82 ignored includes the new perf measurement) |
| `cargo clippy --all-targets` | **zero warnings** |
| `cargo fmt --check` | **clean** |
| `cargo test --test alloc_budget -- --test-threads=1` | **9/9** (incl. the extended idle pin) |
| `cargo semver-checks` vs registry 0.2.1 | **196 checks: 196 pass, 57 skip — "no semver update required"** |
| release run of the guard measurement | identical byte numbers to debug (10.2x) |

New public API since the released 0.2.1 (verified absent at the release
commit): `ImageSession::live_byte_slots`, `ImageSession::slot_info` —
additive only; `invalidate_slot` is `pub(crate)`,
`Driver::pre_image_pass` is `pub(super)`, `p=1` and per-escape wrapping
are wire-behavior changes, not API shape. The fixes fit a compatible
0.2.x release.
