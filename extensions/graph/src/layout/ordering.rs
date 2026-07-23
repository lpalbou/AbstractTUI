//! Crossing reduction: bounded median sweeps over a rank structure.
//!
//! Members are real nodes and dummy waypoint nodes alike; the median
//! heuristic reorders each rank by the median position of its
//! neighbors in the adjacent rank, alternating downward and upward
//! passes. Sweeps are BOUNDED (the pass never chases an optimum), the
//! best ordering seen (by counted crossings) wins, and every tiebreak
//! is stable — same graph, same order.

/// Rank structure under reduction. Member ids `0..n` are real nodes,
/// `n..` are dummies; adjacency lists are built in edge input order.
pub(crate) struct RankStructure {
    /// Member ids per rank, in current left-to-right order.
    pub ranks: Vec<Vec<usize>>,
    /// Member id -> up-rank neighbor member ids (edge input order).
    pub up: Vec<Vec<usize>>,
    /// Member id -> down-rank neighbor member ids (edge input order).
    pub down: Vec<Vec<usize>>,
}

impl RankStructure {
    /// Current position of every member within its rank.
    pub fn positions(&self) -> Vec<usize> {
        let count = self.up.len();
        let mut pos = vec![0usize; count];
        for rank in &self.ranks {
            for (i, &m) in rank.iter().enumerate() {
                pos[m] = i;
            }
        }
        pos
    }

    /// Total edge crossings between all adjacent rank pairs, counted
    /// pairwise per gap (fine at the documented node cap).
    pub fn crossings(&self) -> usize {
        let pos = self.positions();
        let mut total = 0usize;
        for upper in self.ranks.iter().take(self.ranks.len().saturating_sub(1)) {
            // Segments (upper position, lower position) in stable order.
            let mut segs: Vec<(usize, usize)> = Vec::new();
            for &m in upper {
                for &d in &self.down[m] {
                    segs.push((pos[m], pos[d]));
                }
            }
            for (i, a) in segs.iter().enumerate() {
                for b in segs.iter().skip(i + 1) {
                    // Two segments cross iff their endpoint orders invert
                    // strictly on both axes (shared endpoints never cross).
                    if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                        total += 1;
                    }
                }
            }
        }
        total
    }

    /// Run up to `sweeps` full (down + up) median passes, keeping the
    /// best ordering seen and stopping early at zero crossings.
    pub fn reduce_crossings(&mut self, sweeps: u32) {
        let mut best = self.ranks.clone();
        let mut best_crossings = self.crossings();
        for _ in 0..sweeps {
            if best_crossings == 0 {
                break;
            }
            self.median_pass(true);
            self.median_pass(false);
            let now = self.crossings();
            if now < best_crossings {
                best_crossings = now;
                best = self.ranks.clone();
            }
        }
        self.ranks = best;
    }

    /// One median pass. `downward` orders each rank by the median
    /// position of its up-neighbors (top to bottom); otherwise by
    /// down-neighbors (bottom to top). Members without neighbors keep
    /// their current position as the sort key; the sort is stable.
    fn median_pass(&mut self, downward: bool) {
        let rank_count = self.ranks.len();
        let indices: Vec<usize> = if downward {
            (1..rank_count).collect()
        } else {
            (0..rank_count.saturating_sub(1)).rev().collect()
        };
        for r in indices {
            let pos = self.positions();
            let neighbors = if downward { &self.up } else { &self.down };
            let mut keyed: Vec<(f64, usize)> = self.ranks[r]
                .iter()
                .enumerate()
                .map(|(i, &m)| {
                    let key = median_position(&neighbors[m], &pos).unwrap_or(i as f64);
                    (key, m)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            self.ranks[r] = keyed.into_iter().map(|(_, m)| m).collect();
        }
    }
}

/// Median of the neighbor positions (mean of the two middles for even
/// counts), or `None` for isolated members.
fn median_position(neighbors: &[usize], pos: &[usize]) -> Option<f64> {
    if neighbors.is_empty() {
        return None;
    }
    let mut ps: Vec<usize> = neighbors.iter().map(|&m| pos[m]).collect();
    ps.sort_unstable();
    let mid = ps.len() / 2;
    if ps.len() % 2 == 1 {
        Some(ps[mid] as f64)
    } else {
        Some((ps[mid - 1] as f64 + ps[mid] as f64) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two ranks, two edges drawn crossed: [a, b] over [x, y] with
    /// a->y and b->x. One median sweep must untangle them.
    #[test]
    fn median_sweep_untangles_a_crossing() {
        let mut rs = RankStructure {
            ranks: vec![vec![0, 1], vec![2, 3]],
            up: vec![vec![], vec![], vec![1], vec![0]],
            down: vec![vec![3], vec![2], vec![], vec![]],
        };
        assert_eq!(rs.crossings(), 1);
        rs.reduce_crossings(4);
        assert_eq!(rs.crossings(), 0);
    }

    #[test]
    fn reduction_never_increases_crossings() {
        // A straight ladder is already optimal; reduction must keep it.
        let mut rs = RankStructure {
            ranks: vec![vec![0, 1], vec![2, 3]],
            up: vec![vec![], vec![], vec![0], vec![1]],
            down: vec![vec![2], vec![3], vec![], vec![]],
        };
        assert_eq!(rs.crossings(), 0);
        let before = rs.ranks.clone();
        rs.reduce_crossings(4);
        assert_eq!(rs.crossings(), 0);
        assert_eq!(rs.ranks, before, "optimal order is stable");
    }
}
