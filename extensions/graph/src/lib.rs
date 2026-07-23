//! # abstracttui-graph
//!
//! Graph auto-layout for [AbstractTUI](https://docs.rs/abstracttui):
//! the layout half of the diagram lane (backlog 0440), an ADR-0004
//! sibling crate built on core's public API only.
//!
//! ## The one contract
//!
//! Every layout pass is **`GraphDesc -> Layout`**: nodes with cell
//! sizes and edges in, per-node positions/ranks, per-edge waypoint
//! polylines, a bounding box and honesty markers out. Consumers select
//! the ALGORITHM, never a different data contract:
//!
//! - [`layered`] — sugiyama-lite, the workflow/DAG path (v1).
//! - [`force`] — bounded seeded force placement, the knowledge-graph
//!   path (v1.5).
//! - [`grid`] — labeled near-square placement, the honest fallback.
//!
//! ## Honesty markers
//!
//! A layout never lies about degradation: cycle-broken edges are
//! marked ([`EdgeLayout::broken`], [`Layout::broken_edges`]), and
//! [`Layout::fallback`] names every degradation that occurred (node
//! cap exceeded, duplicate node ids dropped, unresolvable edges
//! skipped, grid placement). `None` means the requested algorithm ran
//! cleanly.
//!
//! ## Determinism
//!
//! Same graph + same options = identical `Layout`, golden-test-pinned.
//! No map-iteration order leaks into results, every tiebreak is input
//! order, and float arithmetic sticks to IEEE-exact operations
//! (`+ - * / sqrt`, no transcendentals), so goldens hold across
//! platforms.
//!
//! ## Bounds
//!
//! Everything is bounded: crossing-reduction sweeps
//! ([`LayeredOpts::sweeps`], default 4), the layered node cap
//! ([`LayeredOpts::node_cap`], default 512, past which the grid
//! fallback engages with a label), and the force iteration budget
//! ([`ForceOpts::budget`], default 256, freezing earlier on settle).
//! The force pass is an *act*, not an animation: run it on demand,
//! cache the `Layout`, re-render from the cache (zero idle cost is the
//! caller's story and the engine's rule).
//!
//! ```
//! use abstracttui_graph::{layered, GraphDesc, LayeredOpts};
//!
//! let desc = GraphDesc::new()
//!     .node("fetch", 9, 3)
//!     .node("build", 9, 3)
//!     .node("test", 8, 3)
//!     .edge("fetch", "build")
//!     .edge("build", "test");
//! let layout = layered(&desc, &LayeredOpts::default());
//! assert_eq!(layout.node("fetch").unwrap().rank, 0);
//! assert_eq!(layout.node("test").unwrap().rank, 2);
//! assert!(layout.fallback.is_none(), "clean run, no degradation");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod desc;
pub mod dump;
pub mod layout;
pub mod view;

pub use desc::{Direction, EdgeDesc, GraphDesc, NodeDesc};
pub use layout::{
    force, grid, layered, EdgeLayout, ForceOpts, IterationBudget, LayeredOpts, Layout, NodeLayout,
};
// The view half (cycle 2): the rendering widget over the core canvas
// layer — cards, canvas-stroke edges, selection/pan/tooltips.
pub use view::{GraphAlgo, GraphStyle, GraphView};

// Core geometry types used by the contract, re-exported so consumers
// (and tests) need not name the engine crate for a Point.
pub use abstracttui::base::{Point, Rect, Size};
