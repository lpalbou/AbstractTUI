# AbstractTUI examples

Owner: DESIGN. These are the acceptance targets the engine builds toward:
each demo is the smallest program that proves a layer of the stack in front
of a human. A cycle that claims a capability ships or upgrades the matching
demo — "it works" means the demo runs, looks right, and survives resize.

Every example exits 0 with a one-line notice when there is no interactive
terminal, so `cargo run --example <name>` is safe anywhere (CI included).
`dashboard`, `viewer3d` and `images` also take `--caps`: print the
capability report and exit — the diagnostic surface, no tty needed.

| example | status |
| --- | --- |
| `hello.rs` | REAL — the ergonomics acceptance: full app in 53 lines |
| `dashboard/` | REAL — the flagship: charts, log tail, sortable table, toasts, modal, pane nav |
| `gallery.rs` | REAL — the design system on one screen; the visual regression surface |
| `themes.rs` | REAL — live theme gallery + preview pane + measured contrast ratios |
| `widgets.rs` | REAL — widget gallery: focus/hover/disabled states, tabs, scroll |
| `effects.rs` | REAL — compositor layers: Shimmer/Dissolve/HueDrift shaders, transforms, toasts |
| `splash.rs` | REAL — 3D brandmark or 2D fallback through one player (auto/`--3d`/`--2d`) |
| `viewer3d.rs` | REAL — orbit a GLB: modes, light steering, measured fps |
| `images.rs` | REAL — four mosaic families side by side, dither, protocol placement |
| `components.rs` | REAL — the shareable-component reference (props/children/events) |
| `grid.rs` | REAL — track-grid reflow: fr/cells/percent tracks + spans |
| `feed.rs` | REAL — live background data: bursty worker → bounded ingestion → Feed with follow-tail; drop counter, events/sec, zero-idle proof |
| `transcript.rs` | REAL — streaming conversation: markdown answers typeset live block-by-block, code tint, follow-tail break/re-pin, 10k stress |
| `capture.rs` | TOOL — deterministic screenshot pipeline into `docs/captures/` |
| `common/` | shared helpers (small-terminal guard, key legend) — not a target |

## hello

The 60-second first contact: a rounded, surface-filled panel with the
wordmark, a reactive counter line bound to a signal — 53 lines
including docs, ONE import line (the prelude). Proves the public API
ergonomics bar from the vision doc (a real app in < 60 lines).

- Keys: any key counts, `q`/Ctrl+C quits.
- Needs: any tty; `ABSTRACTTUI_THEME=<id>` themes it.
- Looks like: one calm centered panel, accent title, muted hint line.

## dashboard

The flagship. Header bar (mark + UTC clock + theme name), nav sidebar
(List), braille rx/tx line chart with legend, load cluster (ramped
Progress + Sparkline histories), live event log tail (level-coherent,
ellipsis-clipped), sortable sessions Table, toasts, focus-trapped help
modal, optional spinning 3D mark panel. Startup degradations arrive as
staggered auto-dismissing toasts (REACT's reactive notices bridge);
`caps:` summary lines stay off the glass. Deterministic sin/hash data
walks — no rand, no wall entropy.

- Keys: Tab focus, Alt+arrows pane-hop (spatial nav by geometry), arrows
  select rows, `s` sort, `n` toast, `b` 3D mark (truecolor only), `?`
  help, Ctrl+T theme, `q` quit.
- Needs: 80x24 minimum (guarded below 40x10), gorgeous at 120x35;
  truecolor for the 3D mark. Env: `ABSTRACTTUI_START_THEME=<id>`,
  `ABSTRACTTUI_FIXED_CLOCK=<secs>` (capture determinism), `--caps`.
- Looks like: a shipped ops product — elevated panels on a quiet ground,
  one accent doing the work, data moving only where data lives.

## gallery

The whole design system on one screen: token swatches (grounds, text
tiers, semantics, chart ramp, syntax-on-raised, border pair), every
widget state (badges, action/disabled buttons, input, Select trigger,
multiline TextArea, checkbox + selection pair, progress ramp, spinner
families, focused pane ring), and a content column (2-series line chart,
bar chart, syntax-colored code, a diff-tinted patch, rich markdown). One
keypress restyles the entire board — the theme-switch acceptance surface
and the marketing screenshot. Below ~104 cols the content column bows
out and the board stays composed.

- Keys: `t`/`T` cycle themes, Tab focus, Enter/space activate, `q` quit.
- Needs: 104+ cols for all three columns; degrades to two.
- Looks like: a design-system poster that repaints under one key.

## themes

Every registered theme as a card grid (name + nine-token swatch strip on
the ACTIVE ground), arrow-key navigation with scroll, Enter applies via
`set_theme_by_id` — the entire screen restyles through the one theme
signal. A live preview pane (≥ 96 cols) renders a miniature app mock in
the SELECTED theme's own tokens before you apply. The bottom panel shows
measured contrast ratios (text/muted/faint/accent/selection) from
`theme::contrast_ratio`.

- Keys: arrows move, Enter applies, `q` quits.
- Needs: 96+ cols for the preview pane; guarded below 40x10.
- Looks like: a paint-store wall where the swatch card you point at
  becomes a little application.

## widgets

The widget gallery. Tabs split "interactive" (button — incl. a disabled
one in `text_faint` outside the focus order — text input, selectable
list) from "visual" (border families with the focus ring, badge tones,
ramped progress, spinner sets, separators) inside a vertical Scroll.

- Keys: Tab focus, arrows in lists, F2 advances spinners, Ctrl+T theme,
  `q` quit. Mouse hovers/clicks.
- Needs: any tty; guarded when tiny.
- Looks like: §3 of the style guide, rendered — every state visible.

## effects

Compositor-level: overlay layers via `app.overlays()` wearing RENDER's
cell shaders — a Shimmer title, a Dissolve-in panel, a HueDrift-breathing
accent card — plus layer ColorTransforms and REACT's Toast. One
`reactive::after` loop advances shader clocks at 30 fps.

- Keys: `d` replays the dissolve, `m` cycles dim/grayscale/tint, `n`
  toast, `p` pauses the clock (app goes fully idle), `q` quit.
- Needs: truecolor recommended (shaders quantize below).
- Looks like: motion with restraint — three shader accents on a still UI.

## images

One image, four mosaic families side by side (halfblock 1x2 / quadrant
2x2 / sextant 2x3 / braille 2x4) with aspect-correct fitting; `d`
toggles a 16-color median-cut + Floyd–Steinberg pre-dither, `p` places
the image through the pixel-protocol ladder with the chosen channel
named (kitty/iterm2/sixel/mosaic — degradation visible, never silent).
Takes a PNG/JPEG path or generates a procedural test card.

- Keys: `d` dither, `p` protocol placement, `t` theme, `q` quit.
  `--caps` prints the capability report.
- Needs: any tty; pixel protocols where the terminal offers one.
- Looks like: the same picture four ways, sharpening left to right.

## viewer3d

`cargo run --example viewer3d -- model.glb` (defaults to the workspace
test assets — helmet, x-wing — with friendly instructions when absent).
Titled chrome shows filename + triangle count; the status row carries a
MEASURED fps (painted frames over a 1 s window). Degradations surface in
a reactive warn-ink footer line (notices bridge); `caps:` lines stay off
the glass.

- Keys: drag orbits, wheel zooms, space toggles spin, `1-4` mosaic
  modes, `l/L` light azimuth, `r` reset, `t` theme, `q` quit. `--caps`.
- Needs: a GLB with embedded buffers; truecolor recommended.
- Looks like: a lit, textured model turning inside themed chrome.

## components

The reference for the shareable-component claim: three reusable
components (clickable `stat_card` with props + `on_click`, `field`
composition wrapper, `toolbar`) composed repeatedly with different props
into a settings screen; live signals flow input → summary as you type;
cards carry `Block::shadow` elevation. The form also hosts the choice
family — a channel `Select` sharing its signal with the radio group, a
theme `Combobox` applying live, a features `MultiSelect`. Heavily
commented — this file is documentation.

- Keys: Tab focus, Enter/space activate, type in inputs, `q` quit.
- Needs: any tty.
- Looks like: a settings page built from three lego bricks, edits
  echoing live into the summary card.

## grid

`Display::Grid` live: three track recipes (equal fr · fixed+fr ·
percent-framed) over the same children, cycled with `g`; a col_span hero
card; fr largest-remainder tiling visible on resize.

- Keys: `g` cycles recipes, `t` theme, `q` quit.
- Needs: any tty; resize to watch tracks re-tile.
- Looks like: the same cards snapping between three different skeletons.

## feed

Live background data done the sanctioned way: a worker thread produces
bursty synthetic log events into `bounded_source` (capacity, overflow
policy, honest drop counters), rendered by `Feed` (keyed rich items,
windowed paint) inside `Scroll` with the engine's follow-tail. A whole
burst arrives as ONE repaint; the quiet gaps are byte-for-byte idle;
the status line counts dropped events honestly; events/sec samples
through `reactive::interval`. Drag-select is enabled throughout — drag
paints the highlight, releasing (or `c`) copies via OSC 52.

- Keys: space pauses/resumes the producer · `f` jumps to the tail ·
  wheel/arrows scroll · drag selects, `c` copies · `q` or Ctrl+C quits.
- Needs: any tty.
- Looks like: a log pane filling in bursts, pinned to the tail until
  you scroll up, with a drop counter that never lies.

## transcript

The streaming-conversation proof: scripted turns stream in token by
token through `Feed` + `md::StreamSession` — closed blocks freeze,
only the open block re-typesets, code fences tint from their opening
line — while follow-tail breaks on scroll-up and re-pins at the
bottom; an `s` stress toggle rebuilds with 10,000 history items to
prove windowed drawing. The bottom composer is a `TextArea` (grows
1..4 rows, Enter sends, Alt+Enter newline, ↑↓ history at the buffer
edges) with `/` command + `@` mention completion in an anchored
dropdown at the caret.

- Keys (composer focused, its keys win while typing): Enter send ·
  Alt+Enter newline (Shift+Enter on kitty) · ↑↓ caret then history ·
  `/help` `/theme` `/clear` `/quit` · Ctrl+C quit. Tab off the
  composer for `f` re-follow, space pause, `s` stress, `q`.
- Needs: any tty.
- Looks like: a chat client answering itself — markdown typesetting
  live under a composer that completes your commands.

## splash

Plays the 2-second identity sequence from `docs/design/theme-identity.md`
§2 through the real splash player — wall-clock pacing with frame drop,
per-frame skip checks, hard 2.5 s cutoff, tty/env gates
(`boot::should_splash`). Default AUTO picks the three-planes 3D "A" on
truecolor terminals and the pure-cell 2D fallback (with its own particle
field) elsewhere; both read the same `boot::identity` constants (the
drift test pins the shared beats). The brand sign-off surface.

- Keys: any key skips (fast fade).
- Needs: any tty; truecolor for the 3D source. Force with
  `--3d`/`--2d`/`ABSTRACTTUI_SPLASH`; `ABSTRACTTUI_NO_SPLASH=1`,
  `TERM=dumb`, `NO_COLOR` auto-skip. `ABSTRACTTUI_THEME=<id>` grounds it.
- Looks like: three planes flying into an A, a spark burst on the
  alignment beat, the wordmark tracking open — gone in two seconds.

## capture (tool)

The deterministic screenshot pipeline: runs the built examples under a
real pty at fixed sizes/themes, interprets the bytes with the testing
rig's `VtScreen`, and dumps plain + styled text renders into
`docs/captures/` — plus `themes-table.md` (every theme's token hex from
the registry), in-process splash stills (2D/3D at the burst and settled
beats), and in-process APP stills driven headlessly through
`Driver` + `CaptureTerm` (streaming transcript with the completion
dropdown open, an open Select popup, a diff-tinted `CodeView`, a feed
with follow-tail broken) — those four are clockless and
byte-deterministic. The docs cycle embeds these as fenced "screenshots".

- Run: `cargo build --examples && cargo run --example capture`
  (`-- themes|splash|shots|apps` for one family).
- Needs: unix `script(1)` for the pty shots; nothing for the rest.
