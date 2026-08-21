# 0452 — Mermaid: the subset people actually write, rendered inside documents

Status: completed 2026-08-21
Owner: engine (extensions/mermaid, extensions/graph, widgets/markdown)
Effort: L

## The field ask

Operator, 2026-08-21, after auditing what the mermaid crate could
render: "subgraph and edge chaining are the two most common things
people write in real flowcharts" — and a ```mermaid fence in a document
rendered as source, not as a diagram.

The proof was in this repository: of the three mermaid diagrams in
`docs/`, ONE rendered. `architecture.md`'s module map used `subgraph`;
its frame-loop diagram used edge chaining. Both fell back.

## Shipped

- **Flowchart**: edge chaining, infix labels, `&` cross-product groups,
  the full arrow vocabulary (any body length, `x`/`o` heads, `<-->`),
  five more node shapes, and a quote- and bracket-aware statement
  scanner so an arrow inside a label is text on either side.
- **`subgraph` flattens** with one notice per diagram naming the
  groups — the layout engine has no cluster contract, and a diagram
  that renders without its boxes beats one that does not render.
- **Sequence control flow**: `alt`/`else`, `opt`, `loop`, `par`/`and`
  as labeled frames with dashed dividers, nested; activations as bars
  on the lifeline.
- **`widgets::FenceBlock`** (core) + **`MermaidFence`** (crate): a
  ```mermaid fence renders as a diagram INSIDE the document, in its own
  scroll surface, outline and search index.
- Five renders-but-wrong defects fixed, the class that is worse than a
  fallback: dropped edge labels in horizontal flowcharts, quoted labels
  keeping their quotes, `<br/>` reaching the screen literally,
  first-declaration-wins where mermaid uses the last, and a blank
  rectangle for an empty diagram.

## What the adversarial pass changed

Two review passes ran against this work, and between them they found
more than the implementation did. The one that matters most: block
nesting recursed in layout, rendering AND `Drop`, so a ~10 KB file of
nested `opt`s aborted the process — uncatchable, and fatal to a TUI
rendering markdown it did not write. Nesting is capped at 64 with a
named fallback. The others: activation bars opening three rows below
their own arrow, nested frames sharing or escaping their parent's
border, notes erasing the frame they sit in, `activate` inventing
participants and reordering columns, and a quadratic layout (51 ms at
1600 blocks) now linear via bottom-up extents.

## Evidence

All three of this repository's own diagrams render. 19 tests were
written from defects that actually shipped, including a depth cap, a
four-deep nesting corner count, "nothing inside a frame draws outside
it", and a linear-layout guard; the fuzzer's vocabulary gained the
block keywords, which it could not reach before.

## Limits kept

Group boxes are not drawn (flattened, with a notice) — clusters need
ordering constraints in the layout engine's crossing-reduction sweep,
which is a separate piece of work. `rect`/`critical`/`break`/`box`,
classDiagram, erDiagram, gantt, pie, journey, mindmap, timeline and
gitGraph still fall back atomically, by name.
