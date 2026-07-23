//! Layout passes and the output half of the crate contract: [`Layout`].
//!
//! Every pass is `GraphDesc -> Layout`:
//!
//! - [`layered`] — sugiyama-lite (v1): ranks by longest path, bounded
//!   median crossing-reduction sweeps, aligned-median coordinates, edge
//!   waypoints through rank gaps. The workflow/DAG path.
//! - [`force`] — bounded, seeded, alpha-cooled force placement (v1.5):
//!   the knowledge-graph path (cyclic, non-hierarchical data).
//! - [`grid`] — labeled grid placement: the honest fallback.
//!
//! Consumers select the algorithm, never a different data contract.

mod coords;
mod force;
mod geom;
mod grid;
mod layered;
mod ordering;
mod resolve;

pub use force::{force, ForceOpts, IterationBudget};
pub use grid::grid;
pub use layered::{layered, LayeredOpts};

use abstracttui::base::{Point, Rect};

/// Placement of one node: its card rectangle in cells plus its rank.
///
/// Engine-produced fact carrier (`#[non_exhaustive]` per ADR-0003 §1 —
/// cycle 2 will grow it, e.g. with port anchors). Read fields freely;
/// construct through [`NodeLayout::new`] (the 0430 editor synthesizes
/// layouts from user drags through the same door).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeLayout {
    /// The node id, echoed from the input.
    pub id: String,
    /// Card rectangle in cells; origin-normalized so the layout's
    /// bounding box starts at (0, 0).
    pub rect: Rect,
    /// Layer index along the flow axis. [`layered`] computes it;
    /// [`grid`] reports the grid row; [`force`] computes no hierarchy
    /// and honestly reports 0 for every node.
    pub rank: usize,
}

impl NodeLayout {
    /// Construct a node placement (downstream construction path).
    pub fn new(id: impl Into<String>, rect: Rect, rank: usize) -> Self {
        NodeLayout {
            id: id.into(),
            rect,
            rank,
        }
    }
}

/// Routing of one edge: a waypoint polyline from the source card border
/// to the target card border (endpoints inclusive).
///
/// Renderers draw straight polylines or splines through the waypoints;
/// multi-rank edges carry one interior waypoint per crossed rank gap.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeLayout {
    /// Source node id, echoed from the input.
    pub from: String,
    /// Target node id, echoed from the input.
    pub to: String,
    /// Index of this edge in `GraphDesc::edges`, so caller metadata
    /// (label/style) maps back even for duplicate from/to pairs.
    pub desc_index: usize,
    /// Polyline in cells, source border first, target border last.
    pub waypoints: Vec<Point>,
    /// True when the cycle-breaking heuristic reversed this edge to
    /// obtain a DAG. The polyline still runs from `from` to `to` (it
    /// travels against the flow axis); it is MARKED, never silently
    /// reordered, so renderers can style back edges distinctly.
    pub broken: bool,
}

impl EdgeLayout {
    /// Construct an edge routing (downstream construction path).
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        desc_index: usize,
        waypoints: Vec<Point>,
    ) -> Self {
        EdgeLayout {
            from: from.into(),
            to: to.into(),
            desc_index,
            waypoints,
            broken: false,
        }
    }

    /// Mark this edge as cycle-broken (builder style).
    pub fn broken(mut self) -> Self {
        self.broken = true;
        self
    }
}

/// The output half of the crate contract: positions, ranks, waypoints,
/// bounding box, and the two honesty markers (cycle-broken edge set,
/// fallback label).
///
/// Deterministic: the same `GraphDesc` and options yield an identical
/// `Layout` (golden-test-pinned). Coordinates are origin-normalized:
/// `bounds` always starts at (0, 0) and is the content size a scrolling
/// container should advertise.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Node placements, in input node order (minus dropped duplicates).
    pub nodes: Vec<NodeLayout>,
    /// Edge routings, in input edge order (minus edges whose endpoints
    /// do not resolve — those drops are recorded in `fallback`).
    pub edges: Vec<EdgeLayout>,
    /// Bounding box of all cards and waypoints, anchored at (0, 0).
    pub bounds: Rect,
    /// Honesty label. `None` means the requested algorithm ran cleanly.
    /// `Some` names every degradation that occurred: grid fallback past
    /// the node cap, dropped duplicate node ids, skipped unresolvable
    /// edges. A labeled degraded layout beats a hung or lying solver.
    pub fallback: Option<String>,
}

impl Layout {
    /// Construct a layout from parts, computing the bounding box from
    /// the content (downstream construction path).
    pub fn new(nodes: Vec<NodeLayout>, edges: Vec<EdgeLayout>) -> Self {
        let bounds = bounds_of(&nodes, &edges);
        Layout {
            nodes,
            edges,
            bounds,
            fallback: None,
        }
    }

    /// The cycle-broken edge set, as indices into `GraphDesc::edges`.
    /// Derived from the per-edge [`EdgeLayout::broken`] markers (one
    /// source of truth).
    pub fn broken_edges(&self) -> Vec<usize> {
        self.edges
            .iter()
            .filter(|e| e.broken)
            .map(|e| e.desc_index)
            .collect()
    }

    /// Look up a node placement by id.
    pub fn node(&self, id: &str) -> Option<&NodeLayout> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// Bounding box of cards and waypoints (each waypoint counted as one
/// cell), or `Rect::ZERO` for empty content.
pub(crate) fn bounds_of(nodes: &[NodeLayout], edges: &[EdgeLayout]) -> Rect {
    let mut acc = Rect::ZERO;
    for n in nodes {
        acc = acc.union(n.rect);
    }
    for e in edges {
        for p in &e.waypoints {
            acc = acc.union(Rect::new(p.x, p.y, 1, 1));
        }
    }
    acc
}

/// Assemble a pass result: origin-normalize, compute bounds, fold the
/// honesty notes into the fallback label. Every pass finishes here.
pub(crate) fn assemble(
    mut nodes: Vec<NodeLayout>,
    mut edges: Vec<EdgeLayout>,
    notes: Vec<String>,
) -> Layout {
    let bounds = normalize(&mut nodes, &mut edges);
    Layout {
        nodes,
        edges,
        bounds,
        fallback: resolve::fold_notes(notes),
    }
}

/// Shift every card and waypoint so the joint bounding box starts at
/// (0, 0), then store it. Every pass normalizes through here.
pub(crate) fn normalize(nodes: &mut [NodeLayout], edges: &mut [EdgeLayout]) -> Rect {
    let raw = bounds_of(nodes, edges);
    let (dx, dy) = (-raw.x, -raw.y);
    if dx != 0 || dy != 0 {
        for n in nodes.iter_mut() {
            n.rect = n.rect.translate(dx, dy);
        }
        for e in edges.iter_mut() {
            for p in e.waypoints.iter_mut() {
                *p = p.translate(dx, dy);
            }
        }
    }
    Rect::new(0, 0, raw.w, raw.h)
}
