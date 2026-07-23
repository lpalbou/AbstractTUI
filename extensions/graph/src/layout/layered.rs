//! The layered pass (sugiyama-lite): the workflow/DAG path.
//!
//! Pipeline: resolve -> cycle break (DFS back edges, marked) ->
//! longest-path ranks -> dummy chains for multi-rank edges -> bounded
//! median crossing-reduction sweeps -> aligned-median coordinates ->
//! waypoints through rank gaps -> direction mapping -> component
//! packing. Every stage is deterministic and bounded; graphs past the
//! node cap degrade to the labeled grid placement.

use std::collections::HashMap;

use crate::desc::{Direction, GraphDesc};

use super::coords::assign;
use super::geom::{cross_extent, flow_extent, map_point, map_rect, self_loop};
use super::grid::grid_with_notes;
use super::ordering::RankStructure;
use super::resolve::Resolved;
use super::{assemble, EdgeLayout, Layout, NodeLayout};

/// Options for [`layered`]. Author-written, shape-stable: construct via
/// functional record update over `Default` (ADR-0003 §2):
///
/// ```
/// use abstracttui_graph::{Direction, LayeredOpts};
/// let opts = LayeredOpts {
///     direction: Direction::LeftRight,
///     ..Default::default()
/// };
/// # let _ = opts;
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct LayeredOpts {
    /// Flow direction of the picture (default: [`Direction::TopDown`]).
    pub direction: Direction,
    /// Minimum cells between sibling cards along the cross axis
    /// (default 3, clamped to at least 1).
    pub node_gap: i32,
    /// Cells between rank bands along the flow axis — the corridor edge
    /// waypoints route through (default 2, clamped to at least 1).
    pub rank_gap: i32,
    /// Bound on crossing-reduction sweeps (default 4). Each sweep is
    /// one downward plus one upward median pass; the best ordering seen
    /// wins. More sweeps buy diminishing quality, never correctness.
    pub sweeps: u32,
    /// Node cap (default 512). Graphs with more nodes degrade to the
    /// grid placement with a fallback label naming the cap — a labeled
    /// grid beats a slow or hung solver at terminal scale.
    pub node_cap: usize,
}

impl Default for LayeredOpts {
    fn default() -> Self {
        LayeredOpts {
            direction: Direction::TopDown,
            node_gap: 3,
            rank_gap: 2,
            sweeps: 4,
            node_cap: 512,
        }
    }
}

/// Layered (sugiyama-lite) layout: ranks by longest path, bounded
/// median crossing reduction, aligned-median coordinates, waypoints
/// through rank gaps. Deterministic: same graph, same `Layout`.
///
/// Cycles are broken by the documented DFS heuristic and MARKED
/// ([`EdgeLayout::broken`]); disconnected components lay out side by
/// side along the cross axis.
pub fn layered(desc: &GraphDesc, opts: &LayeredOpts) -> Layout {
    let mut resolved = Resolved::new(desc);
    let notes = resolved.notes();
    let n = resolved.len();

    if n > opts.node_cap {
        let mut all = vec![format!(
            "node cap exceeded ({n} > {}); grid placement fallback",
            opts.node_cap
        )];
        all.extend(notes);
        return grid_with_notes(desc, all);
    }
    if n == 0 {
        return assemble(Vec::new(), Vec::new(), notes);
    }

    let dir = opts.direction;
    let node_gap = f64::from(opts.node_gap.max(1));
    let rank_gap = f64::from(opts.rank_gap.max(1));

    resolved.break_cycles();
    let rank = longest_path_ranks(&resolved);
    let comp_of = resolved.components();
    let comp_count = comp_of.iter().copied().max().map_or(0, |c| c + 1);

    let mut node_out: Vec<Option<NodeLayout>> = (0..n).map(|_| None).collect();
    let mut edge_out: Vec<(usize, EdgeLayout)> = Vec::new();
    let mut running_cross = 0.0f64;
    let comp_gap = node_gap * 2.0;

    for comp in 0..comp_count {
        let locals: Vec<usize> = (0..n).filter(|&g| comp_of[g] == comp).collect();
        let piece = layout_component(
            &resolved,
            &rank,
            &locals,
            dir,
            node_gap,
            rank_gap,
            opts.sweeps,
        );

        // Pack components side by side along the cross axis.
        let offset = running_cross - piece.min_cross;
        running_cross += piece.cross_span() + comp_gap;

        for (g, cross, flow) in piece.node_places {
            let size = resolved.sizes[g];
            let rect = map_rect(dir, cross + offset, flow, size);
            node_out[g] = Some(NodeLayout::new(resolved.id(desc, g), rect, rank[g]));
        }
        for poly in piece.edge_polylines {
            let waypoints = poly
                .points
                .into_iter()
                .map(|(c, f)| map_point(dir, c + offset, f))
                .collect();
            let e = &desc.edges[poly.desc_index];
            let mut el = EdgeLayout::new(e.from.clone(), e.to.clone(), poly.desc_index, waypoints);
            el.broken = poly.broken;
            edge_out.push((poly.desc_index, el));
        }
    }

    let nodes: Vec<NodeLayout> = node_out.into_iter().flatten().collect();

    // Self-edges: a lobe on the card's right face, built from the final
    // rect so all directions share one code path.
    for &(g, desc_index) in &resolved.self_edges {
        let rect = nodes[g].rect;
        let e = &desc.edges[desc_index];
        edge_out.push((
            desc_index,
            EdgeLayout::new(e.from.clone(), e.to.clone(), desc_index, self_loop(rect)),
        ));
    }

    edge_out.sort_by_key(|(i, _)| *i);
    let edges = edge_out.into_iter().map(|(_, e)| e).collect();
    assemble(nodes, edges, notes)
}

/// Longest-path ranks over the broken-cycle DAG orientation (Kahn, FIFO
/// seeded in input order — deterministic). Ranks strictly increase
/// along every oriented edge.
fn longest_path_ranks(resolved: &Resolved) -> Vec<usize> {
    let n = resolved.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for e in &resolved.edges {
        let (u, v) = oriented(e.from, e.to, e.broken);
        out[u].push(v);
        indeg[v] += 1;
    }
    let mut rank = vec![0usize; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    while let Some(u) = queue.pop_front() {
        for &v in &out[u] {
            rank[v] = rank[v].max(rank[u] + 1);
            indeg[v] -= 1;
            if indeg[v] == 0 {
                queue.push_back(v);
            }
        }
    }
    rank
}

const fn oriented(from: usize, to: usize, broken: bool) -> (usize, usize) {
    if broken {
        (to, from)
    } else {
        (from, to)
    }
}

/// One edge's polyline in (cross, flow) space, pre-packing.
struct EdgePolyline {
    desc_index: usize,
    broken: bool,
    points: Vec<(f64, f64)>,
}

/// One component's layout in (cross, flow) space, pre-packing.
struct ComponentPiece {
    /// (global local node id, cross start, flow start).
    node_places: Vec<(usize, f64, f64)>,
    edge_polylines: Vec<EdgePolyline>,
    min_cross: f64,
    max_cross: f64,
}

impl ComponentPiece {
    fn cross_span(&self) -> f64 {
        (self.max_cross - self.min_cross).max(0.0)
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_component(
    resolved: &Resolved,
    rank: &[usize],
    locals: &[usize],
    dir: Direction,
    node_gap: f64,
    rank_gap: f64,
    sweeps: u32,
) -> ComponentPiece {
    // Member space: 0..locals.len() are this component's real nodes (in
    // input order); dummies for multi-rank edges follow.
    let mut member_of = HashMap::with_capacity(locals.len());
    for (m, &g) in locals.iter().enumerate() {
        member_of.insert(g, m);
    }
    let rank_count = locals.iter().map(|&g| rank[g] + 1).max().unwrap_or(1);

    let mut cross_ext: Vec<f64> = locals
        .iter()
        .map(|&g| cross_extent(dir, resolved.sizes[g]))
        .collect();
    let mut flow_ext: Vec<f64> = locals
        .iter()
        .map(|&g| flow_extent(dir, resolved.sizes[g]))
        .collect();

    // Edge chains: [source member, dummies.., target member] in the
    // oriented (rank-increasing) direction.
    struct Chain {
        desc_index: usize,
        broken: bool,
        members: Vec<usize>,
        u: usize,
        v: usize,
    }
    let mut chains: Vec<Chain> = Vec::new();
    let mut dummy_ranks: Vec<usize> = Vec::new();
    for e in &resolved.edges {
        let (gu, gv) = oriented(e.from, e.to, e.broken);
        let (Some(&mu), Some(&mv)) = (member_of.get(&gu), member_of.get(&gv)) else {
            continue; // edge belongs to another component
        };
        let mut members = vec![mu];
        for r in (rank[gu] + 1)..rank[gv] {
            let d = locals.len() + dummy_ranks.len();
            dummy_ranks.push(r);
            cross_ext.push(1.0);
            flow_ext.push(1.0);
            members.push(d);
        }
        members.push(mv);
        chains.push(Chain {
            desc_index: e.desc_index,
            broken: e.broken,
            members,
            u: mu,
            v: mv,
        });
    }

    // Rank structure: reals in input order, then dummies in creation
    // order; adjacency from chain segments.
    let member_count = cross_ext.len();
    let mut ranks: Vec<Vec<usize>> = vec![Vec::new(); rank_count];
    for (m, &g) in locals.iter().enumerate() {
        ranks[rank[g]].push(m);
    }
    for (i, &r) in dummy_ranks.iter().enumerate() {
        ranks[r].push(locals.len() + i);
    }
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); member_count];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); member_count];
    for chain in &chains {
        for pair in chain.members.windows(2) {
            down[pair[0]].push(pair[1]);
            up[pair[1]].push(pair[0]);
        }
    }
    let mut rs = RankStructure { ranks, up, down };
    rs.reduce_crossings(sweeps);
    let coords = assign(&rs, &cross_ext, &flow_ext, node_gap, rank_gap);

    let member_rank = {
        let mut mr = vec![0usize; member_count];
        for (r, members) in rs.ranks.iter().enumerate() {
            for &m in members {
                mr[m] = r;
            }
        }
        mr
    };
    let center = |m: usize| coords.cross[m] + cross_ext[m] / 2.0;

    // Parallel-edge anchor spreading: edges sharing an oriented member
    // pair fan out around the shared centers so duplicates stay
    // distinguishable. Maps are lookup-only (determinism).
    let mut pair_total: HashMap<(usize, usize), usize> = HashMap::new();
    for chain in &chains {
        *pair_total.entry((chain.u, chain.v)).or_insert(0) += 1;
    }
    let mut pair_seen: HashMap<(usize, usize), usize> = HashMap::new();

    let mut node_places = Vec::with_capacity(locals.len());
    for (m, &g) in locals.iter().enumerate() {
        node_places.push((g, coords.cross[m], coords.flow_start[member_rank[m]]));
    }

    let mut edge_polylines = Vec::with_capacity(chains.len());
    for chain in &chains {
        let total = pair_total[&(chain.u, chain.v)];
        let ordinal = pair_seen.entry((chain.u, chain.v)).or_insert(0);
        let k = *ordinal;
        *ordinal += 1;
        let spread = if total > 1 {
            let raw = (2 * k) as f64 - (total - 1) as f64;
            let limit = ((cross_ext[chain.u].min(cross_ext[chain.v])) / 2.0 - 0.5).max(0.0);
            raw.clamp(-limit, limit)
        } else {
            0.0
        };

        let mut points = Vec::with_capacity(chain.members.len());
        let ru = member_rank[chain.u];
        points.push((
            center(chain.u) + spread,
            coords.flow_start[ru] + flow_ext[chain.u],
        ));
        for &d in &chain.members[1..chain.members.len() - 1] {
            let rd = member_rank[d];
            points.push((center(d), coords.flow_start[rd] + coords.band_ext[rd] / 2.0));
        }
        let rv = member_rank[chain.v];
        points.push((center(chain.v) + spread, coords.flow_start[rv] - 1.0));

        if chain.broken {
            // Present the polyline in original from -> to order: the
            // chain was computed on the reversed orientation.
            points.reverse();
        }
        edge_polylines.push(EdgePolyline {
            desc_index: chain.desc_index,
            broken: chain.broken,
            points,
        });
    }

    let min_cross = (0..member_count)
        .map(|m| coords.cross[m])
        .fold(f64::INFINITY, f64::min);
    let max_cross = (0..member_count)
        .map(|m| coords.cross[m] + cross_ext[m])
        .fold(f64::NEG_INFINITY, f64::max);
    ComponentPiece {
        node_places,
        edge_polylines,
        min_cross,
        max_cross,
    }
}
