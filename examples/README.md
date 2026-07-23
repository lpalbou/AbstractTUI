# AbstractTUI examples

Each demo is the smallest program that proves a layer of the stack in
front of a human — it runs, looks right, and survives resize.

Every example exits 0 with a one-line notice when there is no interactive
terminal, so `cargo run --example <name>` is safe anywhere (CI included).
`dashboard`, `viewer3d` and `images` also take `--caps`: print the
capability report and exit — the diagnostic surface, no tty needed.
`ABSTRACTTUI_THEME=<id>` themes any example from the environment. Every
source file opens with a teaching header: what it shows, the keys, and the
docs section it illustrates — the examples are meant to be READ, not just
run.

## The learning path

Read (and run) them in this order — each step assumes the vocabulary of
the previous one:

| step | example | teaches |
| --- | --- | --- |
| 1 · first contact | `hello.rs` | the whole engine in 53 lines: one prelude import, a signal, a themed panel |
| 2 · widgets | `widgets.rs` | the widget gallery: focus/hover/disabled states, tabs, scroll, closable panels (click ✕ — survivors re-flex) |
| 2 · widgets | `components.rs` | HOW to build your own: props, children slots, typed events — plus the choice controls and `Disclosure` |
| 2 · widgets | `gallery.rs` | the whole design system on one board; one keypress restyles it |
| 2 · widgets | `themes.rs` | every built-in theme, applied live, with measured contrast ratios |
| 3 · layout | `grid.rs` | track grids (`fr`/cells/percent/auto), spans, live reflow |
| 4 · interaction | `activate.rs` | selection vs activation on `List` and `Table`; double-click, honestly |
| 4 · interaction | `decide.rs` | the modal decision gate (`ChoicePrompt`): confirmations, multi-pick, chains |
| 5 · content + live data | `feed.rs` | background threads → bounded ingestion → `Feed` with follow-tail |
| 5 · content + live data | `transcript.rs` | streaming markdown chat: tables render live, composer with completion |
| 5 · content + live data | `attachments.rs` | file attachments: terminal drops classified out of pastes into chips, `FilePicker` in a modal |
| 5 · content + live data | `reader.rs` | the document surface: GFM tables, in-flow images, TOC, search |
| 5 · content + live data | `reasoning.rs` | the reasoning controls: `ThinkingFold` streaming (last-wins), `ReasoningSelect` three capability states |
| 5 · content + live data | `voice_mock.rs` | push-to-talk, dB meter, band spectrum, scope — no audio needed |
| 6 · the app shell | `shell.rs` | `PageHost` full pages behind one tab bar + `Drawer` panels from both edges |
| 6 · the app shell | `drawers.rs` | the drawer system alone: modal inspector vs passive nav panel |
| 6 · the app shell | `dashboard/` | the flagship capstone: charts, log tail, sortable table, toasts, modal, pane nav |
| 7 · graphics + 3D | `images.rs` | four mosaic families, dithering, pixel-protocol placement |
| 7 · graphics + 3D | `effects.rs` | compositor layers wearing cell shaders; transforms, toasts |
| 7 · graphics + 3D | `splash.rs` | the 2-second boot identity, 3D or 2D through one player |
| 7 · graphics + 3D | `viewer3d.rs` | orbit a GLB with textures, animation, measured fps |
| 8 · extensions | `workflow` / `network` (in `extensions/graph/`) | graph auto-layout (layered / force) + `GraphView` |
| 8 · extensions | `mermaid` (in `extensions/mermaid/`) | the honest mermaid subset with atomic fallback |
| 9 · testing + capture | `screenshot.rs` | text/ANSI/SVG stills from a key binding or a headless test |
| 9 · testing + capture | `caps.rs` (tool) | the live terminal-capability report |
| 9 · testing + capture | `capture/` (tool) | the deterministic screenshot pipeline into `docs/captures/` |

Extension examples run with `-p`:
`cargo run -p abstracttui-graph --example workflow` (also `network`) and
`cargo run -p abstracttui-mermaid --example mermaid`.

The sections below describe each example in path order: keys,
requirements, and what it should look like.

## hello

The 60-second first contact: a rounded, surface-filled panel with the
wordmark, a reactive counter line bound to a signal — 53 lines
including docs, ONE import line (the prelude). Proves the public API
ergonomics bar from the vision doc (a real app in < 60 lines).

- Keys: any key counts, `q`/Ctrl+C quits.
- Needs: any tty; `ABSTRACTTUI_THEME=<id>` themes it.
- Looks like: one calm centered panel, accent title, muted hint line.

## widgets

The widget gallery. Tabs split "interactive" (button — incl. a disabled
one in `text_faint` outside the focus order — text input, selectable
list, and a closable-panels row: click a pane's ✕ and the survivors
re-flex into the freed space) from "visual" (border families with the
focus ring, badge tones, ramped progress, spinner sets, separators)
inside a vertical Scroll.

- Keys: Tab focus, arrows in lists, F2 advances spinners, Ctrl+T theme,
  `q` quit. Mouse hovers/clicks.
- Needs: any tty; guarded when tiny.
- Looks like: §3 of the style guide, rendered — every state visible.

## components

The reference for the shareable-component claim: three reusable
components (clickable `stat_card` with props + `on_click`, `field`
composition wrapper, `toolbar`) composed repeatedly with different props
into a settings screen; live signals flow input → summary as you type;
cards carry `Block::shadow` elevation. The form also hosts the choice
family — a channel `Select` sharing its signal with the radio group, a
theme `Combobox` applying live, a features `MultiSelect` — and a
`Disclosure` fold/unfold card. Heavily commented — this file is
documentation.

- Keys: Tab focus, Enter/space activate, type in inputs, `q` quit.
- Needs: any tty.
- Looks like: a settings page built from three lego bricks, edits
  echoing live into the summary card.

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

- Keys: `t`/`T` cycle themes, Tab focus, Enter/space activate, space
  advances the spinners when nothing focused consumes it, `q` quit.
- Needs: 104+ cols for all three columns; degrades to two.
- Looks like: a design-system poster that repaints under one key.

## themes

Every registered theme as a card grid (name + nine-token swatch strip on
the ACTIVE ground), arrow-key navigation with scroll, Enter applies via
`set_theme_by_id` — the entire screen restyles through the one theme
signal. A live preview pane (≥ 96 cols) renders a miniature app mock in
the SELECTED theme's own tokens before you apply. The bottom panel shows
measured contrast ratios (text/muted/faint/accent/selection) from
`theme::contrast_ratio`. The top row carries the drop-in `ThemeSwitcher`
chrome: the ☾/☼ menu button (grouped Dark/Light popup, live preview,
type-ahead) and its one-click dark↔light toggle face.

- Keys: arrows move, Enter applies, Tab focuses the switcher (Enter
  opens its menu / flips the toggle), `q` quits.
- Needs: 96+ cols for the preview pane; guarded below 40x10.
- Looks like: a paint-store wall where the swatch card you point at
  becomes a little application.

## grid

`Display::Grid` live: three track recipes (equal fr · fixed+fr ·
percent-framed) over the same children, cycled with `g`; a col_span hero
card; fr largest-remainder tiling visible on resize.

- Keys: `g` cycles recipes, `t` theme, `q` quit.
- Needs: any tty; resize to watch tracks re-tile.
- Looks like: the same cards snapping between three different skeletons.

## activate

Selection vs activation, side by side — the smallest program that shows
the engine's row-widget vocabulary AND the double-click convention. A
`List` (the timing-free picker gesture: a click on the already-selected
row activates and applies the theme) next to a `Table` (a browsing
surface: only a true double-click — or Enter/Space — opens a row; a slow
re-click only re-selects). One status line narrates every event, so the
difference between "the highlight moved" and "the user committed" is
visible per keystroke and per click.

- Keys: Tab pane · arrows select · Enter/Space activate · mouse: click,
  click-on-selected (list), double-click (table) · `q` quit.
- Needs: any tty; a mouse to feel the click gestures.
- Looks like: two panes answering the same gestures differently, with a
  caption explaining each answer as it happens.

## decide

The decision gate (`ChoicePrompt`): three flavors of "block the flow on
a question", each behind a key — a destructive confirmation with
per-option shortcut letters and a danger-tinted option, a multi-pick
with a scrollable body (the 72-col body-width case), and a chained
sequence (`ChoiceSequence`). Must-choose mode refuses Esc visibly.

- Keys: `1`/`2`/`3` open the gates · arrows/digits/letters move or
  jump · Space toggles (multi) · Enter commits · Esc cancels/retreats ·
  `q` quit.
- Needs: any tty.
- Looks like: a focus-trapped question card over a dimmed app, options
  answering single keys.

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
token through `Feed` + `md::DocStreamSession` — closed blocks freeze,
only the open region re-typesets, code fences tint from their opening
line, and the fourth answer streams a markdown TABLE that renders as
a table live (growing a row per line) plus task-list checkboxes and
strikethrough — while follow-tail breaks on scroll-up and re-pins at
the bottom; an `s` stress toggle rebuilds with 10,000 history items to
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

## attachments

File attachments in a composer, both doors. Terminals have no drop
protocol — dropping a file PASTES its path, quoted differently per
terminal — so the composer's `on_paste` hook runs
`input::paste::classify` on every paste: recognized drops become
attachment chips (a real client would fs-check and offer an undo
here — the classifier itself does no I/O), everything else inserts as
ordinary text. Ctrl+O opens the second door: a `FilePicker` in a
`Modal` — breadcrumb, type-to-filter, multi-select with Space, size
column. Enter "sends" (status line) and clears the chips.

- Keys: drop or paste a path · Ctrl+O picker (type filters, Space
  marks, Enter picks, Backspace parent, Esc closes) · Enter send ·
  Ctrl+C quit.
- Needs: any tty; a terminal to drag files onto to feel the classifier.
- Looks like: a chat composer where dropped files land as chips above
  the text, never as pasted paths.

## reader

The mdpad-class markdown reader: loads a `.md` file from the first
argument or an embedded sample exercising the whole doc vocabulary —
GFM tables with alignment + per-cell ellipsis, in-flow mosaic images
decoded LAZILY on first view including a generated PNG and an
honestly-missing one, heading anchors + intra-doc links, and
find-in-document with a highlight overlay, live match count and
next/previous hopping. The TOC panel is a `List` over
`MarkdownView::outline_rows`; jumps scroll via anchor rows from the
same typeset fold that draws — position and pixels cannot drift.

- Keys: `/` search (type, Enter jumps + keeps the query, Esc clears) ·
  `n`/`N` next/previous match · `t` TOC (Enter jumps) · arrows/PgUp/
  PgDn/Home/End + wheel scroll · Ctrl+T theme · `q` quit.
- Needs: any tty. `cargo run --example reader -- README.md` reads a
  real file.
- Looks like: a document you can actually read — tables aligned,
  pictures in the flow, search hits glowing in selection tones.

## reasoning

The reasoning-controls proof (app-kits/1250): one fake chat turn whose
`ThinkingFold` streams reasoning fragments — folded by default, a dot
indicator advancing PER FRAGMENT (no timer: pause the stream and it
freezes), markdown fences/tables tinting live in the capped body —
then receives the trailing COMPLETE aggregate and visibly recomposes
(last wins). The footer `ReasoningSelect` cycles three fake models:
capable (auto/none + its three declared levels only), non-reasoning
(locked, refuses to open, why-line on the trigger) and
capability-unknown (locked-to-none behind a "set anyway" override
that unlocks the full ladder). Each swap REMOUNTS the control with
fresh facts — the documented reset recipe — and the wire line shows
the APP writing the `thinking` key from `on_change` (the engine mints
no wire vocabulary). The footer-right label renders
`reasoning_label`/`reasoning_label_glyph`, the parity grammar.

- Keys: `m` cycle model · `p` replay the turn · Tab focus ·
  Enter/click open the picker, toggle the fold · Ctrl+C quit.
- Needs: any tty.
- Looks like: an agent turn thinking out loud behind a muted fold,
  with the effort picker and its `r: <value>` footer grammar below.

## voice_mock

The whole voice-app surface, zero external anything: Space is
push-to-talk through the key-state service — HOLD-to-talk where kitty
release events are live, PRESS-to-toggle on legacy wires, with the
footer printing the truthful gesture label and the key-state fidelity
(`Full`/`Degraded`).
While "talking", a 30 ms timer synthesizes a deterministic sine+noise
envelope through `bounded_source` into a dB `Meter` (instant attack,
timed decay, peak hold), an 8-band spectrum, and a rolling `AudioScope`
waveform; a fake transcription appends words into a `Feed`. Release (or
toggle off, or focus loss — the mic-privacy rule) stops the synth, the
meters decay to their fixpoint, and the app parks fully idle.

- Keys: Space talk (hold or toggle per fidelity) · `c` clear transcript
  · `q`/Ctrl+C quit.
- Needs: any tty; a kitty-protocol terminal shows Hold mode, everything
  else shows the labeled Latch mode.
- Looks like: a broadcast level meter breathing under your spacebar,
  words landing in the transcript while you "speak".

## shell

The app shell (co-owned: page host + drawers). A global `PageHost`
hosts three full pages — a dashboard-ish overview with status chips
and a live tab badge, a scrolling markdown reader, a settings form —
behind one themed tab bar. Durable page state lives in app-owned
signals (type into Settings, switch away, come back: the draft
survives the remount); the badge follows the alert count without
remounting anything. `Drawer` panels slide over this same shell from
both edges on demand.

- Keys: Ctrl+PgUp/PgDn or click pages, 1-3 jump, `i` inspector drawer,
  `g` nav drawer, Tab focus, `n` raise an alert (watch the Overview
  badge), wheel/PgUp/PgDn scroll the Reader page, Ctrl+T cycles all
  themes, the footer's ☾/☼ `ThemeSwitcher` opens the grouped theme
  menu (upward — the anchor is the bottom row), `q` quit.
- Needs: any tty; `ABSTRACTTUI_THEME=<id>` themes it.
- Looks like: one calm shell — a tab bar with an underline strip and
  a count badge over one full page at a time, with drawers sliding
  over it from both edges on demand.

## drawers

The drawer system in isolation. A right MODAL inspector (scrim, focus
trap, Esc/✕ close, outside press dismisses) hosting a full scrollable
Feed page, and a left PASSIVE nav panel — glanceable: the app keeps
the keyboard until you click into it. Both keep their page state
across close/reopen because it lives in app-owned signals outside the
builders (the Tabs rule); `n` appends feed lines that keep arriving
while the inspector is open.

- Keys: `i` inspector · `g` nav · `n` add a feed line · Ctrl+T theme ·
  `q` quit.
- Needs: any tty; `ABSTRACTTUI_THEME=<id>` themes it.
- Looks like: a live page dimming under an opaque right panel sliding
  in; the left panel floats over it undimmed, keys staying with the
  page until clicked.

## dashboard

The flagship. Header bar (mark + UTC clock + theme name), nav sidebar
(List), braille rx/tx line chart with legend riding a `TimeSeriesState`
history ring + relative time axis ("-15s … now"), load cluster
(ramped Progress + Sparkline histories), live event log tail (level-coherent,
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

## effects

Compositor-level: overlay layers via `app.overlays()` wearing RENDER's
cell shaders — a Shimmer title, a Dissolve-in panel, a HueDrift-breathing
accent card — plus layer ColorTransforms and REACT's Toast. One
`reactive::after` loop advances shader clocks at 30 fps.

- Keys: `d` replays the dissolve, `m` cycles dim/grayscale/tint, `n`
  toast, `p` pauses the clock (app goes fully idle), `q` quit.
- Needs: truecolor recommended (shaders quantize below).
- Looks like: motion with restraint — three shader accents on a still UI.

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

## workflow and network (extensions/graph)

The graph-extension examples, run with
`cargo run -p abstracttui-graph --example workflow` (or `network`).
`workflow` lays a pipeline DAG through the LAYERED pass — status
tints, badge counts, a dotted async edge, and a deliberate retry cycle
so the broken-edge honesty marker is on screen. `network` runs cyclic
concept data through the FORCE pass — seeded placement (same seed,
same picture), hover tooltips, pan across an oversized canvas.

- Keys: Tab focuses the graph · arrows pan · Enter selects, arrows
  then walk nodes, Escape returns to pan · hover for tooltips · `q`.
- Needs: any tty.
- Looks like: node cards joined by sub-cell strokes, laid out for you.

## mermaid (extensions/mermaid)

The mermaid-subset renderer over embedded samples or a `.mmd` file:
`cargo run -p abstracttui-mermaid --example mermaid [-- file.mmd]`.
Four samples show the honest range — a TD flowchart, an LR flowchart
with labels and shapes, a sequence diagram, and a gantt chart falling
back ATOMICALLY (verbatim code fence + named reason + mermaid.live
link), the subset contract made visible.

- Keys: Left/Right (or h/l) switch samples · Tab focuses the diagram
  (arrows pan, Enter selects flowchart nodes) · `q` quit.
- Needs: any tty.
- Looks like: real diagrams in the terminal — and an honest refusal
  when the dialect is out of subset.

## screenshot

The capture recipe, live: a themed panel with `s` bound to
`app::request_screenshot` — each press writes the LAST PRESENTED frame
as `screenshot-demo.{txt,ansi,svg}` under the system temp dir (`cat`
the `.ansi` to replay it; the `.svg` renders on GitHub). Deliberately
no engine-default hotkey — this binding is the documented recipe.
Without a tty it drives the same scene headlessly through
`Driver` + `CaptureTerm`, captures from BOTH truth surfaces (composed
frame and VT-modeled bytes), asserts they agree, writes the same three
artifacts, and exits 0 — the test-artifact recipe in miniature.

- Keys: `s` capture · `q`/Ctrl+C quit.
- Needs: nothing — with a tty it is interactive, without one it
  exports and exits 0 (CI-safe).
- Looks like: a calm panel that names the three files it just wrote.

## caps (tool)

The live terminal-capability report: what the two-pass detection found
on THIS terminal (colors, kitty keyboard, graphics protocols, OSC 52,
pixel geometry), with probe upgrades appearing live — watch the
"images via" line settle on the channel the image ladder will pick.
Headless it prints the environment-detected set and exits 0. The first
stop when images or Shift+Enter behave unexpectedly (see
docs/graphics-and-3d.md § "Verifying image support on your terminal").

- Keys: `q` quits.
- Needs: nothing; most useful on the terminal you are curious about.
- Looks like: a two-column capability table that fills itself in.

## capture (tool)

The deterministic screenshot pipeline: runs the built examples under a
real pty at fixed sizes/themes, interprets the bytes with the testing
rig's `VtScreen`, and dumps plain + styled text renders — and a
rendered `.svg` per shot (`VtScreen::screenshot().to_svg()`) — into
`docs/captures/` — plus `themes-table.md` (every theme's token hex from
the registry), in-process splash stills (2D/3D at the burst and settled
beats), and in-process APP stills driven headlessly through
`Driver` + `CaptureTerm` (streaming transcript with the completion
dropdown open, an open Select popup, a diff-tinted `CodeView`, a feed
with follow-tail broken, a doc-vocabulary reader table) — those five
are clockless and byte-deterministic. The docs embed these as
fenced "screenshots".

- Run: `cargo build --examples && cargo run --example capture`
  (`-- themes|splash|shots|apps` for one family).
- Needs: unix `script(1)` for the pty shots; nothing for the rest.

## common/ (helpers)

Shared helpers for the demos (small-terminal guard, key legend) — not
a runnable target.
