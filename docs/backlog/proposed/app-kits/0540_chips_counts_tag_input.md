# 0540 — Chip & count vocabulary: interactive chips, count badges, tag input

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: app-kits
- Completed: N/A
- Depends on: nothing in-band (extends the shipped `Badge` family;
  0500's Combobox popup is the OPTIONAL suggest surface for TagInput).
- Validator (0590): `examples/triage_shell` (unread badges, TagInput
  on the notes panel) + `examples/admin_console` (state chips, account
  chip).
- Promotion trigger: 0500's MultiSelect (chips are its closed-trigger
  rendering), 0550's unread counts, or the smart-note validator's tag
  surfaces — whichever consumer lands first.

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none (additive widgets extending `Badge`'s family).

## Context
The study brief's four reference UIs share one small-vocabulary need
that appears in every corner: the admin console's state badges per row
and its header ACCOUNT CHIP (A), the chat sidebar's UNREAD COUNT badges
and filter-tab counts ("All / Unread / Asks / …" each with a number)
(C), the smart-note app's TAG CHIPS on notes plus a tag-entry control
(D), and 0500's MultiSelect rendering its picked set as removable
chips. These are one family: a small, tinted, optionally-counted,
optionally-interactive capsule. The engine ships exactly the static
third of it.

## Current code reality
- **`Badge` is static and tone-only**: label + `Tone` on
  `surface_raised`, padded one cell, `shrink(0.0)` so it never
  vanishes under overflow (`src/widgets/badge.rs:32-87`). No count
  variant, no dot form, no interactivity (no `on_click`, not
  focusable), no remove affordance. That is the right primitive to
  extend, not replace — its tone→token mapping (badge.rs:57-66) and the
  audited tone/ground floors are the family's color law.
- **Counts are hand-formatted today**: nothing renders `9+`/`999+`
  clamping or a zero-hides rule; the dashboard formats strings into
  `List` rows (`examples/dashboard/main.rs:342-362` builds nav labels
  by string concat — the seed of the problem: a count is content, not
  label text).
- **No chip container**: wrapping a set of chips needs layout wrap,
  which the solver has (`wrap()`, `cross_gap` — docs/api.md:104-108);
  what is missing is the component that maps `Signal<Vec<Tag>>` to a
  wrapped, keyboard-navigable chip row.
- **No tag-entry control**: `TextInput` (`src/widgets/input.rs:35-41`)
  has no chip-prefix mode; a tag input = chips + an inline editor +
  Enter-commits + Backspace-at-start-removes-last — none exists.
- **Interactivity precedents**: `Button`'s press/release-inside
  contract and pointer capture (`src/widgets/button.rs:29-33`),
  `Checkbox`'s Space toggle (`src/widgets/checkbox.rs:3-4`) — chips
  reuse these contracts, not new ones.
- **Tab titles cannot carry counts**: `Tabs.titles: Vec<String>`
  (`src/widgets/tabs.rs:26-32`) — 0550 owns the filter-tab surface and
  consumes this item's count formatting; recorded here so the two items
  don't both invent it.

## Problem
Counts, tags, and interactive chips are re-hand-rolled as formatted
strings, which breaks the moment they need truncation honesty, tones,
click/remove behavior, or layout wrap — and every hand-roll invents its
own `99+` rule. 0500/0550/0560 would each grow a private chip.

## What we want
Small, sharp additions to the Badge family (one file discipline, one
color law):
1. **`Badge` count form**: `Badge::count(n)` + `.max(99)` → `99+`
   clamping; `.hide_zero(true)` renders nothing at 0 (the unread rule)
   — the formatting lives in the widget, tested once. A **dot** form
   (`Badge::dot(tone)`, one cell) for presence/attention without a
   number.
2. **`Chip`**: an interactive capsule — label + optional tone dot +
   optional remove glyph (`✕`); focusable, Enter/Space activates (a
   chip-as-filter is a PURE toggle, where Enter and Space legitimately
   coincide — the 0250 ruling §2, the shipped Checkbox contract);
   remove glyph is its own hit-zone firing `on_remove`; Backspace/
   Delete while focused also removes (keyboard parity). Disabled state
   per the docs/theming.md:284 rule. Selected/toggled state (a chip can
   act as a filter toggle) rendered with the selection pair.
3. **`ChipGroup`**: maps a `Signal<Vec<ChipData>>` to a wrap-enabled
   row of Chips (solver `wrap()`/`cross_gap`); one tab stop, Left/Right
   move between chips (the RadioGroup one-stop pattern,
   `src/widgets/radio.rs:3-6`); overflow beyond `max_visible` degrades
   to a `+N` chip (honest, clickable → `on_overflow` so the app can
   open a popup with the rest).
4. **`TagInput`**: ChipGroup + an inline `TextInput` tail; Enter
   commits the buffer as a chip (via an app `on_add` that may validate/
   canonicalize), Backspace on an empty buffer removes the last chip;
   compose with 0500's Combobox popup for suggest-while-typing (the
   popup core is shared, not duplicated). Bound model:
   `Signal<Vec<String>>`.
5. **Account chip**: no new widget — a `Chip` with label + dot is the
   header's account affordance; 0560 consumes it (recorded so 0560
   doesn't invent a second capsule).

## Scope / Non-goals
Scope: count/dot forms on Badge, Chip, ChipGroup, TagInput, tests,
gallery entries.
Non-goals: avatars/images in chips (cell art is not worth the mosaic
cost at 1-row scale); drag-reorder of chips; auto-complete ranking
logic (0500's matcher seam owns it); notification/attention POLICY
(what counts as unread is app data — this is rendering vocabulary).

## Expected outcomes
0500's MultiSelect trigger, 0550's sidebar unread badges and tab
counts, 0560's account chip, and the smart-note tag surfaces all speak
one tested vocabulary; `99+`, zero-hides, `+N` overflow, and
remove-a-chip behave identically everywhere.

## Validation
- Unit: count clamping (`0` hidden when configured, `100 → 99+`),
  cluster-safe truncation inside chips (reuse
  `text::truncate_ellipsis`, `src/text/truncate.rs:19`), overflow
  `+N` math, Backspace-removes-last semantics.
- CaptureTerm acceptance: a ChipGroup navigated keyboard-only
  (Left/Right/Enter/Backspace incl. remove glyph clicks); TagInput
  add/remove round-trip; wrap behavior at shrinking widths (chips wrap,
  never half-render); theme switch restyles tones (widget lint applies,
  `src/widgets/mod.rs:8-15`).
- A11y: chips report as focusable items with labels; removed chip moves
  focus predictably (to the next chip, else the input).

## Progress checklist
- [ ] Badge::count / Badge::dot (+ clamping, hide-zero)
- [ ] Chip (activate, remove hit-zone, toggle state, disabled)
- [ ] ChipGroup (one tab stop, wrap, +N overflow)
- [ ] TagInput (inline editor, add/remove semantics, 0500 suggest hook)
- [ ] Acceptance + a11y tests; gallery entries
