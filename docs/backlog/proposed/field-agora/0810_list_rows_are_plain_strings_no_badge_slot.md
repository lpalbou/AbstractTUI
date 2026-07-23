# Proposed: List rows are plain strings — a sidebar with per-row unread badges hand-rolls column math instead of composing Badge

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — paper cut; the string workaround shipped in under an hour
- Class: capability gap

## Context
The watcher's sidebar is the milestone's named composition: "channel List
with unread Badges" (live-data 0060; the launch brief pre-registered this
exact risk: "build the thin sidebar from List; if that hurts, that's a §6
finding"). It hurt mildly: `List` consumes `Vec<String>` — one string per
row — so there is nowhere to put a `Badge` (or any second ink) inside a
row. The unread count ends up hand-formatted into the row string with
padding math, which means:
- no tone color on the count (a warn-class unread chip is not expressible
  — the whole row renders in the List's two inks: text on surface, or the
  selection pair);
- manual truncation + right-alignment per row width
  (`channel_row()` in the app), re-derived on every unread change;
- the count participates in selection highlighting like ordinary text.

`Badge` itself is exactly the right widget (tone, raised chip ground,
shrink-proof) — it just cannot ride a List row.

## Current code reality (0.2.8)
- `src/widgets/list.rs:69` — `List { items: Vec<String>, … }`;
  construction is `List::of`/`List::new(Vec<String>)` (`:87`, `:95`).
  Row painting is a single `canvas.print` of the string with the
  row-level ink pair; there is no per-row view/element slot, no trailing
  accessory, no per-span styling.
- `src/widgets/badge.rs:57` — `Badge::element(&TokenSet)` builds a
  standalone element; nothing accepts it row-scoped.
- The app-kits NavList item (band 0550) is proposed, not shipped — this
  finding is field evidence for its "rows carry accessories" requirement.

## Repro
Build any List whose rows need a trailing count that (a) right-aligns,
(b) carries a semantic tone, (c) survives selection highlight visibly.
Observe all three need hand-rolled string math and (b) is impossible.

## Workaround in the field (delete when fixed)
`src/ui/sidebar.rs::channel_row` in agora-tui: fixed-width string
formatting (`{name:<w$} {count}`) with char-count truncation, rebuilt in
a dyn_view on every unread change. An engine fix (row accessory slot, a
rich-line row variant, or NavList shipping) would delete `channel_row`
and put a real `Badge` with `Tone::Warn` on unread rows.
