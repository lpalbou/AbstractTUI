# Proposed: RichTextView (and MarkdownView) report no intrinsic measure — content inside a measured Scroll renders as zero rows

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — surprising blank panel; one-line workaround once diagnosed
- Class: footgun (API gap)

## Context
agora-tui's presence board is a `RichTextView` (one rich line per agent).
A cycle-2 adversarial finding asked for the panel to be scrollable (busy
hubs exceed its height), so the natural composition is
`Scroll::new(RichTextView::new(rich).element(&t).build())` — the docs'
own posture: "the content extent is measured by the layout solver
(`content_size` is an optional override)".

Rendered result: an EMPTY panel with a full-height scrollbar. The data
was present (the section header, computed from the same signal, showed
"PRESENCE 2/2"); the view drew nothing. Root cause: `RichTextView`
builds a draw-closure element with no `.measure(..)`, so its intrinsic
size is zero — the Scroll's measured content extent is 0 rows, nothing
is laid out, and the scrollbar renders over nothing. `MarkdownView`
appears to share the shape (no measure hook found in either widget).

Elements HAVE the seam (`Element::measure(fn(Size) -> Size)` — the api
guide explicitly says it exists "so a draw widget can answer Auto sizing
like a text leaf instead of defaulting to zero"); these widgets just do
not use it.

## Current code reality (0.2.8)
- `src/widgets/richtext.rs` — no `measure` anywhere; `element(&t)`
  returns a draw-closure Element with layout only.
- `src/widgets/markdown.rs` — same (no measure).
- `src/widgets/scroll.rs:100` — `content_size` exists as the manual
  override; without it the extent comes from the solver's measurement,
  which is zero for these widgets.

## Repro
```rust
let rich = RichText::from_lines(vec![RichLine::from_spans(vec![
    Span::plain("row 1"),
])]);
// Renders an empty viewport + scrollbar; the row never draws:
Scroll::new(RichTextView::new(rich).element(&t).build()).view(cx)
```

## Workaround in the field (delete when fixed)
`src/ui/sidebar.rs` in agora-tui: `Scroll::new(...).content_size(w,
rows.len())` — the app re-derives the height the widget already knows
(its line count; wrapped rows would need the width-aware wrap math the
widget owns). An engine fix — `RichTextView`/`MarkdownView` installing
an honest `measure` (line count at width, post-wrap) — would delete the
override and make the docs' "extent is measured" sentence true for
these widgets.
