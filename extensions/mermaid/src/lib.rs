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
//! | Node shapes | YES | `id`, `id[text]`, `id(text)`, `id{text}`, `id([text])`, `id((text))`, `id[[text]]`, `id[(text)]`, `id{{text}}`, `id>text]`; quoted `"text"` inside brackets | cards; shape = accent + badge sigil (see below) |
//! | Edges | YES | any body length (`-->`, `--->`, `---`, `-----`), dotted (`-.->`, `-..->`, `-.-`), thick (`==>`, `===`); heads `>`, `x`, `o`; `<-->` bidirectional | strokes; dotted/thick as stroke styles |
//! | Edge labels | YES | postfix `\|label\|` and infix (`-- label -->`, `-. label .->`, `== label ==>`) | drawn centred in the rank corridor |
//! | Edge chaining, `&` groups | YES | `A --> B --> C`; `A & B --> C & D` (cross product) | one edge per link |
//! | `subgraph` | FLATTENED | `subgraph id`, `subgraph id [Title]`, nested, `direction` inside | members render, the GROUP BOX does not, WITH a notice |
//! | `sequenceDiagram` | YES | `participant id [as alias]`; messages `->>`, `-->>`, `->`, `-->` with `: text`; `Note left of/right of/over` | deterministic columns/rows — no solver |
//! | sequence blocks | YES | `alt` + `else`, `opt`, `loop`, `par` + `and`, nested | a labeled frame with dashed branch dividers |
//! | sequence activations | YES | `activate id` / `deactivate id`, and the `->>+` / `-->>-` suffixes | a bar on the lifeline; nested bars step right |
//! | sequence `rect`/`critical`/`break`/`box` | NO | — | atomic fallback |
//! | `stateDiagram-v2` (flat) | YES (stretch) | `[*]`, `id`, `id : label`, `-->` with `: label` | compiles to the flowchart engine |
//! | `classDiagram`, `erDiagram`, `gantt`, `pie`, `journey`, `mindmap`, `timeline`, `gitGraph` | NO | — | atomic fallback |
//! | `classDef`, `style`, `click`, `linkStyle`, `class`, `direction`, `%%{init}` | IGNORED | recognized-and-dropped WITH a notice; `%%` comments drop silently | render proceeds |
//!
//! Lexical notes (normalization, not new constructs): `;` is a
//! statement terminator (split like newlines); `%%` comments strip to
//! end of line (quote-aware); the statement scanner is quote- and
//! bracket-aware, so an arrow inside a label is text on EITHER side.
//!
//! Link spelling follows mermaid's own rule, which is the only way to
//! tell a chain from an infix label: a body of exactly TWO dashes
//! (`--`, `==`) opens a LABELLED link whose text runs to the closing
//! body, while THREE or more (`---`, `----`) is a complete open link.
//! So `A -- B --> C` is one labelled edge and `A --- B --> C` is a
//! two-edge chain.
//!
//! Sequence blocks are a TREE in the IR ([`Block`] holding
//! [`Branch`]es), not a start/else/end token stream: the parser
//! balances them once and names any mismatch (`alt` never closed, a
//! stray `end`, `else` in a `par`), so layout and rendering recurse
//! over something that cannot be malformed. The `+`/`-` message
//! suffixes are sugar and expand to the same
//! [`SeqItem::Activate`]/[`SeqItem::Deactivate`] the keywords produce —
//! one concept reaches the renderer, not two spellings of it.
//!
//! Sequence frames are chrome drawn UNDER the conversation: where a
//! frame's border crosses a lifeline or an activation bar, the border
//! wins the cell, because one cell cannot show both and a broken frame
//! reads worse than a broken line. Nesting is capped at 64 — deeper
//! input falls back by name rather than recursing the renderer into a
//! stack overflow, which no `catch_unwind` can save a host from.
//!
//! Labeled downgrades (rendered, and said out loud in a notice): the
//! `x` and `o` arrowheads and `<-->` bidirectionality have no cell
//! glyph and draw as plain arrows; `<br/>` in a label flattens to a
//! word break because a card is one line; a `subgraph` renders its
//! members without the group box.
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
//! | `id((text))` | `rounded` | ● |
//! | `id[[text]]` | `stadium` | ▤ |
//! | `id[(text)]` | `stadium` | ⌸ |
//! | `id{{text}}` | `decision` | ◈ |
//! | `id>text]` | `decision` | ▷ |
//!
//! Documented v1 notes: open links (`---`) compile with the `open`
//! style hint, which `GraphView` renders as an arrowless stroke;
//! sequence self-messages render as a small right-side loop.
//!
//! Declaration rule: layout order is FIRST mention, and the LAST
//! explicit declaration decides a node's shape and text — what
//! mermaid itself renders when an id is declared twice. A bare
//! mention is not a declaration and never resets one. (Participants
//! keep the first-explicit-wins alias rule: a message that
//! auto-registered an id is enriched by the declaration that
//! follows.)
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
//! // A chain, an infix label and a flattened group all parse:
//! let grouped = parse("graph LR\n  subgraph one\n  A -- go --> B --> C\n  end");
//! assert!(matches!(grouped, Ok(Diagram::Flowchart(_))));
//!
//! // What is NOT in the table still falls back, naming its line:
//! let no = parse("graph TD\n  A --> B\n  gantt title Nope");
//! let err = no.unwrap_err();
//! assert_eq!(err.line_no, 3);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compile;
mod fence;
mod flowchart;
pub mod ir;
mod lines;
mod seq_layout;
mod seq_render;
mod sequence;
mod state;
mod view;

pub use compile::{shape_badge, shape_kind, to_graph};
pub use fence::MermaidFence;
pub use ir::{
    Block, BlockKind, Branch, Diagram, EdgeKind, FlowEdge, FlowNode, FlowchartIr, Message,
    MessageKind, NodeShape, Note, NoteAnchor, Participant, SeqItem, SequenceIr, Unsupported,
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
