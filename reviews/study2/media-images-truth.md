# MEDIA study 2 — the image-path truth

Date: 2026-07-22. Engineer: MEDIA. Scope: the whole pixel-image pipeline —
capability detection (`src/term/caps.rs`, `src/term/probe.rs`), the three
protocol emitters (`src/gfx/proto/{kitty,iterm2,sixel}.rs`), the placement
lifecycle (`src/gfx/session.rs`, `src/gfx/pipeline.rs`,
`src/app/driver_images.rs`), and the demo (`examples/images.rs`).

The maintainer's doubt — "I am unsure the 'view image' part ever worked" —
was **well-founded**. The protocol *emitters* were byte-correct against
their specs, but the *lifecycle around them* had four real defects, three
of which guaranteed visible corruption on real terminals the moment an
image moved, changed, or was removed. All four are fixed in this cycle
with tests; the emitters needed one wire-level fix (kitty placement ids).

## Verdict table

| Leg | Verdict | Evidence |
| --- | --- | --- |
| Capability detection (env pass) | **works** | Conservative and correct: kitty via `KITTY_WINDOW_ID`/`TERM=xterm-kitty`, iTerm2/WezTerm via `TERM_PROGRAM`, sixel only via DA1 (never name folklore), tmux zeroes graphics until passthrough is *proven* (caps.rs:283-300). Env-injectable tests cover the matrix. |
| Capability detection (active probe) | **works** (one soft spot) | kitty `a=q` probe with unique id + DA1 sentinel is the spec's own recipe; `CSI 16 t` cell size sanity-clamped; XTSMGRAPHICS register cap folded. Soft spot: the reply check is substring-based (`contains("i=4242")` matches `i=42421`) — filed as 0688, no live path emits colliding replies today (all our emissions are `q=2`). |
| kitty emitter bytes (chunking, `f=32/24/100`, `o=z`, `q=2`, deletes) | **works** | Spec-checked against sw.kovidgoyal.net/kitty/graphics-protocol: 4096-byte chunks, non-final 4-aligned, keys on first escape only, `m` sequencing, zlib flag, PNG geometry omission — all pinned by `src/gfx/proto/kitty.rs` tests + `tests/adv_proto.rs` structural validators + the `KittyModel` referee. |
| kitty placement lifecycle (move) | **was broken — fixed** | `a=p` without a placement id **accumulates placements** (spec: pid-less puts on the same image id create multiple placements) — every `ImageSession` move left a ghost at the old rect on kitty/ghostty. Fixed: fixed placement id `p=1` on `a=T` and `a=p` → same (i,p) **replaces** (the spec's flicker-free move). Tests: `kitty_session_move_keeps_exactly_one_visible_placement` (tests/adv_image_lifecycle.rs), `pid_placements_replace_and_anonymous_ones_accumulate` (kitty_model.rs). |
| Image-overlay placement (all channels) | **was broken — fixed** | The driver blit double-offset mosaic patches (grid positions are already screen cells; `render_images` added `job.rect.origin()` AGAIN — driver_images.rs), so overlay images painted at 2× their offset. The cycle-4 test asserted only "a painted cell exists" and the theme-cleared background satisfied it. Fixed (blit origin ZERO) + screen-truth test `mosaic_move_and_remove_restore_the_vacated_cells`. |
| Vacated-cell repair (move/remove/channel switch) | **was broken — fixed** | Three-way hole: mosaic corpses lived in the root surface (tree damage ≠ surface damage — nothing repainted them); iTerm2/sixel corpses lived in the terminal over cells the diff model believed unchanged (equality suppressed the erasing repaint); a caps upgrade switched channels and stranded the old channel's pixels. Fixed: `Driver::pre_image_pass` (driver_images.rs) folds vacated rects into the frame's tree damage + poisons `prev` for cursor-paint channels + invalidates overlapping cursor-paint slots so they re-emit. Tests: the lifecycle suite (3 channels). |
| tmux passthrough | **was broken for real images — fixed** | The whole multi-escape kitty stream was wrapped in ONE `ESC Ptmux;` frame; tmux hard-caps a single input sequence at 1 MiB (tmux/tmux#487) and **discards** larger ones — real photographs vanished silently. Fixed: one wrapper per escape (`tmux_wrap_per_escape`, pipeline.rs), pinned by `tmux_wrap_is_per_escape_so_the_1mib_input_cap_cannot_bite` incl. byte-exact unwrap round-trip. Single-frame protocols (iTerm2 OSC, sixel DCS) still wrap whole — they cannot be split; their >1 MiB behavior is documented in 0688. |
| Scroll optimization × live images | **was broken — guarded** | Terminals scroll protocol images WITH the text (kitty spec: "images must be scrolled along with text"; sixel pixels scroll on xterm-class), desyncing the session's placement bookkeeping. The driver now takes the plain diff while byte-channel images are live (`ImageSession::live_byte_slots`); the byte-win-preserving upgrade (re-place by id after a shift) is filed as 0675. |
| Ladder warnings | **was silent — fixed** | `RenderedImage.warnings` (`#FALLBACK …`) were dropped by the driver; they now reach the startup-notices lane, deduped per distinct warning. |
| iTerm2 emitter bytes | **byte-correct, terminal-unverified** | `OSC 1337 File=inline=1;size=…;width=N;height=N…:base64 BEL` — keys and framing match iterm2.com/documentation-images.html; payload is a real PNG (decoder round-trip pinned in iterm2.rs tests). No iTerm2 on this machine (host is Terminal.app) — see the recipe below. |
| sixel emitter bytes | **byte-correct, terminal-unverified** | `DCS 0;1;0 q` + raster attrs + percent-scaled registers + `!`RLE/`$`/`-` — validated by a full test-side sixel *decoder* replaying emissions to pixels (sixel.rs tests: exact two-color replay, multi-band, transparency holes, register base/budget, dither brightness-preservation). Bottom-row scroll hazard is real on most emulators and filed as 0680. |
| kitty end-to-end on a real kitty | **terminal-unverified** | Byte-level: every emission passes the strict frame parsers + the KittyModel referee (chunk rules, id accounting, leak-freedom, tmux unwrap round-trip). This machine has no kitty/iTerm2/WezTerm/ghostty installed (`/Applications` checked; host terminal is `Apple_Terminal`, which supports none of the three protocols) — the maintainer recipe below is the remaining step. |
| Live PTY (mosaic leg) | **works, live-verified** | `cargo test --test live_smoke live_images -- --ignored`: real pty, real termios, `d`/`p`/`t`/`q` scripted → exit 0, 0 unknown sequences, no panic text (run 2026-07-22, post-fix). Under `TERM=xterm-256color` the ladder correctly lands on mosaic. |

## Bugs found and fixed (test names)

1. **Kitty move ghosts** — pid-less `a=p` accumulates placements.
   Fix: `p=1` on `a=T`/`a=p` (src/gfx/proto/kitty.rs); KittyModel learned
   replace-on-same-(id,pid) + pid-scoped deletes (src/testing/kitty_model.rs).
   Tests: `kitty_session_move_keeps_exactly_one_visible_placement`,
   `pid_placements_replace_and_anonymous_ones_accumulate`,
   `place_and_delete_forms`, updated
   `transmit_place_move_delete_accounting_via_emitters`.
2. **Mosaic double-offset + move/remove corpses** — blit origin fixed to
   `Point::ZERO`; `Driver::pre_image_pass` repaints vacated rects from
   tree truth. Tests: `mosaic_move_and_remove_restore_the_vacated_cells`
   (failed against the old code, byte-for-byte proof in the test run log).
3. **iTerm2/sixel corpses invisible to the diff** — vacated rects poison
   `prev` so byte-identical cells re-emit and overwrite the terminal-held
   pixels. Test: `sixel_move_and_remove_force_cell_reemission_under_the_old_rect`.
4. **tmux 1 MiB discard** — per-escape wrapping. Test:
   `tmux_wrap_is_per_escape_so_the_1mib_input_cap_cannot_bite`.
5. **Scroll-shift desync (guard)** — plain diff while byte images live.
   Session census: `slot_info_and_byte_slot_census_track_the_channels`,
   `invalidate_slot_forces_reemission_but_refuses_kitty`.
6. **Silent ladder degradations** — warnings forwarded to notices (deduped).

Whole tree after the wave: **1,449 passed / 0 failed**, clippy zero
warnings, fmt clean. The changed-behavior surface is additive: two new
`ImageSession` methods, one new driver pass, one emitter key (`p=1`),
per-escape tmux wrapping; no public API removed or changed shape.

## What "worked" all along

The mosaic path (the `widgets::Image` widget and the four-panel demo) was
always correct — half-block/quadrant/sextant/braille rendering, fitting,
and dithering are solid and well-tested. The maintainer's experience of
"images seem to work" on an ordinary terminal was the mosaic path. The
*protocol* path (the `p` key in `examples/images.rs`, `Overlays::image`
on a kitty/iTerm2/sixel terminal) is the part that had never really been
exercised on real terminals — and it had exactly the ghost/corpse bugs
you'd expect of an unexercised path. The static single placement WOULD
have displayed correctly on kitty/iTerm2; anything dynamic then corrupted.

## Verify on your terminal (10 lines)

```sh
cargo run --example images -- --caps   # 1. what the env pass claims (any terminal)
cargo run --example images            # 2. mosaic panels — should look right EVERYWHERE
# 3. press 'p'  → picture appears top-right; footer says which channel drew it
# 4. press 'p' again → picture disappears COMPLETELY (no ghost, no leftover strip)
# 5. press 'd' / 't' a few times → panels re-render; the placed image stays intact
# 6. press 'q' → clean exit, no stray pixels on the shell afterwards
# Expected per terminal:
#   iTerm2/WezTerm → footer "placed via iterm2", real pixels; kitty/ghostty → "via kitty";
#   xterm -ti vt340/foot → "via sixel"; Terminal.app → "mosaic (env pass; …)" and the
#   placement is a colored half-block mosaic (Terminal.app has no pixel protocol).
```

Under tmux (kitty/iTerm2 outer): needs `set -g allow-passthrough on`; the
probe verifies the route and the footer should still name the pixel channel.

## Out-of-scope findings (filed, band 0600)

See `docs/backlog/proposed/media-av/`: 0660 images-in-Feed, 0665 animated
image sessions, 0670 cell-size refresh under font zoom, 0675 scroll
re-place upgrade, 0680 sixel bottom-row clamp + DECSET 8452, 0685
probed-caps app signal (the example's channel label is env-pass-only
today — labeled in the example), 0688 detection/transport robustness
(probe reply strict parse; single-frame >1 MiB protocols under tmux;
iTerm2 `MultipartFile`).

## Hardest open questions

1. **WezTerm's kitty lane**: WezTerm answers the kitty `a=q` probe when
   `enable_kitty_graphics` is on, which promotes it past its (complete)
   iTerm2 lane onto its (historically partial) kitty lane — placement-id
   replace semantics there are unverified. If the maintainer sees ghosts
   on WezTerm specifically, the ladder may need a WezTerm-names-itself
   demotion (XTVERSION is already parsed).
2. **Erase-vs-scrollback**: our corpse-erase repaints cells, which is
   correct for the alt screen the engine runs on — but kitty keeps
   *uploads* (not placements) until deleted, and `driver.finish` deletes
   only what the session knows. A crash between transmit and session
   save leaks terminal-side image memory until the terminal's quota
   eviction; only a kitty `a=d,d=A` at enter would be stronger, and
   that would erase OTHER programs' images. Deliberately not done.
