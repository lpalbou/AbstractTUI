//! Integer flex arithmetic. Terminals have no fractional cells, so all
//! distribution happens in integers with LARGEST-REMAINDER rounding:
//! floor everyone's real share, then hand the leftover cells to the
//! largest fractional remainders (ties -> lowest index). This guarantees
//! `sum(shares) == total` exactly — no lost or invented columns — and is
//! deterministic, which the damage/diff layers depend on.

/// Split `total` (>= 0) proportionally to `weights`. Zero/negative
/// weights get nothing. Returns per-index integer shares summing to
/// `total` (or all zeros when no positive weight exists).
pub(crate) fn distribute(total: i32, weights: &[f64]) -> Vec<i32> {
    let n = weights.len();
    let mut shares = vec![0i32; n];
    if total <= 0 || n == 0 {
        return shares;
    }
    let sum: f64 = weights.iter().filter(|w| **w > 0.0).sum();
    if sum <= 0.0 {
        return shares;
    }
    let mut remainders: Vec<(usize, f64)> = Vec::with_capacity(n);
    let mut used: i64 = 0;
    for (i, &w) in weights.iter().enumerate() {
        if w <= 0.0 {
            continue;
        }
        let exact = total as f64 * (w / sum);
        let floor = exact.floor();
        shares[i] = floor as i32;
        used += floor as i64;
        remainders.push((i, exact - floor));
    }
    let mut leftover = (total as i64 - used) as i32;
    // Largest remainder first; ties broken by lower index so the result
    // is stable across runs and platforms (f64 math here is exact enough
    // for cell counts; the tie-break absorbs any equality).
    remainders.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    for (i, _) in remainders {
        if leftover <= 0 {
            break;
        }
        shares[i] += 1;
        leftover -= 1;
    }
    shares
}

/// One flow child's inputs to the main-axis solve.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FlexItem {
    /// Hypothetical main size before flexing (already min/max clamped).
    pub basis: i32,
    pub min: i32,
    pub max: i32,
    pub grow: f64,
    pub shrink: f64,
}

/// Resolve final main-axis sizes for one line. `available` is the space
/// for the children themselves (gaps and margins already subtracted).
///
/// Freeze loop (the flexbox resolution algorithm, integer edition): grow
/// or shrink the unfrozen items by weight, clamp violators to their
/// min/max, freeze them, redistribute what remains. Each pass freezes at
/// least one item or ends, so it terminates in <= n passes.
pub(crate) fn resolve_main_sizes(items: &[FlexItem], available: i32) -> Vec<i32> {
    let n = items.len();
    let mut sizes: Vec<i32> = items
        .iter()
        .map(|i| i.basis.clamp(i.min, i.max.max(i.min)))
        .collect();
    if n == 0 {
        return sizes;
    }
    let mut frozen = vec![false; n];
    loop {
        let used: i32 = sizes.iter().sum();
        let free = available - used;
        // Pick the active set and weights for this pass.
        let weights: Vec<f64> = (0..n)
            .map(|i| {
                if frozen[i] {
                    0.0
                } else if free > 0 {
                    items[i].grow
                } else {
                    // CSS scaled shrink factor: bigger items give up more.
                    items[i].shrink * items[i].basis.max(0) as f64
                }
            })
            .collect();
        if free == 0 || weights.iter().all(|w| *w <= 0.0) {
            break;
        }
        let deltas = distribute(free.abs(), &weights);
        let mut violated = false;
        for i in 0..n {
            if deltas[i] == 0 {
                continue;
            }
            let target = if free > 0 {
                sizes[i] + deltas[i]
            } else {
                sizes[i] - deltas[i]
            };
            let clamped = target.clamp(items[i].min.max(0), items[i].max.max(items[i].min.max(0)));
            if clamped != target {
                violated = true;
                frozen[i] = true;
            }
            sizes[i] = clamped;
        }
        if !violated {
            break; // clean distribution: done, sums match exactly
        }
        // Some item hit a bound: it keeps its clamped size (frozen) and
        // the loop redistributes the residue among the survivors.
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_remainder_is_exact_and_stable() {
        // 10 cells over three equal weights: 4/3/3, first index wins ties.
        assert_eq!(distribute(10, &[1.0, 1.0, 1.0]), vec![4, 3, 3]);
        // Skewed weights still sum exactly: exact shares 3.5/1.75/1.75,
        // floors 3/1/1, the two leftover cells go to the larger (0.75)
        // remainders.
        let shares = distribute(7, &[2.0, 1.0, 1.0]);
        assert_eq!(shares.iter().sum::<i32>(), 7);
        assert_eq!(shares, vec![3, 2, 2]);
        // Zero weights excluded entirely.
        assert_eq!(distribute(5, &[0.0, 1.0]), vec![0, 5]);
        assert_eq!(distribute(5, &[0.0, 0.0]), vec![0, 0]);
    }

    #[test]
    fn grow_respects_max_and_redistributes() {
        let items = [
            FlexItem {
                basis: 0,
                min: 0,
                max: 3,
                grow: 1.0,
                shrink: 1.0,
            },
            FlexItem {
                basis: 0,
                min: 0,
                max: i32::MAX,
                grow: 1.0,
                shrink: 1.0,
            },
        ];
        let sizes = resolve_main_sizes(&items, 10);
        // First capped at 3; the residue flows to the second.
        assert_eq!(sizes, vec![3, 7]);
        assert_eq!(sizes.iter().sum::<i32>(), 10);
    }

    #[test]
    fn shrink_respects_min() {
        let items = [
            FlexItem {
                basis: 8,
                min: 6,
                max: i32::MAX,
                grow: 0.0,
                shrink: 1.0,
            },
            FlexItem {
                basis: 8,
                min: 0,
                max: i32::MAX,
                grow: 0.0,
                shrink: 1.0,
            },
        ];
        // 16 desired into 10: first can only give 2 (min 6), second gives the rest.
        let sizes = resolve_main_sizes(&items, 10);
        assert_eq!(sizes, vec![6, 4]);
        assert_eq!(sizes.iter().sum::<i32>(), 10);
    }

    #[test]
    fn no_flex_leaves_sizes_alone() {
        let items = [
            FlexItem {
                basis: 4,
                min: 0,
                max: i32::MAX,
                grow: 0.0,
                shrink: 0.0,
            },
            FlexItem {
                basis: 2,
                min: 0,
                max: i32::MAX,
                grow: 0.0,
                shrink: 0.0,
            },
        ];
        assert_eq!(resolve_main_sizes(&items, 20), vec![4, 2]);
    }
}
