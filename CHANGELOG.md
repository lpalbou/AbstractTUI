# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-07-22

### Added

- widgets: `TextArea` — the multiline composer (backlog 0120).
  Cluster-atomic editing over the same `text::segments` authority as
  `TextInput` (shared `ClusterMap`), byte-tiling soft wrap (every byte
  keeps a visual home; hanging break-whitespace clips at the row edge),
  vertical caret movement with a remembered goal column, per-visual-row
  Home/End with honest end affinity (full rows wrap the caret to the
  next row — a phantom row when needed), selection across rows, word
  jumps across line breaks, grow-to-content within `rows(min, max)`
  then internal scroll, Enter-submits + Alt+Enter/kitty-Shift+Enter
  newline presets (`SubmitPolicy`), edge-triggered history recall with
  a draft that survives the round trip, block paste (newlines kept,
  never a submit), placeholder, disabled state, `Role::TextArea` +
  live access value. `TextAreaState` is the durable app wire: value
  signal, caret byte, focus, programmatic `replace_range`, history —
  and `caret_cell()`, the caret's solved screen cell (the dropdown
  anchor, 0120 §6).
- app: anchored passive-panel substrate + completion controller
  (`app::anchored`; the 0500 spec's PASSIVE routing slice, designed
  jointly with 0120). `place_panel` implements the placement contract
  (below-preferred, flip above when below is short and above is longer,
  viewport clamp, match-anchor or content width); `AnchoredPanel` rides
  a non-modal overlay layer above the live stack via the one budgeted
  engine delta `Overlays::top_z()`, never takes focus (keys stay with
  the anchor owner), and closes with its opener's scope (anchor-unmount
  safety). `Completion` wires trigger-character providers ('/', '@',
  any prefix) onto a `TextAreaState`: dropdown at the caret, Down/Up
  navigate, Enter/Tab accept, Esc dismisses (same-token mute), typing
  refilters, click accepts, zero idle cost while closed. The OWNED and
  TOOLTIP popup modes (select family, menus, tooltips) remain future
  0500 work. `examples/transcript.rs` gained a bottom composer with
  `/` command + `@` mention completion.

- app: screen-text selection + clipboard copy (backlog 0270, all three
  tiers). `app::selection::selection()` enables an opt-in engine selection
  layer: left-drag paints the theme selection inks over the composed frame
  (damage-contract honest — only changed cells repaint), release or
  Enter/`c`/Ctrl+C copies the rendered text via OSC 52 through presenter
  custody, Esc/click clears; selections row-flow within the pane under the
  drag anchor (new `UiTree::pane_rect_at`), never split wide glyphs, trim
  trailing whitespace per row. Honesty: this extracts SCREEN text
  (what-you-see); logical widget-content selection remains backlog 0160.
  `app::selection::mouse_capture()` suspends/resumes mouse reporting at
  runtime (new `Terminal::set_mouse_reporting` verb on both platform
  backends + `CaptureTerm`) so native terminal selection works on demand;
  `app::selection::copy_to_clipboard()` is the app-reachable clipboard
  verb (backlog 0150's clipboard leg). Modifier-bypass matrix and OSC 52
  delivery honesty documented in `docs/troubleshooting.md`; API guide
  section in `docs/api.md`; `examples/feed.rs` demonstrates drag-select.

- reactive: async source→signal bindings (`channel_source`, `latest_source`),
  bounded coalescing ingestion (`bounded_source` with `DropOldest` /
  `DropNewest` / `Coalesce` policies and an honest stats signal including
  drop and fold-panic counters — a panicking coalesce fold degrades labeled
  instead of poisoning the lane), a cancellable `interval` timer, and waker
  deduplication (one wake per posted burst). New example `examples/feed.rs`
  and guide `docs/live-data.md`.
- widgets: `Feed` — a virtualized, append-only transcript/message widget with
  keyed rich blocks (text, markdown, code, custom draw) and streaming
  markdown items (`push_stream`/`stream_append`: only the open block
  re-typesets per token). New example `examples/transcript.rs`.
- render: `md::StreamSession` — incremental markdown typesetting with a
  parse-equivalence guarantee against batch parsing.
- widgets: `Scroll::follow_tail` (pin-to-bottom that disengages on user
  scroll and re-pins at the bottom edge) and measured content extent — the
  explicit `content_size` hint is now optional.
- docs: first ADRs (`docs/adr/`): API stability policy toward 0.2/1.0, the
  two-`Style` ruling, and struct-extensibility policy.

### Fixed

- layout: the zero-collapse debug diagnostic no longer writes to stderr
  while a session is live (raw diagnostic lines corrupted the alternate
  screen), and its dedup now keys on the layout situation (parent rect,
  axis, declared size, child index) instead of the node key — `dyn`
  regenerations re-minted node keys every data tick and re-reported the
  same collapsed row endlessly. Notices now reach the in-app
  startup-notices lane each frame and flush to stderr only after the
  terminal is restored.
- examples: the dashboard's fixed-height rows (header, metric rows,
  progress/sparkline lines, legend, footer) declare `shrink(0.0)` so
  short terminals clip panels instead of collapsing their rows to zero.
- ui: `autofocus` inside a regenerated `dyn_view` subtree no longer panics
  the reactive runtime.
- app: modal content shortcuts work from the frame the modal opens
  (keyboard ownership moves to the modal tree immediately).
- app/layout: overflowing modal content no longer silently crushes
  fixed-size rows to zero; default one-row controls resist shrink, `Scroll`
  defaults take leftover space, and debug builds emit a zero-collapse
  diagnostic naming the offending node.

### Changed

- `term::Capabilities` and `GraphicsCaps` are now `#[non_exhaustive]`
  (construct via `Default` plus mutation or the new customization
  constructor; in-crate FRU continues to work).

## [0.1.0] - 2026-07-21

First public release.

### Added

- **Terminal kernel** — raw mode, alternate screen, and terminal capability
  detection (colors, graphics protocols, keyboard protocol, clipboard,
  notifications) with Unix and Windows backends.
- **Input** — byte-stream parser producing structured key, mouse, paste, and
  resize events; kitty keyboard protocol support with legacy fallback.
- **Compositor** — z-ordered layers with alpha blending, damage tracking,
  diff/present with minimized escape traffic, per-cell shaders, and scroll
  optimization.
- **Reactive runtime** — fine-grained signals, memos, effects, scopes, and a
  scheduler; updates repaint only what changed.
- **Layout** — flexbox-style solver with grid tracks, wrapping, and gaps.
- **Widgets** — 20+ built-ins (text input, list, table, tabs, buttons,
  checkboxes, radios, progress, spinner, charts, markdown, rich text, code,
  scroll views, images, 3D viewport, and more), styled through theme tokens
  only.
- **Themes** — design-token system with 26 built-in themes (including
  Catppuccin, Rosé Pine, Tokyo Night, Nord, One, Dracula, Monokai, Gruvbox,
  Solarized, Everforest families) and a tested contrast audit.
- **Images** — PNG and JPEG decoding delivered over four channels: kitty
  graphics, iTerm2 inline images, sixel, and unicode mosaic fallback.
- **3D** — GLB (glTF 2.0) loading and software rasterization with animation
  support, rendered into the same composited scene.
- **Boot** — AbstractTUI visual identity splash.
- **Testing** — headless test terminal, VT interpreter, golden snapshot
  harness, and randomized-input utilities, usable by downstream crates.
- **Examples** — 12 runnable examples, from `hello` to a full dashboard,
  theme browser, and 3D viewer.

[0.2.0]: https://github.com/lpalbou/abstracttui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lpalbou/abstracttui/releases/tag/v0.1.0
