//! # abstracttui-mermaid
//!
//! Honest-subset mermaid rendering for
//! [AbstractTUI](https://docs.rs/abstracttui) (backlog 0450): a
//! hand-rolled parser over an EXHAUSTIVE spelling table, flowcharts
//! compiled to [`abstracttui-graph`](https://docs.rs/abstracttui-graph)
//! layout, solverless sequence diagrams, and an ATOMIC fallback — a
//! diagram either renders whole or renders as the code fence it
//! already is, plus a notice naming the first unsupported construct.
//! Partial rendering of a half-understood diagram misleads; the code
//! block never lies.
//!
//! ## The subset table (the contract)
//!
//! The YES rows enumerate accepted SPELLINGS; any spelling outside
//! them triggers the atomic fallback naming the first unrecognized
//! line. Growth = new table rows with tests, never silent acceptance.
//!
//! | Mermaid | v1 | Accepted spellings (exhaustive) | Behavior |
//! | --- | --- | --- | --- |
//! | `flowchart` / `graph` TD/TB/LR/BT/RL | YES | header keyword + direction token only | layered layout (BT/RL as transposes) |
//! | Node shapes | YES | `id`, `id[text]`, `id(text)`, `id{text}`, `id([text])`; quoted `"text"` inside brackets | cards; shape = accent + badge sigil (see below) |
//! | Edges | YES | `-->`, `---`, `-.->`, `==>`; label as postfix `\|label\|` only | strokes; dotted/thick as stroke styles |
//! | `subgraph` | NO (v2) | — | atomic fallback |
//! | `sequenceDiagram` | YES | `participant id [as alias]`; messages `->>`, `-->>`, `->`, `-->` with `: text`; `Note left of/right of/over` | deterministic columns/rows — no solver |
//! | sequence `loop`/`alt`/`par`/activations | NO (v2) | — | atomic fallback |
//! | `stateDiagram-v2` (flat) | YES (stretch) | `[*]`, `id`, `id : label`, `-->` with `: label` | compiles to the flowchart engine |
//! | `classDiagram`, `erDiagram`, `gantt`, `pie`, `journey`, `mindmap`, `timeline`, `gitGraph` | NO | — | atomic fallback |
//! | `classDef`, `style`, `%%{init}` directives | IGNORED | recognized-and-dropped WITH a notice; `%%` comments drop silently | render proceeds |
//!
//! Lexical notes (normalization, not new constructs): `;` is a
//! statement terminator (split like newlines); `%%` comments strip to
//! end of line (quote-aware). The infix label form (`--label-->`) and
//! `&`-chaining are named v2 fallbacks; edge chaining
//! (`A --> B --> C`) falls back one-edge-per-statement.
//!
//! ## Shape mapping (cell-honest)
//!
//! Terminal cards do not rotate into diamonds; mermaid shapes arrive
//! as the card's ACCENT KIND + a badge sigil:
//!
//! | Spelling | Kind | Badge |
//! | --- | --- | --- |
//! | `id[text]` / bare `id` | (plain card) | — |
//! | `id(text)` | `rounded` | ○ |
//! | `id{text}` | `decision` | ◆ |
//! | `id([text])` | `stadium` | ◎ |
//!
//! Documented v1 notes: open links (`---`) compile with the `open`
//! style hint, which `GraphView` renders as an arrowless stroke;
//! sequence self-messages render as a small right-side loop.
//!
//! Declaration rule, BOTH diagram kinds (first-explicit-wins): order
//! is first mention; the first EXPLICIT declaration fixes a node's
//! shape/text and a participant's alias; bare mentions and implicit
//! registrations never reset a declaration, and an implicit
//! registration (a message naming an undeclared id) is ENRICHED by
//! the first explicit declaration that follows it. Later explicit
//! re-declarations are ignored.
//!
//! ## Fallback + escape hatch
//!
//! [`MermaidView`] renders unsupported sources as the verbatim code
//! fence + one notice + an optional
//! [mermaid.live](https://mermaid.live) link ([`live_link_url`]) —
//! the code travels in the URL fragment, never to a server.
//!
//! ```
//! use abstracttui_mermaid::{parse, Diagram};
//!
//! let ok = parse("graph TD\n  A[Start] -->|go| B{Choice}");
//! assert!(matches!(ok, Ok(Diagram::Flowchart(_))));
//!
//! let no = parse("graph TD\n  subgraph one\n  A --> B\n  end");
//! let err = no.unwrap_err();
//! assert_eq!(err.line_no, 2);
//! assert!(err.reason.contains("subgraph"));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compile;
mod flowchart;
pub mod ir;
mod lines;
mod seq_layout;
mod seq_render;
mod sequence;
mod state;
mod view;

pub use compile::{shape_badge, shape_kind, to_graph};
pub use ir::{
    Diagram, EdgeKind, FlowEdge, FlowNode, FlowchartIr, Message, MessageKind, NodeShape, Note,
    NoteAnchor, Participant, SeqItem, SequenceIr, Unsupported,
};
pub use seq_render::SeqStyle;
pub use view::{live_link_url, MermaidView};

// The graph-contract types a flowchart compiles into, re-exported so
// consumers need not name the graph crate for the common path.
pub use abstracttui_graph::{Direction, GraphDesc, LayeredOpts};

use lines::statements;

/// Parse mermaid source against the subset table: a whole [`Diagram`]
/// or the named [`Unsupported`] verdict — never a partial acceptance.
pub fn parse(source: &str) -> Result<Diagram, Unsupported> {
    let (stmts, notices) = statements(source);
    let Some(header) = stmts.first() else {
        return Err(Unsupported::new(1, "", "empty source: no diagram header"));
    };
    let mut tokens = header.text.split_whitespace();
    let kw = tokens.next().unwrap_or("").trim_end_matches(':');
    let rest: Vec<&str> = tokens.collect();
    let body = &stmts[1..];

    match kw {
        "flowchart" | "graph" => {
            let dir = match rest.as_slice() {
                [d] => match *d {
                    "TD" | "TB" => Direction::TopDown,
                    "LR" => Direction::LeftRight,
                    "BT" => Direction::BottomTop,
                    "RL" => Direction::RightLeft,
                    other => {
                        return Err(Unsupported::new(
                            header.line_no,
                            &header.text,
                            format!("unsupported direction `{other}` (TD/TB/LR/BT/RL)"),
                        ))
                    }
                },
                _ => {
                    return Err(Unsupported::new(
                        header.line_no,
                        &header.text,
                        "header must be exactly `flowchart <dir>` / `graph <dir>`",
                    ))
                }
            };
            flowchart::parse_flowchart(dir, body, notices).map(Diagram::Flowchart)
        }
        "sequenceDiagram" if rest.is_empty() => {
            sequence::parse_sequence(body, notices).map(Diagram::Sequence)
        }
        "stateDiagram-v2" if rest.is_empty() => {
            state::parse_state(body, notices).map(Diagram::Flowchart)
        }
        "classDiagram" | "erDiagram" | "gantt" | "pie" | "journey" | "mindmap" | "timeline"
        | "gitGraph" | "stateDiagram" | "quadrantChart" | "requirementDiagram" | "C4Context"
        | "sankey-beta" | "xychart-beta" | "block-beta" | "packet-beta" | "kanban"
        | "architecture-beta" => Err(Unsupported::new(
            header.line_no,
            &header.text,
            format!("diagram kind `{kw}` is not in the v1 subset"),
        )),
        _ => Err(Unsupported::new(
            header.line_no,
            &header.text,
            "unrecognized diagram header",
        )),
    }
}
