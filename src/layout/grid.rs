//! Track grid: `Cells`/`Percent`/`Auto`/`Fr` tracks, row-major
//! auto-placement with col/row spans, per-axis gaps, per-cell alignment.
//!
//! Semantics (a CSS-grid subset sized for terminal reality):
//! - Columns come from the container's `Display::Grid { cols, .. }`; an
//!   EMPTY cols list behaves as one full-width column.
//! - Track sizing order: `Cells` and `Percent` resolve directly; `Auto`
//!   fits the largest intrinsic size of the children whose placement
//!   STARTS in the track (span > 1 contributes `ceil(size / span)` — an
//!   approximation, deliberate); `Fr` shares the remaining space by
//!   weight, distributed largest-remainder so fr tracks tile EXACTLY
//!   (the same rounding law as flex grow; property-tested).
//! - Children auto-place row-major: first slot (left-to-right,
//!   top-to-bottom) with `col_span` consecutive free columns;
//!   `row_span` extends occupancy downward. Spans clamp to the column
//!   count.
//! - Rows: explicit `rows` specs first; rows beyond the spec are
//!   implicit `Auto`.
//! - Cell alignment: children fill their cell area (Stretch, the
//!   default). A child with explicit size or non-Stretch `align_self`
//!   sizes to content/explicit and aligns inside the cell —
//!   `align_self` drives the VERTICAL axis, `justify_self` does not
//!   exist yet (horizontal follows `align_self` too; a split is a
//!   later decision, documented).
//!
//! OWNER: REACT.

use crate::base::{Rect, Size};

use super::flex_math::distribute;
use super::solve::{clamp_axis, intrinsic_size, resolve_dim};
use super::style::{Align, Style, Track};
use super::tree::{LayoutId, LayoutTree};

struct Placement {
    id: LayoutId,
    col: usize,
    row: usize,
    col_span: usize,
    row_span: usize,
}

/// Resolve track sizes along one axis. `auto_fit[i]` is the intrinsic
/// requirement collected for track `i` (0 where nothing starts there).
fn resolve_tracks(tracks: &[Track], extent: i32, gap: i32, auto_fit: &[i32]) -> Vec<i32> {
    if tracks.is_empty() {
        return vec![extent.max(0)];
    }
    let gaps_total = gap * (tracks.len() as i32 - 1).max(0);
    let mut sizes: Vec<i32> = Vec::with_capacity(tracks.len());
    let mut weights: Vec<f64> = Vec::new();
    for (i, t) in tracks.iter().enumerate() {
        match t {
            Track::Cells(c) => sizes.push((*c).max(0)),
            Track::Percent(p) => sizes.push(((extent as f32) * p.clamp(0.0, 1.0)).round() as i32),
            Track::Auto => sizes.push(auto_fit.get(i).copied().unwrap_or(0).max(0)),
            Track::Fr(w) => {
                sizes.push(0); // filled below
                weights.push((*w).max(0.0) as f64);
            }
        }
    }
    if !weights.is_empty() && weights.iter().any(|w| *w > 0.0) {
        let fixed: i32 = sizes.iter().sum();
        let leftover = (extent - fixed - gaps_total).max(0);
        let fr_sizes = distribute(leftover, &weights);
        let mut fr_iter = fr_sizes.into_iter();
        for (i, t) in tracks.iter().enumerate() {
            if matches!(t, Track::Fr(_)) {
                sizes[i] = fr_iter.next().unwrap_or(0);
            }
        }
    }
    sizes
}

/// Prefix offsets for track starts given sizes and a gap.
fn offsets(base: i32, sizes: &[i32], gap: i32) -> Vec<i32> {
    let mut out = Vec::with_capacity(sizes.len());
    let mut cursor = base;
    for (i, s) in sizes.iter().enumerate() {
        out.push(cursor);
        cursor += s;
        if i + 1 < sizes.len() {
            cursor += gap;
        }
    }
    out
}

/// Extent covered by `span` tracks starting at `start`, including the
/// gaps BETWEEN them.
fn span_extent(sizes: &[i32], start: usize, span: usize, gap: i32) -> i32 {
    let end = (start + span.max(1)).min(sizes.len());
    let tracks: i32 = sizes[start..end].iter().sum();
    tracks + gap * (end.saturating_sub(start) as i32 - 1).max(0)
}

pub(super) fn layout_grid(tree: &mut LayoutTree, content: Rect, style: &Style, flow: &[LayoutId]) {
    let (cols_spec, rows_spec) = match &style.display {
        super::style::Display::Grid { cols, rows } => (cols.clone(), rows.clone()),
        _ => return,
    };
    let col_gap = style.gap.max(0);
    let row_gap = style.cross_gap.max(0);
    let ncols = cols_spec.len().max(1);

    // ---- auto-placement (row-major, SPARSE — the CSS default) -----------
    // A forward-only cursor: each child scans from where the previous
    // one left off and NEVER backfills earlier gaps (CSS grid sparse
    // packing). The cursor's monotonicity is also the complexity bound
    // (RT6 risk 9): total cells visited across ALL children is at most
    // the occupancy area = O(rows x cols), with rows bounded by
    // sum(row_span) — linear in the input, never O(children^2).
    let mut occupancy: Vec<Vec<bool>> = Vec::new();
    let ensure_row = |occ: &mut Vec<Vec<bool>>, r: usize| {
        while occ.len() <= r {
            occ.push(vec![false; ncols]);
        }
    };
    let mut placements: Vec<Placement> = Vec::with_capacity(flow.len());
    let (mut cur_row, mut cur_col) = (0usize, 0usize);
    for &child in flow {
        let cstyle = tree
            .nodes
            .get(child.0)
            .expect("grid child alive")
            .style
            .clone();
        let col_span = (cstyle.col_span.max(1) as usize).min(ncols);
        let row_span = cstyle.row_span.max(1) as usize;
        let (mut row, mut col) = (cur_row, cur_col);
        let placed = 'scan: loop {
            ensure_row(&mut occupancy, row + row_span - 1);
            'cols: while col + col_span <= ncols {
                for rr in 0..row_span {
                    for cc in 0..col_span {
                        if occupancy[row + rr][col + cc] {
                            col += 1;
                            continue 'cols;
                        }
                    }
                }
                break 'scan (row, col);
            }
            row += 1;
            col = 0;
        };
        let (row, col) = placed;
        for rr in 0..row_span {
            for cc in 0..col_span {
                occupancy[row + rr][col + cc] = true;
            }
        }
        // Sparse cursor: the next child starts searching AFTER this
        // one's slot on the same row.
        cur_row = row;
        cur_col = col + col_span;
        if cur_col >= ncols {
            cur_row += 1;
            cur_col = 0;
        }
        placements.push(Placement {
            id: child,
            col,
            row,
            col_span,
            row_span,
        });
    }
    let nrows = occupancy.len().max(1);

    // ---- intrinsic requirements for Auto tracks --------------------------
    // Column pass first (Auto column widths shape row heights).
    let mut col_fit = vec![0i32; ncols];
    for p in &placements {
        if !matches!(cols_spec.get(p.col), Some(Track::Auto)) {
            continue;
        }
        let est = intrinsic_size(tree, p.id, content.size());
        let per = (est.w + p.col_span as i32 - 1) / p.col_span.max(1) as i32;
        col_fit[p.col] = col_fit[p.col].max(per);
    }
    let col_sizes = resolve_tracks(&cols_spec, content.w, col_gap, &col_fit);

    // Row pass: explicit specs cover leading rows; implicit rows are
    // Auto. Fit against the CHILD'S resolved column width.
    let mut row_fit = vec![0i32; nrows];
    for p in &placements {
        let is_auto = match rows_spec.get(p.row) {
            Some(Track::Auto) => true,
            None => true, // implicit rows are Auto
            _ => false,
        };
        if !is_auto {
            continue;
        }
        let avail_w = span_extent(&col_sizes, p.col, p.col_span, col_gap);
        let est = intrinsic_size(tree, p.id, Size::new(avail_w, content.h));
        let per = (est.h + p.row_span as i32 - 1) / p.row_span.max(1) as i32;
        row_fit[p.row] = row_fit[p.row].max(per);
    }
    // Build the effective row spec (explicit + implicit Auto rows).
    let mut effective_rows: Vec<Track> = Vec::with_capacity(nrows);
    for r in 0..nrows {
        effective_rows.push(rows_spec.get(r).copied().unwrap_or(Track::Auto));
    }
    let row_sizes = resolve_tracks(&effective_rows, content.h, row_gap, &row_fit);

    // ---- assign rects (cell fill or aligned-in-cell) ---------------------
    let col_offsets = offsets(content.x, &col_sizes, col_gap);
    let row_offsets = offsets(content.y, &row_sizes, row_gap);
    for p in &placements {
        let cell = Rect::new(
            col_offsets[p.col],
            row_offsets[p.row],
            span_extent(&col_sizes, p.col, p.col_span, col_gap).max(0),
            span_extent(&row_sizes, p.row, p.row_span.min(nrows - p.row), row_gap).max(0),
        );
        let cstyle = tree
            .nodes
            .get(p.id.0)
            .expect("grid child alive")
            .style
            .clone();
        let align = cstyle.align_self.unwrap_or(Align::Stretch);
        let explicit_w = resolve_dim(cstyle.width, cell.w);
        let explicit_h = resolve_dim(cstyle.height, cell.h);
        let rect = if align == Align::Stretch && explicit_w.is_none() && explicit_h.is_none() {
            cell // the fast, common case: fill the cell area
        } else {
            let est = if explicit_w.is_none() || explicit_h.is_none() {
                intrinsic_size(tree, p.id, cell.size())
            } else {
                Size::ZERO
            };
            let w = clamp_axis(
                explicit_w.unwrap_or(est.w),
                cstyle.min_width,
                cstyle.max_width,
            )
            .min(cell.w);
            let h = clamp_axis(
                explicit_h.unwrap_or(est.h),
                cstyle.min_height,
                cstyle.max_height,
            )
            .min(cell.h);
            // Alignment applies to BOTH axes in v1 (no justify_self yet
            // — module doc records the split as a later decision).
            let (dx, dy) = match align {
                Align::Start | Align::Stretch => (0, 0),
                Align::Center => ((cell.w - w) / 2, (cell.h - h) / 2),
                Align::End => (cell.w - w, cell.h - h),
            };
            // Stretch with ONE explicit axis: the other fills the cell.
            if align == Align::Stretch {
                Rect::new(
                    cell.x,
                    cell.y,
                    explicit_w.map(|w| w.min(cell.w)).unwrap_or(cell.w),
                    explicit_h.map(|h| h.min(cell.h)).unwrap_or(cell.h),
                )
            } else {
                Rect::new(cell.x + dx, cell.y + dy, w.max(0), h.max(0))
            }
        };
        tree.assign_rect(p.id, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::solve;

    #[test]
    fn auto_span_boundary_contributes_ceil_to_start_track_only() {
        // THE DOCUMENTED APPROXIMATION (RT6 risk 10), pinned exactly:
        // a row-spanning child contributes ceil(h/span) to its START
        // row only. Consequence at the boundary: if the SECOND spanned
        // row ends up smaller (nothing else sizes it), the child's cell
        // area is SMALLER than its intrinsic height — content may clip.
        // This test is the precise statement of that behavior; a future
        // multi-pass distributor may relax it, at which point these
        // asserts change deliberately.
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::default().grid(vec![Track::Fr(1.0), Track::Fr(1.0)], vec![]));
        // Spanning child: intrinsic height 5 over 2 rows -> start row
        // gets ceil(5/2) = 3.
        let spanner = tree.add_leaf(
            Style::default().row_span(2),
            Box::new(|_avail| crate::base::Size::new(4, 5)),
        );
        // Row-0 neighbor is short; row-1 neighbor is short too — row 1
        // is sized ONLY by its own child (1), not by the spanner's
        // remainder.
        let short0 = tree.add_leaf(Style::default(), Box::new(|_| crate::base::Size::new(4, 1)));
        let short1 = tree.add_leaf(Style::default(), Box::new(|_| crate::base::Size::new(4, 1)));
        tree.add_child(root, spanner);
        tree.add_child(root, short0);
        tree.add_child(root, short1);
        solve(&mut tree, root, Rect::new(0, 0, 10, 20));
        let s = tree.rect(spanner);
        // Start row 3 (ceil half) + row 1 sized by short1 (1) = 4 —
        // LESS than the intrinsic 5: the approximation's boundary.
        assert_eq!(
            s.h, 4,
            "spanner cell = 3 (start ceil) + 1 (neighbor row): {s:?}"
        );
        assert_eq!(
            tree.rect(short0).h,
            3,
            "row 0 sized by the spanner's ceil share"
        );
        assert_eq!(
            tree.rect(short1).h,
            1,
            "row 1 ignores the spanner remainder"
        );
    }

    #[test]
    fn sparse_placement_never_backfills_and_is_linear() {
        // CSS-default SPARSE packing (RT6 risk 9): after a wide child
        // forces a new row, later children never return to earlier
        // gaps. Also the complexity guard: the cursor is forward-only,
        // so total scan work is bounded by the occupancy area.
        let mut tree = LayoutTree::new();
        let root = tree.add(
            Style::default().grid(vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)], vec![]),
        );
        let a = tree.add_leaf(Style::default(), Box::new(|_| crate::base::Size::new(2, 1)));
        // b spans 3 -> cannot fit beside a -> row 1; the GAP on row 0
        // (cols 1..3) must stay EMPTY (sparse), not host c.
        let b = tree.add_leaf(
            Style::default().col_span(3),
            Box::new(|_| crate::base::Size::new(2, 1)),
        );
        let c = tree.add_leaf(Style::default(), Box::new(|_| crate::base::Size::new(2, 1)));
        tree.add_child(root, a);
        tree.add_child(root, b);
        tree.add_child(root, c);
        solve(&mut tree, root, Rect::new(0, 0, 12, 10));
        assert_eq!(tree.rect(a).y, 0);
        assert!(tree.rect(b).y > tree.rect(a).y, "wide child wraps");
        assert!(
            tree.rect(c).y > tree.rect(b).y,
            "sparse: c never backfills row 0's gap: {:?}",
            tree.rect(c)
        );
    }

    #[test]
    fn fr_tracks_tile_exactly_at_any_extent() {
        // Property: for arbitrary extents and mixed track lists, fr
        // tracks absorb EXACTLY the leftover (nothing lost to rounding).
        let mut rng = 0x9E3779B97F4A7C15u64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for _ in 0..300 {
            let extent = (next() % 200) as i32 + 1;
            let gap = (next() % 3) as i32;
            let n = (next() % 5) as usize + 1;
            let tracks: Vec<Track> = (0..n)
                .map(|_| match next() % 4 {
                    0 => Track::Cells((next() % 10) as i32),
                    1 => Track::Percent((next() % 40) as f32 / 100.0),
                    _ => Track::Fr(((next() % 30) as f32 / 10.0) + 0.1),
                })
                .collect();
            let sizes = resolve_tracks(&tracks, extent, gap, &vec![0; n]);
            let has_fr = tracks.iter().any(|t| matches!(t, Track::Fr(_)));
            let non_fr: i32 = tracks
                .iter()
                .zip(&sizes)
                .filter(|(t, _)| !matches!(t, Track::Fr(_)))
                .map(|(_, s)| *s)
                .sum();
            let gaps = gap * (sizes.len() as i32 - 1).max(0);
            let total: i32 = sizes.iter().sum::<i32>() + gaps;
            if has_fr && extent >= non_fr + gaps {
                assert_eq!(
                    total, extent,
                    "fr must tile: {tracks:?} @ {extent} gap {gap}"
                );
            }
            assert!(sizes.iter().all(|s| *s >= 0));
        }
    }
}
