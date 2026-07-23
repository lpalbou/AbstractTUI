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
abstracttui = "0.2"
abstracttui-graph = "0.1"    # graph layout + GraphView
abstracttui-mermaid = "0.1"  # mermaid subset (depends on -graph)
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
edges dotted in their own ink), the fallback label as a non-scrolling
notice line, and pan via `Scroll`.

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

Run the crate examples: `cargo run -p abstracttui-graph --example
workflow` (layered pipeline with a marked retry cycle) and `--example
network` (force-directed).

## Mermaid: the honest subset

Mermaid has no spec grammar, and "faithful" is the wrong bar for a
terminal. `abstracttui-mermaid` renders an exhaustive, tested subset
natively and falls back ATOMICALLY on everything else. The table
below is the contract, verbatim from the backlog item that ruled it
(0450); the YES rows enumerate accepted SPELLINGS, and any spelling
outside them triggers the fallback naming the first unrecognized
line — unknown syntax is safe by construction:

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

The STRETCH row shipped: flat `stateDiagram-v2` parses as a third
front end to the flowchart IR (`[*]` becomes synthetic start/end
nodes). Cell-honest shape mapping: terminal cards do not rotate into
diamonds — a shape arrives as the card's accent kind plus a badge
sigil (`id{..}` → `decision` + ◆, `id(..)` → `rounded` + ○,
`id([..])` → `stadium` + ◎). Lexical notes (normalization, not new
constructs): `;` is a statement terminator, `%%` comments strip to
end of line (quote-aware).

### The atomic fallback

If a diagram contains ANY construct outside the table (styling
directives excepted), the WHOLE diagram renders as the code fence it
already is — verbatim source, monospace — plus one notice naming the
first unsupported construct and line, plus an optional
[mermaid.live](https://mermaid.live) link (`live_link_url`): the
diagram travels in the URL FRAGMENT, never to a server; nothing is
shared until the user opens the link. Partial rendering of a
half-understood diagram misleads; the code block never lies.

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

Run the demo: `cargo run -p abstracttui-mermaid --example mermaid`
(four embedded samples including an honest fallback; or pass a `.mmd`
path).

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
