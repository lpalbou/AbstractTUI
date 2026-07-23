# Proposed: a Disclosure/Card widget (fold/unfold with a clean title row) — Feed items need packaged collapse semantics

## Metadata
- Created: 2026-07-23
- Status: Completed (2026-07-24 disclosure wave, jointly with
  first-app 0260 — the standalone `Disclosure` widget + the Feed
  click-hit-info enabler + the documented message-card recipe; the
  feed-NATIVE card kind stays future work behind 0280's draw-only
  block boundary; see the completion report at the bottom)
- Completed: 2026-07-24
- Severity: P2 — the hand-rolled version costs real code in every message-centric app; workaround ships
- Class: capability gap (feature request)

## Context
Direct operator feedback on the live agora watcher (2026-07-23,
screenshot review): "each message should be a card that can fold/unfold
[with] a clean title" — the dense transcript wall is not readable at hub
scale. The engine backlog already holds the idea as
`proposed/first-app/0260_disclosure_fold_unfold_widget.md`; this item is
the second independent consumer asking, now with field requirements from
a message-board domain.

What the watcher needs per message card:
- a one-row TITLE surface: fold glyph (▸/▾), status chips, sender,
  seq/time, addressee, decorations, and the message title — always
  visible, visually separated from neighbors;
- a collapsible BODY region (multi-block: text/markdown/code) that
  folds to nothing (or to an N-row preview) and unfolds in place;
- keyboard toggling on the SELECTED card (Feed `selected_key` exists)
  and click-to-toggle on the title row;
- fold state owned by the app (a signal), so policy defaults
  ("newest expanded, rest folded") stay app-side;
- correct extent accounting during fold/unfold (windowing + follow-tail
  must not jump), and O(changed-item) re-typeset.

## Current code reality (0.2.8)
- No disclosure/card widget exists. The nearest primitives:
  `FeedItem::max_rows` (static row cap with an honest overflow marker —
  fold in one direction, no interaction), `Feed::selected_key`
  (selection band), `FeedState::sync` fingerprints (an app can encode
  fold state into the fingerprint and re-render the item).
- first-app/0260 records the same gap from the first validator app.

## Repro / the hand-rolled workaround (delete when this lands)
agora-tui (`src/ui/panes.rs`) builds cards by hand: fold state rides a
`Signal<HashMap<key,bool>>` + a tail-policy signal, the SyncSpec
fingerprint is `(rev, folded, is_tail)`, folded items render a single
rich header line with an inline preview, expanded items render
header + body blocks, and Enter toggles the override for the selected
key. It works, but every message-centric app will re-write exactly this
(state plumbing, glyphs, extent care, selection interplay), and
click-to-toggle needs item-level hit info the app cannot reach today.

## Proposed direction (engine's call)
A `Disclosure`/`Card` feed-item kind (or wrapper widget): title blocks +
body blocks + a bound `Signal<bool>` (or keyed state on FeedState),
rendering the fold affordance from theme tokens, handling
click-on-title, and keeping extent/windowing exact across toggles.
first-app/0260's shape likely covers it; this item adds the field
requirement list above and a second consumer's vote.

---

## Completion report (2026-07-24, disclosure wave — with first-app 0260)

The engine's call landed as WRAPPER WIDGET + ENABLERS + RECIPE, not a
feed-item kind. Why: feed blocks are draw-only cell closures
(first-app 0280 — no widget lifecycle, focus, or handlers inside feed
items), so an engine-owned INTERACTIVE card kind inside the feed
cannot exist until that boundary is resolved; pretending otherwise
would have shipped a card whose title row cannot focus or capture
clicks through the feed's own machinery. What the watcher's field
requirements got instead, point by point:

- **One-row title surface**: the standalone `Disclosure` widget
  (title + `▸`/`▾` accent glyph + right-aligned muted `detail` slot,
  truncation title-first, selection-pair focus). INSIDE the feed, the
  title row stays an app-rendered rich line (your current shape) —
  now toggleable by click through the new hit info.
- **Collapsible body, N-row preview**: `Disclosure` bodies cap at
  `max_body_rows(n)` behind an auto-hiding scrollbar (standalone
  cards); in-feed, `FeedItem::max_rows` previews + fingerprint
  re-render remain the shape (recipe below).
- **Keyboard toggling on the SELECTED card + click-to-toggle on the
  title row**: Enter via `Feed::selected_key` (app shortcut, as you
  ship today) + `Feed::on_item_press(|key, row_within_item| ...)` —
  THE gap this filing named as unreachable ("click-to-toggle needs
  item-level hit info the app cannot reach today") is now
  `row_within_item == 0`. The row math is public as
  `FeedState::item_at_row` (gap rows honestly report `None`).
- **Fold state owned by the app**: `Disclosure::folded(Signal<bool>)`
  for standalone cards; in-feed the `Signal<HashMap<key, bool>>` +
  `(rev, folded)` sync fingerprint pattern is now the DOCUMENTED
  recipe (api.md "The message-card recipe", pointed at from
  live-data.md) — the packaged form of what agora-tui's
  `src/ui/panes.rs` hand-rolled.
- **Correct extent accounting / O(changed-item) re-typeset**: the
  fingerprint path re-typesets exactly the toggled item (existing
  sync machinery); `tests/wave_disclosure.rs` pins toggle damage
  containment and stable sibling rows for the standalone widget.

**Delete-when-this-lands ledger for agora-tui**: keep the panes.rs
fold-state map and fingerprint (that IS the recipe, now documented
and supported); DELETE the "no click path" workaround — wire
`on_item_press` gated on row 0; consider `Disclosure` directly for
non-feed card surfaces (detail drawers, settings).

**Gates** (2026-07-24, whole tree): see
`reviews/wave7/disclosure-handoff.md` — tests green, clippy zero,
fmt clean, `cargo semver-checks` vs 0.2.10 additive-clean.
