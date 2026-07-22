# 0460 — mdpad-class markdown reader enablement (core gap list)

## Metadata
- Created: 2026-07-22
- Status: Proposed (enablement item — v1-able per gap; the maintainer
  wants mdpad rebuilt on AbstractTUI and extended with diagrams)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: ADR-0001 (the md-vocabulary additions below extend a
  DOCUMENTED subset — doc-contract changes count as API,
  docs/adr/0001-api-stability-policy.md:49-54, so they land additively
  with the doc updated in the same change). ADR impact: none new.

## Context
mdpad (/Users/albou/projects/gh/mdpad) is the maintainer's shipped
markdown reader/editor: styled headings, staged-layout tables,
selection+clipboard over SSH, TOC panel, incremental search, link
following, built-in editor, single static binary (mdpad README.md
"Highlights"). The rebuild-on-AbstractTUI goal is the perfect
viewer-class validator (roadmap app classes: Viewers): it exercises
the depth items (0160/0165), the composer (0120), and this track's
diagram stack (0450) against a real product with users. This item is
the honest inventory: what exists, what is already planned elsewhere
(referenced by band, never duplicated), and what are NEW core gaps
this study found. It is an enablement map, not an epic to build mdpad
in this repo.

## Current code reality
Read side by side: mdpad's model and AbstractTUI's markdown pipeline.
- **mdpad's block vocabulary** (mdpad src/markdown/model.rs:54-84):
  Heading, Paragraph, List, CodeBlock, Quote, **Table** (alignments,
  head, rows), Rule, **Html** (rendered verbatim and dimmed),
  **FootnoteDef**; inline **Image** (model.rs:29). Its renderer owns
  staged table layout, search, selection, TOC, links (mdpad
  src/render/table.rs, src/ui/{search,selection,toc,navigate}.rs).
- **AbstractTUI's markdown subset is honestly smaller**
  (src/render/md.rs:5-17): inline bold/italic/code/link + headings,
  lists, quotes, fenced code, rules — and the NOT-supported list
  names tables, HTML, images, reference links, task lists explicitly
  (md.rs:14-17).
- What the engine already has for a reader: `MarkdownView` +
  deterministic `rows()` fold (src/widgets/markdown.rs:17-19),
  `Scroll` with measured extent + follow (src/widgets/scroll.rs:11-33),
  a real `Table` widget whose column-width solver is reusable
  (`solve_columns`, src/widgets/table.rs:374), the `Image` widget —
  which is MOSAIC-only by design ("bitmap display through the gfx
  mosaic pipeline", src/widgets/image.rs:1; `from_path` decodes PNG
  only, image.rs:117-130); the full kitty/iterm2/sixel ladder lives in
  `gfx::ImageSession` + overlay image entries, which do NOT flow
  inside scrolled document content — link spans + OSC 8 emission
  (src/render/rich.rs:54), themes with light/dark
  (mdpad's `--light` flag becomes a theme switch), and the md
  streaming session for live re-render on edit
  (src/render/md.rs:48-53).
- Already planned elsewhere, referenced by band (not duplicated here):
  **0160** selection+copy (mdpad's drag-select/clipboard story, incl.
  OSC 52 over SSH), **0165** link hit-testing (follow links, anchor
  jumps, back-stack activation), **0120** TextArea (the built-in
  editor `e`), **0150** terminal verbs (clipboard/title), **0140**
  lexers (syntax highlighting breadth), live-data **0010** (file
  reload `r` as a source binding) — bands 0100-0190/0010-0090.
- **NEW core gaps this comparison exposes** (nothing in any band
  covers them; recommended as app-widgets-band items, integrator to
  number — this track does not write in 0100-0190):
  1. **Markdown tables**: parse `| a | b |` + alignment row into a
     block; typeset as a STATIC rich-pipeline block sharing the width
     ALGORITHM with the Table widget (`solve_columns`,
     src/widgets/table.rs:374) — NOT embedding the interactive Table
     widget itself (it is a focus/selection control; a document table
     is typeset content — peer correction P2-8a, accepted; the same
     draw-only truth holds in feeds: `FeedBlock::Custom` is a draw
     closure, src/widgets/feed.rs:90-101, so an embedded "widget"
     there would be a painted table regardless). mdpad's staged
     algorithm (protect typical content, wrap the space-hungry
     columns, degrade to per-row records — mdpad README.md "Tables
     that actually work") is the quality bar.
  2. **Markdown images**: `![alt](src)` as a block-level element.
     Honest capability split (peer correction P2-8b, accepted):
     in-flow document images render MOSAIC via the Image widget
     (universal, correct in scrolled content; needs the widget's
     decode path widened from PNG-only to `decode_image`'s PNG+JPEG,
     src/widgets/image.rs:121-130 vs src/gfx/decode.rs:58-67);
     pixel-protocol (kitty/iterm2/sixel) images inside a SCROLLING
     document are an OPEN compositing question (the ladder lives in
     overlay-scoped `gfx::ImageSession`, not in-flow) — named as its
     own design note inside the seed, not promised. Alt-text fallback
     where graphics degrade, per the labeled-degradation discipline.
  3. **Heading anchors + document map**: stable heading ids from the
     parsed blocks (slugging), a TOC extraction API (`Vec<(level,
     text, id)>`), and scroll-to-anchor — the TOC panel and `#anchor`
     jumps ride these.
  4. **Search highlighting**: a span-overlay mechanism so a match set
     highlights WITHOUT re-parsing the document (mdpad's incremental
     search); relates to 0160's selection rendering — one
     overlay-span design should serve both.
- The diagram delta — the maintainer's stated reason to rebuild:
  mdpad ships mermaid as a deep link only (mdpad
  src/render/mermaid.rs:1-14); 0450 renders the subset natively.

## Problem
A viewer-class app cannot reach mdpad parity today: no md tables, no
md images, no anchors/TOC, no search highlight — and each gap, built
app-side, re-derives typesetting the engine owns (the 0120 lesson,
again). Without this inventory the rebuild would discover the gaps
one crash at a time.

## What we want
1. The four new core gaps filed as app-widgets-band items — explicit
   cross-references for the integrator's fold, NOT authored here
   (band discipline: this track never writes in 0100-0190):
   **md tables → app-widgets** (seed 1 above; shares `solve_columns`
   with 0530's table-upgrade lane — coordinate, don't duplicate),
   **md images → app-widgets** (seed 2; the in-flow pixel-protocol
   question is its named design note), **heading anchors/TOC →
   app-widgets** (seed 3; 0165 consumes the anchor ids for `#anchor`
   jump activation), **search-highlight overlay → app-widgets**
   (seed 4; design WITH 0160's selection rendering — one overlay-span
   mechanism for both). Integrator numbers them; each is
   independently v1-able and additive.
2. This item then tracks ENABLEMENT: the checklist below flips as the
   dependencies land; when all rows are green, "mdpad-on-AbstractTUI"
   is an app project (its own repo, consuming the published crate +
   extensions), not an engine project.
3. A capability-parity table in this file kept honest per row:
   exists / planned(band) / new-gap / extension(0450) — the rebuild's
   go/no-go dashboard.

## Scope / Non-goals
Scope: the inventory, the four gap seeds, the parity dashboard.
Non-goals: building mdpad here (apps validate, never live in the
engine repo — roadmap principle 2); footnotes and raw-HTML blocks
(mdpad has them; no second consumer — file only if the rebuild proves
them essential); editor undo/redo (app-side per 0120's non-goals).

## Expected outcomes
The mdpad rebuild starts with a truthful dependency list instead of
archaeology; the engine gains four broadly-useful content features
justified by the Viewers class (every one also serves chat/feed and
console transcripts — tables and images in agent output are routine).

## Validation
- Each gap item lands with its own tests (table wrap goldens vs the
  mdpad quality bar; image fallback labeling; anchor stability;
  highlight overlay damage cost).
- The parity table's every row cites either a shipped symbol, a
  backlog id, or "app-side" — no unaccounted feature.
- Dashboard exit: a spike app (examples-level, not shipped) renders
  mdpad's own README.md with tables, images, TOC jump and search —
  the README is the acceptance fixture.

## Progress checklist
- [ ] Four core-gap seeds handed to the integrator (band 0100-0190)
- [ ] md tables (parse + Table-machinery typesetting)
- [ ] md images (block route to Image widget + labeled fallback)
- [ ] heading anchors + TOC extraction + scroll-to-anchor
- [ ] search-highlight span overlay (designed with 0160)
- [ ] 0450 mermaid available as extension
- [ ] Parity dashboard green; spike renders mdpad's README
