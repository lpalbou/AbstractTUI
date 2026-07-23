//! The diagram IR: what the subset parser produces.
//!
//! [`crate::parse`] yields `Result<Diagram, Unsupported>` — a whole
//! diagram or a NAMED reason, never a partial acceptance (the atomic
//! fallback contract). Vocabulary enums are `#[non_exhaustive]` per
//! ADR-0003 §3: v2 table rows (subgraphs, more shapes, sequence
//! blocks) grow them.

use abstracttui_graph::Direction;

/// A parsed diagram. Grows with the subset table (`#[non_exhaustive]`).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diagram {
    /// `flowchart`/`graph` — and `stateDiagram-v2` flat, which
    /// compiles to the same IR (the stretch row's engine reuse).
    Flowchart(FlowchartIr),
    /// `sequenceDiagram`.
    Sequence(SequenceIr),
}

/// A flowchart: direction, nodes in first-mention order, edges in
/// source order, plus notices for recognized-and-dropped directives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowchartIr {
    /// Flow direction from the header (TD/TB = TopDown, LR, BT, RL).
    pub direction: Direction,
    /// Nodes in first-mention order; the first EXPLICIT shape/text
    /// declaration wins (later re-declarations are ignored).
    pub nodes: Vec<FlowNode>,
    /// Edges in source order.
    pub edges: Vec<FlowEdge>,
    /// Recognized-and-dropped directives (`classDef`, `style`,
    /// `%%{..}%%` init directives) — the table's IGNORED row. Render
    /// proceeds; these are notice lines, not errors.
    pub notices: Vec<String>,
}

/// One flowchart node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowNode {
    /// The node id (`[A-Za-z0-9_]+`).
    pub id: String,
    /// Display text from the shape brackets (`None` = the id shows).
    pub text: Option<String>,
    /// The declared shape.
    pub shape: NodeShape,
}

/// Node shape vocabulary (the accepted spellings; grows with the
/// table, hence `#[non_exhaustive]`).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum NodeShape {
    /// Bare `id` — no shape declared.
    #[default]
    Plain,
    /// `id[text]` — process rectangle.
    Rect,
    /// `id(text)` — rounded.
    Rounded,
    /// `id{text}` — decision diamond.
    Diamond,
    /// `id([text])` — stadium.
    Stadium,
}

/// One flowchart edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Postfix `|label|` text.
    pub label: Option<String>,
    /// The arrow spelling class.
    pub kind: EdgeKind,
}

/// Edge arrow vocabulary (`#[non_exhaustive]`: v2 spellings grow it).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum EdgeKind {
    /// `-->` — directed.
    #[default]
    Arrow,
    /// `---` — open (undirected) link.
    Open,
    /// `-.->` — dotted directed.
    Dotted,
    /// `==>` — thick directed.
    Thick,
}

/// A sequence diagram: participants in declaration/encounter order,
/// events in source order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SequenceIr {
    /// Explicit `participant` declarations first (their order), then
    /// implicit participants in message-encounter order.
    pub participants: Vec<Participant>,
    /// Messages and notes, in source order.
    pub items: Vec<SeqItem>,
    /// Recognized-and-dropped directives (same contract as
    /// [`FlowchartIr::notices`]).
    pub notices: Vec<String>,
}

/// One sequence participant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    /// The participant id used in messages.
    pub id: String,
    /// Display alias (`participant id as alias`).
    pub alias: Option<String>,
}

impl Participant {
    /// The display label: alias if declared, else the id.
    pub fn label(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.id)
    }
}

/// One sequence event (`#[non_exhaustive]`: v2 adds blocks).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeqItem {
    /// A message between participants (self-messages allowed).
    Message(Message),
    /// A note anchored to one or two participants.
    Note(Note),
}

/// One sequence message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Sender id.
    pub from: String,
    /// Receiver id.
    pub to: String,
    /// Arrow spelling class.
    pub kind: MessageKind,
    /// Message text (the `: text` part — required by the spelling).
    pub text: String,
}

/// Message arrow vocabulary (`#[non_exhaustive]`).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    /// `->>` — solid line, filled head.
    #[default]
    SolidArrow,
    /// `-->>` — dashed line, filled head.
    DashedArrow,
    /// `->` — solid line, open head.
    SolidOpen,
    /// `-->` — dashed line, open head.
    DashedOpen,
}

impl MessageKind {
    /// Dashed line class (`-->>`, `-->`).
    pub fn dashed(self) -> bool {
        matches!(self, MessageKind::DashedArrow | MessageKind::DashedOpen)
    }

    /// Filled arrowhead class (`->>`, `-->>`).
    pub fn filled(self) -> bool {
        matches!(self, MessageKind::SolidArrow | MessageKind::DashedArrow)
    }
}

/// One sequence note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Where the note anchors.
    pub anchor: NoteAnchor,
    /// Note text.
    pub text: String,
}

/// Note anchoring vocabulary (`#[non_exhaustive]`).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteAnchor {
    /// `Note left of id: ..`
    LeftOf(String),
    /// `Note right of id: ..`
    RightOf(String),
    /// `Note over id: ..` / `Note over a,b: ..`
    Over(String, Option<String>),
}

/// The atomic-fallback verdict: the FIRST unrecognized construct,
/// named. Producing this for ANY unsupported construct — unknown
/// diagram kinds and unknown spellings alike — is the crate's safety
/// contract: unknown syntax is safe by construction.
///
/// Engine-produced fact carrier: `#[non_exhaustive]` (a column field
/// is a plausible growth); construct via parsing.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    /// 1-based source line number of the offending construct.
    pub line_no: usize,
    /// The offending statement, verbatim (trimmed).
    pub line: String,
    /// Why it is unsupported, in words (names v2 rows where known).
    pub reason: String,
}

impl Unsupported {
    pub(crate) fn new(line_no: usize, line: &str, reason: impl Into<String>) -> Unsupported {
        Unsupported {
            line_no,
            line: line.to_string(),
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unsupported mermaid at line {}: {} ({})",
            self.line_no, self.reason, self.line
        )
    }
}
