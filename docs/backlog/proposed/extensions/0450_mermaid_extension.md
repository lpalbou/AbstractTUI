# 0450 — `abstracttui-mermaid`: honest-subset diagram rendering

## Metadata
- Created: 2026-07-22
- Status: Proposed (needs-design; sibling crate; consumes 0420 strokes
  + 0440 layout)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: 0400's ADR (sibling crate; also rules whether the
  core dependency posture, docs/design/00-vision.md:46-53, binds
  extension crates — this item assumes YES and hand-rolls its parser).
  ADR impact: none in core.

## Context
Mermaid fences are the de-facto diagram notation of the markdown
world; every reader/viewer class app meets them (the mdpad rebuild is
the first validator, 0460). The honest prior art is the maintainer's
own: mdpad investigated in-terminal mermaid and REJECTED it —
"mermaid.js cannot lay out without a browser engine… a faithful
text-grid layout engine is a multi-thousand-line subsystem — both at
odds with a lean single binary" (mdpad src/render/mermaid.rs:1-14) —
and shipped a mermaid.live deep link instead (live_view_url,
mermaid.rs:24-34). Both halves of that verdict inform this item: the
layout subsystem is exactly what 0440 builds ONCE for the graph
extension (amortized, not per-app), and "faithful" is the wrong bar —
the right bar is an honest SUBSET rendered natively with atomic
fallback, plus the live-link affordance kept for everything else.

## Current code reality
- Mermaid arrives as a fenced code block with a lang tag: the core
  parser already isolates it verbatim (`Block::CodeFence { lang,
  lines }`, src/render/md.rs:80-85; "no inline parsing", md.rs:11).
  The fallback path therefore EXISTS: an unsupported diagram renders
  as a code fence exactly as today (MarkdownView code-fence typesetting,
  src/widgets/markdown.rs:14-16; Feed's `FeedItem::code`,
  src/widgets/feed.rs:139).
- Layout authority: none in core (see 0440 — the only line-drawing is
  chart.rs's private grid). 0440's `layout::layered()` is
  deliberately render-independent and public for this consumer, and
  its module contract is `GraphDesc -> Layout` for EVERY pass
  (0440 §1): if mermaid ever admits a non-hierarchical diagram kind,
  it routes to 0440's v1.5 `force()` under the same data contract —
  renderer untouched, algorithm swapped. v1 needs `layered()` only
  (flowcharts are ranked by construction; sequence layout is
  solverless).
- Strokes/arrowheads: 0420 (beziers on braille/quadrant dot grids;
  per-grid single color — diagram edge coloring must respect the
  cell-color rule documented at src/widgets/chart.rs:326-328).
- Link affordance for the escape hatch: `Span::with_link`
  (src/render/rich.rs:54) + OSC 8 emission exist; 0165 (band 0100)
  makes links app-activatable. The mdpad-style "view in browser" link
  rides these — the URL-fragment encoding technique is documented in
  mdpad mermaid.rs:7-14 (code travels in the fragment, never sent to
  a server).
- Text metrics for node sizing: `text::width`/`measure`
  (src/text/mod.rs:63,190) — node boxes size from the same width
  authority as all rendering.

## Problem
Markdown content with diagrams has no native story: readers show
mermaid as raw code (information present, structure lost) or punt to
a browser. Nobody should build this per-app — parser + layout +
strokes are exactly the amortizable extension stack.

## What we want (proposed shape — needs-design)
A sibling crate `abstracttui-mermaid`:
1. **Hand-rolled subset parser** (mermaid has no spec grammar; the
   subset is defined by THIS table, tested against real corpus
   files). Parse result: a diagram IR or a named unsupported-reason.
2. **The honest subset table** (v1 — the item's contract; the docs
   ship it verbatim). Grammar-actionability rule first (peer finding
   P2-7, accepted): **the YES rows enumerate accepted SPELLINGS, and
   any spelling outside them triggers the atomic fallback naming the
   first unrecognized line** — unknown syntax is safe by construction,
   not just unknown diagram kinds. The fixture corpus pins to a named
   mermaid docs version/date at implementation time (mermaid has no
   spec grammar; the pin is the only stable reference).

   | Mermaid | v1 | Accepted spellings (exhaustive) | Behavior |
   | --- | --- | --- | --- |
   | `flowchart` / `graph` TD/TB/LR/BT/RL | YES | header keyword + direction token only | 0440 layered layout (BT/RL as transposes) |
   | Node shapes | YES | `id`, `id[text]`, `id(text)`, `id{text}`, `id([text])`; quoted `"text"` inside brackets | box glyph variants |
   | Edges | YES | `-->`, `---`, `-.->`, `==>`; label as `--\|label\|` postfix form only (the `--label-->` infix form and `&`-chaining are v2 — fallback) | 0420 strokes; dotted/thick as glyph styles |
   | `subgraph` | NO (v2, needs 0440 clusters) | — | atomic fallback |
   | `sequenceDiagram` | YES | `participant id [as alias]`; messages `->>`, `-->>`, `->`, `-->` with `:` text; `Note left of/right of/over` | deterministic columns/rows — no graph solver |
   | sequence `loop`/`alt`/`par`/activations (`+`/`-`, `activate`) | NO (v2) | — | atomic fallback |
   | `stateDiagram-v2` (flat states + transitions) | STRETCH | `[*]`, `id`, `id : label`, `-->` with `:` labels | flowchart engine reuse; else fallback |
   | `classDiagram`, `erDiagram`, `gantt`, `pie`, `journey`, `mindmap`, `timeline`, `gitGraph` | NO | — | atomic fallback |
   | Styling directives (`classDef`, `style`, `%%` comments, themes) | IGNORED (parsed, dropped with notice; comments silently) | recognized-and-dropped list enumerated in docs | render proceeds |

3. **Atomic fallback, never partial**: if a diagram contains ANY
   unsupported construct (styling directives excepted), the WHOLE
   diagram renders as the code fence it already is, plus a one-line
   labeled notice naming the first unsupported construct, plus the
   optional mermaid.live link (mdpad's affordance, kept). Partial
   rendering of a half-understood diagram misleads; the code block
   never lies. This is the engine's labeled-degradation principle
   (roadmap principle 4) applied to parsing.
4. **Rendering**: flowcharts through 0440 `layered()` + 0430/0440's
   card/edge drawing (compact node recipe); sequence diagrams through
   a dedicated deterministic layout (lifeline columns from
   participant order, message rows in source order — no solver).
   Theme tokens throughout; deterministic output pinned by goldens.
5. **Integration recipe**: a documented adapter from
   `Block::CodeFence{lang: "mermaid"}` to the widget, usable from
   MarkdownView-based apps and Feed items (`CustomBlock`,
   src/widgets/feed.rs:98-106, is the seam — its honest
   height-at-width callback fits a laid-out diagram). Honest limit
   until **0480** (draw-closure link registration, this band — the
   seam peer finding P1-1 demanded, now fully specified) lands: a
   diagram rendered inside a `CustomBlock` draw closure cannot mint
   link ids, so node/edge URIs (and the mermaid.live escape link on
   the diagram itself) are not activatable in-feed — the live-link
   affordance renders as a plain link SPAN beside the block (the rich
   pipeline owns that path) until 0480 exists; app-side activation
   additionally needs 0165 (band 0100).

## Scope / Non-goals
Scope: parser for the table above, flowchart + sequence rendering,
atomic fallback + notice + live-link, corpus tests, goldens, docs with
the subset table. Non-goals: mermaid-faithful styling/theming
(tokens rule); the full grammar (the table IS the contract; growth =
table rows with tests, never silent acceptance); interactive editing
of diagrams; pie/gantt (pie is a chart — apps have `chart.rs`; gantt
is a table-class layout better served app-side until a consumer
proves it).

## Expected outcomes
The mdpad rebuild (0460) renders common flowcharts and sequence
diagrams natively where mdpad shipped a link; unsupported diagrams
look exactly as they do today plus an honest notice; no app hand-rolls
a mermaid parser.

## Validation
- Corpus: a fixtures directory of real-world mermaid samples (mermaid
  docs examples for supported rows; deliberately exotic samples for
  fallback rows) — every table row has at least one accepting and the
  NO-rows one falling-back test.
- Goldens: fixed flowchart/sequence sources → pinned cells (chart
  determinism discipline, src/widgets/chart.rs:22-23).
- Fallback atomicity: one unsupported line anywhere → byte-identical
  code-fence rendering + notice.
- Parser robustness: no input panics (fuzz the parser with the
  decode_image marker-soup pattern, src/gfx/decode.rs:118-135 as the
  house style).

## Progress checklist
- [ ] 0400 ruled; 0420/0440 landed (hard deps)
- [ ] Subset parser + IR + unsupported-reason reporting
- [ ] Flowchart rendering over 0440/0420
- [ ] Sequence rendering (deterministic, solverless)
- [ ] Atomic fallback + notice + live-link affordance
- [ ] Corpus + goldens + fuzz; docs with the table
