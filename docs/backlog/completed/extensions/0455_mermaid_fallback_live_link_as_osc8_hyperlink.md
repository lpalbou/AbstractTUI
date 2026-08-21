# 0455 — mermaid fallback: the mermaid.live link is dead text when clipped; emit it as an OSC-8 hyperlink

## Metadata
- Created: 2026-07-25
- Status: Proposed (wave-12 pixel review,
  `reviews/wave12/visual-to-code-handoff.md` §3; agreed by the code
  seat — `reviews/wave12/code-to-visual-handoff.md` "#3: agreed …
  small, next wave")
- Track: extensions (`abstracttui-mermaid`, beside completed 0450)
- Class: UX defect / capability adoption gap (the engine has the
  currency; the fallback renderer doesn't spend it)
- Severity: P3 — the escape hatch exists but is unusable exactly when
  it matters (long sources produce long URLs produce clipping)
- Engine: reproduced on 0.2.18 / abstracttui-mermaid 0.1.0 (wave 12);
  re-verified present 2026-07-25 (`one_line` paints plain text, no
  link id)

## The evidence

`mermaid--gantt-fallback.svg` (wave-12 capture family): the fallback's
"view online: https://mermaid.live/edit#base64:…" line
ellipsis-truncates at the pane edge — a clipped URL is unusable — and
the styled dump shows NO link id on those cells. The whole point of
the atomic fallback (0450) is the honest escape hatch; a dead link
under clipping defeats it precisely for the diagrams big enough to be
unsupported.

## Current code reality (verified 2026-07-25)

- `extensions/mermaid/src/view.rs:160-165` — the fallback emits
  `one_line(format!("view online: {}", live_link_url(&self.source)),
  tokens.text_faint)`; `one_line` (`view.rs:197-216`) is a single-row
  truncating text line. No link registration anywhere in the crate.
- The engine currency exists and is caps-gated end to end:
  `render::rich` spans carry link URLs
  (`src/render/rich.rs:27, 53, 304` — `span.style.link(register_link
  (url))`), `Surface::register_link` interns URIs
  (`src/render/surface.rs:121`), and the presenter emits OSC 8 only
  when `caps.hyperlinks` holds (`src/render/present_tests.rs:299-329`
  — "no OSC 8 without the capability").

## What we want

The display text may clip; the LINK must stay whole:

1. Emit the URL cells with a registered link id (the display string
   can stay `view online: …` with ellipsis — OSC 8 carries the full
   URI regardless of what the cells show); on `caps.hyperlinks`
   terminals the clipped line becomes clickable.
2. On terminals WITHOUT hyperlink support, never mid-URL-ellipsis:
   wrap the URL across lines (it is the last resort surface — a
   selectable, copyable multi-line URL beats a truncated one).

Both halves are small; the handoff sized this "small, next wave".

## Validation

- Styled dump of the fallback view shows a link id on the "view
  online" cells; the resolved URI equals `live_link_url(source)`
  byte-for-byte (the fragment must never truncate).
- Caps-off rendering wraps the URL (no ellipsis inside the URL), pinned
  at a narrow width.
- Existing fallback tests (verbatim fence, named notice) unchanged.

## Non-goals

- Link hit-testing/activation inside the engine (that is app-widgets
  0165 + extensions 0480's channel; OSC 8 works TODAY without them —
  the terminal owns the click).
- Changing `live_link_url`'s state format (verified against the live
  editor's serde in 0450 — do not touch).

## Related

- Completed extensions 0450 (the fallback contract), extensions 0480
  (draw-closure link registration — the generalized seam; this item
  needs only the rich-span path), app-widgets 0165 (consumer half).

## Shipped 2026-08-21

The fallback's mermaid.live URL carries the whole diagram in its
fragment, so on one row it truncated to dead text — too long to read,
impossible to copy whole, clickable nowhere. Two honest shapes now:

- terminal does OSC 8 (via 0480's `StyledCanvas::register_link`): one
  short row that IS the link;
- terminal does not: the URL WRAPS across rows, so a mouse selection
  can take all of it — never a mid-URL ellipsis, exactly as this item
  specified.

Pinned by a test that reconstructs the whole URL from the rendered
rows.
