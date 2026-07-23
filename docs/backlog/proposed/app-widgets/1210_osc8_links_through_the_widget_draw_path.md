# 1210 — OSC-8 hyperlinks through the widget draw path (StyledCanvas link seam)

## Metadata
- Created: 2026-07-25 (wave 13, ARCHITECT-MD lane; 1200+ numbering —
  see 1200 for the range note)
- Status: Proposed
- Track: app-widgets
- Completed: N/A

## Problem

The engine has a COMPLETE hyperlink pipeline below the widgets:
`Span::with_link(url)` carries URLs through parse and wrap,
`Surface::register_link` interns them (id table, dedup, drop counting),
`render::Style.link` stamps cells, the presenter emits OSC-8, and
terminal caps detection gates it (`caps.hyperlinks`). `RichText::draw`
— the SURFACE-level path — registers span links (rich.rs:304).

But every WIDGET renders through `StyledCanvas`, whose
`print_styled(p, text, style)` cannot mint a link id (ids are
per-surface; the ui canvas has no registration door) — so
`print_span_clipped` drops `span.link` on the floor. Markdown links
render underlined in `link` ink and never become clickable
terminal hyperlinks. mdpad keeps links working through wrap and table
layout (a style-channel id trick); our cell model carries links
NATIVELY and loses them one trait short of the pixels. ADR-0004 §5
already names "the link-registration seam when landed" as part of the
extension anchor surface — this item is that landing.

## Proposal

1. Extend `StyledCanvas` with a default-method registration door:
   `fn register_link(&mut self, uri: &str) -> u16 { 0 }` (0 = no link,
   the existing sentinel; default keeps every existing impl compiling —
   additive). The compositor-backed canvases forward to their surface's
   table.
2. `print_span_clipped` (and the richtext walk) grows a link-aware
   variant: when `span.link` is `Some`, register once per span draw and
   stamp `style.link` before printing.
3. Consumers get it for free: MarkdownView/Feed/RichTextView links
   become real OSC-8 hyperlinks wherever caps allow; `BufferCanvas`
   test doubles expose the table for assertions.
4. Coordinate with 0165 (link hit testing): the same span→cells walk
   that registers ids is the geometry 0165 needs for pointer hits —
   one seam, two consumers; land the canvas door first.

## Validation

- Frame test: a markdown link draws cells whose link id resolves to the
  URL in the target surface table; the presenter emits OSC-8 under
  hyperlink caps and nothing without them.
- The dedup/drop-counting contracts of `Surface::register_link` hold
  through the widget path (many spans, one table entry).
- Zero-idle: parked link-bearing views emit no bytes (id stamping is
  paint-time state, not animation).
