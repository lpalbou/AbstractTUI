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

Supported in v1 (the crate docs carry the exhaustive spelling table —
the contract):

- `flowchart`/`graph` in all four directions (TD/TB, LR, BT, RL),
  node shapes `id`, `id[text]`, `id(text)`, `id{text}`, `id([text])`
  (quoted text inside brackets), edges `-->`, `---`, `-.->`, `==>`
  with postfix `|label|`. Compiled to
  [`abstracttui-graph`](https://docs.rs/abstracttui-graph) layout and
  rendered by its `GraphView` — mermaid is a **compiler** here, not a
  second renderer.
- `sequenceDiagram`: participants (with `as` aliases), the four
  message arrows with `: text`, `Note left of/right of/over`. Rendered
  by a deterministic, solverless column/row plan.
- `stateDiagram-v2` **flat**: transitions with
  labels and `[*]` — a third front end to the flowchart engine.
- `classDef`/`style`/`%%{init}` directives are recognized and dropped
  with a notice; `%%` comments drop silently.

Everything else — `subgraph`, sequence blocks/activations,
classDiagram, erDiagram, gantt, pie, journey, mindmap, timeline,
gitGraph, infix labels, `&`-chaining — falls back atomically with a
named reason.

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

Run the demo: `cargo run -p abstracttui-mermaid --example mermaid`
(optionally with a `.mmd` file path).

## License

MIT, same as AbstractTUI.
