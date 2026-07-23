# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
