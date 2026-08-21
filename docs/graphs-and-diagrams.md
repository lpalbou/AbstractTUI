# Graphs and diagrams — the extension family

Core stays lean; diagram-class capability ships as sibling crates you
install only when needed ([ADR-0004](adr/0004-extension-packaging.md)):

- **`abstracttui-graph`** — graph auto-layout (`GraphDesc -> Layout`:
  layered, force and grid passes) plus `GraphView`, a read-only graph
  widget with selection, pan, tooltips and canvas-stroke edges.
- **`abstracttui-mermaid`** — honest-subset mermaid rendering:
  flowcharts and flat state diagrams compile onto `abstracttui-graph`,
  sequence diagrams render through a deterministic solverless plan,
  and everything outside the subset falls back atomically.

Both are ordinary crates on the public core API — no private hooks, no
cargo features, the same dependency posture as core (std + the family;
the mermaid parser is hand-rolled).

## Installing

```toml
[dependencies]
abstracttui = "0.4"
abstracttui-graph = "0.3"    # graph layout + GraphView
abstracttui-mermaid = "0.3"  # mermaid subset (depends on -graph)
```

## The one data contract: `GraphDesc -> Layout`

You describe the graph; a layout pass returns positions. Every pass
shares the same input and output types — consumers select the
ALGORITHM, never a different contract:

```rust
use abstracttui_graph::{layered, GraphDesc, LayeredOpts};

let desc = GraphDesc::new()
    .node("fetch", 9, 3)          // id, card width/height in cells
    .node("build", 9, 3)
    .edge("fetch", "build");
let layout = layered(&desc, &LayeredOpts::default());
// layout.nodes: Rect + rank per node; layout.edges: waypoint
// polylines; layout.bounds: the content size a Scroll advertises.
```

Honesty markers ride the output: cycle-broken edges are MARKED
(`EdgeLayout::broken`, `Layout::broken_edges()`), and
`Layout::fallback` names every degradation (node cap exceeded,
duplicate ids dropped, unresolvable edges skipped, grid placement) —
`None` means the requested algorithm ran cleanly. Every pass is
deterministic (same input, identical `Layout`, golden-pinned; float
arithmetic avoids transcendentals so goldens hold across platforms)
and bounded (sweep counts, node caps, iteration budgets — documented
on the option types).

## Picking a pass

| Pass | Use for | Shape |
| --- | --- | --- |
| `layered(&desc, &LayeredOpts)` | workflows, dependency/build graphs, pipelines, state machines — DAG-shaped data (cycles get broken and marked) | sugiyama-lite: longest-path ranks, bounded median crossing-reduction sweeps, aligned-median coordinates, waypoints through rank gaps; directions TD/LR/BT/RL |
| `force(&desc, &ForceOpts)` | knowledge graphs, networks — cyclic, dense, non-hierarchical data that defeats layering | seeded, alpha-cooled repulsion + edge springs + optional `rank_bias`; a bounded ACT that freezes on settle (never an idle animation — cache the `Layout`, re-render from the cache) |
| `grid(&desc)` | the honest fallback | near-square row-major placement, always labeled |

The grid is also what `layered` degrades TO: past the node cap
(default 512) it returns the grid placement with the cap named in
`Layout::fallback` — a labeled grid beats a hung solver at terminal
scale. Measured on a dev machine (unoptimized profile): 500 nodes /
718 edges lay out in ~14 ms (`layered`) and ~30 ms (`force`, budget
64).

## GraphView

`GraphView` renders a `Layout`: node cards (title on the border, an
optional kind-tinted left accent, a reactive badge slot), edges as
sub-cell canvas strokes (smoothed beziers through the waypoints,
arrowheads, dotted/thick styles from `EdgeDesc::style`, cycle-broken
edges dotted in their own ink — drawn through core's public
[canvas layer](api.md#canvas--canvas--vector-strokes)), the fallback
label as a non-scrolling notice line, and pan via `Scroll`.

```rust
use abstracttui::prelude::*;
use abstracttui_graph::{GraphDesc, GraphStyle, GraphView, NodeDesc};

// Inside a view builder (cx: Scope), colors caller-resolved from the
// active theme per the widget token rule:
let t = use_theme(cx).get().tokens;
let style = GraphStyle::from_tokens(&t)
    .kind_accent("ok", t.ok)
    .kind_accent("error", t.error);
let view = GraphView::new(
    GraphDesc::new()
        .with_node(NodeDesc::new("a", 12, 3).label("Fetch").kind("ok"))
        .with_node(NodeDesc::new("b", 12, 3).label("Parse").kind("error"))
        .edge("a", "b"),
)
.style(style)
.badges(|id| (id == "a").then(|| "3".to_string()))
.tooltips(std::time::Duration::from_millis(300))
.on_node_press(|id| eprintln!("pressed {id}"))
.view(cx);
```

Interaction is ONE focus stop: arrows pan until a node is selected;
Enter selects the first node, then arrows move the selection
spatially (aligned-first, deterministic tiebreaks), Enter presses it
(`on_node_press`), Escape deselects. Clicking a card selects and
presses; hovering shows a tooltip when enabled. Layout runs at
view-build time (an act): rebuild the view (`dyn_view` over your data
signal) to relayout — a parked `GraphView` costs zero idle,
test-pinned. Algorithm selection: `.algo(GraphAlgo::Force(opts))`, or
`.with_layout(layout)` to render positions you computed (or dragged)
yourself.

Run the examples: `cargo run --example workflow` (layered pipeline with
a marked retry cycle) and `cargo run --example network`
(force-directed).

## Mermaid: the honest subset

Mermaid has no spec grammar, and "faithful" is the wrong bar for a
terminal. `abstracttui-mermaid` renders an exhaustive, tested subset
natively and falls back ATOMICALLY on everything else. The table
below is the contract, verbatim from the crate docs
([docs.rs/abstracttui-mermaid](https://docs.rs/abstracttui-mermaid));
the YES rows enumerate accepted SPELLINGS, and any spelling outside
them triggers the fallback naming the first unrecognized line —
unknown syntax is safe by construction:

| Mermaid | Status | Accepted spellings | Behavior |
| --- | --- | --- | --- |
| `flowchart` / `graph` TD/TB/LR/BT/RL | YES | header keyword + direction token | layered layout (BT/RL as transposes) |
| Node shapes | YES | `id`, `id[text]`, `id(text)`, `id{text}`, `id([text])`, `id((text))`, `id[[text]]`, `id[(text)]`, `id{{text}}`, `id>text]`; quoted `"text"` inside brackets | cards; shape = accent + badge sigil (see below) |
| Edges | YES | any body length (`-->`, `--->`, `---`, `-----`), dotted (`-.->`, `-..->`, `-.-`), thick (`==>`, `===`); heads `>`, `x`, `o`; `<-->` | strokes; dotted/thick as stroke styles |
| Edge labels | YES | postfix `\|label\|` and infix (`-- label -->`, `-. label .->`, `== label ==>`) | drawn centred in the rank corridor |
| Chaining / `&` groups | YES | `A --> B --> C`; `A & B --> C & D` | one edge per link; `&` is a cross product |
| `subgraph` | FLATTENED | `subgraph id`, `subgraph id [Title]`, nesting, `direction` inside | members render; the GROUP BOX does not, with a notice |
| `sequenceDiagram` | YES | `participant id [as alias]`; messages `->>`, `-->>`, `->`, `-->` with `: text`; `Note left of/right of/over` | deterministic columns/rows — no solver |
| sequence blocks | YES | `alt` + `else`, `opt`, `loop`, `par` + `and`, nested | a labeled frame with dashed branch dividers |
| sequence activations | YES | `activate id` / `deactivate id`, and the `->>+` / `-->>-` suffixes | a bar on the lifeline; nested bars step right |
| sequence `rect`/`critical`/`break`/`box` | NO | — | atomic fallback |
| `stateDiagram-v2` (flat) | YES | `[*]`, `id`, `id : label`, `-->` with `: label` | compiles to the flowchart engine |
| `classDiagram`, `erDiagram`, `gantt`, `pie`, `journey`, `mindmap`, `timeline`, `gitGraph` | NO | — | atomic fallback |
| `classDef`, `style`, `click`, `linkStyle`, `class`, `direction`, `%%{init}` | IGNORED | recognized-and-dropped WITH a notice; `%%` comments drop silently | render proceeds |

**The link rule that decides chaining vs labels.** A body of exactly
TWO dashes (`--`, `==`) OPENS a labelled link whose text runs to the
closing body; THREE or more (`---`, `----`) is a complete open link. So
`A -- B --> C` is one labelled edge and `A --- B --> C` is a two-edge
chain — mermaid's own rule, and the only way to tell them apart without
guessing.

Sequence frames are chrome drawn UNDER the conversation: where a
frame's border crosses a lifeline or an activation bar, the border wins
the cell — one cell cannot show both, and a broken frame reads worse
than a broken line. Block nesting is capped at 64; deeper input falls
back by name rather than recursing the renderer into a stack overflow.

**Labeled downgrades** (rendered, and said out loud in a notice): the
`x` and `o` arrowheads and `<-->` bidirectionality have no cell glyph
and draw as plain arrows; `<br/>` in a label flattens to a word break
because a card is one line; a `subgraph` renders its members without
the group box.

Flat `stateDiagram-v2` parses as a third
front end to the flowchart IR (`[*]` becomes synthetic start/end
nodes). Cell-honest shape mapping: terminal cards do not rotate into
diamonds — a shape arrives as the card's accent kind plus a badge
sigil (`id{..}` → `decision` + ◆, `id(..)` → `rounded` + ○,
`id([..])` → `stadium` + ◎). Lexical notes (normalization, not new
constructs): `;` is a statement terminator, `%%` comments strip to
end of line (quote-aware); the infix label form (`--label-->`),
`&`-chaining, and edge chaining (`A --> B --> C`) are named v2
fallbacks.

### The atomic fallback

If a diagram contains ANY construct outside the table (styling
directives excepted), the WHOLE diagram renders as the code fence it
already is — verbatim source, monospace — plus one notice naming the
first unsupported construct and line, plus an optional
[mermaid.live](https://mermaid.live) link (`live_link_url`): the
diagram travels in the URL FRAGMENT, never to a server; nothing is
shared until the user opens the link. Partial rendering of a
half-understood diagram misleads; the code block never lies.

### Diagrams inside a markdown document

A ```mermaid fence renders as a diagram in place, in the document's own
scroll surface — not as a code block, and not as a separate widget the
app has to splice in:

```rust
use abstracttui::widgets::MarkdownView;
use abstracttui_mermaid::MermaidFence;
use std::rc::Rc;

MarkdownView::new(source).fence_block(Rc::new(MermaidFence::new()))
```

The seam is `widgets::FenceBlock` in the core: a claimant measures a
fence it recognizes and paints its rows. Fences it declines render as
code, unchanged. The engine ships no diagram languages — this is how a
sibling crate brings one without the core knowing it exists.

`MermaidFence` renders once per (source, width, theme) and serves rows
from that, so scrolling a long document does not re-render its
diagrams.

### Usage

```rust
use abstracttui_mermaid::MermaidView;

// In a view builder:
let view = MermaidView::new(
    "graph TD\n  A[Start] --> B{Ship?}\n  B -->|yes| C(Done)",
)
.view(cx);

// Pure-data entry points for other consumers:
// parse(&str) -> Result<Diagram, Unsupported>   (IR or named reason)
// to_graph(&FlowchartIr) -> (GraphDesc, LayeredOpts)
// live_link_url(&str) -> String                  (the escape hatch)
```

Run the demos:

- `cargo run --example mermaid` — nine embedded
  samples: chaining, infix labels, `&` groups, the shape vocabulary, a
  flattened `subgraph` with its notice, sequence control-flow blocks,
  activations, and an honest fallback (or pass a `.mmd` path).
- `cargo run --example mermaid_doc` — a markdown
  document whose ```mermaid fences render as diagrams in place (or pass
  a `.md` path).

## Honest limits (v1)

- Open links (`---`) carry the `open` style hint and render as
  arrowless strokes in `GraphView` (mermaid's undirected reading).
- Sequence gaps size to ADJACENT-pair message labels; long labels
  between distant columns truncate with an ellipsis.
- Edge chaining (`A --> B --> C`), infix labels, `&`-chaining and
  `subgraph` are named fallbacks, not silent acceptance — growth is
  new table rows with tests.
- Force layouts report `rank` 0 for every node (no hierarchy is
  computed — honest, not missing data).
