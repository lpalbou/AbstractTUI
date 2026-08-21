# abstracttui-mermaid

Honest-subset mermaid rendering for
[AbstractTUI](https://github.com/lpalbou/abstracttui): an
[ADR-0004](https://github.com/lpalbou/abstracttui/blob/main/docs/adr/0004-extension-packaging.md)
sibling crate — a hand-rolled parser, with std + the abstracttui
family as the whole dependency posture.

```sh
cargo add abstracttui abstracttui-graph abstracttui-mermaid
```

The family guide (pass selection, the subset contract in context,
worked examples) lives in the repo:
[docs/graphs-and-diagrams.md](https://github.com/lpalbou/abstracttui/blob/main/docs/graphs-and-diagrams.md).
API reference: [docs.rs/abstracttui-mermaid](https://docs.rs/abstracttui-mermaid).

## The deal

Mermaid has no spec grammar, and "faithful" is the wrong bar for a
terminal. This crate renders an **exhaustive, tested subset** natively
and falls back **atomically** on everything else: a diagram either
renders whole, or renders as the code fence it already is — plus one
notice naming the first unsupported construct, plus an optional
[mermaid.live](https://mermaid.live) link (the code travels in the URL
fragment; nothing is sent anywhere by this crate). Partial rendering
of a half-understood diagram misleads; the code block never lies.

Supported (the crate docs carry the exhaustive spelling table — the
contract):

- `flowchart`/`graph` in all four directions, ten node shapes, the full
  arrow vocabulary (any body length, dotted, thick, `x`/`o` heads),
  postfix **and** infix edge labels, edge chaining (`A --> B --> C`)
  and `&` groups (`A & B --> C & D`). Compiled to
  [`abstracttui-graph`](https://docs.rs/abstracttui-graph) layout and
  rendered by its `GraphView` — mermaid is a **compiler** here, not a
  second renderer.
- `subgraph` blocks **flatten**: members render, the group box does
  not, and the loss is a notice. The layout engine has no cluster
  contract yet; a diagram that renders without its boxes beats one that
  does not render.
- `sequenceDiagram`: participants (with `as` aliases), the four message
  arrows with `: text`, `Note left of/right of/over`, the control-flow
  blocks `alt`/`else`, `opt`, `loop` and `par`/`and` (nested, drawn as
  labeled frames), and activations — `activate`/`deactivate` or the
  `->>+` / `-->>-` suffixes — as bars on the lifeline. Rendered by a
  deterministic, solverless column/row plan.
- `stateDiagram-v2` **flat**: transitions with labels and `[*]`.
- `classDef`/`style`/`click`/`linkStyle`/`class`/`direction`/`%%{init}`
  are recognized and dropped with a notice; `%%` comments drop
  silently.

Rendered with a **labeled downgrade** (never silently): `x`/`o`
arrowheads and `<-->` draw as plain arrows, and `<br/>` flattens to a
word break because a card is one line.

Everything else — sequence `rect`/`critical`/`break`/`box`,
classDiagram, erDiagram, gantt, pie, journey, mindmap, timeline,
gitGraph — falls back atomically with a named reason.

## In a markdown document

```rust
use abstracttui::widgets::MarkdownView;
use abstracttui_mermaid::MermaidFence;
use std::rc::Rc;

MarkdownView::new(doc).fence_block(Rc::new(MermaidFence::new()))
```

A ```mermaid fence becomes a diagram in place, inside the document's
own scroll surface, outline, and search index.

## Example

```rust
use abstracttui_mermaid::{live_link_url, parse};

// Pure data first: parse to the IR, or a NAMED refusal.
let src = "graph TD\n  A[Start] --> B{Ship?}\n  B -->|yes| C(Done)";
assert!(parse(src).is_ok());
assert!(parse("gantt\n  title Nope").is_err()); // named Unsupported

// The escape hatch for out-of-subset diagrams: the source travels in
// the URL fragment — nothing is sent anywhere by this crate.
let url = live_link_url("gantt\n  title Nope");
assert!(url.starts_with("https://mermaid.live/edit#"));

// In an app view, rendering is one line:
// MermaidView::new(src).view(cx)
```

Pure-data entry points for other consumers: `parse(&str)` (IR or a
named `Unsupported`), `to_graph(&FlowchartIr)` (the graph-crate
contract), `live_link_url(&str)`.

Run the demos from the WORKSPACE ROOT — the examples live in the root
crate so one `cargo run --example` list covers the whole family:
`cargo run --example mermaid` (nine samples, optionally with a `.mmd`
path) and `cargo run --example mermaid_doc` (a markdown document whose
```mermaid fences render as diagrams in place).

## License

MIT, same as AbstractTUI.
