# Proposed: anchored panel places short lists over the chrome below instead of flipping up

## Metadata
- Created: 2026-07-22
- Status: Proposed (UX defect report — first-app finding, 0.2.0 adoption wave)
- Completed: N/A

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
