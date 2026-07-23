//! The force pass (v1.5): the knowledge-graph path.
//!
//! An alpha-cooled placement ACT, not an animation state: repulsion
//! between all card pairs (scaled by card radii), springs along edges,
//! an optional rank bias along a flow axis. The pass runs a bounded
//! iteration budget on demand, freezes as soon as the system settles,
//! and is bit-deterministic under a fixed seed + budget: the arithmetic
//! is restricted to IEEE-exact operations (+ - * / sqrt — no
//! transcendentals), iteration order is input order, and the PRNG is a
//! hand-rolled splitmix64 stream.
//!
//! Zero-idle is the CALLER's story: cache the returned [`Layout`] and
//! re-render from it; re-run the pass only on graph mutation or an
//! explicit re-layout request. Rendering never re-simulates.

use crate::desc::{Direction, GraphDesc};

use super::geom::{clip_border, self_loop};
use super::resolve::Resolved;
use super::{assemble, EdgeLayout, Layout, NodeLayout};

use abstracttui::base::Rect;

/// Iteration budget for [`force`] (contract spelling from the 0440
/// item; a plain count — one iteration is one full force/integrate
/// step over all nodes).
pub type IterationBudget = u32;

/// Options for [`force`]. Author-written, shape-stable: construct via
/// functional record update over `Default` (ADR-0003 §2):
///
/// ```
/// use abstracttui_graph::{Direction, ForceOpts};
/// let opts = ForceOpts {
///     seed: 7,
///     rank_bias: Some(Direction::TopDown),
///     ..Default::default()
/// };
/// # let _ = opts;
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ForceOpts {
    /// PRNG seed for the initial scatter. Same seed + same budget +
    /// same graph = identical `Layout` (golden-pinned).
    pub seed: u64,
    /// Iteration bound (default 256). The pass may freeze earlier when
    /// the system settles; it never runs longer.
    pub budget: IterationBudget,
    /// Optional flow tendency: edges are nudged so targets sit
    /// downstream of sources along the given direction's flow axis.
    /// `None` (default) lays the graph out isotropically.
    pub rank_bias: Option<Direction>,
    /// Characteristic spacing in cells (default 14): edge rest length
    /// and repulsion range both derive from it, plus the two cards'
    /// radii.
    pub spacing: f64,
}

impl Default for ForceOpts {
    fn default() -> Self {
        ForceOpts {
            seed: 0xAB57_AC77,
            budget: 256,
            rank_bias: None,
            spacing: 14.0,
        }
    }
}

/// Displacement threshold under which the system counts as settled.
const SETTLE_EPSILON: f64 = 0.05;
/// Spring constant (pull per cell of length error).
const SPRING: f64 = 0.08;
/// Rank-bias correction factor per cell of shortfall.
const BIAS: f64 = 0.05;

/// Force-directed layout: bounded, seeded, alpha-cooled; freezes on
/// settle. Computes no hierarchy — every node reports rank 0, cycles
/// need no breaking (springs are direction-blind), and `broken` is
/// false on every edge.
pub fn force(desc: &GraphDesc, opts: &ForceOpts) -> Layout {
    run(desc, opts).0
}

/// The pass plus the number of iterations actually run (internal:
/// unit tests pin the freeze-on-settle behavior through this).
pub(crate) fn run(desc: &GraphDesc, opts: &ForceOpts) -> (Layout, u32) {
    let resolved = Resolved::new(desc);
    let notes = resolved.notes();
    let n = resolved.len();
    if n == 0 {
        return (assemble(Vec::new(), Vec::new(), notes), 0);
    }

    let spacing = if opts.spacing > 0.0 {
        opts.spacing
    } else {
        14.0
    };
    let radius: Vec<f64> = resolved
        .sizes
        .iter()
        .map(|s| (f64::from(s.w) + f64::from(s.h)) / 4.0)
        .collect();

    // Seeded scatter in a box sized by the total card area (direct
    // rectangular draw — no polar coordinates, no transcendentals).
    let mut rng = SplitMix64::new(opts.seed);
    let area: f64 = resolved
        .sizes
        .iter()
        .map(|s| f64::from(s.w) * f64::from(s.h))
        .sum();
    let side = area.sqrt() * 2.0 + spacing;
    let mut px: Vec<f64> = Vec::with_capacity(n);
    let mut py: Vec<f64> = Vec::with_capacity(n);
    for _ in 0..n {
        px.push(rng.next_f64() * side);
        py.push(rng.next_f64() * side);
    }

    let (bias_vertical, bias_sign) = match opts.rank_bias {
        Some(d) => (d.is_vertical(), if d.is_reversed() { -1.0 } else { 1.0 }),
        None => (true, 0.0),
    };

    let mut ran = 0u32;
    let mut dx = vec![0.0f64; n];
    let mut dy = vec![0.0f64; n];
    for it in 0..opts.budget {
        ran = it + 1;
        let t = 1.0 - f64::from(it) / f64::from(opts.budget.max(1));
        let step_cap = spacing * (0.1 + 0.9 * t);
        dx.iter_mut().for_each(|v| *v = 0.0);
        dy.iter_mut().for_each(|v| *v = 0.0);

        // Repulsion between all pairs, scaled so bigger cards keep more
        // distance. Coincident points separate along a deterministic,
        // index-derived nudge.
        for i in 0..n {
            for j in (i + 1)..n {
                let (mut ex, mut ey) = (px[i] - px[j], py[i] - py[j]);
                let mut d2 = ex * ex + ey * ey;
                if d2 < 1e-4 {
                    ex = 0.01 * ((i + 1) as f64);
                    ey = 0.013 * ((j + 1) as f64);
                    d2 = ex * ex + ey * ey;
                }
                let k = spacing + radius[i] + radius[j];
                let f = (k * k) / d2;
                dx[i] += ex * f / 8.0;
                dy[i] += ey * f / 8.0;
                dx[j] -= ex * f / 8.0;
                dy[j] -= ey * f / 8.0;
            }
        }

        // Springs along edges toward their rest length.
        for e in &resolved.edges {
            let (ex, ey) = (px[e.to] - px[e.from], py[e.to] - py[e.from]);
            let d = (ex * ex + ey * ey).sqrt().max(1e-3);
            let rest = spacing + radius[e.from] + radius[e.to];
            let f = (d - rest) * SPRING;
            let (ux, uy) = (ex / d, ey / d);
            dx[e.from] += ux * f;
            dy[e.from] += uy * f;
            dx[e.to] -= ux * f;
            dy[e.to] -= uy * f;
        }

        // Rank bias: pull each edge's target downstream of its source
        // along the flow axis when it falls short. Accumulates into the
        // displacement like every other force, so the step cap and the
        // settle measurement govern it too.
        if bias_sign != 0.0 {
            for e in &resolved.edges {
                let want = (spacing + radius[e.from] + radius[e.to]) * 0.6;
                let actual = if bias_vertical {
                    (py[e.to] - py[e.from]) * bias_sign
                } else {
                    (px[e.to] - px[e.from]) * bias_sign
                };
                let short = want - actual;
                if short > 0.0 {
                    let adj = short * BIAS * bias_sign;
                    if bias_vertical {
                        dy[e.to] += adj;
                        dy[e.from] -= adj;
                    } else {
                        dx[e.to] += adj;
                        dx[e.from] -= adj;
                    }
                }
            }
        }

        // Integrate with the cooled step cap; freeze on settle.
        let mut max_step = 0.0f64;
        for i in 0..n {
            let len = (dx[i] * dx[i] + dy[i] * dy[i]).sqrt();
            if len > 1e-12 {
                let step = len.min(step_cap);
                px[i] += dx[i] / len * step;
                py[i] += dy[i] / len * step;
                max_step = max_step.max(step);
            }
        }
        if max_step < SETTLE_EPSILON {
            break;
        }
    }

    // Snap card centers to cells.
    let mut nodes = Vec::with_capacity(n);
    for g in 0..n {
        let size = resolved.sizes[g];
        let x = (px[g] - f64::from(size.w) / 2.0).round() as i32;
        let y = (py[g] - f64::from(size.h) / 2.0).round() as i32;
        nodes.push(NodeLayout::new(
            resolved.id(desc, g),
            Rect::new(x, y, size.w, size.h),
            0,
        ));
    }

    let center = |r: Rect| {
        (
            f64::from(r.x) + f64::from(r.w) / 2.0,
            f64::from(r.y) + f64::from(r.h) / 2.0,
        )
    };
    let mut edge_out: Vec<(usize, EdgeLayout)> = Vec::new();
    for e in &resolved.edges {
        let (ra, rb) = (nodes[e.from].rect, nodes[e.to].rect);
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
    (assemble(nodes, edges, notes), ran)
}

/// splitmix64: tiny, seedable, std-only, bit-stable everywhere.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) from the top 53 bits.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::GraphDesc;

    #[test]
    fn single_node_settles_immediately() {
        let desc = GraphDesc::new().node("only", 6, 3);
        let (_, ran) = run(&desc, &ForceOpts::default());
        assert_eq!(ran, 1, "no forces, first iteration settles");
    }

    #[test]
    fn pair_freezes_well_under_a_large_budget() {
        let desc = GraphDesc::new()
            .node("a", 6, 3)
            .node("b", 6, 3)
            .edge("a", "b");
        let opts = ForceOpts {
            budget: 10_000,
            ..Default::default()
        };
        let (_, ran) = run(&desc, &opts);
        assert!(
            ran < 2_000,
            "spring/repulsion equilibrium settles early, ran {ran}"
        );
    }

    #[test]
    fn prng_streams_are_seed_stable_and_in_range() {
        let mut rng = SplitMix64::new(42);
        let mut rng2 = SplitMix64::new(42);
        let a: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
        let b: Vec<u64> = (0..8).map(|_| rng2.next_u64()).collect();
        assert_eq!(a, b, "same seed, same stream");
        let mut rng3 = SplitMix64::new(43);
        assert_ne!(a[0], rng3.next_u64(), "different seed, different stream");
        for _ in 0..64 {
            let f = rng3.next_f64();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
