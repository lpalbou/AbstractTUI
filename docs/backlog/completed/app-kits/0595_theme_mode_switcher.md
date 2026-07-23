# 0595 — Theme modes + `ThemeSwitcher`: the drop-in theme control

## Metadata
- Created: 2026-07-25
- Status: Completed (same wave; filed directly in completed/ — the
  0535 precedent)
- Track: app-kits (the chrome layer over theme/0500 machinery)
- Completed: 2026-07-25
- Trigger: operator order — "without creating a dependency to
  abstractuic, create a central set of themes available to all
  applications … clearly organized by dark and light themes … surface
  an easy way to change themes in any app. Possibly even create a nice
  memorable icon button to change that theme."

## ADR status
- Governing ADRs: ADR-0001 (API stability — everything here is
  ADDITIVE vs 0.2.20, `cargo semver-checks` clean). The one internal
  visibility change (`select.rs`'s `mod core` → `pub(crate) mod core`)
  exposes nothing publicly.

## Context

The engine already owned the central registry (26 themes, 36 tokens,
contrast-floor + decisive-ground invariants test-pinned) and one-signal
runtime switching; what was missing was (a) polarity as a first-class
vocabulary (apps hand-rolled `t.dark` filters), (b) a drop-in chrome
control (every app re-derived the themes-example gallery), and (c) the
one-click dark↔light flip.

## Survey findings (abstractuic, read-only)

- `THEME_SPECS` (ui-kit/src/theme.ts) lists **21** themes (15 dark +
  6 light; the 2026-07-12 note's "22" no longer matches); `theme.css`
  carries the same 21 blocks. The engine's 26-theme registry is a
  **strict superset** (21 ports + 5 originals).
- Value-level diff of all 21 × 12 seeded fields against today's
  `theme.css`: **zero drift**. Nothing to port, nothing upstream
  improves on us — recorded as a finding, not an invention prompt.
- Consequence honored: **no theme values changed** this wave (apps pin
  theme ids; cosmetic churn restyles shipped apps). The lone soft spot
  (everforest-light's 4.25:1 text/raised) remains a documented,
  faithful-port audit exception — both values verbatim upstream.

## Design rulings

- **`ThemeMode` is closed** (Dark/Light, no `#[non_exhaustive]`): the
  decisive-ground invariant (`|L(bg) − 0.5| ≥ 0.15`) makes a third
  value structurally unrepresentable. `Theme::mode()` derives from the
  audited `dark` flag — ONE source; never a second luminance threshold.
- **`themes_by_mode` order = registry order filtered** (curated,
  house-first), registrations trailing — one ordering convention with
  `list()`, not a second alphabetical one; "first of mode is the house
  palette" is documented and pinned because the toggle's cold-start
  default rides it.
- **Toggle semantics = remembered choice, not similarity folklore**:
  `toggle_mode()` restores the target mode's last-used theme, recorded
  by `set_theme` itself (the single signal-write choke point, so every
  switch path feeds the memory); cold start = first-of-mode. A
  "closest theme" heuristic (family-name suffix parsing,
  longest-common-prefix) was rejected: `abstract-dark` LCP-pairs with
  `abstract-dawn`, not `abstract-light` — folklore in the engine.
- **One type, two constructors** (`ThemeSwitcher::new()` /
  `::toggle()`) over a boolean flag or a second widget: one chrome,
  one glyph vocabulary, intent readable at the call site.
- **Glyph = current mode, `☾`/`☼`**: a static `◐` spends the cell on
  decoration; the mode-reflecting glyph doubles as the app's polarity
  indicator while the a11y label carries the action. `☾` U+263E and
  `☼` U+263C are East-Asian-NEUTRAL (single-width in every convention
  — safer than `◐`, which is Ambiguous) and absent from emoji-data,
  unlike `☀` U+2600 (emoji flag; some stacks render it double-width).
- **Composition over hand-rolling**: the popup reuses the select core
  (movement skips disabled rows → group headers for free; type-ahead;
  `option_rows_view`) plus the OWNED anchored popup substrate (modal
  stacking above everything live, SCREEN anchors, Escape/outside/
  resize/anchor-gone dismissal). The full `Select` face could not fit
  (its trigger is a framed row; its popup is `MatchAnchor`-width — a
  1-cell anchor would collapse it), so the seam is the core, made
  `pub(crate)`.
- **Live preview** on highlight movement (the `commit_on_move`
  semantic, documented as designed for theme pickers): Enter/click
  commit, Escape restores pre-open, outside press keeps the preview —
  the select-family contract verbatim. The menu re-resolves its own
  tokens per step (an outer theme-tracking `dyn_view`), so the list is
  rendered in the theme being previewed.
- **Short-viewport window cap**: the popup's visible-row window is
  capped at open by the longer side of the anchor, so the
  highlight-follow window always fits the solved rect (Select's own
  fixed `max_visible` can window the highlight out of a clamped rect —
  named here, not fixed there).
- **`on_change` fires when a switch STICKS** (commit / outside-press
  with a changed theme; every toggle flip) — never on preview steps,
  Escape-restores, or mechanical endings (resize, opener unmount).

## What shipped

- `src/theme/mode.rs` (+ mod wiring): `ThemeMode`, `Theme::mode()`,
  `themes_by_mode()`, tests.
- `src/app/theme.rs`: per-mode last-used memory in `set_theme` +
  `toggle_mode()`, tests.
- `src/app/theme_switcher.rs` + `theme_switcher_tests.rs`: the widget,
  both faces; `select.rs`'s core module widened to `pub(crate)`.
- Exports: `app::{toggle_mode, ThemeSwitcher}`,
  `theme::{ThemeMode, themes_by_mode}`, all four in the prelude.
- Demos: `examples/themes.rs` (toolbar: both faces),
  `examples/shell.rs` (footer switcher).
- Docs: theming.md "Theme modes & the switcher", api.md
  "app::ThemeSwitcher", CHANGELOG `[Unreleased]`.

## Validation

- Mode: partition exactness + flag agreement over all built-ins, spot
  pins (tokyo-night/observer-night/midnight Dark; dawn/latte/one-light/
  everforest-light/solarized-light/paper Light), house-first ordering
  + registry-order subsequence, registration mode-grouping.
- Toggle: cold-start house default, choice round-trip (nord ↔ light),
  cross-path memory (set_theme_by_id feeds it).
- Switcher, through the REAL `Driver` + `CaptureTerm` (the wave-11
  retint + anchored-layer P1 precedents): grouped open below trigger
  with headers + ● mark; movement live-previews ON SCREEN BYTES and
  Enter commits; Escape restores; committing a light theme flips the
  glyph; type-ahead prefix jump + repeated-letter cycling (fresh-open
  discipline documents the 900 ms window vs test key cadence);
  popup-inside-Modal anchors at the SCREEN cell; 38×6 viewport clamps
  the window and still commits; outside-press keeps preview and
  reports once (Escape and no-change commits stay silent); toggle face
  flips + remembers + never opens; 10-flip spam round-trips; zero idle
  closed and after open/close; a11y roles/labels/value for both faces
  and the popup menu.

## Progress checklist
- [x] `ThemeMode` + `Theme::mode()` + `themes_by_mode` (+ tests)
- [x] `toggle_mode()` + set_theme memory (+ tests)
- [x] `ThemeSwitcher` menu + toggle faces over select core + Popup
- [x] Demos wired (themes toolbar, shell footer)
- [x] Docs (theming.md, api.md) + CHANGELOG + this item
- [x] Gates: workspace tests, clippy, fmt, semver-checks vs 0.2.20,
      examples headless

## Completion report

- Final path: `docs/backlog/completed/app-kits/0595_theme_mode_switcher.md`
- Date: 2026-07-25
- Outcome: polarity vocabulary + remembered-choice toggle + the
  one-line theme control, all additive; zero theme-value churn (the
  survey proved the registry already strictly-supersets abstractuic
  with zero drift).
- Follow-ups revealed (named, not built):
  1. **Select's own short-viewport window**: the fixed `max_visible`
     window can place the highlight outside a viewport-clamped popup
     rect; the switcher caps its window at open — the same cap could
     move into the select faces.
  2. **`themes-table.md` regeneration**: the generated token reference
     predates this wave; no values changed, so it stays accurate — but
     a mode column would be a natural addition next regen.
  3. **Upstream name coordination**: if abstractuic ever adds a theme
     the engine lacks, port under the identical id (the 2026-07-12
     house rule) — the survey script pattern in this item's report is
     the diff to re-run.
