# 0545 — PageHost: the global tab / page host (full pages behind a themed tab bar)

## Metadata
- Created: 2026-07-24
- Status: Completed 2026-07-24 (full scope; see the completion report
  at the end)
- Track: app-kits
- Completed: 2026-07-24
- Depends on: nothing new — the engine ships every ingredient
  (`dyn_view_scoped` per-generation scopes, `Element::shortcut`
  normalized-chord resolution, Capture-phase interception, the 0297
  disposal-safety law, `text::truncate_ellipsis`, theme tokens,
  `Role::Tabs`). Engine deltas: NONE (pure widget composition).
- Validator: `examples/shell.rs` (the app-shell demo, co-owned with the
  drawer item — headless exit-0) + `tests/wave_page_host.rs` (real
  Driver/CaptureTerm).
- Relationship to 0550 (navigation kit): DISTINCT, cross-referenced.
  0550's `FilterTabs` is a count-bearing strip WITHOUT panels — it
  filters ONE surface and must never mount/dispose content. THIS item
  is the page-level host: N full complex pages, exactly one mounted.
  0550 already records the split ("Tabs keeps the panel-switching job
  it does well — FilterTabs is a sibling"); PageHost supersedes Tabs
  for app-shell page switching while Tabs remains the small in-content
  strip. The bar-overflow behavior here follows 0550's ruling for
  strips (window the strip, keep the active tab visible).
- Promotion trigger: filed at build start; moves to completed/ with
  the completion report when the gates pass.

## ADR status
- Governing ADRs: none in this repo (no ADR system yet, see 0170).
  Stays inside the cycle-7 router ruling (`src/ui/compose.rs:136-155`):
  navigation state IS a signal; PageHost renders and mutates it, it
  never owns routing — no history, no deep links.

## Context — the maintainer's brief (verbatim intent)
"the gateway console-tui navigates with next/previous page — it works,
but not super intuitive nor visual. I believe we need a global tab
system... applications could leverage it... these higher-level
containers should be able to contain full complex pages."

## Current code reality (survey)
- **`Tabs` is a small strip, not a page host**
  (`src/widgets/tabs.rs`): titles are `Vec<String>` with no ids
  (tabs.rs:26-32), no count/badge slot, no truncation and no overflow
  (the span walk at tabs.rs:181-209 runs off the rect — 0550 recorded
  the same gap), and Left/Right only work while the BAR ITSELF is
  focused (tabs.rs:119-127). What it does prove: lazy panel mounting
  through one `dyn_view` (tabs.rs:214-221), the §3.3 token law (active
  `text`+BOLD, idle `text_muted`, `border_focus` cell-strip underline,
  tabs.rs:76-84), and the 0297 write-before-callback discipline
  (tabs.rs:283-304).
- **The console-tui hand-rolls everything this item ships**
  (`abstractgateway/console-tui/src/ui/mod.rs`, read-only evidence):
  `screen: Signal<usize>` + a span-drawn `screen_bar` whose CLICK
  HIT-TEST must mirror the draw arithmetic by hand (mod.rs:694-698 —
  "the hit-test mirrors the draw arithmetic above"), a
  `dyn_view_scoped` body switch (mod.rs:732-743), root-element digit
  shortcuts (mod.rs:409-425), `]`/`[` + Ctrl+N/P chords with the live
  finding that plain-char chords DIE while a text input has focus
  (mod.rs:373-375), and footer hints re-stating the vocabulary
  (mod.rs:786-789). A PageHost deletes the bar drawing, the mirrored
  hit-test, the digit-shortcut loop and the prev/next plumbing.
- **Chord-eating hazard found during survey**: `Scroll`, `List`,
  `Table` match `Key::PageUp/PageDown` MODIFIER-BLIND
  (scroll.rs:397-406, list.rs:331-340, table.rs:208-217), and the
  documented resolution order runs handlers before shortcuts
  (tree.rs:605-611) — so Ctrl+PgUp/PgDn registered as ordinary
  shortcuts would be eaten by any focused scrollable inside a page.
  Container chords must be CAPTURE-phase (the modal-swallow precedent,
  event.rs Phase docs) to be reliable.
- **Focus dies with a disposed page** (`src/ui/mount.rs:292-302`):
  unmounting the focused node drops tree focus to None, and key
  dispatch then targets the TREE root (tree.rs:641-644) — off the
  host's routing path: the 0230 dead-keys class. The repair precedent
  is on the record (0515 completion report, decision 5): a
  Capture-phase handler records the anchor's ViewId; programmatic
  focus needs no focusability (focus.rs:250-270, clause 3).
- **Wire reality** (`src/input/editor_matrix_tests.rs:95`): legacy
  Ctrl+PgDn = `\x1b[6;5~` — the matrix even labels it "tab switch";
  kitty keypad twins map (kitty.rs:82-83). Letter chords have two wire
  spellings folded by `KeyChord::normalized`/`KeyEvent::means_char`
  (event.rs:85-138); the shortcut table compares normalized
  (tree.rs:684-713).

## Specification (v1)

### Model
- `PageHost::new().page(id, title, |cx| view)` — N pages; each page
  builder receives the GENERATION scope (`dyn_view_scoped`): built on
  activation, disposed on switch. `.badge(id, || Option<String>)`
  attaches a reactive badge/count getter (read tracked inside the bar
  region — a count change repaints the BAR only).
- Controlled: `.active(Signal<String>)` (app-owned, id-valued;
  external writes switch pages, `on_change` does NOT fire).
  Uncontrolled: internal signal, `.initial(id)` picks the start page.
  Unknown/stale ids fold to the FIRST page (documented, tested).
- `.on_change(|id: &str|)` fires on host-driven switches AFTER the
  active write (0297: the callback may dispose the host's scope).

### The keep-alive decision (recorded)
Inactive pages are UNMOUNTED. No keep-alive option ships: a
hidden-but-mounted page keeps its scope alive — its `interval` timers
tick, its sources ingest, its effects run — which violates the
zero-idle law for invisible content. The honest model is the engine's
own state discipline (compose.rs store pattern + the console-tui
`UiState` precedent): durable page state lives in app-owned signals
created OUTSIDE the page builders; the builder re-reads them on
remount. The recipe is documented on the widget and demoed in
`examples/shell.rs`.

### The tab bar
- Two rows (Tabs parity): titles+badges, then the `border_focus`
  cell-strip under the active tab. Tokens only: active `text`+BOLD,
  idle `text_muted`, badges `info`, indicators `text_muted`, ground
  `surface` (§3.3).
- ONE pure function (`plan_bar`) computes segment geometry; the draw
  closure AND the click hit-test both consume it — the console-tui
  mirror-drift class is dead by construction.
- Overflow: the strip WINDOWS (0550's strip ruling): the active tab is
  always visible, `‹`/`›` indicators mark hidden tabs and are click
  targets for prev/next; the window start is sticky (moves only when
  the active tab would leave it). A single tab wider than the bar
  truncates via `text::truncate_ellipsis`.
- A11y: the bar is one tab stop, `Role::Tabs`, `access_value` =
  "title (i/N)" plus the badge when present (existing roles only —
  the Role enum is sealed until 0.3).

### Navigation
- Click a tab (click-to-focus lands on the bar); Left/Right cycle
  while the bar is focused (wrap — cycling gesture, tmux precedent).
- CHORDS (default Ctrl+PgUp / Ctrl+PgDn, `.chords(prev, next)`
  replaces): intercepted at CAPTURE phase on the host root —
  container-reserved vocabulary, so a focused Scroll/List/Table
  cannot eat them (survey finding); plain PgUp/PgDn stay with the
  content. Matching compares NORMALIZED chords (both wire spellings).
- Number jump 1-9: OPT-IN (`.number_jump(true)`), registered at the
  SHORTCUT layer (never capture) — a focused TextInput keeps typing
  digits; apps own their number keys unless they opt in.
- Focus anchoring: a chord/digit switch re-anchors focus on the host
  root (Capture-recorded ViewId + `request_focus`; programmatic focus
  needs no focusability) so the next chord is never dead after the
  focused node died with the old page. Click keeps bar focus;
  bar-arrows keep bar focus.

### Console-tui fit (migration sketch — they adopt at their pace)
Their five browse screens (Connection/Providers/Routes/
Users & Entities/Runtimes/Review) become `.page(id, title, builder)`
calls with builders `|gcx| connection::view(gcx, &ctx, &t)` etc.;
`UiState.screen: Signal<usize>` becomes a `Signal<String>` handed to
`.active(..)` (wizard-mode gating keeps writing the SAME signal — the
gate logic stays app-side, exactly the router ruling); `screen_bar`
(~60 lines incl. the mirrored hit-test), the digit-shortcut loop and
the `]`/`[`+Ctrl+N/P plumbing are DELETED (digit jumps come back as
`.number_jump(true)`; wizard-refusal notices stay app-side by keeping
wizard mode on a plain signal and gating in an `on_change`/effect).
Zero behavioral loss; the wizard chrome renders above the host
unchanged.

## Scope / Non-goals
Scope: PageHost (ids, badges, controlled/uncontrolled, chords, digit
jumps, windowed bar, a11y), the state-ownership recipe, the shell
demo, unit + wave tests.
Non-goals: routing/history/deep-links (router ruling); keep-alive
(recorded above); closable/reorderable tabs (terminal app shells are
static-page shells; additive later); vertical tab rails (0550's
NavList is that surface); Tabs deprecation (it keeps the small
in-content job).

## Validation plan
Unit (`src/widgets/page_host_tests.rs`, real UiTree + dispatch):
mount/unmount on switch (build counters), state-via-signals recipe
survives switches, controlled vs uncontrolled, unknown-id fold,
on_change order + disposal safety (0297), plan_bar windowing/
truncation/stickiness, chord nav incl. BOTH wire spellings of a
letter chord, digit opt-in vs focused-input shielding, capture
priority over a focused Scroll, focus re-anchor after chord switch,
a11y value, empty host.
Wave (`tests/wave_page_host.rs`, real Driver/CaptureTerm): click +
chords through real bytes (legacy `\x1b[6;5~`, kitty CSI-u letter
spelling), damage containment on switch (replayed switch-frame bytes
leave outside chrome untouched), zero idle with a parked host (idle
turns: no render, no bytes), a full-page Feed inside a page scrolls
normally, `unknown_seq_count == 0` everywhere.

## Progress checklist
- [x] plan_bar (segments, windowing, truncation, indicators)
- [x] PageHost builder + element assembly (bar + page region)
- [x] Chords (capture) + digit jumps (shortcut, opt-in) + re-anchor
- [x] Controlled/uncontrolled + on_change (0297)
- [x] A11y value + tokens audit (RT1-9b lint list)
- [x] examples/shell.rs (3 pages, DRAWER seam marked)
- [x] Unit + wave suites; api.md; CHANGELOG

## Completion report (2026-07-24, TABS — wave 8)

**Feasibility verdict**: fully feasible, zero engine deltas. Page
switching IS the charter's own router pattern (signal +
`dyn_view_scoped`, compose.rs:136-155); the bar is a themed strip; the
one genuine conflict found — scrollable widgets consume
PageUp/PageDown modifier-blind — is solved by making host chords
Capture-phase container vocabulary (the modal-swallow precedent), not
by touching the scrollables.

**Shipped** (all semver-ADDITIVE vs published 0.2.11;
`cargo semver-checks check-release --baseline-version 0.2.11`:
196 pass, "no semver update required"):

- `src/widgets/page_host.rs` (524) — the builder
  (`page`/`badge`/`active`/`initial`/`on_change`/`chords`/
  `number_jump`/`layout`/`view`/`element`), element assembly (bar +
  page region), Capture chord interceptor + labeled shortcut twins,
  digit jumps, the focus anchor. Exports appended to `widgets::mod`
  (+ the RT1-9b lint list) and the prelude.
- `src/widgets/page_host_bar.rs` (291) — ONE pure geometry plan
  (`plan_bar`) consumed by BOTH the draw closure and the click
  hit-test (`hit_bar`), plus `draw_bar` (tokens only).
- `src/widgets/page_host_tests.rs` (578) +
  `page_host_bar_tests.rs` (77) — 20 unit tests.
- `tests/wave_page_host.rs` (265) — 5 acceptance tests through the
  real Driver/CaptureTerm wire.
- `examples/shell.rs` — the co-owned app-shell demo (3 full pages,
  live badge, DRAWER regions marked; DRAWER's drawers landed inside
  the marked seams the same day — the append-shaped structure held).
- docs/api.md `## widgets::PageHost`; CHANGELOG under
  `## [Unreleased]`; examples/README row + section.

**Design decisions on the record**:
1. **Chords are container-reserved at Capture phase** — survey found
   Scroll/List/Table match PageUp/PageDown modifier-blind
   (scroll.rs:397-406, list.rs:331-340, table.rs:208-217) and
   handlers outrank shortcuts (tree.rs:605-611), so bubble-layer
   chords would go dead over any focused scrollable. Plain PgUp/PgDn
   stay content-side (test-pinned both ways). Labeled shortcut twins
   keep keymap-help truthful and act as defense in depth.
2. **No keep-alive** — a hidden-but-mounted page's timers tick
   against the zero-idle law. Unmount + app-owned signals is the
   recipe (documented on the widget, demoed, test-pinned).
3. **Focus re-anchor after chord/digit switches** — `remove_subtree`
   drops focus to None when the focused node dies with a page
   (mount.rs:292-302) and the next key would target the tree root,
   off the host's path (the 0230 class). A Capture-phase recorder
   notes the host's ViewId (registration order before the
   interceptor); switches `request_focus` it (programmatic focus
   needs no focusability — focus_init clause 3; the 0515 anchor
   precedent). Click keeps bar focus (engine click-to-focus).
4. **One geometry source** — the console-tui's draw/hit-test mirror
   drift class is dead by construction: `plan_bar` is pure and
   shared; the sticky window anchor is render bookkeeping (a Cell),
   not reactive state.
5. **Wrap on prev/next** (cycling gesture, tmux precedent) — noted
   divergence from Tabs' clamping bar arrows; consistent inside the
   host (bar arrows, chords, indicators all wrap).
6. **Theme reactivity by build path** — `view(cx)` resolves tokens
   inside the bar's dyn (tracked context read: the bar retints on
   theme switch without remounting the active page); `element(cx, t)`
   captures the explicit tokens (test/custom-theming path, caller
   owns retint).
7. **Unknown ids fold to the first page** (controlled signals may
   transiently hold stale ids); unknown `.initial`/`.badge` ids are
   builder-time mistakes and debug_assert.

**Validation** (gate numbers at completion: whole tree
`cargo test` 1884 passed / 0 failed; lib suite 1353; clippy zero on
lib + my targets (whole-tree clean once DRAWER's in-flight edits
settled); fmt clean; alloc pins 10/10 incl. the idle-honesty pins;
`examples/shell` headless exit-0; semver-checks 196 pass):
- Unit (`widgets::page_host::tests`, 20):
  `pages_mount_lazily_and_dispose_on_switch`,
  `state_outside_builders_survives_switches`,
  `controlled_signal_drives_pages_and_external_writes_skip_on_change`,
  `uncontrolled_initial_is_honored_and_unknown_controlled_id_folds_to_first`,
  `on_change_may_dispose_the_host_scope` (0297),
  `click_selects_tabs_and_bar_arrows_cycle_with_wrap`,
  `ctrl_chords_beat_a_focused_scroll_and_plain_paging_stays_content_side`,
  `chords_stay_alive_after_the_focused_node_died_with_its_page`,
  `letter_chords_fire_on_both_wire_spellings`,
  `number_jump_is_opt_in_and_yields_to_a_focused_input`,
  `badges_render_reactively_without_remounting_the_page`,
  `a11y_bar_reports_tabs_role_position_and_badge`,
  `switch_damage_stays_inside_the_host_region`,
  `oversized_titles_truncate_with_an_ellipsis`,
  `overflow_indicators_page_by_click_and_mark_hidden_sides`,
  `empty_host_renders_and_ignores_navigation`, and the plan_bar
  geometry pins (`plan_bar_lays_everything_out_when_it_fits`,
  `plan_bar_windows_around_the_active_tab_with_a_sticky_start`,
  `plan_bar_clamps_a_single_oversized_tab_into_the_budget`,
  `plan_bar_handles_empty_and_degenerate_widths`).
- Wave (`tests/wave_page_host.rs`, real Driver/CaptureTerm, 5, all
  with `unknown_seq_count == 0`):
  `pages_switch_by_click_chords_and_digits_through_the_wire`
  (legacy `CSI 6;5~`/`CSI 5;5~` + SGR click + digit),
  `letter_chords_fire_on_both_wire_spellings_through_the_wire`
  (legacy byte vs kitty `CSI 108;2u`),
  `switch_frame_bytes_stay_inside_the_host_region` (the replayed
  switch-frame bytes leave the header row blank — and showed the
  diff's cell economy: only the changed "BETA" cells re-emit),
  `parked_host_idles_at_zero`,
  `full_page_feed_scrolls_inside_a_page_and_chords_still_switch`.

**Follow-ups revealed** (none blocking):
1. The main tree is not focus-initialized by the engine — a host
   mounted under a wrapper has dead chords until focus enters
   (documented; `examples/shell.rs` calls `tree().focus_first()`).
   An engine-level `focus_init` for the MAIN tree at mount is a
   design question for the 0230 lane, not this widget's.
2. Scroll/List/Table's modifier-blind PageUp/PageDown matching
   stands (the capture design routes around it) — an engine-wide
   "consume only the modifier combinations you implement" sweep
   would let future containers use bubble-layer chords.
3. Closable/reorderable tabs and a vertical rail stay out (0550's
   NavList is the rail); additive later if a consumer names them.
4. Theme switches while built via `element(cx, t)` keep the at-build
   palette (the select-family posture); `view(cx)` retints the bar
   live.
