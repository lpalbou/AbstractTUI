# Games backlog track — band 0700–0790

## Status
Proposed (FIELD study 2, 2026-07-22 — the retro-games feasibility pass,
`reviews/study2/field-games.md`). Numbering band: **0700–0790 only** —
this track never writes outside it. Sibling study-2 bands: media-av
(MEDIA), quality (QUALITY). Established bands: live-data 0010–0090,
app-widgets 0100–0190, ports 0200–0290, control-plane 0300–0390,
extensions 0400–0490, app-kits 0500–0590. Following the app-kits
precedent, the overview's counts/ledgers are NOT updated by this study
(parallel authors would collide there); folding this band into
`overview.md` is a named follow-up for the single-writer merge pass.
(FOLD DONE: convergence cycle 2, 2026-07-22 — track row, item rows,
counts, and band note landed in `overview.md`.)

## Purpose
The maintainer named retro-style games (roguelike RPG, BattleTech-DOS
hex tactics) as a future app surface. The feasibility read
(reviews/study2/field-games.md) found the engine closer than expected —
turn-based games are buildable today — and exactly four gaps that are
GENERAL capabilities justified beyond games (roadmap principle 1:
general-needs-first):

| ID | Title | Also serves |
| --- | --- | --- |
| 0700 | Key press/release state (held keys) — **COMPLETED 2026-07-23, wave 3 INPUTAV** (moved to `../../completed/games/`; `app::keys` + fidelity honesty; WASD held-pan proof in tests/wave_inputav.rs; media-av/0610 consumed it same wave) | push-to-talk (MEDIA), any hold-to-act UI |
| 0710 | Game tick: public frame tasks + fixed-timestep helper | shader clocks, physics-y dashboards, demos |
| 0720 | Sprite/tile toolkit: masked blit, sheets, cell-art palette swap | icon art, map thumbnails, any cell-art surface |
| 0730 | Board-grid math: square + hex coordinates | map viewers, seat/floor plans, pathfinding demos |

## Deliberate non-items (homes elsewhere)
- **Audio/SFX triggers** — MEDIA's band (media-av); games contribute the
  requirement shape (zero-latency trigger from a frame callback,
  `bell()` degradation), not an item.
- **Save-games** — control-plane 0340 Persist; games are another named
  consumer at its promotion.
- **Vector strokes / braille canvas** (hex outlines, minimaps) —
  extensions 0420; games inherit its public dot canvas.

## Sequencing
0700 and 0730 are v1-able any time, with two precisions from the
convergence pass (cycle 2, reviews/study2/convergence-cycle2.md):

- **0700 rides the 0293 chain**: the service lands independently, but
  its kitty-true fidelity on the majority macOS terminals (iTerm2,
  VS Code/Cursor, Warp) requires first-app/0293 first — the enter-time
  kitty flags are never re-pushed after the probe proves the protocol,
  so releases never reach the wire there. Chain: 0293 (enable flags
  post-probe) → 0700 (expose state) → media-av 0610 (consume).
- **0730's home (core vs sibling) is a 0400 classification** — the item
  now routes placement through the extensions decision table instead of
  presuming `base::grid` (0420 went core, 0440's pure layout math went
  sibling; both precedents are argued in the item).

0710 touches the reactive layer's public surface — small but wants a
design nod from the frame-pacing owner. 0720's masked blit is
independent; its sheet/palette half composes with `gfx` and can trail.
A first dogfood game (a roguelike example) is the natural 0590-style
validator once 0700 + 0730 land; do not file it as an item until then.
