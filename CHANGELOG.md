# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- ci/tests: the RT5 controlling-terminal probes fork the test binary to
  give their pty child a real ctty, and fork from the THREADED default
  suite deadlocks in the parent's atfork handlers on GitHub's macos
  runners — the macOS test job hung to its 30-minute timeout. The two
  probes are now ignore-gated into the serial live tier, where the CI
  live-pty jobs (ubuntu, and a new macos leg for the Darwin verdict
  halves) run them with `--test-threads=1`. Local invocation:
  `cargo test --lib -- --ignored --test-threads=1 term::rt5_live_tests`.

## [0.3.4] - 2026-08-19

### Fixed

- render: `Screenshot::to_svg` produced files that DISTORT in
  Chromium-based viewers: merged text runs contributed no characters
  for never-painted cells while `textLength` still spanned them (every
  glyph in the run stretched), and run padding used real spaces under
  `xml:space="preserve"`, which Chromium ignores — leading/trailing
  spaces collapsed and stretched the survivors. Blank cells now emit
  U+00A0, so a run's character count always equals the cells it spans
  and nothing is collapsible. Affects every SVG export.

### Added

- widgets: **`DrawerDock`** (app-kits 1255) — a right-edge rail of
  always-visible vertical tabs, each fronting a docked side panel: at
  most one open, and the panel column vanishes entirely while none is.
  The panel DOCKS in layout (content reflows around it) — the
  transient, sliding, scrimmed cousin remains `app::drawer`. Open state
  is a bindable `Signal<Option<String>>` (`None` = collapsed): the dock
  renders and mutates it, apps drive it from keys or restore it across
  sessions. Closed drawers are disposed (the PageHost generation-scope
  recipe; durable state lives in app signals outside the builders).
  Tabs carry reactive badge dots (`drawer_badge`); titles truncate
  clear of the ✕ close corner and rail labels render one grapheme
  cluster per row. Collapse via the active tab, the panel's ✕ corner
  region, or Esc while focus is inside the panel. Drawer builders must
  not write `open` synchronously (redirect from `on_change` or an
  effect; debug builds assert). `cargo run --example drawer_dock`
  demonstrates it;
  `tests/shot_drawer_dock.rs` drives every state through wire-level
  clicks and doubles as the screenshot harness
  (`DRAWER_DOCK_SHOTS=<dir>` writes SVG captures).

## [0.3.3] - 2026-08-19

`0.3.2` was tagged but never reached crates.io: the release pipeline's
publish guard reported the version as already present and skipped it.
`0.3.3` supersedes it and contains everything 0.3.2 listed. A dependency
on `0.3.2` cannot resolve from the registry — require `0.3.3`.

### Fixed

- widgets: the scrollbar thumb keeps a usable minimum length. The
  proportional height of a long buffer rounds to zero (2000 rows in a
  20-row pane), which drew a single-cell thumb that is hard to see and
  impossible to follow while scrolling. The thumb now has a floor of
  three cells, yielding on very short bars so it always keeps room to
  travel, and still filling the track when the content fits. Affects
  every scrollbar in the engine: `Scroll`, `List`, `Table`,
  `FilePicker`.
- app: a successful clipboard copy now reports its size — `copied 240
  characters (3 lines) to the clipboard` — instead of announcing which
  route it took. A copy leaves the app, so its size is the only evidence
  the user has that it landed and landed whole. A warning is reserved
  for the case that needs action: no working clipboard route at all.
- ci: the core publish step always attempts `cargo publish` and checks
  the sparse index only to excuse a failure, so a release can no longer
  report success without publishing.

## [0.3.2] - 2026-08-17

### Added

- widgets: **Codex-compatible navigation chords in `TextInput` and
  `TextArea`** (first-app/1310) — Codex's default editor keymap adopted
  verbatim: `move_word_left`/`move_word_right` (Alt+b/Alt+f, Alt+←/→,
  Ctrl+←/→), `move_line_start`/`move_line_end` (Home/Ctrl+A,
  End/Ctrl+E), `delete_backward_word` (Alt+Backspace, Ctrl+Backspace,
  Ctrl+W) and `delete_forward_word` (Alt+Delete, Ctrl+Delete, Alt+D).
  Shift extends the selection by word on every form. The letters claim
  exactly one modifier, so AltGr (Ctrl+Alt) never reads as a word chord.
  Codex's character-motion, kill and yank chords (`Ctrl+B`/`F`/`D`/`U`/
  `K`/`Y`) are deliberately NOT claimed — they collide with app
  bindings too often. Note that a focused editor now consumes the
  chords above, which previously fell through to global actions.

- widgets: `scroll::freeze_follow_tail` / `scroll::follow_tail_frozen` —
  hold every follow-tail `Scroll` on the thread still without
  disengaging it. Frozen, a pinned scroller keeps the rows it is showing
  and appends grow below the viewport; thawing re-pins to the tail as it
  stands then (`follow` never flips, so "following" chrome does not
  flicker).

### Fixed

- widgets: an anchor parked ON the caret (Shift+Left then Shift+Right)
  survived the next delete, which moved the caret out from under it and
  turned it into a phantom one-character selection that the following
  keystroke silently replaced. `delete_selection` drops an empty anchor
  in both text widgets now.
- widgets: a word delete at the buffer edge reported an edit that never
  happened — holding `Alt+Backspace` at position 0 fired `on_change`
  (and pushed history) per key repeat.
- app: a live screen selection now FREEZES follow-tail scrollers
  (first-app/1300 — `app::selection` drives the freeze above). Selection is
  screen-space, so a streaming transcript that kept scrolling under a
  drag copied whatever landed on those cells by release: the highlight
  said one thing and the clipboard got another. Now the transcript holds
  still for the length of the drag and returns to the live tail when the
  region clears. Field report: abstractcode-tui, 2026-08-16 ("I cannot
  copy text while it works"). No app wiring — streaming apps inherit it.

## [0.3.1] - 2026-08-01

### Fixed

- app: the drawer header's ✕ close affordance now activates for a left
  press anywhere in the header row's trailing corner REGION (the glyph,
  the padding margin beside it, and two cells of jitter room before
  it), not just the exact glyph cell. The ✕ is a one-cell target at the
  panel's outermost column — precisely where a real terminal whose
  window width is not an exact multiple of the cell width quantizes an
  edge click into the neighbouring column. A cell-perfect synthetic
  test suite cannot observe that class of miss; the field report
  (agora-tui, 2026-08-01: "cannot close the preview with the button")
  is the evidence. Content clicks are unaffected: the region is header
  chrome only, and the title area stays inert.

## [0.3.0] - 2026-07-30

`0.2.27` was published with the `RunConfig` change below and yanked: the
new fields break exhaustive struct literals, which a patch release must
not do. Use `0.3.0`.

### Added

- widgets: `List::on_remove` — removable rows in one call. Draws the
  trailing `✕`, routes the click, and re-settles selection on the
  rebuild so it can never name a row that no longer exists. Replaces
  `row_accessory` + `accessory_width` + `on_accessory_click` + a
  hand-rolled glyph for the dismiss case.
- widgets: `List` hover ink — the row under the pointer takes accent ink
  and the hot zone takes bold, so pointing at a row's accessory keeps the
  row lit. A hot `on_remove` dismiss draws in `error`, matching the Block
  close affordance; a plain `row_accessory` badge draws in `accent`,
  because red would misreport an unread count. Selected rows keep their
  audited selection pair. Hover and clicks resolve through one hit test,
  so what lights up under the pointer is exactly what a press acts on.
- widgets: `List::offset_y` — bind the first visible content row, the
  `Scroll::offset_y` convention. Without it a rebuild starts back at the
  top, so dismissing a row scrolled the reader to the head of the list.
  A bound offset is clamped into range on every build.
- app: `RunConfig::hover_ink` — opt in to `MouseMode::AnyMotion` (mode
  1003) so hover-reactive visuals receive motion with no button held.
  Off by default: 1003 reports every pointer cell, and an app with no
  hover visuals should not wake its event loop for it. Setting the flag
  keeps kitty-keyboard auto-detection, which hand-building `EnterOptions`
  would forfeit.
- app: `RunConfig::platform_clipboard` — refuse the host clipboard
  fallback (`pbcopy` / `wl-copy` / `xclip`). It spawns a child process
  synchronously from the UI thread, so embedders and test harnesses can
  now decline it.
- app: `App::run_with(cfg)` — `App::run` against the platform terminal
  with an explicit `RunConfig`, so opting into the above needs no
  hand-built `Terminal`.
- examples: `interaction_affordances.rs` — hover ink, removable rows,
  live filtering, and nested scroll wheel bubbling. The filter shows the
  index-mapping rule: a filtered list reports positions in the subset it
  was given, so the example maps back before touching its own data.

### Fixed

- widgets: removing the selected row left `selection` naming a row that
  no longer existed — no row highlighted, `access_value` announced a
  phantom row to screen readers, and the first arrow key moved the wrong
  way. Selection is now re-derived on every build: by
  [`List::selection_key`] when the key survives, otherwise the same slot
  clamped into the shorter list.
- widgets: `List::scroll_to` uses ensure-visible row math (not a raw row
  top clamp), shares that math with keyboard selection, and resolves
  against the measured viewport height; empty lists no longer panic.
- widgets: `List` / `Scroll` wheel events bubble to a parent scroller
  when the offset is already at the top or bottom, so a nested list has
  no dead zone at its edges. An unmeasured `Scroll` still consumes the
  gesture rather than handing it to an ancestor.
- widgets: `Scroll` no longer skips the offset repair when content is
  exactly viewport-height. The guard meant a measured pane that shrank to
  its viewport kept a stale offset and rendered blank until a gesture
  rescued it — the 0281 field bug the repair exists to prevent.
- widgets: wheel-scrolling a follow-tail `Scroll` computes from the
  pinned position in one write instead of two, so a wheel-up no longer
  jumps to the head and each tick notifies the graph once.
- widgets: hovering a `List` and then wheeling re-resolves the hot row —
  the ink no longer lights a row the pointer has left.
- widgets: `List` rows built with `rich_items` take hover ink too. It is
  the row's base ink, so spans that set their own color keep it.
- widgets: an empty `List` no longer announces a selected row through
  `access_value`; it reports `0 items`.
- examples/presence_board: the accessory glyph is `✕` U+2715. The
  previous `×` U+00D7 is East-Asian-ambiguous and could take two cells,
  overflowing the accessory column on terminals set to ambiguous-wide.
- app: a copy that reached the host clipboard no longer also emits OSC
  52. Terminals and tmux cap that payload, so the second write could
  truncate a long selection the host had stored whole. When there is no
  host clipboard (or it fails), OSC 52 is still sent even unadvertised.
- app: `cargo test` no longer shells out to the host clipboard. The
  previous `cfg!(test)` guard never covered integration tests, so the
  suite overwrote the clipboard of whoever ran it.

### Changed

- **BREAKING** app: `RunConfig` gained the `hover_ink` and
  `platform_clipboard` fields. Adding fields to a constructible struct
  breaks exhaustive literals, which is why this is `0.3.0` rather than a
  patch. Construct it with `..RunConfig::default()` and the addition —
  and any later posture flag — costs you nothing:

  ```rust
  let cfg = RunConfig { hover_ink: true, ..RunConfig::default() };
  ```

- widgets: the `List` accessory column is one click target edge to edge
  on the row that draws it, and rows whose `row_accessory` returns `None`
  leave that column as ordinary body. Accessory clicks still never move
  selection.

### Performance

- widgets: `List` resolves each row's accessory label once per build and
  keeps it truncated and measured. Painting and hit-testing borrow it, so
  hover motion and row painting no longer allocate per row per frame.
- widgets: `List` measures its viewport through the shared `size_probe`
  seam instead of writing a signal from its draw closure. Paint no longer
  mutates reactive state (RT1-2), which could flush effects mid-walk and
  tear the frame being painted.

## [0.2.26] - 2026-07-29

### Added

- widgets: `List::row_accessory` / `on_accessory_click` — trailing column with
  its own hit target (engine-owned body/accessory/scrollbar widths; no manual
  column math). `List::rich_items` draws styled spans on the body column.
  `List::on_row_double_click` — Table-style timed double-click on the row
  body (`EventCtx::click_count() >= 2`); supersedes `on_activate` for that
  press when bound.
- ui: `Element::on_paste` — Capture-phase paste intercept on the path toward
  focus (`PasteAction::Consume` stops before the focused editor).
- examples: `presence_board.rs` — agent sidebar with rich rows, double-click
  DM, trailing × moderation affordance.

### Documentation

- `docs/api.md`, `docs/faq.md`: List accessories, `click_count()` semantics,
  scrollable rich list recipe, app-level paste hook.
- `examples/README.md`: `presence_board` on the learning path.

## [0.2.25] - 2026-07-28

### Added

- term: `platform_clipboard` — when the terminal does not advertise OSC 52
  copy, selection release-copy and explicit `copy_to_clipboard` calls use
  the host clipboard (`pbcopy` on macOS, `wl-copy`/`xclip` on Linux,
  `clip.exe` on Windows). The driver shows a one-time labeled notice when
  the fallback path is used.

## [0.2.24] - 2026-07-26

### Added

- app: `ReasoningSelect` — the reasoning-effort control (backlog
  app-kits/1250, the cross-seat reasoning plan's tui section). The
  effort ladder `none|minimal|low|medium|high|xhigh` + `auto`
  (`REASONING_LADDER`/`REASONING_AUTO`); capability facts arrive AS
  DATA (`ReasoningFacts::capable/non_reasoning/unknown`,
  `#[non_exhaustive]`) and the widget enforces the three-state
  coupling: capable offers auto/none + the DECLARED levels only
  (verbatim, deduplicated); non-reasoning renders locked
  (`r: none (locked) — model does not reason`), faint, unfocusable,
  and refuses to open; unknown is locked-to-none with a
  "set anyway (capability unknown — passed verbatim)" override row
  that unlocks the full ladder per instance (remount-with-fresh-facts
  is the documented model-change reset). Composes the select-family
  core (movement, type-ahead, owned SCREEN-anchored popup — the
  inside-a-Modal anchor pin carried over); `on_change(&str)` fires
  once per changing commit against the EFFECTIVE value; the engine
  mints no wire vocabulary (apps write the `thinking` key).
- app: `reasoning_label`/`reasoning_label_glyph` + `LockState` — the
  parity footer grammar (`r: <value>`, ` (locked)` /  ` ⊘` when
  locked), ONE public source for code-tui/code/console footers,
  golden-tested. Glyph research recorded: no padlock exists outside
  emoji-data; `⊘` U+2298 is the one candidate narrow in BOTH
  unicode-width conventions; the PLAIN form is canonical.
- widgets: `ThinkingFold` + `ThinkingFoldState` — the reasoning-text
  fold: a Disclosure-based card, FOLDED by default, muted "Thinking"
  title, token-count detail slot, body through the shared markdown
  typeset recipe (fences/tables tint mid-stream; capped at
  `max_body_rows` with a scrollbar past it). Streaming semantics
  pinned: `append(fragment)` re-typesets the open tail only and
  advances a PER-APPEND dot indicator (data-driven — no timers, a
  quiet or completed fold schedules nothing); `complete(aggregate)`
  REPLACES the fragments (last-wins; double-complete replaces again);
  post-complete fragments are refused. Input is result METADATA,
  never parsed from reply prose.
- widgets: `Disclosure::title_muted(bool)` +
  `Disclosure::detail_signal(Signal<String>)` — the ambient-card title
  tone and the LIVE detail slot (writes repaint the title row without
  remounting it, so a focused header keeps focus; empty = none; the
  signal wins over the static slot). ThinkingFold is the first
  consumer.
- examples: `reasoning.rs` — a fake chat turn streaming a ThinkingFold
  (fragments, then the recomposed aggregate: last-wins visible on
  screen) + a footer ReasoningSelect cycling three fake models
  (capable / locked / unknown with override), the remount-reset
  recipe, and the wire line showing the APP writing the `thinking`
  key. Headless-safe (skips cleanly without a tty).

## [0.2.23] - 2026-07-25

### Documentation

- faq.md: new entry "How does AbstractTUI compare to ratatui, Textual,
  Bubble Tea, notcurses, and Ink?" — the architectural comparison
  (update/rendering models side by side, the structural differences in
  media and headless testing, and the honest maturity limits: young
  ecosystem, Windows compile-checked only, a11y roles without a
  screen-reader bridge, no async-runtime integration), cross-linked to
  architecture.md. llms.txt/llms-full.txt re-spliced.

### Added

- tests: the modal-over-page honesty net (`tests/wave_modal_bg.rs` +
  `wave_modal_bg_parts/fixture.rs`) — the gateway-console field report
  "that modal crashes the background" (operator screenshot 2026-07-25)
  investigated repro-first: the console's exact screen shape (PageHost
  chrome, review page with grow/pinned blocks, the 76x18 sandbox
  modal) driven through the real `Driver`/`CaptureTerm` across the
  whole operator session — open (store reset riding the open's own
  batch), Select popup over the modal, toast raised + expired over the
  modal, typing, page dyn re-renders UNDER the open panel, busy-line
  ticking, resize ladders incl. width- and height-clamped panels and a
  page in clean-absence crush, close — at seven sizes with two oracles
  at every step (forced-full-redraw equality; byte integrity outside
  the panel ∪ legit bands). All green at HEAD: the incremental
  pipeline is honest for this class; the screenshot decodes as the
  DESIGNED composition of an oversized, undressed translucent panel
  over a dark page (console-side fix, one chokepoint —
  `reviews/wave13/modal-bg-diagnosis.md`, consumed by the gateway
  seat). No engine code changed.

### Added

- widgets: `Block::on_close` — the panel close affordance (backlog
  app-kits/0605, operator order: "a cross … on each discussion panel so
  I can close them and the other panels get more space"). Opt-in per
  panel (callback presence, the `on_click`/`Drawer::on_close`
  convention): a muted `✕` (U+2715 — East-Asian-narrow, emoji-data-free;
  `×` U+00D7 is Ambiguous and was rejected) rides the title row's right
  end inside the border corner; hover tints it `error` (a
  consequence-bearing action, the diff-vocabulary tint), press adds
  BOLD. MOUSE-ONLY and never focusable (a focusable ✕ steals a panel's
  first focus — the drawer 0.2.12 lesson; keyboard close stays
  app-side), activating on press + release-inside (the Button 0.2.20
  capture convention — never fire-on-down). Truncation order pinned:
  the title truncates and then disappears BEFORE the ✕ yields its
  padded 3-cell run, the pads drop next (bare glyph), and at 1–2 total
  columns the ✕ yields too — nothing ever paints on or outside the
  frame, crushed blocks (zero area either axis) paint and hit nothing.
  `on_close` may dispose the panel's scope synchronously (the 0297
  disposal law — removing the pane in the callback IS the intended
  usage; survivors re-flex into the space). Honest a11y: a `button`
  labeled `Close {title}`, outside the tab order. Composes inside
  `PageHost` pages, `Drawer` overlays (screen-space clicks translate
  to the layer) and `Scroll`ed columns mid-scroll; parked closable
  blocks cost zero idle bytes (all Driver-level test-pinned).
  `examples/widgets.rs` gained a closable-panels row (restore button
  included); docs/api.md gained the Block close-affordance section.

### Added (content rendering — wave 13, ADR-0005)

- adr: ADR-0005 "Content rendering responsibilities" — markdown / code /
  diff / JSON / YAML text rendering IS core (pure cell math, zero
  dependency pressure, every app class needs it; the `abstracttui[md]`
  feature idea is rejected per ADR-0004's classification); diagram-class
  content stays extension territory (mermaid); mdpad remains an app and
  the quality bar. Driven by the operator's console complaint ("the
  markdown viewer is absolutely terrible and it doesn't even scroll …
  not rendering tables either in panels"); the console's own bare-text
  rendering is reported to the gateway seat in
  `reviews/wave13/console-md-adoption.md`.
- widgets: `MarkdownView` and `CodeView` carry an INTRINSIC MEASURE —
  markdown answers its typeset row count at the offered width (one
  width-keyed cache shared with the draw, so clamps can never drift
  from pixels), code answers lines × (gutter + longest line).
  `Scroll::new(view)` now scrolls the true extent out of the box (the
  wave-12 "Scroll over measureless content collapses to zero" class,
  fixed in the engine), and content-sized panels hug the document
  instead of rendering NOTHING (the vanished-table-in-a-Modal repro).
  Both defaults gained `basis(Cells(0))` beside `grow(1.0)`, so
  definite flex layouts are byte-stable and fixed siblings are never
  crushed (the 0240 modal-overflow class, guarded by test).
- text: `JsonLexer` + `YamlLexer` + the `DataKind` vocabulary
  (`#[non_exhaustive]`, the `DiffKind` precedent — keys are not a
  `TokenKind` the frozen C-like vocabulary can express), and
  `widgets::data_token_color` — the ONE data-kind→ink mapping: keys
  `syntax_func`, strings `syntax_string`, numbers `syntax_number`,
  literals `syntax_keyword`, comments `syntax_comment`, punctuation
  `syntax_punct`, YAML tags/anchors/doc-markers `syntax_type`.
  `CodeView::lang("json"|"jsonc"|"json5"|"jsonl"|"ndjson"|"yaml"|"yml")`
  routes; ` ```json `/` ```yaml ` markdown/Feed fences tint through the
  same recipe. Stateless per line, total over hostile bytes
  (goldens + a mixed-UTF-8 corpus pinned).
- render/md: `TableBlock.declared` (+ `with_declared`) — records
  whether each column's alignment was WRITTEN (`:--`) or defaulted
  (bare `---`); `parse_doc` fills it from the delimiter row. Enables
  the numeric auto-alignment below without ever overriding an
  explicit marker.

### Changed (content rendering — wave 13)

- widgets/markdown tables: the overflow ladder is now HONEST. Under
  crush the old fair-share/Flex mix clamped columns to zero in
  first-come order (rightmost cells silently vanished — the operator's
  "tables don't render in panels" shape). Now every column floors at
  `min(natural, 3)` and grows toward natural proportionally to need
  (largest-remainder tiling, the shared `distribute`); when even the
  floors overflow, the grid degrades to a RECORD layout — `Header:
  value` lines per row, labels bold, records blank-separated, long
  values wrapped. Nothing is ever silently dropped (repro tests:
  `crushed_columns_floor_instead_of_zeroing`,
  `unfittable_table_degrades_to_records`).
- widgets/markdown tables: numeric columns with NO written alignment
  marker auto-right-align (the mdpad rule — "the way a human would lay
  them out"); explicit `:--`/`:-:`/`--:` are honored verbatim; a
  word-shaped cell keeps the column left; conservative numeric test
  (digits + sign/grouping/percent/currency only).
- widgets/markdown headings: H3+ carry a faint hash prefix (`### ` in
  `text_faint`, ported from mdpad) — L1/L2 stay clean (ink-differentiated),
  deeper levels stay tellable apart where the inks stop
  differentiating. The outline fold points at the same rows (anchor
  test updated deliberately: heading rows now END with the heading
  text at H3+).

## [0.2.21] - 2026-07-25

### Added

- theme: first-class polarity (backlog app-kits/0595). `ThemeMode::{Dark,
  Light}` — a closed enum (the decisive-ground invariant admits no third
  value) — `Theme::mode()` derived from the audited `dark` flag (one
  source, never a second luminance threshold), and
  `theme::themes_by_mode(mode)` listing one mode's themes in the curated
  order (house palette first, then families, runtime registrations
  trailing; ordering documented and test-pinned).
- app: `toggle_mode()` — flip dark ↔ light keeping the user's theme
  CHOICE per mode. `set_theme` (the one signal-write choke point) records
  every switch as its mode's last-used theme, so
  `nord → toggle → abstract-light → toggle → nord` round-trips; a mode
  never visited falls back to its house palette (first-of-mode by the
  documented ordering).
- app: `ThemeSwitcher` — the drop-in one-cell theme control for any
  app's chrome, composing the select-family substrate (`pub(crate)`
  select core + the OWNED anchored popup) instead of hand-rolling a
  fourth movement/type-ahead implementation. Menu face
  (`ThemeSwitcher::new()`): a `☾`/`☼` icon button (glyph shows the
  CURRENT mode; both glyphs East-Asian-neutral and emoji-data-free —
  deliberately not `☀` U+2600, which some terminal stacks promote to a
  double-width emoji) opening a grouped Dark/Light popup — headers as
  skipped rows, current theme marked `●`, live preview on movement
  (the `commit_on_move` theme-picker semantic), type-ahead with
  repeated-letter cycling, Enter/click commits, Escape restores the
  pre-open theme, outside press keeps the preview; the menu re-resolves
  its own tokens per preview step; SCREEN-space anchored (correct
  inside `Modal`/`Drawer` — the anchored-layer P1 class, test-pinned)
  with the highlight window capped to the solved rect on short
  viewports. Toggle face (`ThemeSwitcher::toggle()`): one click =
  `toggle_mode()`, no popup. `on_change(|theme|)` fires once per switch
  that sticks (never on previews, Escape, or mechanical dismissals).
  Zero idle while closed (test-pinned). A11y: `button "theme"` /
  `"toggle theme mode"` with the active theme label as value; the popup
  is a `menu` of `menuitem` rows. Wired into `examples/themes.rs` (both
  faces) and `examples/shell.rs` (footer). Docs: theming.md "Theme
  modes & the switcher" + api.md "app::ThemeSwitcher".
- Survey note (the abstractuic port audit, re-run this wave): the
  engine's 26-theme registry is a strict superset of abstractuic's 21
  `THEME_SPECS` themes, and every one of the 21 matches today's
  `theme.css` byte-for-byte on all twelve seeded fields — zero drift,
  nothing new to port, no value changes shipped (themes apps pin are
  never churned cosmetically).

## [0.2.20] - 2026-07-25

### Added

- widgets: file-attachment surfaces (backlog first-app/0273, the
  code-tui field request — commons#5407/5408). Three additive pieces:
  `TextInput::on_paste` + `TextArea::on_paste(FnMut(&str) ->
  PasteAction)` intercept every paste BEFORE insertion with the raw
  text (`PasteAction::{Insert, Consume}`, `#[non_exhaustive]`;
  `Insert` is byte-identical to an unhooked editor, `Consume` inserts
  nothing — no `on_change`, caret/history untouched; fires in masked
  fields; disposal-safe from both arms); `input::paste::classify` —
  the pure, zero-I/O file-drop classifier corpus-tested against the
  researched drop spellings of Terminal.app, iTerm2, Ghostty
  (incl. the GTK newline-joined multi-drop), WezTerm (all five
  `quote_dropped_files` modes), kitty, Windows Terminal (incl. WSL
  single-quoting), GNOME/VTE (`g_shell_quote`d paths) and raw
  `file://` uri-lists, with the documented refuse-when-ambiguous
  policy (a false positive eats user text; a false negative just
  pastes); and `FilePicker` — the modal-friendly picker widget
  (breadcrumb, live type-to-filter, kind glyphs + optional size
  column, keyboard nav, opt-in multi-select with cross-directory
  marks, `on_pick`) over the pure `FileSource` seam with
  `StdFileSource` (`std::fs`, dirs-first case-insensitive, hidden
  toggle) beside it. New example `examples/attachments.rs` wires all
  three; api.md gains the "File attachments" section with the
  terminal-drop reality table.

## [0.2.19] - 2026-07-25

### Fixed

- examples: the pixel-review wave — every example captured in staged
  states through the screenshot pipeline and reviewed as renderings;
  five examples shipped broken or degraded pixels (components stat
  cards and grid cards rendered EMPTY, gallery's badge strip
  overpainted its panel border, dashboard's io meter leaked through a
  border and its help modal crushed its key list, voice_mock's meter
  and scope were structurally invisible, shell's reader page was
  empty, decide's footer split across a gap row, screenshot's note
  dropped its filenames). All fixed; `docs/captures/` refreshed for
  the affected examples (two-run byte-identical determinism held).
- docs: guide staleness sweep — graphics-and-3d.md signature +
  canvas cross-link + screenshot-veil honesty + Viewport3D default;
  live-data.md gains the one-clock `set_loop_clock` testing note;
  faq/troubleshooting damage-visualizer honesty
  (`render::Compositor::set_debug_damage` is embedder-only today);
  graphs-and-diagrams.md mermaid subset table re-synced verbatim with
  the crate (four rows had drifted).

### Added

- rustdoc: type-level docs for eleven previously bare docs.rs pages
  (List, Table, Block, Tabs, Scroll, TextInput, MarkdownView,
  CodeView, Image, Viewport3D, PageHost) plus expanded Scope and
  Style/LayoutStyle orientation docs.
- internal: 12 of the 26 oversized engine files split into named
  `#[path]` siblings (the whole `three/` family, term/caps, ui/mod,
  ui/tree, app/overlays, testing/vt, term/unix — zero behavior
  change, every public path preserved; inventory in backlog 0990).
  All 19 perf budgets re-run and pinned
  (`reviews/wave12/perf-status.md`) — zero emission drift from the
  week's changes; the held-borrow reentrancy contract documented on
  `widgets::SharedCallback` with two tripwire tests; Windows msvc
  check clean workspace-wide.
- examples: `capture -- review` family — staged multi-state review
  shots for every example (never published, never part of `all`).

## [0.2.18] - 2026-07-25

### Added

- widgets: canonical `.view(cx)` on every element-only widget —
  Sparkline, LineChart, BarChart, Progress, Spinner, Badge, Separator,
  RichTextView, CodeView, MarkdownView, Logo (api.md's "the canonical
  build is `.view(cx)`" sentence was false for these eleven: they had
  only `element(&tokens)`). Uniform shape: theme tokens resolve from
  context; `element` stays the explicit-theming door. Pinned by
  `wave11_review::canonical_view_builds_render_for_every_element_only_widget`.
- reactive: `set_loop_clock(Option<Instant>)` is public — custom loop
  authors driving `run_due_timers` publish their clock around user-code
  phases exactly as the driver does automatically, so timer ARMS ride
  the same injected timeline as fires; `None` restores real-time
  arming for bare rigs.
- examples: `activate` — selection vs activation on `List` and `Table`
  side by side (Enter/Space always; the timing-free click-on-selected
  picker gesture; `Table`'s true double-click), with one narrated
  status line — the 0.2.16 double-click convention's smallest runnable
  teacher. Referenced from api.md's Double-click section.
- tests: ten new pty smoke cases (`tests/live_smoke.rs`, `--ignored`) —
  feed, gallery, decide, drawers, shell, screenshot, caps plus the
  extension examples workflow/network/mermaid: every example in the
  repo now runs under the VT referee (the shared build is
  `--workspace --examples`). Driver-level coverage fills in
  `tests/wave11_review.rs`: live theme switching through the Driver
  with a screenshot diff, and the first Viewport3D integration test
  (render/orbit/zoom through real SGR input).

### Fixed

- reactive: timer arm/fire clock coherence — `after`/`interval` (and
  `Connection` retry arms) measured their deadlines from a fresh
  `Instant::now()` but FIRED against the loop's injectable clock, so
  under an injected test clock on a loaded machine a zero-delay timer
  could never come due (live symptom: the drawer feed-page scroll test
  flaking in full-suite runs — the Scroll width probe's `after(0)`
  never fired). Arms now measure from the LOOP's clock: the turn's
  published time inside a driven turn (via the new `set_loop_clock`,
  published/cleared by the driver around each turn), the fire-pass
  clock inside a timer callback, real time outside any turn.
- app: a closing modal `Drawer` released the keyboard only after the
  slide finished — a key pressed during the ~160 ms closing flight
  died in the focus-trapped dying panel (Esc-then-shortcut lost the
  shortcut; found live by the new drawers pty smoke). A close now
  releases the input trap the instant it begins (modal routing off
  immediately; focus blurred on the next turn's phase U — Esc arrives
  mid-dispatch of that very tree, so the blur cannot be synchronous);
  a reopen reversing the flight re-arms the trap and re-establishes
  initial focus.
- widgets: `Viewport3D` shipped with a zero-height default layout — a
  draw-only node with no intrinsic measure solves to zero rows, so an
  un-`layout()`ed viewport silently rendered nothing and hit-tested
  nothing (every in-repo call site was passing `.grow(1.0)` by hand).
  The default is now grow-into-region, matching the Meter/AudioScope
  family. Found by this widget's first driver-level integration test.
- app: `Select` type-ahead read the wall clock for its prefix-jump
  window instead of the ambient event time the click-chain doctrine
  established. Now ambient-first with a real-time fallback — production
  behavior unchanged; injected-clock tests can script the prefix window
  deterministically.

### Documentation (quality wave 11, docs seat)

- api.md: the canvas custom-trace example now compiles (it called
  `dyn_view(cx, |_| ...)` — the real signature is
  `dyn_view(style, || ...)`); the Drawer install snippet is a complete
  compilable function (it referenced free `Duration`/`cx` with no
  imports); all 33 non-ignored api.md snippets are compile-verified.
  Section order regrouped so the guide reads top-down again (PageHost
  beside the widget library, canvas with the render family,
  "Stability and limits" closing ahead of the extensions coda);
  PageHost added to the widget catalog list; cross-links added
  (double-click ↔ List/Table sections, testing ↔ screenshots &
  captures, canvas ↔ graphs-and-diagrams).
- getting-started: the first app is genuinely ONE import (the redundant
  `use abstracttui::widgets::Button;` contradicted the "one import
  covers the common path" line — `Button` rides the prelude); the
  headless-testing section links the screenshot export recipe;
  "Where next" gains the theming / graphics-and-3d / live-data guides.
  README's first app gets the same one-import fix ("Fifteen lines").
- examples/README.md restructured as a LEARNING PATH (first contact →
  widgets → layout → interaction → content + live data → app shell →
  graphics/3D → extensions → testing/capture) with a step table;
  detail sections added for `activate`, `decide`, `caps`, and the
  extension examples. Every example source header now carries a
  `Docs:` pointer to the guide section it illustrates.
- troubleshooting: three new entries for this week's field classes —
  content vanishing on small terminals (the engine's any-size
  guarantees + the `shrink(0.0)`/`Scroll` recipes), double-click not
  activating (the deliberate Table timing rules + the ambient
  event-time requirement in headless tests), and screenshot
  pixel-region veils (honesty, and the mosaic recipe when the still
  must contain the picture). faq: how to capture a screenshot; custom
  vector graphics vs the extension crates.
- architecture: the layer-map diagram gains the `canvas` module.
- api.md, cycle-2 folds from the code seat's wave: the canonical-build
  sentence's one honest exception documented (Meter/AudioScope are
  `view(cx)`-only, with the why); the controlled-mode binding naming
  convention stated (`value`/`selection`/`active`/`folded`/`offset_*` —
  state names; `Drawer::bind` recorded as the historical outlier);
  drawer close input semantics (keys route to the app the instant a
  close begins); the one-clock story extended to timer arms
  (`set_loop_clock` in the reactive section, cross-referenced from
  Double-click); `Viewport3D`'s grow-into-region default; the
  Tabs-index vs PageHost-id `on_change` rationale.
- Stale counts fixed everywhere (22 runnable examples in `examples/`
  plus 3 extension-crate examples; CONTRIBUTING example/test counts);
  llms.txt / llms-full.txt regenerated at 0.2.17 (package facts said
  crate 0.2.13).

## [0.2.17] - 2026-07-25

### Fixed

- widgets: `Table` no longer consumes `s` when NO `on_sort_requested`
  handler is bound — the 0980 field finding itself, retrofitted (the
  0.2.16 Enter/Space consumed-only-when-bound rule now covers sort
  too). Live impact: the gateway console's `s` = steer key died
  whenever its runs table held focus; an unbound Table now lets `s`
  bubble to app shortcuts
  (`s_passes_through_to_app_shortcuts_when_no_sort_handler`).
- changelog: restored the `## [0.2.15]` heading — the 0.2.15 entries
  (fusion-class fix, Modal resize re-clamp, the wave-10 size/ratio
  sweep) had been folded under `[0.2.16]` when the double-click wave
  reused the stamped section as its Unreleased home. Errata only; no
  code change.

## [0.2.16] - 2026-07-24

### Added

- ui: double-click synthesis (app-kits 0535 — the gateway-console
  request "single click selects; double click should open like Enter").
  Terminals report only raw SGR press/release, so the ENGINE now
  synthesizes multi-click counts: each `UiTree` folds mouse events
  through an embedded chain (same button, ≤ 400 ms, ≤ 1 cell Chebyshev;
  wheel/drag/other-button reset; modifiers don't break it; a
  selection-layer gesture claim resets too) and every mouse-Down
  handler reads the press's chain position via the new
  `EventCtx::click_count()` (1 isolated, 2 = double-click's second
  press, 3 = triple's third, saturating). Both presses deliver
  normally — selection on click 1 is never suppressed; the second press
  ADDITIONALLY reads ≥ 2. Time is never an implicit wall-clock read:
  the driver publishes its (`set_clock`-injectable) clock as the
  ambient event time each turn (`ui::set_event_time`/`event_time`), so
  one injected clock drives animations, timers, AND double-click tests;
  a bare `UiTree` without a published time counts every press 1,
  deterministically. The pure state machine is public as
  `ui::ClickChain` (`observe(now, &event) -> count`, configurable
  `window`/`tolerance`) for custom input paths.
- widgets: `Table::on_activate(usize)` — the "open this row" event
  (the 0250 selection-vs-activation split applied to Table, delivering
  the activation slice of app-kits 0530 §3): fires on Enter (always),
  Space (single-select alias; a future multi-select mode claims Space
  as toggle within that mode), and double-click (`click_count() >= 2`
  on the ALREADY-selected row — the row guard: a chained press that
  drifted onto a neighbor row only re-selects, so fast click-walking
  down rows never spuriously opens). Deliberately unlike `List`, a slow
  second click on the selected row does NOT activate (a table is a
  browsing surface; re-clicking a row to focus the pane must never open
  its editor — `List`'s timing-free click-on-selected picker gesture is
  unchanged and subsumes double-click). Enter/Space are consumed ONLY
  when a callback is bound (the field-gateway 0980 rule: an
  activation-less Table leaves them to app shortcuts), and the callback
  runs after all Table bookkeeping (disposal-safety law, test-pinned).
- docs: api.md "Double-click" section (the convention, the ambient
  clock rule, the custom-row recipe with the same-logical-row guard) +
  a "Table — selection vs activation" section; List's sections updated
  (the "no double-click synthesis" claim is superseded by subsumption).

## [0.2.15] - 2026-07-24

### Fixed

- ui: the FUSION class (gateway-console field incident 2026-07-24) —
  a node crushed to ZERO AREA by flex overflow pressure no longer runs
  its draw closure with the degenerate rect. Empty rects never
  intersect anything, so they fell through the paint cull and a
  hand-rolled closure that clips on one axis only (a title bar
  truncating horizontally, then painting "its" row) smeared that row
  onto whichever sibling owned the y — two texts fused on one row.
  Collapse is now CLEAN ABSENCE: the node's own paint skips, its
  children still walk (their rects are truthful — absolute children
  and main-axis-min flow children of an empty parent can be non-empty
  and still paint), the zero-collapse startup notice still names the
  crush (it rides the solver, not the draw), and rects crossing the
  empty threshold in either direction repaint correctly (fresh-paint-
  oracle pinned). `probe_when_culled` measurement probes are exempt:
  a scroll extent reading zero IS the reading the offset repair
  depends on (first-app/0281).
- app: `Modal` re-clamps on terminal resize (the Drawer contract,
  extended): the panel re-solves against the fresh viewport from the
  same size request — re-centering, clamping inside the new bounds,
  and recovering its requested size when room returns. Before this,
  at-open bounds were kept forever and a shrink could park a
  focus-trapped modal ENTIRELY off-screen (an invisible panel owning
  every key reads as a locked app).

### Added

- tests: the wave-10 size/ratio adversarial sweep
  (`tests/wave_size_sweep.rs` + parts) — heavy-fixture scenes driven
  through the real `Driver`/`CaptureTerm` across
  {80x24, 100x24, 60x16, 200x20, 60x50, 40x12}: chrome survival under
  content over-demand (pinned, unpinned, Scroll-absorbed), PageHost
  tab-bar windowing at 60/40 columns (goldens), oversized modals at
  open and across resizes, drawer extent clamps at small widths, live
  resize ladders with chrome+PageHost+drawer+modal all open compared
  cell-for-cell against fresh-paint oracles and the composed-frame
  screenshot, and CJK/emoji truncation walked for wide-pair soundness
  at glyph-splitting widths. Findings + the engine-guarantees vs
  app-recipes table: `reviews/wave10/size-ratio-sweep.md`.
- docs: `api.md` layout section gains "Small terminals & content
  pressure" — the engine's any-size guarantees and the two app-side
  recipes (`shrink(0.0)` pins on incompressible chrome; render
  `use_startup_notices` somewhere visible).

## [0.2.14] - 2026-07-24

### Added

- render: screenshots (control-plane 0370) — `render::Screenshot`, a
  captured screen as a plain value (grid of `ShotCell` = glyph + fg/bg/
  underline color + the full `Attrs` set), capturable from every truth
  surface: `Driver::screenshot()` (the composed frame as last
  presented — pure read, no re-render, no damage; byte-channel image
  placements stamped as labeled `pixel_regions()` from the live session
  bookkeeping), the component-reachable `app::request_screenshot(..)`
  verb (thread-local drain in the driver's phase U, the
  `request_full_redraw` shape; served the same turn a key handler
  requests it; no default hotkey — apps bind their own), the testing
  rig's `VtScreen::screenshot()` (what emitted bytes actually
  produced), and `Screenshot::from_surface(..)` for any surface. Three
  deterministic exporters + file conveniences: `to_text()` (plain
  UTF-8, trailing blanks trimmed), `to_ansi()` (replayable-with-`cat`
  SGR text using the presenter's own minimal-transition builders,
  fidelity pinned by a roundtrip law — replaying the export through
  the VT interpreter reproduces the capture exactly, with CHA column
  re-anchors after fusion-arming clusters: ZWJ/VS16/ambiguous-width +
  trailing regional indicators; row-relative so scrollback replay
  works), and `to_svg()`/`to_svg_with(fg, bg)`
  (GitHub-renderable: merged background rects, column-pinned
  `textLength` text runs, explicit decoration rects, XML-escaped,
  labeled placeholder veils over pixel-image regions). `Screenshot` +
  `request_screenshot` join the prelude; the `capture` example now
  emits a `.svg` beside every still in `docs/captures/`; new
  `screenshot` example carries the key-binding + headless-test-artifact
  recipes (exits 0 without a tty). Docs in `docs/api.md`
  ("Screenshots & captures"). Measured (300x100, dense styling):
  capture ~0.55 ms, text ~0.07 ms, ANSI ~0.43 ms, SVG ~0.87 ms.

## [0.2.13] - 2026-07-24

### Added

- canvas: the public sub-cell vector layer (extensions 0420) —
  `DotCanvas` (braille 2x4 / quadrant 2x2 dot grids, `#[non_exhaustive]`
  `DotMode`), stroke primitives `line`/`polyline` (Bresenham with
  parametric pre-clipping: far off-grid segments cost O(grid), never
  O(length)), `bezier_quad`/`bezier_cubic` (adaptive flattening,
  flatness tolerance in dot units, depth-bounded ≤ 4096 segments) and
  `ellipse_arc` (parameter-stepped ≤ 2048 segments, in-crate
  deterministic sin/cos so dot sets are bit-identical across
  platforms), `blit`/`blit_styled` into any `Canvas`/`StyledCanvas`
  (ONE stroke color per grid; empty cells skipped, later blits win
  overlapping cells — the documented cell-color z-order; styled blits
  carry attributes + link ids), eighth-block fills `fill_v`/`fill_h`,
  and the shared glyph vocabularies (`braille_bit`, `QUADRANT_CHARS`,
  `V_EIGHTHS`, `H_EIGHTHS` — deduplicated with `gfx::mosaic_fit`,
  tables not fitters). `Sparkline`/`LineChart`/`BarChart`/`Progress`
  now draw through the layer with byte-identical goldens (the
  refactor proof); the stroke + blit steady state allocates nothing
  (pinned in `tests/alloc_budget.rs`). `DotCanvas`/`DotMode` join the
  prelude; docs in `docs/api.md` ("Canvas & vector strokes") and the
  `canvas` module docs (worked doctest).
- workspace: the sibling-crate extension family scaffold (ADR-0004
  §3) — the root manifest becomes a workspace with
  `members = ["extensions/*"]` (root package unchanged;
  `extensions/` excluded from the published core crate), CI
  build/test/clippy/rustdoc gates run `--workspace` so family crates
  ride against core HEAD (the semver gate stays scoped to the
  published core until family crates publish), and
  `extensions/README.md` records the family contract (public API
  only, dual-form core dependency, core-first publish order).
- extensions family (first residents, released alongside this
  version): `abstracttui-graph` 0.1.0 (graph auto-layout —
  `GraphDesc -> Layout` with layered/force/grid passes — plus the
  `GraphView` widget) and `abstracttui-mermaid` 0.1.0 (honest-subset
  mermaid rendering: flowcharts/state compiled onto the graph crate,
  solverless sequence diagrams, atomic fallback). Each crate carries
  its own CHANGELOG; the family guide is
  `docs/graphs-and-diagrams.md`.
- docs: `docs/graphs-and-diagrams.md` — the extension-family guide
  (layout pass selection, the data contract, `GraphView` usage, the
  mermaid subset table, install lines, worked examples); release
  workflow extended to publish the family crates after core with an
  already-published skip (a core-only re-release stays green).

## [0.2.12] - 2026-07-24

### Added

- widgets: `PageHost` — the page-level tab host (app-kits 0545, the
  maintainer's "global tab system"). N full pages addressed by id,
  each a builder `FnMut(Scope) -> View` on a per-activation
  generation scope: only the active page is mounted; switching
  disposes the outgoing scope (no keep-alive by design — hidden pages
  would keep timers ticking against the zero-idle law; durable state
  lives in app-owned signals, the documented recipe). Themed windowed
  tab bar (sticky window around the active tab, `‹`/`›` overflow
  indicators as prev/next click targets, ellipsis truncation,
  reactive count/badge slots repainting the bar only), controlled
  (`active(Signal<String>)`) or uncontrolled (`initial(id)`) with
  disposal-safe `on_change(id)` (0297). Navigation: click,
  Left/Right on the focused bar (wrap), container-reserved chords
  (default Ctrl+PgUp/PgDn, Capture-phase so modifier-blind
  scrollables cannot eat them; normalized matching covers both wire
  spellings), opt-in digit jumps 1-9 riding the shortcut table, and
  focus re-anchoring after a chord switch (the 0230 dead-keys class).
  A11y: one tab stop, `Role::Tabs`, value `"Title (i/N) [badge]"`.
  Exported from the prelude; demoed in `examples/shell.rs` (the
  app-shell demo, co-owned with the drawer wave); acceptance in
  `tests/wave_page_host.rs`.
- app: `Drawer` — the global drawer system (app-kits 0585, the
  maintainer's "entity-app drawer" brief). Edge-anchored overlay
  panels (`DrawerEdge::{Left, Right, Top, Bottom}`, `DrawerSize::
  {Cells, Percent}`) hosting FULL pages over the app without touching
  its layout; install once (`install(cx, build)`), drive through
  `DrawerHandle` (`open`/`close`/`toggle`/`is_open`) or a bound
  `Signal<bool>` (controlled mode — one truth both ways). Focus
  modes: `Modal` (default: focus-trapped tree, Esc closes, optional
  outside-press dismissal + `overlay`-token scrim — the deliberate
  divergence from the non-modal web `AfDrawer`: a keyboard-first
  terminal panel must own keys to be usable) and `Passive`
  (glanceable; click-to-focus per the focused-overlay key rule; never
  a scrim). Slide transition rides `reactive::animate` +
  `LayerHandle::set_offset` — frames only while easing, damage billed
  to the drawer band, `motion(Duration::ZERO)` is the instant mode;
  closed drawers remove their layers and dispose the mount scope
  (state survives outside the builder, the Tabs rule — a hidden
  mounted tree would spin the frame loop on undrained damage).
  Stacking: fixed per-edge z slots below `MODAL_Z` (modal-from-drawer
  layers above; popups above everything; toasts on top), ONE drawer
  per edge (`DrawerCloseReason::Replaced`), resize RE-CLAMPS instead
  of dismissing. `on_close` observes every close with its reason
  (`Api`/`Escape`/`OutsidePress`/`Replaced`/`HostGone`,
  `#[non_exhaustive]`). Exported from the prelude; demoed in
  `examples/drawers.rs` + the `examples/shell.rs` drawer regions;
  acceptance in `tests/wave_drawers.rs` (slide-frames-then-idle-zero,
  band containment, feed page scrolls, modal-from-drawer, both wire
  spellings).
- testing: `CaptureTerm::push_resize(size)` — deliver a terminal
  resize through the headless driver (the wave-8 acceptance drives
  resize-while-overlays-open scenarios with it; previously only
  in-crate tests could re-publish a viewport).

### Fixed

- reactive: `animate`'s frame task now cancels quietly when the
  follower's owning scope was disposed MID-FLIGHT (dyn_view
  regeneration, a drawer replaced/host-gone while sliding) instead of
  panicking on a disposed-signal write — the Meter frame-task rule
  applied to the shared follower (0585;
  `follower_scope_disposed_mid_flight_cancels_quietly`).
- widgets: `PageHost` tab clicks now hit-test against the plan AS
  DRAWN (the pixels the user sees) instead of recomputing from the
  live model — a badge widening in the same batch as a click shifted
  the segment geometry under the pointer, landing the press on the
  wrong tab (cycle-2 cross-review F1;
  `wave_shell_review2::click_resolves_against_the_drawn_bar_*`). The
  draw closure publishes `(plan, width)`; the handler recomputes only
  before the first draw.
- app: the drawer header's ✕ is now a MOUSE-ONLY affordance (not
  focusable) — as the panel's first focusable it stole a modal
  drawer's initial focus from the CONTENT, leaving a hosted
  `PageHost`'s container chords dead until a click (the 0230 class
  inside a drawer; cycle-2 cross-review F2;
  `wave_shell_review2::page_host_inside_a_modal_drawer_owns_chords_while_open`).
  Esc remains the keyboard close; content now receives `focus_init`.
- app: closing a drawer (and shrinking one on resize) now repaints the
  region the panel occupied. A close slides the panel to its
  off-screen closed origin BEFORE removal, so `LayerHandle::remove`'s
  current-bounds damage clipped to EMPTY and the vacated cells never
  recomposited — an instant (`motion: ZERO`) or scrimless (passive)
  close left the frozen panel on screen (a modal scrim's full-viewport
  removal masked it for modal drawers only; cycle-1's close test
  asserted `layer().is_none()`, not the pixels, so it slipped through).
  `finish`/`reclamp` now name the panel's last-visible rect via the new
  crate-internal `Overlays::damage_root_under_rect` (cycle-3 acceptance
  F1; `wave_drawers::instant_scrimless_close_repaints_the_vacated_region`,
  `wave_drawers::scrimless_right_drawer_shrink_on_resize_leaves_no_stale_edge`,
  and the end-to-end `wave_shell_accept`).
- app: a drawer open whose one-per-edge claim was STOLEN while user
  code ran mid-open (the replaced incumbent's `on_close` reopening it,
  or the build closure opening another same-edge drawer) no longer
  mounts anyway — two panels could land on ONE fixed z slot, the
  equal-z trap the slots exist to avoid. The LAST claim owns the
  slot; the preempted open aborts before creating layers and fires no
  close reason (it never completed — which is also what makes a
  mutually-reopening callback pair terminate instead of recurse).
  (Cycle-2 cross-review;
  `wave_shell_review::reopen_from_on_close_during_replacement_keeps_one_drawer_per_edge`.)
- app: opening (or reopening mid-close) a MODAL drawer now blurs
  passive drawer trees — overlay key dispatch walks topmost-z first
  and a FOCUSED non-modal tree above the modal's fixed edge slot kept
  every key, so Esc closed a passive strip while the modal stayed
  open beneath its scrim. Clicking back into an unveiled passive
  panel re-steals the keyboard deliberately (the engine's one focus
  story). (Cycle-2 cross-review;
  `wave_shell_review::modal_drawer_takes_keys_from_a_focused_passive_drawer_above_it`.)
- app: the drawer scrim's veil cell is captured AT OPEN
  (`Mount::veil`) and reused by the resize re-clamp — the re-clamp
  repainted with the CURRENT theme's `overlay` token, minting a
  mixed-theme drawer after a theme switch + resize while open
  (latent with the registry themes, which share one overlay value;
  real for runtime-registered themes). Tokens resolve at open, the
  documented rule. (Cycle-2 cross-review;
  `drawer::tests::review::scrim_repaint_on_resize_keeps_the_at_open_veil_token`.)

## [0.2.11] - 2026-07-24

### Added

- widgets: `Disclosure` — the fold/unfold card (first-app 0260 +
  field-agora 0850, both operator-requested). A one-row title header
  (fold glyph `▸`/`▾` in accent ink, truncate-ellipsis title, optional
  right-aligned muted `detail` slot) over a body that mounts on expand
  and unmounts on fold. Toggle by clicking the title row or
  Enter/Space while focused (one tab stop, selection-pair focus
  affordance); state is widget-internal (`initially_folded`, default
  FOLDED — progressive disclosure) or app-owned
  (`folded(Signal<bool>)`, two-way — the toggle-all policy hook).
  `max_body_rows(n)` (default 8) caps the unfolded body at
  `min(content, n)` rows with a scrollbar when content overflows
  (`0` = uncapped natural height); `Disclosure::text`/`::markdown`
  convenience bodies typeset once through the shared Feed recipe and
  survive fold cycles; `.body(|scope| view)` hosts any `View`, built
  per expansion. `on_toggle(FnMut(bool))` runs after the state write
  (disposal-safe, backlog-0297 law). A11y: `region` labeled by the
  title wrapping a `button` whose value reads "collapsed"/"expanded".
  Exported from the prelude; demoed in `examples/components.rs`.
- widgets: `Feed::on_item_press(FnMut(&str, i32))` (field-agora 0850)
  — item-level press hit info: a left press over an item's rows
  reports `(key, row_within_item)` (row 0 = the item's first typeset
  row, the click-on-card-title gate). Gap rows and the void past the
  tail fire nothing; unbound feeds attach no handler. The row math is
  public as `FeedState::item_at_row(row)` (the inverse of `row_of`).
- widgets: `Scroll::extent_signal(Signal<(i32, i32)>)` — read back the
  content extent (measured mode publishes the solver's answer; hint
  mode lands the hint verbatim). `Disclosure` sizes its capped body
  region from it; apps get "N more rows" chrome.
- widgets: `Scroll::scrollbar_auto_hide(bool)` — opt-in: hide the
  vertical scrollbar while content fits the viewport (the column stays
  reserved so content width never re-wraps; the hidden strip ignores
  drags). Default `false` keeps the always-on bar byte-stable.
- docs: api.md gained the Disclosure section, the Feed item-press
  section and the message-card recipe (Feed + Disclosure semantics);
  live-data.md points at them from the transcript recipes.

### Fixed

- widgets: `Disclosure` review hardening (adversarial pass,
  `tests/wave_disclosure_review.rs` + `reviews/wave7/disclosure-review.md`):
  the header no longer paints its fold glyph outside a 1-cell-wide
  rect (draw closures are not clipped to their element — damage
  contract §5), and a capped body whose content measures ZERO rows now
  settles to the 1-row floor instead of standing at the full cap
  ("limited to", never "padded to" — only `(0, 0)` is the unmeasured
  sentinel, matching Scroll's offset-repair reading).

## [0.2.10] - 2026-07-23

### Added

- app: `ChoicePrompt::body_width(cols)` (first-app 0271, the adoption
  blocker) — a minimum content width the BODY contributes to the
  panel's measure. The panel was content-derived from the options,
  prompt, hint and buttons while the body closure stayed invisible to
  width, so a 72-col approval-card body clipped inside a ~45-col panel
  sized by three short options. The declared width folds into the same
  max/clamp as every other content line: the prompt wraps at the
  widened width, options/hint gain the room, narrow viewports still
  clamp with the existing margins (the body then clips inside its
  region — never the options). Like `body_rows`, it participates only
  when a `body` is set. `examples/decide.rs` gate 2 demos the 72-col
  case.
- app: `ChoicePrompt::dismiss_label(label)` (first-app 0271) — the
  dismiss affordance's vocabulary follows the caller: the button, the
  hint's Esc segment (`Esc Defer`) and the advertised Esc shortcut all
  carry the label, for surfaces whose Esc is not a cancel (the
  approval consumer's Esc DEFERS — the gated run keeps waiting;
  "Cancel" beside a "Deny" option mislabeled the consent surface). The
  outcome stays `ChoiceOutcome::Cancelled` (the caller's wiring maps
  it); the unset default keeps the built-in "Cancel"/"Esc cancels"
  byte-identical; irrelevant under `dismissable(false)` (must-choose
  still refuses visibly). Button/hint widths are measured from the
  actual label.
- app: `ChoicePromptHandle::retire()` (first-app 0271) — HOST close
  without resolving: the modal closes, `on_resolve` never fires, and
  the consumed exactly-once flag keeps every later ending (Esc,
  buttons, `cancel()`, stray keys) inert. Retiring means the host owns
  the outcome (picker-replace, policy auto-approval while the prompt
  is up) — distinct by construction from the user's Esc, which still
  resolves `Cancelled`; consumers no longer thread a side-channel flag
  to keep "user deferred" apart from "host retired". Idempotent; a
  no-op after resolution.

## [0.2.9] - 2026-07-23

### Added

- app: `ChoicePrompt::body(|mcx| view)` + `body_rows(n)` (first-app
  0287) — a structured, optionally scrollable, reactive body between
  the prompt heading and the options: per-call approval cards, an
  alternate JSON view behind a caller signal, a live tier line
  (`dyn_view`s inside the body re-render while the gate is up). The
  body is a v1 DISPLAY region: clipped to its solved row budget, the
  options are allocated first and never crushed (the 0240 law, floor
  one row under pressure), the gate autofocuses the options so keys
  stay the gate's vocabulary, and the wheel scrolls a
  `Scroll`-wrapped body while the pointer is over it.
  `examples/decide.rs` gate 2 demos it.
- ui: `KeyChord::normalized()`, `KeyEvent::normalized()`, and
  `KeyEvent::means_char(c)` — the shifted-letter spelling fold as
  public API for app-side matchers.

### Fixed

- ui/app: a shifted letter has TWO wire spellings — legacy Shift+A
  arrives as `Char('A')` with no mods, the kitty keyboard protocol
  sends `Char('a')` + SHIFT (base-key identity) — and every matcher
  compared exactly one, so the other wire's users pressed a DEAD key
  (first-app 0288/0286; the first app's live P0). One shared fold now
  covers every match surface: `ChoicePrompt` option keys
  (`option_key(…, 'A')` fires on kitty terminals; a declared `'a'`
  still never fires on Shift+A — case stays meaningful, only the
  spelling folds), tree `Element::shortcut` resolution, the `Actions`
  registry (collisions are judged on the normalized spelling), and
  `KeyState::pressed_chord`. Registrations keep their authored
  spelling for display. Shifted non-letter symbols keep their
  documented wire split (layout-dependent; the engine does not guess).

## [0.2.8] - 2026-07-23

### Added

- app: `ChoicePrompt` — the modal decision gate (app-kits/0515): block
  a flow on a structured question (prompt + options with optional
  muted detail rows, `allow_multiple` sets, an `allow_other` free-text
  row with an inline autofocused editor) and continue in `on_resolve`,
  which fires EXACTLY ONCE with `ChoiceOutcome::{Answered, Cancelled}`
  — Enter/click-commit, Confirm/Cancel buttons, Escape, and the
  returned `ChoicePromptHandle::cancel()` all funnel through one
  resolve path; the modal closes before the callback runs (the 0297
  disposal-safety law), so resolving may dispose the opener or open
  the next gate. Outside presses never dismiss (a gate has explicit
  endings only). Movement follows the 0250 vocabulary (arrows/wheel
  move, `1-9` jump, Space toggles in multiple mode, commit is
  explicit); long lists window around the highlight with an `i/N`
  note; the panel sizes itself from the question. Model types
  (`ChoiceQuestion`/`ChoiceOption`/`ChoiceAnswer`) are plain data for
  agent-driven callers; `ChoiceSequence` chains several questions
  (`Completed(answers)` / `Cancelled { index, answers }`). All
  re-exported in the prelude; demo `examples/decide.rs`; section in
  docs/api.md.
- app: `ChoicePrompt` cycle-2 fold (wave-5 REVIEWER findings F1–F7):
  per-option shortcut letters (`option_key` / `ChoiceOption::key` —
  case-sensitive explicit activation, rendered dim in the row, named
  in the hint), must-choose mode (`dismissable(false)` — Esc refuses
  visibly, no Cancel button, `handle.cancel()` keeps the programmatic
  lever), LAYERED Esc from the Other editor (first Esc retreats to
  the list keeping the draft, second cancels/refuses — the Combobox
  precedent, position folded from review), danger-tinted options
  (`ChoiceOption::danger` / `.danger(id)` — `Error` token ink),
  `option_with(ChoiceOption)` combination escape hatch, a
  keys-truthful hint row, and the accessibility contract: Heading
  question, Menu region with current-choice value, MenuItem
  "selected" / Checkbox on/off rows, Input editor with focus truth,
  visible focus affordance (selection pair focused, accent ink
  unfocused — the RadioGroup precedent).

### Fixed

- app: selection click-through (first-app/0285, field P0) — with select
  mode on, the screen-text selection layer consumed EVERY left Down and
  Up ahead of overlay/tree routing, so a drag-less click never reached
  any widget: every `Button` in the app (including modal approve/deny
  buttons) was dead by mouse. The layer now owns the gesture only once
  it DRAGS: a plain Down passes through (the anchor arms silently), the
  first Drag off the anchor cell claims the gesture (and resolves the
  press the tree already saw — a release outside every rect un-presses
  the widget without firing and drops the pointer capture), a drag-less
  Up passes so the widget fires, and a click on a VISIBLE region stays
  consumed (dismissal, Esc parity — both halves). Same-cell drag wiggle
  still clicks (cell quantization is the drag slop). Drag-copy,
  release-copy-ends-gesture (0290), copy keys, and wheel routing are
  unchanged. Click rules stated in docs/api.md.
- ui: pointer-capture heal — a pressed widget's own visual re-render
  disposed the captured instance (Button's `pressed` write on Down
  regenerates its `dyn_view` hit leaf inside that same dispatch), and
  the stale capture was silently dropped: a release OUTSIDE the widget
  then routed by position, never reached it, and wedged the pressed
  visual until the next click. A stale capture now re-points at the
  press cell's current occupant, making the documented "capture keeps
  the release routed here" contract actually hold; when the pressed
  subtree genuinely died, the gesture tail lands on whatever is beneath,
  which never armed a press (harmless by the release-inside-decides
  rule).

## [0.2.7] - 2026-07-23

### Added

- widgets: `FeedState::sync_with(cx, read, spec)` (first-app/0282) —
  the `sync` bridge behind a borrow-based source, for items living
  INSIDE a larger reactive shape (one field of a `Signal<Fold>`, a
  focus-selected convo's nested vec). The closure hands the items over
  in place (zero copies); every signal it reads becomes a dependency
  of the sync effect; a stats-only fold write re-runs the drain but
  the fingerprint walk renders nothing. `sync` now delegates to
  `sync_with` — one shared drain core (fast paths, rebuild policy,
  one-writer self-heal), byte-identical semantics.
- app: `Completion::trigger_at(char, TriggerPosition, provider)` +
  `TriggerPosition::{Anywhere, StartOfInput, StartOfLine}`
  (first-app/0292) — per-trigger position policy: slash commands scope
  to the draft's first token (leading whitespace tolerated), mentions
  stay `Anywhere` (the default plain `trigger` registers). A token
  outside its policy never opens the dropdown nor consults the
  provider. Re-exported in the prelude.
- app: `PanelPlacement::{BelowPreferred, AbovePreferred}` +
  `place_panel_biased`, `AnchoredPanel::open_passive_biased`,
  `Completion::placement` (first-app/0294) — opener-stated side bias
  for the anchored panel: `AbovePreferred` mirrors the classic rule so
  a bottom composer's SHORT candidate list sits above the caret
  instead of on the chrome row below (the status-bar occlusion).
  Default stays `BelowPreferred` everywhere — existing callers
  byte-identical (test-pinned parity grid). Re-exported in the
  prelude.
- widgets: `FeedItem::max_rows(rows)` + `FeedItem::overflow_marker(f)`
  (first-app/0283) — width-aware row cap on the most recently appended
  Text/Rich feed block, applied POST-WRAP at the width the engine
  typesets at. Overflow shows the first `rows - 1` wrapped rows plus
  an honest marker row ("… (+K more lines)" or the closure's wording,
  `text_muted` ink); K is the hidden wrapped-row count at the current
  width, so it changes on resize. A capped block is never taller than
  `rows`, never hides content silently, and extent/windowing count the
  marker row. Uncapped blocks and streaming items are byte-identical.

### Fixed

- widgets: `Scroll` now repairs a bound offset that a CONTENT shrink
  (details fold, session switch) or viewport growth left beyond the
  new maximum (first-app/0281) — the pane used to render void until a
  gesture rescued it. The repair clamps the offset signal down when
  the measured extent or viewport box changes; in-range programmatic
  writes are never touched, growth never moves a reading user, and
  follow-tail is neither disengaged nor armed by a repair. Includes a
  crate-internal paint-walk exemption so the content-extent probe
  keeps measuring while the content wrapper is fully scrolled out of
  the clip (the state where the probe previously starved).
- widgets: `TextArea`/`TextInput` placeholders clip to the widget's
  interior in BOTH branches (first-app/0284) — a hint longer than the
  interior used to overwrite the widget's own right stroke and escape
  the rect at narrow widths (draw closures clip to damage regions,
  not element rects). Clipped with `truncate_ellipsis`, so a cut hint
  is honest about it; at interior width 1 it degrades to a bare `…`.

## [0.2.6] - 2026-07-23

### Added

- app: `request_full_redraw()` — the public Ctrl+L-class verb
  (first-app/0299). Component-reachable (thread-local request drained
  by the driver's next turn, the `mouse_capture()` shape): the next
  frame re-emits EVERY cell with absolute anchoring and re-places
  protocol images, healing terminal content destroyed externally
  (Cmd+K, `printf '\033c'`) that model-side damage can never repair.
  One full-frame emission, then idle returns to zero bytes. Re-exported
  in the prelude.
- app: `set_redraw_on_focus_gained(bool)` / `redraw_on_focus_gained()`
  — opt-in auto-heal (first-app/0299 ask 2): a full redraw whenever
  the terminal reports focus-in (DEC 1004), so an externally cleared
  screen fixes itself at the next focus round-trip. Default OFF:
  existing sessions stay byte-identical (tmux pane switches fire focus
  events constantly). Not a `RunConfig` field — that struct is
  literal-constructible, so a new field would be a semver-major
  change.
- widgets: `TextArea::placeholder_while_focused(bool)` and
  `TextInput::placeholder_while_focused(bool)` (first-app/0291) —
  opt-in placeholder while focused-and-empty, painted one cell past
  the caret in the same `text_faint` ink (the caret block stays
  visible). Without it, an `.autofocus()`ed composer never renders its
  placeholder at all (focused from boot; the classic rule paints only
  when unfocused). Default OFF: existing apps render byte-identically.

### Fixed

- app: suspend-resume (and the new full-redraw verb) now genuinely
  re-place protocol images. `resync_unknown_screen` marked image
  overlays dirty, but `ImageSession::sync` answers `Unchanged` for an
  unmoved same-version slot — so a resumed screen restored every cell
  and silently lost every kitty/iTerm2/sixel placement. The resync now
  forgets terminal-side image state per channel (kitty: `release` —
  delete + full retransmit, leak-free either way; cursor-paint
  channels: `invalidate_slot`) so the next sync re-emits in full.

## [0.2.5] - 2026-07-23

### Fixed

- examples/images (field bug, tall-narrow terminals — but width-
  independent): the four mosaic panes rendered as ~2-column EMPTY
  bordered strips bunched left. The pane row had no `grow`, so inside
  the growing `dyn_view` HOST it sat at its intrinsic width (four
  border-only panes + gaps = 11 cells) and the `grow(1.0)` panes split
  2 cells each — all chrome, zero interior (the RT8-6 multi-pane
  collapse class hidden behind a dyn_view boundary; the checked-in
  docs capture showed the same strips at 100 cols). The row now grows
  and the panes carry an explicit zero basis (exact quarters);
  `docs/captures/images{,.styled}.txt` regenerated. Regression-pinned
  headlessly at 70x60, 70x45, 100x30 and 110x30
  (`tests/wave_images_layout.rs`).
- widgets: `Image` answered 0x0 to `Auto` sizing (draw-only element, no
  measure) — the same collapse class at the widget level: images in
  unsized rows or content-sized panels vanished entirely. The widget
  now measures as its native cell footprint through the mosaic mode's
  subpixel density (CSS natural-size analog; broken sources answer the
  labeled state's 7x2 footprint so the label survives). Explicitly
  sized/grown compositions are unaffected.
- widgets: `Image` fit math (`resolve_fit`) is now total — degenerate
  rects/empty sources resolve to an empty target instead of a clamp
  panic when called directly, and any rect >= 1x1 keeps a >= 1x1
  target at every hostile aspect ratio (tall-narrow panes, one-cell
  strips, 1000x2 sources; unit-pinned).

### Added

- ui: `Element::measure(fn(Size) -> Size)` — intrinsic content size for
  draw widgets, the same contract text leaves fulfil through
  `text::measure`. A measured element mounts as a layout leaf; the
  measure wins over children aggregation. This is the engine door the
  `Image` fix rides through (and any custom chart/canvas widget with a
  real content size).
- term: pinned env-evidence test for `TERM_PROGRAM=vscode` (VS Code,
  Cursor and forks — xterm.js): truecolor + OSC 8 hyperlinks
  (xterm.js >= 4.3) + focus reporting (DEC 1004) claimed; OSC 52,
  kitty keyboard/graphics, sixel and undercurl stay probe-gated. The
  detection itself already existed; the claims are now test-pinned
  with a citation comment.

## [0.2.4] - 2026-07-23

### Added

- examples: `caps` — the live terminal-capability report (probe upgrades
  render on screen; the `images via` line names the channel the image
  ladder picks; headless runs print the environment-detected set).

### Documentation

- graphics-and-3d: new "Verifying image support on your terminal"
  section — the two-command recipe, per-terminal expectations (kitty /
  WezTerm / Ghostty / iTerm2 / VS Code / foot / Terminal.app / tmux),
  and how to read the capability rows; FAQ image answer now points at
  the `caps` report.

## [0.2.3] - 2026-07-23

### Added (input/AV wave — games/0700, media-av/0610 + 0620 + 0650)

- `app::keys` (games/0700): key press/release STATE as a first-class
  input fact. `use_key_state(cx)` arms a driver-fed service tapping the
  pre-conversion input stream (releases were decoded and then dropped at
  the routing seam): `is_down`/`keys_down` (held keys, chords included),
  per-turn `pressed`/`pressed_chord`/`released` edge sets sealed by the
  driver's phase U, and `focus_cleared` (FocusLost empties the down-set
  and synthesizes labeled release edges). CAPABILITY HONESTY:
  `KeyFidelity::Full` only where kitty release events are actually live
  (protocol spoken + event-type flags pushed, re-published at the 0293
  mid-session upgrade); on `Degraded` legacy wires press edges stay
  honest but the surface never claims "held" — deliberately NO
  repeat-timeout hold approximation (a dropped repeat would fabricate a
  release mid-hold). `hold_gesture_label(fidelity, chord)` gives hint
  lines the truthful wording. Zero cost until armed; zero per-turn cost
  while quiet (alloc-budget pinned).
- `app::PushToTalk` (media-av/0610): the capture-gesture contract over
  the key-state service. Hold-to-talk on `Full` fidelity (press starts,
  release stops; same-turn taps fire start then stop), labeled
  latch/toggle mode on `Degraded` wires (never a fake hold),
  `Signal<CaptureState>` as the one truth for meters/badges/feeds,
  `on_start`/`on_stop(reason)` callbacks, and the mic-privacy rule:
  focus loss stops capture in every mode and capture never auto-restarts
  on focus return.
- `widgets::Meter` (media-av/0620): level meter with real ballistics —
  instant attack, frame-clocked decay (default 20 dB/s over the span,
  frame-rate-independent), peak-hold marker (~1.5 s then fall), optional
  `db_floor` log mapping, one channel (horizontal/vertical) or N band
  bars, eighth-block sub-cell fill, zone inks from the `ok`/`warn`/
  `error` tokens. THE IDLE LAW, test-pinned: a silent meter reaches its
  fixpoint and stops requesting frames — unchanged input costs zero
  frames and zero allocations.
- `widgets::AudioScope` (media-av/0620): rolling waveform strip over a
  `Signal<Vec<f32>>` window on the braille chart substrate (pair with
  `bounded_source` + `DropOldest`: the source window IS the ring). No
  clock of its own — quiet data means nothing re-renders.
- `examples/voice_mock.rs` (media-av/0650): the whole voice surface with
  no audio and no network — push-to-talk on Space with the truthful
  gesture label and live key-state fidelity in the footer, a timer-driven
  sine+noise fake mic through `bounded_source` into the dB meter, band
  spectrum and scope, and a fake transcription feeding words into a
  `Feed` while "talking". Joins the live pty smoke matrix.
- tests: `tests/wave_inputav.rs` (driver-level key-state/PTT/meter
  acceptance over scripted kitty and legacy wires, incl. a WASD
  held-key pan proof for the games lane) and an alloc-budget extension
  pinning idle turns at zero allocations with a parked meter, quiet
  scope, armed key state and a bound PTT mounted.

### Added (reader wave — app-widgets 0142/0144/0146/0148)

- `render::md::DocBlock` + `parse_doc` (0142): the doc vocabulary —
  GFM tables (`TableBlock`: header, `:--`/`:-:`/`--:` alignment,
  body rows, inline styles in cells, `\|` escapes), whole-line
  `![alt](src)` image blocks (`ImageBlock`), and `- [ ]`/`- [x]`
  task items (`TaskBlock`) — additive beside the exhaustive core
  `Block` enum (`DocBlock` is `#[non_exhaustive]` from birth; on
  table-free sources `parse_doc` equals `parse` wrapped in `Core`,
  test-pinned). `DocStreamSession` streams it with the same
  freeze/equivalence contract as `StreamSession`; a table opens at
  header+delimiter, grows per pipe line, seals at the first non-pipe
  line (any chunking equals the batch parse — fuzz-pinned against the
  hostile corpus).
- `~~strikethrough~~` in the core inline vocabulary (attribute-only:
  `Attrs::STRIKE`; unclosed/empty degrade literal like `*`/`` ` ``;
  `\~` escapes).
- `render::md::outline`/`slugify`/`Heading` (0146): heading extraction
  with GitHub-compatible anchor ids (lowercase, punctuation dropped,
  spaces to dashes, `-1`/`-2` dedup — golden table incl. unicode;
  documented deviation: combining marks drop). Widget layer:
  `MarkdownView::outline_rows` (headings paired with their typeset row
  at a width — TOC jump targets from the SAME fold that draws) and
  `MarkdownView::resolve_anchor` for `[text](#anchor)` links.
- `MarkdownView` renders the doc vocabulary (0142/0144): tables typeset
  through the Table widget's own `solve_columns` (one column policy —
  natural widths when they fit, fair-share flex + per-cell ellipsis
  truncation when they don't; bold header, border-ink separator,
  alignment honored); images render as MOSAIC rows in the flow —
  header-only sizing at typeset (`gfx::probe_dimensions`, PNG IHDR +
  JPEG SOF walk, fuzz-pinned to match the real decoders), LAZY decode
  on first draw cached by (path, signature, size) across rebuilds,
  alt-text captions, labeled missing-file/decode-failure states.
  Pixel-protocol images in scrollable flow are deliberately deferred
  (mosaic cells are cell-safe in any scroll context; the
  placement/eviction question is named in 0144).
- `MarkdownView::find` + `.highlights(matches, current)` +
  `MdSearchMatch` (0148): find-in-typeset-text (literal + Unicode
  case-fold with offset-true mapping; matches snap to grapheme
  clusters, never span wrapped rows) painted as a non-destructive
  style patch in selection tones with a distinct current match
  (BOLD+UNDERLINE); zero cost with an empty query. The row-local
  text↔cells mapping (`byte offset ↔ column`, both directions) is the
  shared substrate 0160 content selection consumes.
- `gfx::probe_dimensions` (0144): header-only PNG/JPEG dimension
  probing. `widgets::Image::from_path` widened from PNG-only to the
  unified magic-routed decoder (PNG + baseline JPEG).
- `examples/reader.rs`: the mdpad-class reader — tables, lazy images
  (incl. an honestly-missing one), TOC panel with anchor jumps, `/`
  search with live count and `n`/`N` navigation, theme cycling; joins
  the live pty smoke matrix (`live_reader`).

### Changed (integration — Feed adopts the doc vocabulary)

- `Feed` markdown items (static AND streaming) now typeset the full doc
  vocabulary: `FeedItem::markdown` parses through `md::parse_doc` and
  streaming items ride `md::DocStreamSession` (was `StreamSession`) —
  GFM tables, in-flow images (probe-sized at typeset, decoded lazily on
  first draw; feed items measure and window without decoding), task
  lists and `~~strikethrough~~` render inside feed transcripts. A
  streamed markdown TABLE renders as a table live: the in-flight table
  is the OPEN region (re-typeset per delta) until its first non-pipe
  line seals it — closed blocks stay frozen, and streamed-vs-static
  pixel parity plus the per-token typeset cost pins now cover the doc
  vocabulary. Core-only sources typeset identically through the doc
  fold (the pre-existing app-shot captures regenerate byte-identical).
  No API shape changes (`cargo semver-checks` clean vs 0.2.2).
- `examples/transcript.rs` gains a fourth scripted turn streaming a
  table + task list + strikethrough; `live_transcript` joins the pty
  smoke matrix (composer-path exit through the `/quit` completion).
- `examples/capture` apps family gains `reader-table` (doc vocabulary
  as a byte-deterministic in-process still).
- Fixed the residual rustdoc warning on `gfx::probe` (unresolved
  `decode_image` intra-doc link); `RUSTDOCFLAGS="-D warnings" cargo
  doc` and `cargo clippy --all-targets` are clean tree-wide.

### Added (content wave — app-widgets 0102/0104/0190)

- Rich feed lines (0102): `FeedItem::rich(RichText)` /
  `.rich_block(...)` / `FeedItem::rich_lines(Vec<RichLine>)` — multi-ink
  spans inside one feed line (severity-tinted log lines, chat headers)
  without a `FeedBlock::Custom` draw closure. Typeset through the SAME
  span-preserving wrap and row walk as every other block (parity with
  `RichTextView` is cell-exact, test-pinned); span styles stay patches
  (`fg: None` inherits the item ink per theme). The public `FeedBlock`
  enum is exhaustive in 0.2.x, so the new kind rides `FeedItem`
  constructors; the enum gains `#[non_exhaustive]` + a true `Rich`
  variant inside the 0.3 budget (planned/0002).
- `FeedState::sync` + `widgets::SyncSpec` (0104): the diffing bridge
  from a `Signal<Vec<T>>` source of truth to the keyed feed —
  key/fingerprint/render closures plus an optional visibility filter
  (one truth, no app-side mirror predicate). Appended tail keys take
  the O(1) push path, changed fingerprints update in place, and
  everything violating push order (shrink, reorder, mid-list insert or
  visibility flip) takes the documented rebuild path inside the
  engine. Pixel parity with a hand-pushed feed is test-pinned across
  reorder, mid-list update, burst append and full replace.
- Feed selection by key (the deferred 0100 item 6):
  `Feed::selected_key(Signal<Option<String>>)` highlights the selected
  item's row band in `selection_bg` (item inks stay), and
  `FeedState::row_of(key)` answers the scroll-to-key target for a
  wrapping `Scroll`.
- `widgets::TimeSeries` + `TimeSeriesState` (0190): the bounded history
  ring monitors hand-rolled until now — `push(t, v)` with cadence-slot
  quantization, drop-by-age (`new(cadence, window)`) or drop-by-count
  (`with_slots`) retention, NAN padding for missed slots so a sampling
  pause renders as a HOLE (the chart gap contract) instead of a
  time-compressed lie, and a reactive handle whose tracked
  `samples()`/`span()` reads re-render chart panels per push. Steady
  pushes never grow the ring (test-pinned).
- Chart time axes (0190): `LineChart::time_axis(span)` embeds relative
  time labels in the existing axis rule row ("now" anchored at the
  plot's right edge, nice ticks leftward — `-15s`, `-1m` — density
  adapting to width, deterministic); `Sparkline::time_axis(span)` adds
  an optional label row (one-row rects degrade to the bare trend).
  The dashboard example's traffic panel migrated onto the ring +
  time axis, deleting its hand-walked `(0..WINDOW)` sample vectors —
  dashboard captures regenerated.

### Added (scheduled deep gates — backlog 0180, closing leg)

- `.github/workflows/perf.yml`: weekly + dispatchable scheduled run of
  the release-mode perf suites, `fuzz_big` and the soak on a hosted
  runner — timing suites retry once to absorb runner load noise (the
  budgets are load-sensitive with 30–70 % quiet-host headroom), fuzz
  and soak never retry, and the printed measurements upload as a run
  artifact for week-over-week trend reading.
- Byte-emission RATCHETS in `tests/perf_app_surfaces.rs`: the suites'
  printed byte medians are now asserted against quiet-host baselines ×
  1.5 (feed token frame, select popup open/close, selection drag,
  composer keystroke, codeview scroll — measurement added there — and
  both feed-scroll guard phases). Byte counts are load-independent, so
  the ratchets assert in every profile; emission regressions can no
  longer hide behind "the host was busy".
- Cycle-2 cross-review probes (`tests/wave_c2_review.rs`): sync-rebuild
  × selection-key interaction, one-writer/NaN-fingerprint
  characterizations, `TimeSeries` cadence-boundary pins, streamed-vs-
  batch equivalence amplified over a hostile table corpus (CRLF,
  code-span pipes, alignment lookalikes), CJK/emoji search-cell pins,
  slug-dedup literal collisions, image cache invalidation on rewrite,
  the 0293 fidelity flip mid-hold, and the physical-fact rule vs the
  selection layer's key claims.

### Added (connection lifecycle — live-data 0040)

- `reactive::connection` + `ConnState` + `Connection` +
  `ConnectionEvents`: the engine-owned reconnect story. One dial fn
  supplied by the app (the engine does NO network I/O — transports
  stay 0050's decision); a `Signal<ConnState>` the UI renders honestly
  (`Connecting` / `Connected` / `Degraded(reason)` / `Reconnecting {
  attempt, next_in }` / `Closed`); retries armed on the existing timer
  heap (zero wakeups until due, zero cost forever once `Closed` —
  test-pinned); cancellation via `close()`, `retry_now()`, or scope
  death. Reports cross threads on the posted-jobs lane; a superseded
  attempt's late reports are inert and counted (`stale_reports` — the
  `dead_sends` convention), so a zombie worker can never flip the
  live attempt's state.
- `reactive::Backoff`: pure jittered exponential backoff — FULL jitter
  (uniform in `[0, min(cap, base × 2^n)]`), defaults base 500 ms / ×2 /
  cap 30 s (the agora client's parameters), `reset()` on success,
  `seeded(n)` for deterministic tests. Ends the un-jittered
  thundering-herd hand-roll the first consumer shipped
  (`reviews/study2/field-consumer-tensions.md` §3.5).
- docs: `docs/live-data.md` § "Connection lifecycle" (state diagram +
  worker-thread example + the full-jitter rationale); `docs/api.md`
  § `reactive::connection`.

### Fixed (disposal-safety law engine-wide — first-app 0297)

- The 0250 "bookkeeping-before-callbacks" ruling is now the LAW on
  every widget callback, not a List/Table accident: a user callback
  may dispose its widget's scope synchronously (the modal
  approve/deny close), so consumers can delete their one-tick retire
  deferrals. Two offenders found and fixed: `Button`'s mouse-Up arm
  wrote `pressed` AFTER `on_click` (the confirmed 0297 filing), and
  `TextArea`'s handler re-published the caret cell AFTER
  `on_change`/`on_submit` (found by this audit — a submit-and-close
  composer panicked on the dead caret signal). Callbacks now run
  strictly LAST; one knowable consequence documented in
  `docs/api.md` § "The widget disposal-safety law" (a callback
  mutating the widget's own state sees it rendered next event).
- Disposal pinned per site: Button (both arms), Checkbox, RadioGroup,
  Tabs, TextInput (`on_change`/`on_submit`), TextArea
  (`on_change`/`on_submit`), Table `on_sort_requested` (its
  `on_select` was pinned by 0250), and the Select commit path (the
  popup follows its owner's scope down — the AnchorGone cascade,
  pinned because it hangs on more than ordering).

### Fixed (wave-3 cycle-3 close — cycle-2 review demands, CLOSER)

- `FeedState::sync` one-writer violations SELF-HEAL (review C-1, P2):
  every item mutation bumps a feed-internal counter; a drain that
  finds the counter moved past the bridge's own record takes the
  rebuild path — a stray manual `push` onto a synced feed is evicted
  at the very next drain and feed order is restored to source order
  (violations used to be silently PERMANENT: strays survived every
  fast-path drain, and a manually-pushed key the source appended later
  replaced in place at the old index, diverging order forever). One
  u64 compare per drain; self-heal semantics documented in the `sync`
  rustdoc.
- `SyncSpec` fingerprint docs (review C-2/C-3): float fingerprints
  must compare by bits (`to_bits` — IEEE `NaN != NaN` re-renders the
  item every drain, pixels correct, cost silent), and the
  rebuild-storm cost is named where consumers read (a source that
  reorders every drain rebuilds every drain: O(visible) renders — sync
  a stable order and sort at render). Doc-only by judgment: no
  engine-side fix exists for a user-supplied `PartialEq` short of
  rejecting float fingerprints wholesale.
- `TimeSeries` pause padding is UNIFORM (review C-4, boundary fixed +
  C-6): the `missed >= capacity` restart is deleted — it contradicted
  the module's own gap claim by exactly one slot and collapsed the
  display to a lone zero-span dot (an x-axis compression, the thing
  the gap contract exists to avoid). NAN padding is now capped at
  `capacity - 1` slots per push (same bounded work), so a pause of ANY
  length ≥ the window shows a full window of hole ending in the fresh
  sample; the cap applies in u64 space before the usize cast, closing
  C-6's 32-bit wrap. Tests updated deliberately (restart pin →
  uniform-padding pin, both sides).
- Markdown image cache identity gains the platform file id (review
  R-3 — the known mtime-alone class): unix folds `dev + ino` into the
  probe/decode cache signature (a write-tmp-then-rename rewrite mints
  a new inode), windows folds `creation_time` (std's file-index
  accessors are unstable; in-place same-size same-mtime overwrites
  stay undetected there — documented degradation). Same-length
  rewrites under 1s-mtime filesystems / `rsync -a` / `tar` no longer
  serve stale pixels; pinned by a rename-rewrite test with the mtime
  set back (`File::set_modified`) so only the inode discriminates.
- `Driver::suspend(app, term)` — the job-control suspend seam (review
  I-2, P2): key-state hygiene BEFORE the stop (`keys::on_suspend`
  drains held keys into synthesized releases and flags the frame —
  releases during a stop are unobservable and Ctrl+Z keeps focus, so
  no FocusLost ever covers it; the stuck-hold/stuck-mic class), then
  `Terminal::suspend`, then the resume re-sync (size re-query +
  prev-poison + presenter invalidate + damage-all + image re-place —
  both halves of "the screen is unknown"). New:
  `KeyState::suspend_cleared()` (sealed per turn like focus),
  `StopReason::Suspended` (PushToTalk stops in EVERY mode before the
  stop signal, latch included, and never auto-restarts on resume),
  `CaptureTerm` models the suspend round trip (`suspend_count()`).
  The keys module doc names suspend beside focus loss.
- Backlog id collisions renumbered (review handoff item): the
  first-app full-redraw filing 0300 → **0299** (collided with
  control-plane/0300, outside first-app's 0220–0299 band) and the
  mid-close textarea-placeholder filing 0310 → **0291** (same class,
  collided with control-plane/0310); README + overview rows updated,
  renumber notes in both files (the 0292/0294 precedent).

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

[Unreleased]: https://github.com/lpalbou/abstracttui/compare/v0.2.17...HEAD
[0.2.24]: https://github.com/lpalbou/abstracttui/compare/v0.2.23...v0.2.24
[0.2.23]: https://github.com/lpalbou/abstracttui/compare/v0.2.22...v0.2.23
[0.2.22]: https://github.com/lpalbou/abstracttui/compare/v0.2.21...v0.2.22
[0.2.21]: https://github.com/lpalbou/abstracttui/compare/v0.2.20...v0.2.21
[0.2.20]: https://github.com/lpalbou/abstracttui/compare/v0.2.19...v0.2.20
[0.2.19]: https://github.com/lpalbou/abstracttui/compare/v0.2.18...v0.2.19
[0.2.18]: https://github.com/lpalbou/abstracttui/compare/v0.2.17...v0.2.18
[0.2.17]: https://github.com/lpalbou/abstracttui/compare/v0.2.16...v0.2.17
[0.2.16]: https://github.com/lpalbou/abstracttui/compare/v0.2.15...v0.2.16
[0.2.15]: https://github.com/lpalbou/abstracttui/compare/v0.2.14...v0.2.15
[0.2.14]: https://github.com/lpalbou/abstracttui/compare/v0.2.13...v0.2.14
[0.2.13]: https://github.com/lpalbou/abstracttui/compare/v0.2.12...v0.2.13
[0.2.12]: https://github.com/lpalbou/abstracttui/compare/v0.2.11...v0.2.12
[0.2.11]: https://github.com/lpalbou/abstracttui/compare/v0.2.10...v0.2.11
[0.2.10]: https://github.com/lpalbou/abstracttui/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/lpalbou/abstracttui/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/lpalbou/abstracttui/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/lpalbou/abstracttui/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/lpalbou/abstracttui/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/lpalbou/abstracttui/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/lpalbou/abstracttui/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/lpalbou/abstracttui/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/lpalbou/abstracttui/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/lpalbou/abstracttui/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/lpalbou/abstracttui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lpalbou/abstracttui/releases/tag/v0.1.0
