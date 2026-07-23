//! The input half of the crate's one data contract: [`GraphDesc`].
//!
//! Consumers describe *what* the graph is (nodes with cell sizes, edges
//! with optional metadata); every layout pass in this crate consumes the
//! same description and produces the same [`crate::Layout`]. Algorithms
//! are selectable; the contract is not.

use abstracttui::base::Size;

/// Flow direction of a layout, in mermaid vocabulary: TD / LR / BT / RL.
///
/// Directions are transposes of one canonical computation: the layered
/// pass lays ranks along the *flow* axis and orders nodes along the
/// *cross* axis, then maps (cross, flow) into screen (x, y). Node cards
/// never rotate — a `w x h` card stays `w x h` in every direction.
///
/// This is a closed vocabulary by design (four flows exist), so the enum
/// is deliberately exhaustive per ADR-0003 §3.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Rank 0 at the top, flow downward (mermaid `TD`/`TB`). The default.
    #[default]
    TopDown,
    /// Rank 0 at the left, flow rightward (mermaid `LR`).
    LeftRight,
    /// Rank 0 at the bottom, flow upward (mermaid `BT`).
    BottomTop,
    /// Rank 0 at the right, flow leftward (mermaid `RL`).
    RightLeft,
}

impl Direction {
    /// True when the flow axis is vertical (TD / BT).
    pub const fn is_vertical(self) -> bool {
        matches!(self, Direction::TopDown | Direction::BottomTop)
    }

    /// True when rank 0 sits at the far end of its axis (BT / RL), i.e.
    /// the canonical picture is mirrored along the flow axis.
    pub const fn is_reversed(self) -> bool {
        matches!(self, Direction::BottomTop | Direction::RightLeft)
    }
}

/// One node: a stable string id plus its card size in terminal cells.
///
/// `kind` and `label` are caller metadata carried through untouched —
/// layout never reads them (renderers and 0450's mermaid mapping do).
///
/// Author-written, shape-stable struct: plain fields + [`Default`], the
/// FRU idiom (`NodeDesc { id, size, ..Default::default() }`) per
/// ADR-0003 §2, or the fluent constructors below.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeDesc {
    /// Unique node id. Duplicate ids are a caller mistake: the first
    /// occurrence wins and the drop is recorded in the layout's
    /// `fallback` label (never silent).
    pub id: String,
    /// Card size in cells. Non-positive extents are clamped to 1x1 at
    /// layout time so degenerate inputs still produce a usable picture.
    pub size: Size,
    /// Optional caller metadata (e.g. a semantic class for theming).
    pub kind: Option<String>,
    /// Optional display label; layout carries it through untouched.
    pub label: Option<String>,
}

impl NodeDesc {
    /// A node with the two required facts: id and card size in cells.
    pub fn new(id: impl Into<String>, w: i32, h: i32) -> Self {
        NodeDesc {
            id: id.into(),
            size: Size::new(w, h),
            ..Default::default()
        }
    }

    /// Attach a semantic kind (builder style).
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    /// Attach a display label (builder style).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// One directed edge between two node ids.
///
/// `label` and `style` are caller metadata carried through untouched.
/// Same extensibility class as [`NodeDesc`]: plain + `Default` + FRU.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeDesc {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Optional display label (v1 renderers place it at the midpoint).
    pub label: Option<String>,
    /// Optional style hint (opaque to layout).
    pub style: Option<String>,
}

impl EdgeDesc {
    /// An edge with the two required facts: endpoint ids.
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        EdgeDesc {
            from: from.into(),
            to: to.into(),
            ..Default::default()
        }
    }

    /// Attach a display label (builder style).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Attach a style hint (builder style).
    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

/// A whole graph description: the single input type of every layout pass.
///
/// Node and edge ORDER is meaningful: it is the deterministic tiebreak
/// for every heuristic in this crate (same `GraphDesc` in, same
/// [`crate::Layout`] out — golden-test-pinned).
///
/// ```
/// use abstracttui_graph::{layered, GraphDesc, LayeredOpts};
///
/// let desc = GraphDesc::new()
///     .node("a", 8, 3)
///     .node("b", 8, 3)
///     .edge("a", "b");
/// let layout = layered(&desc, &LayeredOpts::default());
/// assert_eq!(layout.nodes.len(), 2);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphDesc {
    /// Nodes, in caller order.
    pub nodes: Vec<NodeDesc>,
    /// Edges, in caller order. `Layout` edges echo their index here so
    /// metadata (label/style) maps back even with duplicate endpoints.
    pub edges: Vec<EdgeDesc>,
}

impl GraphDesc {
    /// An empty graph.
    pub fn new() -> Self {
        GraphDesc::default()
    }

    /// Append a node (fluent form of pushing a [`NodeDesc`]).
    pub fn node(mut self, id: impl Into<String>, w: i32, h: i32) -> Self {
        self.nodes.push(NodeDesc::new(id, w, h));
        self
    }

    /// Append a fully-specified node.
    pub fn with_node(mut self, node: NodeDesc) -> Self {
        self.nodes.push(node);
        self
    }

    /// Append an edge (fluent form of pushing an [`EdgeDesc`]).
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push(EdgeDesc::new(from, to));
        self
    }

    /// Append a fully-specified edge.
    pub fn with_edge(mut self, edge: EdgeDesc) -> Self {
        self.edges.push(edge);
        self
    }
}
