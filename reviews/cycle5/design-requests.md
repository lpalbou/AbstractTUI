# DESIGN cycle-5 requests + verification notes

## To REACT — cycle-5 items VERIFIED + ADOPTED (landed mid-cycle)

1. All three landed and the dashboard adopted them the same evening:
   `use_viewport(cx)` replaced the guard-overlay `Rc<Cell<Size>>`
   hand-tracking (Modal/Toast placement now follows live resizes);
   `focus_signal` is wired on the nav List AND the sessions Table with
   the pane Blocks' rings bound to them — §3.2's composition rule is
   finally demonstrable in the flagship. **D4-1 re-verified**: header
   ink is `text_muted`; nord measures ~3.98:1 on `surface_raised`
   (was ~2.1 with accent_alt) — floor cleared with margin, finding
   closed. One nit: `use_viewport` is exported from `app` but not yet
   in the prelude — integrator, worth adding at the next refresh.

## To GFX3D

2. **JPEG**: `jpeg_entropy.rs`/`jpeg_fixtures.rs` are in-tree but no
   public decode surfaced by my close — `examples/images.rs` sniffs PNG
   only and says so in its header. When `gfx::jpeg::decode` (or a
   unified `gfx::decode_image`) lands, the example gains the second
   format in one match arm; a unified sniffing entry point would be my
   preference (extension-blind, magic-byte honest).
3. **Viewport3D**: `Camera::framing` + drag/wheel + spin made viewer3d
   almost writing itself — nice seam. One observation from the fps
   probe: at 100x30 half-block the spin cadence holds 30 fps in debug
   builds on this machine; braille (2x4 subpixels) is the heavy mode.
   No ask — just data for your perf budget.

## To RENDER

4. The splash byte capture (pty eyeball, see below) shows frames emitted
   WITHOUT `?2026` sync brackets: `splash_present_caps` delegates to
   KERNEL's `From<&Capabilities>`, and under `script`'s pty the env pass
   claims no sync support (TERM=xterm-256color only) — correct behavior,
   noted so nobody reads the missing brackets as a presenter bug. On
   kitty-class terminals the brackets appear.

## Pty eyeball results (task: watch the 3D splash bytes)

`script -q` + `stty rows 30 cols 100`, NO_COLOR unset, truecolor env:

- `--3d`: 210 KB over the 2 s timeline, 5.9k truecolor SGR runs, 5.1k
  half-block glyphs — the mark renders and animates; skip hint + tagline
  + wordmark all present (wordmark arrives per-letter, so it greps as
  fragments, verified via the letter runs); mode bracketing exact
  (1049h/25l on enter, full reverse order + SGR reset on leave); clean
  `completed (2.0s timeline)` exit.
- `--2d`: 10 KB, mark strokes + tagline + same clean bracketing.
- Degenerate pty (0x0 from bare `script`): enters, runs the timeline,
  renders zero cells, restores cleanly — no panic, no garbage. Good
  accident: that is now a verified edge case.
- Art verdict: timings stand as constants — no retune needed this cycle
  (the 0.9 s alignment beat reads clearly even at 30 fps half-block).

## To REDTEAM

5. New surfaces: three ORIGINAL themes (`abstract-aurora`, `-paper`,
   `-ember`) through the full audit with zero new exceptions — attack
   their derived tokens like the ports'; `examples/images.rs` test-card
   determinism + the aspect-fit math (mode subpixel densities); the
   dashboard's `b` flourish claims byte-identical idle damage when OFF —
   verify with the capture rig (it renders a zero-width element).
