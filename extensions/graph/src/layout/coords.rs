//! Coordinate assignment after ordering: aligned-median cross
//! positions and flow bands per rank.
//!
//! Deliberately lite (the item prices full Brandes-Koepf out for
//! cell-quantized terminals): initial packed placement, then a bounded
//! number of median alignment passes. Each pass computes every
//! member's desired start (median of its neighbor centers) and packs
//! the rank optimally: minimizing squared deviation from the desired
//! starts under minimum-separation constraints is exactly isotonic
//! regression after the cumulative-width change of variables, solved
//! by pool-adjacent-violators. Colliding siblings therefore center as
//! a block on their mean (the diamond stays symmetric) instead of
//! drifting rightward as naive clamp scans do.

use super::ordering::RankStructure;

/// Cross positions per member and flow bands per rank.
pub(crate) struct Coords {
    /// Member id -> cross-axis start coordinate.
    pub cross: Vec<f64>,
    /// Rank -> flow-axis start of the rank band.
    pub flow_start: Vec<f64>,
    /// Rank -> flow extent of the band (max member flow extent).
    pub band_ext: Vec<f64>,
}

/// Alignment passes: down, up, down (bounded by construction).
const ALIGN_PASSES: [bool; 3] = [true, false, true];

pub(crate) fn assign(
    rs: &RankStructure,
    cross_ext: &[f64],
    flow_ext: &[f64],
    node_gap: f64,
    rank_gap: f64,
) -> Coords {
    let member_count = cross_ext.len();
    let mut cross = vec![0.0f64; member_count];

    // Initial placement: pack each rank left to right.
    for rank in &rs.ranks {
        let mut cur = 0.0;
        for &m in rank {
            cross[m] = cur;
            cur += cross_ext[m] + node_gap;
        }
    }

    // Median alignment. A member with neighbors wants its center on
    // their median center; the left-to-right scan clamps to minimum
    // separation from the member already placed on its left.
    for &downward in ALIGN_PASSES.iter() {
        let rank_count = rs.ranks.len();
        let indices: Vec<usize> = if downward {
            (1..rank_count).collect()
        } else {
            (0..rank_count.saturating_sub(1)).rev().collect()
        };
        for r in indices {
            let neighbors = if downward { &rs.up } else { &rs.down };
            let members = &rs.ranks[r];
            let wants: Vec<f64> = members
                .iter()
                .map(|&m| {
                    median_center(&neighbors[m], &cross, cross_ext)
                        .map_or(cross[m], |c| c - cross_ext[m] / 2.0)
                })
                .collect();
            let exts: Vec<f64> = members.iter().map(|&m| cross_ext[m]).collect();
            let starts = pack_rank(&wants, &exts, node_gap);
            for (&m, start) in members.iter().zip(starts) {
                cross[m] = start;
            }
        }
    }

    // Flow bands: each rank is as tall (along flow) as its tallest
    // member; bands are separated by the rank gap.
    let mut flow_start = Vec::with_capacity(rs.ranks.len());
    let mut band_ext = Vec::with_capacity(rs.ranks.len());
    let mut cur = 0.0f64;
    for rank in &rs.ranks {
        let ext = rank.iter().map(|&m| flow_ext[m]).fold(0.0f64, f64::max);
        flow_start.push(cur);
        band_ext.push(ext);
        cur += ext + rank_gap;
    }

    Coords {
        cross,
        flow_start,
        band_ext,
    }
}

/// Optimal rank packing: place members (in order) at least `gap`
/// apart, minimizing total squared deviation from their desired
/// starts. With `t_i = start_i - offset_i` (offset = cumulative widths
/// and gaps), the separation constraint becomes `t` nondecreasing and
/// the problem is isotonic regression — solved exactly by
/// pool-adjacent-violators with mean pooling. Deterministic, O(rank).
fn pack_rank(wants: &[f64], exts: &[f64], gap: f64) -> Vec<f64> {
    let count = wants.len();
    let mut offset = 0.0f64;
    // (sum of adjusted wants, member count) per block.
    let mut blocks: Vec<(f64, f64)> = Vec::with_capacity(count);
    for i in 0..count {
        let w = wants[i] - offset;
        offset += exts[i] + gap;
        blocks.push((w, 1.0));
        while blocks.len() >= 2 {
            let cur = blocks[blocks.len() - 1];
            let prev = blocks[blocks.len() - 2];
            if prev.0 / prev.1 > cur.0 / cur.1 {
                blocks.pop();
                let last = blocks.last_mut().expect("two blocks checked");
                last.0 += cur.0;
                last.1 += cur.1;
            } else {
                break;
            }
        }
    }
    let mut t = Vec::with_capacity(count);
    for &(sum, n) in &blocks {
        let mean = sum / n;
        for _ in 0..(n as usize) {
            t.push(mean);
        }
    }
    let mut offset = 0.0f64;
    (0..count)
        .map(|i| {
            let start = t[i] + offset;
            offset += exts[i] + gap;
            start
        })
        .collect()
}

/// Median of the neighbor CENTER coordinates (mean of the middles for
/// even counts), or `None` for isolated members.
fn median_center(neighbors: &[usize], cross: &[f64], cross_ext: &[f64]) -> Option<f64> {
    if neighbors.is_empty() {
        return None;
    }
    let mut centers: Vec<f64> = neighbors
        .iter()
        .map(|&m| cross[m] + cross_ext[m] / 2.0)
        .collect();
    centers.sort_by(f64::total_cmp);
    let mid = centers.len() / 2;
    if centers.len() % 2 == 1 {
        Some(centers[mid])
    } else {
        Some((centers[mid - 1] + centers[mid]) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two siblings wanting the same spot split it symmetrically (the
    /// isotonic block centers on the shared desire) instead of the
    /// first taking it and the second drifting right.
    #[test]
    fn colliding_siblings_center_as_a_block() {
        let starts = pack_rank(&[0.0, 0.0], &[8.0, 8.0], 3.0);
        assert_eq!(starts, vec![-5.5, 5.5]);
        // Non-colliding wants are honored exactly.
        let free = pack_rank(&[0.0, 40.0], &[8.0, 8.0], 3.0);
        assert_eq!(free, vec![0.0, 40.0]);
    }

    /// A two-rank chain aligns the child's center on the parent's.
    #[test]
    fn chain_aligns_centers() {
        let rs = RankStructure {
            ranks: vec![vec![0], vec![1]],
            up: vec![vec![], vec![0]],
            down: vec![vec![1], vec![]],
        };
        let coords = assign(&rs, &[10.0, 4.0], &[3.0, 3.0], 3.0, 2.0);
        let parent_center = coords.cross[0] + 5.0;
        let child_center = coords.cross[1] + 2.0;
        assert!((parent_center - child_center).abs() < 1e-9);
        assert_eq!(coords.flow_start, vec![0.0, 5.0]);
        assert_eq!(coords.band_ext, vec![3.0, 3.0]);
    }
}
