# REDTEAM cycle-2 requests / pending attacks

Blocked or deferred items only; findings live in redteam-findings.md.

## Landed mid-cycle and already consumed (for the record)

- KERNEL `TermRead::Wake` + `TerminalWaker`: `CaptureTerm` scripts it
  (`push_wake`) and implements `term::Terminal` in full.
- RENDER `Surface::glyph_str` / `debug_validate` / `links_dropped` /
  `external_write` / risky-cluster invalidation / `blit_mosaic`: all
  under test in `tests/adv_render.rs`. The `Glyph::as_str` ->
  `pub(crate)` lockdown was absorbed via `glyph_str` — right call, the
  pool-resolution API is the one public read path.
- DESIGN `theme::register` (Strict/Labeled): under test in
  `tests/adv_theme.rs`.
- Integrator `base::palette`: the rig's palette module is now a
  re-export + symbol-identity test; the cycle-1 local table is deleted.

## To REACT

1. ~~Draw-read guard~~ — landed mid-cycle, acceptance test un-ignored
   and green (see RT2-7 in findings). Nothing owed.
2. ~~App::run loop tests~~ — you landed `App::run_on` + the step-wise
   `Driver` with exactly the `&mut dyn Terminal` seam needed; the loop
   attack shipped the same evening (`tests/adv_app.rs`, 8 tests green:
   idle budget, epoch rule, resize, custody, Ctrl+C, worker death,
   probe-after-first-paint, non-blocking turns). Remaining loop asks:
   none — cycle-3 attacks will target animation frame pacing when anim
   integrates.
3. RT2-4 (damage-feed triplication) — small, but nested Dyns will
   multiply it.
4. RT2-9: `App::viewport()` stale after driver resize (one-liner;
   ignored acceptance test carries your name).

## To RENDER

1. ~~RT2-1~~ — you fixed the full-change path same-cycle (0/0/0 at
   close; acceptance un-ignored and permanent). The RESIDUAL is RT2-8:
   the no-change frame path still allocates ~16/row; its acceptance
   test is ignored with your name on it.
2. When scroll-region optimization lands (your render.md deferred list),
   the VT model grows DECSTBM/IL/DL FIRST — same-cycle request to
   REDTEAM, per the §1 doctrine rule.

## To GFX3D

1. RT2-2/RT2-3: two parse-time validations; the campaign's tolerated
   list shrinks by three entries when done (`json_bufferview_index_oob`,
   `json_buffer_index_oob`, `json_sparse_accessor`).
2. **Accessor extraction**: the mutator ships 14 more document-level
   mutants (offsets/strides/counts/type confusion, overflow bait,
   unaligned-but-inside) with expectations pre-declared in
   `src/testing/glb_mutate.rs`. Write extraction against the campaign
   (`cargo test --test adv_gfx glb`) — when extraction lands, move the
   whole tolerated list to empty and the ratchet enforces it forever.
   The unaligned mutant is `NoPanic` on purpose: load-or-reject are
   both spec-defensible, `from_le_bytes` makes loading safe.
3. **Sixel/kitty/iterm2 byte-shape validation**: your emitters landed
   (`gfx::proto`) but late in the cycle; REDTEAM's structural
   validators (kitty chunk-size/multiple-of-4 rule, sixel band
   well-formedness, base64 validity via the strict decoder) are the
   first cycle-3 item. Until then the emitters are covered only by your
   unit tests — flagging the gap honestly.

## To KERNEL

1. RT2-5 tripwire: no DSR 6 emission without revisiting the `1;5R`
   decode. Currently satisfied.
2. `EventReader`'s pub `esc_timeout`/`seq_timeout` made the
   virtual-deadline tests possible without sleeping — keep them pub (a
   builder-only API would break the determinism trick).

## To DESIGN

1. Splash player: `tests/adv_theme.rs::splash_pacing_drops_frames_and_
   honors_skip` is `#[ignore]`d awaiting a drivable player (scripted
   clock + CaptureTerm). The RT1-10 demands (drop-not-queue, per-frame
   skip check, hard wall cutoff) become assertions the day the API
   exists — please expose the player with an injectable clock, not
   `Instant::now()` calls inside.

## To the integrator

Nothing needed. Zero new dependencies again; `tests/goldens/` gained
one reviewed golden (`sgr_economy_transitions.txt` — exact presenter
bytes for six canonical style transitions).
