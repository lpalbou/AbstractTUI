//! Flowchart IR -> `abstracttui-graph` compilation.
//!
//! Mermaid stays a COMPILER: this module maps the IR onto the graph
//! crate's data contract and the vocabulary its view actually speaks —
//! node shapes become `kind` strings (accent tints) plus badge sigils,
//! edge kinds become the view's `dotted`/`thick` style hints, the
//! direction maps 1:1. Rendering is `GraphView`'s job, once, for every
//! consumer.

use abstracttui::text::width;
use abstracttui_graph::{EdgeDesc, GraphDesc, LayeredOpts, NodeDesc};

use crate::ir::{EdgeKind, FlowchartIr, NodeShape};

/// Node card height in cells (border + badge/content + border — the
/// GraphView compact recipe).
const NODE_H: i32 = 3;
/// Card width bounds in cells (labels truncate at draw time past the
/// cap; tiny labels still get a readable card).
const NODE_W_MIN: i32 = 7;
const NODE_W_MAX: i32 = 34;

/// Compile a flowchart into the graph contract: the `GraphDesc` plus
/// the `LayeredOpts` carrying the mermaid direction. Deterministic;
/// pure data.
pub fn to_graph(fc: &FlowchartIr) -> (GraphDesc, LayeredOpts) {
    let mut desc = GraphDesc::new();
    for node in &fc.nodes {
        let label = node.text.as_deref().unwrap_or(&node.id);
        let w = (width(label) + 4).clamp(NODE_W_MIN, NODE_W_MAX);
        let mut nd = NodeDesc::new(&node.id, w, NODE_H).label(label);
        if let Some(kind) = shape_kind(node.shape) {
            nd = nd.kind(kind);
        }
        desc = desc.with_node(nd);
    }
    for edge in &fc.edges {
        let mut ed = EdgeDesc::new(&edge.from, &edge.to);
        if let Some(label) = &edge.label {
            ed = ed.label(label);
        }
        if let Some(style) = edge_style(edge.kind) {
            ed = ed.style(style);
        }
        desc = desc.with_edge(ed);
    }
    let opts = LayeredOpts {
        direction: fc.direction,
        ..Default::default()
    };
    (desc, opts)
}

/// Shape -> `NodeDesc::kind` string (the GraphView accent vocabulary;
/// also the join key for badge sigils). `Plain`/`Rect` carry no kind —
/// they are the default card.
pub fn shape_kind(shape: NodeShape) -> Option<&'static str> {
    // In-crate exhaustive match ON PURPOSE: a new shape variant must
    // update this mapping or fail to compile.
    match shape {
        NodeShape::Plain | NodeShape::Rect => None,
        NodeShape::Rounded => Some("rounded"),
        NodeShape::Diamond => Some("decision"),
        NodeShape::Stadium => Some("stadium"),
    }
}

/// Shape -> badge sigil (GraphView renders badges top-right in the
/// card). The cell-honest v1 mapping of mermaid's shape variants:
/// cards stay cards; the shape arrives as a sigil + accent tint.
pub fn shape_badge(shape: NodeShape) -> Option<&'static str> {
    match shape {
        NodeShape::Plain | NodeShape::Rect => None,
        NodeShape::Rounded => Some("○"),
        NodeShape::Diamond => Some("◆"),
        NodeShape::Stadium => Some("◎"),
    }
}

/// Edge kind -> the GraphView stroke-style hint. `Open` maps to the
/// `open` hint, which the view renders as an arrowless stroke (the
/// undirected-link reading of mermaid's `---`).
fn edge_style(kind: EdgeKind) -> Option<&'static str> {
    match kind {
        EdgeKind::Arrow => None,
        EdgeKind::Open => Some("open"),
        EdgeKind::Dotted => Some("dotted"),
        EdgeKind::Thick => Some("thick"),
    }
}
