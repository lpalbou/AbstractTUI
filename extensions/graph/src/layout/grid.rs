//! The grid pass: honest fallback placement.
//!
//! Row-major near-square grid in input order — no hierarchy, no
//! crossing reduction, and it SAYS so: every grid layout carries a
//! fallback label. It is also where [`super::layered`] degrades past
//! its node cap (a labeled grid beats a hung solver).

use crate::desc::GraphDesc;

use super::geom::{clip_border, self_loop};
use super::resolve::Resolved;
use super::{assemble, EdgeLayout, Layout, NodeLayout};

use abstracttui::base::Rect;

/// Horizontal cells between grid slots (matches the layered default).
const SLOT_GAP_X: i32 = 3;
/// Vertical cells between grid slots (matches the layered default).
const SLOT_GAP_Y: i32 = 2;

/// Grid placement: nodes in a near-square row-major grid, edges as
/// straight border-to-border segments, `rank` reporting the grid row.
///
/// The output is always labeled (`Layout::fallback`) — this pass
/// computes no hierarchy and never pretends to.
pub fn grid(desc: &GraphDesc) -> Layout {
    grid_with_notes(
        desc,
        vec!["grid placement: no hierarchy computed".to_string()],
    )
}

/// Grid placement with caller-supplied honesty notes prepended (the
/// layered node-cap fallback path names the cap here).
pub(crate) fn grid_with_notes(desc: &GraphDesc, mut notes: Vec<String>) -> Layout {
    let resolved = Resolved::new(desc);
    notes.extend(resolved.notes());
    let n = resolved.len();
    if n == 0 {
        return assemble(Vec::new(), Vec::new(), notes);
    }

    let cols = near_square_columns(n);
    let slot_w = resolved.sizes.iter().map(|s| s.w).max().unwrap_or(1) + SLOT_GAP_X;
    let slot_h = resolved.sizes.iter().map(|s| s.h).max().unwrap_or(1) + SLOT_GAP_Y;

    let mut nodes = Vec::with_capacity(n);
    for g in 0..n {
        let (row, col) = (g / cols, g % cols);
        let size = resolved.sizes[g];
        let rect = Rect::new(col as i32 * slot_w, row as i32 * slot_h, size.w, size.h);
        nodes.push(NodeLayout::new(resolved.id(desc, g), rect, row));
    }

    let mut edge_out: Vec<(usize, EdgeLayout)> = Vec::new();
    for e in &resolved.edges {
        let (ra, rb) = (nodes[e.from].rect, nodes[e.to].rect);
        let center = |r: Rect| {
            (
                f64::from(r.x) + f64::from(r.w) / 2.0,
                f64::from(r.y) + f64::from(r.h) / 2.0,
            )
        };
        let waypoints = vec![clip_border(ra, center(rb)), clip_border(rb, center(ra))];
        let d = &desc.edges[e.desc_index];
        edge_out.push((
            e.desc_index,
            EdgeLayout::new(d.from.clone(), d.to.clone(), e.desc_index, waypoints),
        ));
    }
    for &(g, desc_index) in &resolved.self_edges {
        let d = &desc.edges[desc_index];
        edge_out.push((
            desc_index,
            EdgeLayout::new(
                d.from.clone(),
                d.to.clone(),
                desc_index,
                self_loop(nodes[g].rect),
            ),
        ));
    }
    edge_out.sort_by_key(|(i, _)| *i);
    let edges = edge_out.into_iter().map(|(_, e)| e).collect();
    assemble(nodes, edges, notes)
}

/// Smallest column count whose square covers `n` (near-square grid).
fn near_square_columns(n: usize) -> usize {
    let mut cols = 1usize;
    while cols * cols < n {
        cols += 1;
    }
    cols
}
