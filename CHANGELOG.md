# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2026-07-22

### Fixed (image path — study-2 MEDIA audit)

- kitty: session-driven moves now REPLACE their placement instead of
  accumulating ghosts. `transmit_display`/`place` carry the fixed
  placement id `p=1` — the spec is explicit that pid-less `a=p` on the
  same image id creates ADDITIONAL placements, so every move left the
  old copy visible on spec-correct terminals (kitty, ghostty). The
  `KittyModel` referee now models replace-on-same-(id,pid) and
  pid-scoped deletes.
- image overlays: a moved overlay double-offset its mosaic patches
  (grid positions are already screen cells; the driver blit added the
  rect origin AGAIN), so the placement painted at 2× the offset and
  moves left the old cells behind. Blit origin fixed to zero and the
  vacated-rect lifecycle added: `Driver::pre_image_pass` folds vacated
  rects (move / remove / channel switch) into the frame's tree damage
  and, for cursor-paint byte channels (iTerm2/sixel), poisons the
  previous-frame model so the diff re-emits cells it believes
  unchanged — the repaint that actually erases the terminal-held
  pixels. New lifecycle acceptance suite: `tests/adv_image_lifecycle.rs`.
- tmux passthrough: graphics payloads are now wrapped ONE escape per
  `ESC Ptmux;` frame instead of the whole emission in a single wrapper —
  tmux discards any single input sequence over 1 MiB (tmux #487), so
  chunked kitty transmissions of real photographs vanished silently.
- scroll optimization: the driver takes the plain diff while byte-channel
  images are live (`ImageSession::live_byte_slots`) — terminals scroll
  protocol images WITH the text (kitty spec mandates it), which would
  move terminal-held placements out from under the session's
  bookkeeping.
- image ladder warnings (`#FALLBACK …`) queued by the session sync are
  now forwarded to the startup-notices lane (deduped); the driver
  previously dropped them, making degradations silent against the
  charter.
- image overlays (study-2 adversarial review): a PARKED mosaic
  placement no longer corrodes when content repaints beneath it — the
  driver's pre-image pass now re-blits mosaic slots whose cells any
  damage rect will clear+redraw (wire-free: the diff suppresses the
  byte-identical cells). iTerm2/sixel placements still decay under
  beneath-repaints (re-emission costs the full payload per frame —
  deliberate, documented on `Overlays::image`; design space of backlog
  0660). Review: `reviews/study2/quality-on-media.md`; test
  `mosaic_image_survives_content_repaint_beneath_it` (failed pre-fix).

### Added

- tests: a second explicit perf suite, `tests/perf_app_surfaces.rs`,
  measuring the app-layer surfaces through the real frame loop — feed
  token streaming, select popup open/close, full-screen selection drag,
  composer keystroke with the completion dropdown open, diff-tinted
  `CodeView` scroll, and startup time-to-first-frame under a real pty —
  with byte-emission proportionality asserts beside the timing budgets
  (run: `cargo test --test perf_app_surfaces --release -- --ignored
  --test-threads=1`). And a new allocation pin in
  `tests/alloc_budget.rs`: idle turns allocate NOTHING with a streaming
  `Feed`, an armed `interval`, and a parked `Select` popup mounted.
- tests (study-2 adversarial review of the image fixes): a two-image
  kitty move/remove test (fixed placement id `p=1` is scoped per image
  id — no cross-image collision), a scroll-guard scoping test (byte
  channels force the plain diff, mosaic keeps the optimization, a
  parked image adds zero bytes per frame), a beneath-repaint survival
  test, a per-wrapper byte bound on the tmux per-escape test, the
  parked protocol image folded into the idle allocation pin (test
  renamed `idle_turns_with_feed_interval_parked_popup_and_parked_image_allocate_nothing`),
  and an `#[ignore]`d guard-cost measurement
  (`perf_feed_scroll_with_parked_protocol_image_90x30`: 172 B/frame
  scrolled vs 1,758 B/frame plain — 10.2x, the honest price of
  correctness until 0675's re-place-by-id upgrade).
- examples: the gallery board now shows the choice family (`Select`
  trigger), the multiline composer (`TextArea`, seeded two rows), and a
  diff-tinted patch beside the code sample; the capture pipeline gained
  an in-process `apps` family (streaming transcript with the completion
  dropdown open, an open Select popup, a diff `CodeView`, a feed with
  follow-tail broken) — clockless, byte-deterministic stills under
  `docs/captures/`.

### Fixed

- docs: stale claims — the example count (12 → 14) in
  CONTRIBUTING/docs-index/getting-started, `FeedState::clear` described
  as future work in live-data.md after it shipped, and the dependency
  versions in the llms-full.txt package facts (`miniz_oxide` 0.9,
  `windows-sys` 0.61).

### Fixed (capability truth — first-app fix wave, 0293)

- kitty keyboard enter-flags now FOLLOW the probe: when the active probe
  proves the protocol on a terminal the env pass could not claim
  (iTerm2 ≥ 3.5, VS Code/Cursor, Warp), the driver pushes the standard
  flags at the upgrade moment via the new `Terminal::set_kitty_keyboard`
  verb — Shift+Enter/Ctrl+Enter-class chords start working without a
  restart. The push/pop accounting lives in the terminal's entered
  session options, so `leave` pops exactly what was pushed, job-control
  suspend/resume stays symmetric (pop on suspend, re-push on resume),
  and the panic-hook emergency restore pops while the alternate screen
  is still active (kitty flag stacks are per screen buffer). Explicit
  `RunConfig::enter` postures are never upgraded. Tests:
  `tests/wave_probe_caps.rs`,
  `pty_runtime_kitty_push_pops_on_suspend_repushes_on_resume_and_pops_on_leave`.
- the inverse over-claim on the same lines: WezTerm ships
  `enable_kitty_keyboard = false` by default, so its env claim is now
  evidence-gated — `Capabilities::detect_env` no longer asserts
  `kitty_keyboard` for WezTerm; the probe raises it (and the driver then
  pushes the flags) when the user enabled the protocol.

### Added (capability + picker APIs — first-app fix wave, 0295/0296/0685)

- `app::use_caps(cx) -> Signal<Capabilities>` + `app::current_caps()`
  (both in the prelude): the driver's LIVE capability view — env pass
  published at session enter, upgraded whenever a probe reply actually
  changes a field (equality-deduped). Apps render honest key hints and
  graphics-channel labels; the `images` example's footer and the
  `transcript` example's newline hint now derive from it (closes
  media-av 0685 with first-app 0295 — one accessor, both consumers).
- `app::select::SelectHandle` (prelude): programmatic open for
  `Select`/`Combobox`/`MultiSelect` via `.handle(&h)` + `h.open()` — the
  command-summoned picker verb (`/theme`-style flows). Anchors at the
  trigger's last-painted rect (one-frame-after-mount refusal
  documented), refuses disabled/empty/unmounted faces by returning
  `false`, and the wiring dies with the face's scope (dyn_view
  regenerations rewire; generation-guarded against stale cleanups).
- `TextArea`: Ctrl+J now inserts a newline under every submit policy —
  the UNIVERSAL fallback chord (`0x0a` IS Ctrl+J on the legacy wire), so
  composer hint text can promise a newline chord on terminals where
  Shift+Enter cannot be reported (0295's built-in-fallback ask).

### Fixed (rendering correctness — first-app fix wave, 0298/0290)

- resize: the first post-resize frame now re-anchors ABSOLUTELY —
  `Driver::apply_resize` invalidates the presenter (cursor + pen) next
  to the existing prev-poison, so the frame's first cursor motion is an
  absolute CUP and its first SGR is reset-based (backlog 0298, P0). The
  poison already re-emitted every CELL, but the first run was still
  PLACED by relative motion from the pre-resize parked cursor — a ghost
  after an emulator reflow moves the physical cursor (macOS Terminal's
  bottom-anchored growth in the field incident), which offset the run
  and left a stale band of the previous frame on screen (live report:
  stale header band above the live frame after a workflow-picker close
  around a resize). The splash player (`boot::player`) already
  invalidated on resize; the driver now upholds the same rule. New
  acceptance suite `tests/adv_resize_modal.rs`: every
  {resize↑↓←→, modal close} interleaving — including both in one turn —
  is verified cell-for-cell against a fresh-driver oracle over a
  garbage-prefilled VT screen with a reflow-moved cursor, plus a byte
  pin that the first post-resize motion is CUP, never CUU/CUD/CUF/CR.
- selection: EVERY copy now ends the gesture (backlog 0290). The
  mouse-release copy — and the mid-drag Enter/`c`/Ctrl+C key-copies —
  clear the region along with the copy, so the app's next keystrokes
  route normally at once. The retained region used to keep consuming
  Enter/`c`/Ctrl+C after the release had already copied: typing
  "cargo check" into a composer lost both `c`s and Enter submitted
  nothing until a click/Esc, with no effective app-side workaround
  (the selection layer consumes keys before tree dispatch). Esc still
  cancels a live drag without copying; a fresh click re-anchors; wheel
  routing is untouched. `SelectionAct::Copy` now carries the region
  (crate-internal), and the highlight repaints from truth on the copy
  frame. Regression: `release_copy_frees_enter_and_c_for_the_app`
  (tests/adv_selection.rs).

## [0.2.1] - 2026-07-22

### Added

- widgets: `List::on_activate(FnMut(usize))` — the explicit activation
  event (backlog 0250, ruling in reviews/study/platform-on-appkits.md):
  fires on Enter (always), Space (no toggle meaning in a List), and a
  click on the already-selected row; `on_select` stays the
  selection-changed notification and fires on movement exactly as
  before. When unbound, Enter/Space pass through to app shortcuts
  (existing consumers unchanged).
- widgets: `TextInput::masked(bool)` — secret/password mode (backlog
  0510 §masked, shipped early): the draw substitutes one `•` per
  grapheme cluster (count-honest, geometry identical) and
  `access_value` exports the same bullets, so the accessibility/
  automation tree never carries plaintext. Editing, selection, cursor
  math, and paste untouched — except Alt+arrow word jumps, which
  treat the whole masked value as ONE word (start/end, like Home/End;
  real word boundaries would reveal the secret's word structure
  through caret positions).
- text: diff highlighting (backlog 0140's additive slice) —
  `text::DiffLexer` classifies unified-diff lines into the new
  `#[non_exhaustive]` `text::DiffKind` vocabulary (added/removed/hunk
  header/file header/meta/context; stateless per line, totality-fuzzed,
  documented `---`-content ambiguity resolved header-first).
  `widgets::diff_token_color` maps kinds onto the audited semantic inks
  (added `ok`, removed `error`, hunk `info`, chrome `text_muted`;
  measured ≥3.0:1 on the `surface_raised` code ground across all 26
  themes, test-pinned). `TokenKind` is deliberately untouched — its
  `#[non_exhaustive]` question is parked in the written 0.3 budget.
- widgets: `CodeView::lang(label)` — best-effort lexer selection by
  language label: `"diff"`/`"patch"`/`"udiff"` route to the diff
  mapping, `"rust"`/`"c"` pick the matching `CLikeLexer` preset,
  unknown labels keep today's rendering. Markdown and Feed code fences
  labeled `diff` route automatically through the same shared recipe.
- app: the anchored-popup substrate is COMPLETE (backlog 0500's two
  remaining routing modes, extending `app::anchored`): `Popup` — the
  OWNED mode, a modal tree at `top_z() + 1` (layers above any modal
  stack), placement/flip/clamp via the shipped `place_panel` contract
  plus `open_including_anchor_row` (the popup starts at the trigger
  row), Escape/outside-press/anchor-death/viewport-resize dismissal
  with `DismissReason`
  (`Commit`/`Escape`/`OutsidePress`/`AnchorGone`/`Resize`)
  delivered once through `on_dismiss` (a resize invalidates both the
  solved placement and the captured anchor rect, so an open popup
  closes instead of floating at stale coordinates); and `Tooltip` — the
  hover-timed, non-interactive passive-label mode (one-shot timer,
  zero wakeups until due; extensions 0430's consumer).
- app: the choice-control family (backlog 0500): `Select` (closed
  one-of-N over a `Signal<usize>`; popup type-ahead with same-char
  cycling; opt-in `commit_on_move` live preview whose Escape restores
  the pre-open value), `Combobox` (popup-mounted `TextInput` on the
  trigger row — zero visual jump; case-insensitive substring filter;
  the filter text is never the value; count/"no matches" status line),
  `MultiSelect` (Space/click toggles a working copy, Enter commits the
  key set once, Esc abandons; collapsed row joins labels and degrades
  to "N selected"). Shared contract: highlight-vs-value separation
  (0250 — `on_change` on commit only, and only on a real change),
  `SelectOption { key, label, hint, disabled }` with disabled rows
  skipped by movement, `Role::Button` + live access value on the
  trigger (a dedicated `Role::Select` variant would break the
  published exhaustive enum — parked in the 0.3 budget, 0002 entry 1)
  and `Menu`/`MenuItem` in the popup, theme-token styling
  consistent with `TextInput`/`List`. Faces live in `app::select`
  (they ride the overlay store; layer map R4-1) and re-export through
  the prelude; `App::mount` now provides the overlay store as reactive
  context so `Select::new(..).view(cx)` works with no wiring.
  `examples/components.rs` gained a picker section (theme Combobox
  wired to `set_theme_by_id` — the 0200 console's `/theme` recipe).
- docs/backlog: the written 0.3 breaking budget
  (`docs/backlog/planned/0002_the_0_3_breaking_budget.md`) — ADR-0001
  §2's required budget list, seeded with the `Role` non_exhaustive
  (+`Tree`/`TreeItem`) entry, the `TokenKind` non_exhaustive ruling,
  and the `Scroll::content_size` deprecation fate.
- CI: three new gates — `msrv` (pinned 1.87.0 toolchain,
  `cargo check --all-targets --locked`), `semver` (cargo-semver-checks
  against the latest published crates.io release; enforces ADR-0001's
  additive-only regime between budgeted windows), and
  `live pty (ubuntu)` (the ignored live_smoke suite under a real
  pseudo-terminal, examples prebuilt, serial — backlog 0180). Cargo.toml
  now declares `rust-version = "1.87"` (floor: `is_multiple_of`,
  stabilized 1.87, used in gfx/three; MSRV bumps are minor-version
  events per CONTRIBUTING).

### Fixed

- widgets: `List` and `Table` now complete ALL internal bookkeeping
  (selection write, ensure-visible scrolling) BEFORE invoking their
  selection callbacks — `on_select`, and on `List` also `on_activate`
  (0250 ruling clause 4; `Table` has no activation event) — so a
  callback that synchronously disposes the widget's scope (the
  modal-picker close) no longer panics on the widget's own
  post-callback signal use.
- widgets: arrow keys on an EMPTY focused `List` no longer index past
  the row prefix sums (latent panic).

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
- Compatibility note (recorded 2026-07-22, after release): this release
  also added `Role::TextArea` to the public exhaustive `ui::access::Role`
  enum — technically a breaking change for downstream code that matches
  `Role` exhaustively, shipped without a migration note (no known
  consumer matches `Role`; verified against abstractcode-tui). Migration:
  add a `Role::TextArea` arm or a `_` arm. `Role` is slated for
  `#[non_exhaustive]` in the written 0.3 budget
  (`docs/backlog/planned/0002_the_0_3_breaking_budget.md`).

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

[0.2.2]: https://github.com/lpalbou/abstracttui/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/lpalbou/abstracttui/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/lpalbou/abstracttui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lpalbou/abstracttui/releases/tag/v0.1.0
