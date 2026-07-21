# AbstractTUI — Vision & Architecture Charter

AbstractTUI is a standalone Rust engine that treats the terminal as a real
display device. It must be the most efficient, versatile, responsive and
aesthetically serious way to build terminal applications — the foundation for
a family of future TUI apps.

## What makes it new (the bet)

Existing TUI stacks pick one of two camps: immediate mode with full-frame
redraw + diff (ratatui, FTXUI) or retained widget trees with coarse
invalidation (textual, ink). AbstractTUI takes a third architecture:

1. **Fine-grained reactivity (SolidJS-style, not VDOM, not immediate mode).**
   `Signal` reads are tracked; a write re-runs exactly the computations that
   depended on it, which damage exactly the screen regions they own. No
   full-frame rebuild, no tree diffing. Idle apps burn zero CPU; a blinking
   cursor damages one cell.
2. **A real compositor.** Z-ordered layers with RGBA alpha blending, per-layer
   offset/opacity (animations translate/fade layers without re-rendering
   their content), damage-region flattening, then frame diff, then a
   byte-emission stage that plays the terminal like an instrument (cursor
   motion economy, SGR run minimization, DEC 2026 synchronized output,
   truecolor with 256/16 downlevel).
3. **Graphics and 3D are citizens, not stunts.** One `gfx` pipeline serves
   bitmaps via the best available channel — kitty graphics > iTerm2 > sixel >
   unicode mosaic (half/quadrant/sextant/braille + dithering) — and `three`
   rasterizes GLB models (abstract3d / meshvault outputs) into that same
   pipeline. Every capability degradation is explicit and labeled.
4. **Design tokens end-to-end.** Widgets consume semantic tokens only; themes
   (ported from AbstractUIC's family) are hot-swappable at runtime via a
   signal. Contrast floors are test-pinned.

## Product requirements (from the maintainer)

- Dynamic widgets with reactive effects and triggers; clicks, hover, keyboard
  shortcuts; images; different themes; fully customizable "like a React web
  page" with shareable components and events.
- 3D: load and display GLB (e.g. `abstract3d/out/**.glb`,
  `meshvault/frontend/testmodels/*.glb`).
- Standalone: only low-level MIT/Apache dependencies when absolutely needed.
- macOS, Linux, Windows.
- Boot identity: a ~2s 3D splash animation with a clean visual identity,
  skippable, auto-disabled for non-TTY.

## Dependency policy (hard rule)

Allowed: `libc` (unix), `windows-sys` (windows), `unicode-width`,
`unicode-segmentation`, `miniz_oxide` (inflate for PNG). Nothing else without
an integrator-approved review note in `reviews/`. Notably hand-rolled: ANSI
emission, input parsing, flexbox solver, signals runtime, JSON (for glTF),
PNG chunking/defilter, base64, sixel encoding, 3D math + rasterizer.
Dev-dependencies for tests/benches follow the same spirit (std-first).

## Platform matrix

| Concern | macOS/Linux | Windows |
| --- | --- | --- |
| Raw mode / altscreen | termios via libc, /dev/tty | VT processing via windows-sys |
| Resize | SIGWINCH-safe + ioctl | console events / polling |
| Input | byte stream ESC parser | VT input preferred, fallback ReadConsoleInput |
| Rendering | identical ANSI path everywhere (VT on Windows 10+) | same |

## Performance budgets (bench-pinned, REDTEAM enforces)

- Full-screen 200x60 animated redraw: diff+present < 2 ms on dev machine.
- Idle: zero wakeups (event-driven; animations request frames).
- Steady-state frame: no heap allocation in diff/present hot path.
- Input latency: event -> present of damaged frame in < 5 ms for small damage.
- 3D: 80x24-cell viewport (160x96 px half-block) shaded mesh ≥ 30 fps.

## Quality bars

- Input parser never panics on any byte sequence (fuzzed).
- Diff/present correctness is property-tested against the in-crate VT model:
  emitted bytes applied to the previous screen == the intended screen.
- Terminal is always restored (panic hook + Drop), including cursor, modes,
  kitty keyboard flags.
- Public API compiles a real app in < 60 lines (ergonomics test).
- No `unsafe` outside `term::{unix,windows}` FFI boundaries.

## Layer map

`base` → `term` → `input` → `render`/`text` → `anim` → `reactive` →
`layout` → `ui` → `widgets` / `gfx` / `three` → `theme` → `app` → `boot`.
`testing` cuts across with a captured terminal + VT interpreter.

## Team & adversarial process

Six persistent agents: KERNEL, RENDER, REACT, GFX3D, DESIGN, REDTEAM.
Cycle cadence: build → adversarial attack (findings in `reviews/cycleN/`)
→ fix → repeat, 10 cycles. Findings are filed as
`reviews/cycleN/<attacker>-on-<module>.md` with severity (P0 blocks the next
build wave). Only the integrator edits `Cargo.toml`, `src/lib.rs`,
`src/base/*`, `OWNERSHIP.md`. Never commit; never mention AI tooling as
author anywhere in code or docs.
