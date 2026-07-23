# 0450 — `abstracttui-mermaid`: honest-subset diagram rendering

## Metadata
- Created: 2026-07-22
- Status: Completed (cycle 2 by GRAPH — workspace member
  `abstracttui-mermaid` over `abstracttui-graph` + core; the subset
  table shipped verbatim as the crate-docs contract)
- Track: extensions
- Completed: 2026-07-24

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
- [x] 0400 ruled; 0420/0440 landed (hard deps)
- [x] Subset parser + IR + unsupported-reason reporting
- [x] Flowchart rendering over 0440/0420 (compiled, not re-rendered)
- [x] Sequence rendering (deterministic, solverless)
- [x] Atomic fallback + notice + live-link affordance
- [x] Corpus + goldens + fuzz; docs with the table

## Completion report

### 2026-07-24 — SHIPPED (`extensions/mermaid`, cycle 2, GRAPH seat)

`abstracttui-mermaid` 0.1.0: hand-rolled parser (ADR-0004 §4 — deps
are exactly `abstracttui` + `abstracttui-graph`, dual-form spelled),
38 crate tests green, `cargo package` verifies.

- **Parser architecture**: lexical normalizer (`lines.rs`: quote-aware
  `%%` stripping, `;` statement splitting, 1-based line numbers) under
  three statement classifiers (flowchart / sequence / state-flat), one
  arm per accepted spelling. `parse() -> Result<Diagram, Unsupported>`
  is total: the first non-classifying statement IS the verdict (line
  number + verbatim line + named reason) — atomicity by construction,
  no partial IR type exists. Known v2 constructs get targeted reasons
  (subgraph, infix labels, `&`-chaining, edge chaining, activations,
  sequence blocks, composite states); the IGNORED row (`classDef`,
  `style`, `%%{init}`) accumulates notices and proceeds.
- **stateDiagram-v2 flat SHIPPED (stretch)**: it is a third front end
  to `FlowchartIr` (~130 lines): transitions -> edges, `[*]` ->
  synthetic `[*]start`/`[*]end` ids (brackets are outside the user id
  charset — collision-proof), rendering rides the flowchart engine
  unchanged. Composite states fall back named.
- **Compiler, not renderer**: `to_graph(&FlowchartIr) -> (GraphDesc,
  LayeredOpts)` is public pure data; `MermaidView` instantiates
  `GraphView::new(desc).algo(Layered(opts))`. Shapes map to the view's
  real vocabulary: kind accents (`decision`/`rounded`/`stadium`) +
  badge sigils (◆ ○ ◎); edges to the `dotted`/`thick` style hints
  (`---` carries an `open` hint — documented v1 limit: the view has no
  arrowless stroke vocabulary yet, so open links draw an arrowhead).
- **Sequence**: solverless integer plan (`seq_layout`: columns from
  participant order, gaps from box halves + adjacent-pair labels; rows
  from source order; left-overflowing notes shift the picture, never
  clip) + cell-glyph painter (`seq_render`: lifelines, solid/dashed
  runs, filled/open heads, self-message loops, note boxes, boxes on
  top). Golden-pinned rows (`sequence_golden_alice_john`).
- **Fallback + escape hatch**: unsupported sources render the verbatim
  code fence + one notice naming the first construct + the
  mermaid.live link — `#base64:` URL-safe-base64 state (verified
  against the live editor's own serde.ts); the code travels in the URL
  fragment only. `live_link(false)` opts out.
- **Corpus** (`fixtures/`, docs pin 2026-07-24, mermaid v11 docs): 11
  `accept_*` fixtures covering every YES-row spelling, 19 `fallback_*`
  covering every NO row + known v2 spellings; `tests/corpus.rs` makes
  the naming convention the assertion, counts pinned as minimums.
- **BT/RL geometry fixture** (`compile_graph.rs`): card height 3
  makes every band extent odd (fractional dummy centers); pinned
  BT == exact cell mirror of TD (rects `H-(y+h)`, waypoints `H-1-y`)
  and RL of LR, with no waypoint inside any card — regression-locking
  the cycle-2 `map_point` cell-interval fix from the consumer side.
- **Fuzz** (decode_image house style): 3000 byte soups + 2000 token
  soups + full truncation sweeps parse without panic; truncated real
  sources also build + draw views.
- **Tests (38)**: corpus 1; flowchart parser 8; sequence/state parser
  8; compiler + BT/RL 4; render/fallback 5 (incl. the pinned sequence
  golden and the atomic-fence proof); fuzz 4; unit 7 (normalizer,
  seq plan, base64/json/live-link); plus 1 doctest.
- **Follow-ups revealed** (graph-crate lane, filed in the wave
  report): arrowless stroke vocabulary for `---`; per-edge stroke
  styling of parallel edges; a `GraphView` seam for edge-label
  positioning if mermaid ever needs non-midpoint labels.
