# 0900: Completion panel occludes the row above a bottom-docked composer

- **Status:** proposed
- **Band:** field-agora (agora-tui field reports)
- **Engine:** abstracttui 0.2.12
- **Severity:** P2 — the occluded row is the composer's own prompt
  (target + status), exactly what the user needs while choosing a
  destination-sensitive command.

## What happened

agora-tui's chat composer is a bottom-docked TextArea with
`Completion` attached (`/` commands, `@` agents,
`PanelPlacement::AbovePreferred`). The anchored panel places its
bottom edge directly above the caret's row — which is the composer's
PROMPT row (`▸ #target · status`). With the dropdown open, the
visible line becomes a splice of the last candidate over the prompt:
`▸ /group ✓ sent #standup #18`. While picking `/dm` or a channel
command, the destination label is unreadable (adversarial design
review P2-3).

## Ask

A way for the opener to reserve rows between the anchor and the
panel: `Completion::margin_rows(n)` (or an offset on
`place_panel_biased`) so the panel's bottom edge lands N rows above
the anchor row. Default 0 keeps every existing layout byte-identical.

## Workaround

None clean app-side: the prompt row must sit adjacent to the input
for the compose block to read as one unit, and the panel's placement
is engine-owned.
