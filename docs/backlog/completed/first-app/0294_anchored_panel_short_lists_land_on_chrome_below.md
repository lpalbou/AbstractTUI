# Proposed: anchored panel places short lists over the chrome below instead of flipping up

## Metadata
- Created: 2026-07-22
- Status: Completed (shipped the opener-stated placement bias)
- Completed: 2026-07-23

## ADR status
- Governing ADRs: None. ADR impact: none — placement policy of the 0500
  passive-panel slice.

## Context
`abstractcode-tui`'s composer sits directly above a one-row status bar (the
key legend). Its `'/'` completion, driven live at 24 rows: with MANY
candidates (`/s` → 4 rows) the dropdown correctly flips ABOVE the caret;
with 1–2 candidates it lands BELOW — on the status-bar row — clobbering the
key legend for as long as the dropdown is open:

```
│ ente /theme pick a theme cancel ctrl+d details pgup/dn scroll … │
```

## Current code reality
`place_panel` (src/app/anchored.rs:87-…) prefers below whenever
`below_rows >= content_rows` and flips above only when below is SHORT and
above is LONGER. A 1-row candidate list always "fits" below as long as ANY
row exists under the anchor — the policy cannot express "the rows below me
are occupied chrome". The engine has no knowledge of which rows are chrome,
and the app has no placement input on `Completion`.

## Problem or opportunity
Every composer-over-status-bar layout (the transcript shape, the engine's
own `examples/transcript.rs`) hits this: short completions cover the very
hints row users glance at while typing commands. The panel is transient, so
nothing corrupts — but the most common completion state (1 candidate left
as you finish typing) is the one that occludes.

## Proposed direction (engine's call)
- A placement preference on `Completion`/`AnchoredPanel` (`PreferAbove` /
  `PreferBelow` / `Auto`), or
- an app-supplied exclusion rect (or "reserve N bottom rows") consulted by
  `place_panel`, or
- flip-above whenever the anchor sits in the bottom N rows of the viewport
  (N = content height) — cheap heuristic, no API change.

## App-side state
No clean app-side workaround exists (the Completion API exposes no
placement input); `abstractcode-tui` ships with the occlusion and points
here.

## Completion report (2026-07-23, 0.2.6 field wave)

Shipped the item's FIRST proposed direction — a placement preference
the opener states (`app/anchored.rs`) — chosen over the exclusion
rect (a second geometry input for one bit of intent) and over the
bottom-N heuristic (a silent behavior flip for every existing
bottom-anchored caller; the item's own analysis says the engine
cannot know which rows are chrome, so the OPENER says it):

- `PanelPlacement::{BelowPreferred, AbovePreferred}` (default
  `BelowPreferred`) + the pure `place_panel_biased(viewport, anchor,
  content, width, placement)`. `AbovePreferred` is the exact mirror
  rule: above wins whenever the content fits there or above offers no
  fewer rows; the panel falls below only when below is genuinely
  longer — so the viewport-edge flip works in both directions. Width,
  height clamp, x clamp and the empty-result contract unchanged.
- `place_panel` now delegates with `BelowPreferred` — signature and
  results byte-identical (test-pinned by a parity grid over
  viewport/anchor/content cells including the clamp + no-room edges).
- `AnchoredPanel::open_passive_biased(...)` stores the bias so the
  open AND every `update` re-placement prefer the stated side;
  `open_passive` delegates with the default. `Completion::placement`
  threads it to the dropdown — the consumer's composer states
  `AbovePreferred` and short candidate lists stop covering the key
  legend.
- The DEFAULT does not flip (the item argues for a placement input,
  not a new default): existing panels, popups, tooltips and selects
  are byte-identical. The owned mode (`Popup`/`place_owned`) keeps
  the classic rule — no consumer filed a need; `place_panel_biased`
  is public, so the owned mode can adopt the same bias additively if
  one does.

Tests (`app/anchored_policy_tests.rs`):
- `place_panel_biased_above_preferred_mirrors_the_rule` — the filed
  shape (one chrome row below, short content) places above; top-edge
  anchor falls below; cramped-both-sides takes the longer side.
- `place_panel_biased_below_preferred_is_byte_identical_to_place_panel`
  — the parity grid.
- `above_preferred_completion_keeps_short_lists_off_the_chrome_below`
  — the full live shape: bottom composer over a one-row status bar,
  `/th` → 1 candidate sits ABOVE the caret; long lists honor the bias
  too.
- `below_preferred_default_still_lands_on_the_row_below` — the
  compatible default pinned on the same chrome shape (the opener opts
  out, nothing flips silently).
- `passive_panel_biased_opens_above_and_update_keeps_the_bias` —
  substrate-level: biased open, update re-placement keeps the stored
  bias, mirror flip at the top edge. Owned-mode behavior stays pinned
  by the existing `place_owned_plain_mode_matches_place_panel...`
  suite.

Docs: `docs/api.md` anchored-panel section (one-line placement note),
CHANGELOG under `[Unreleased]`. Gates: whole-tree tests green, clippy
clean, semver-additive vs 0.2.6 (new enum, new fn, new method,
new builder setter; `place_panel`/`open_passive` untouched).
