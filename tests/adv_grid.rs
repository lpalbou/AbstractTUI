//! VERIFY cycle-6 grid layout property tests. The grid solver's own unit
//! test pins fr tiling at the track-resolver level; these pin the
//! end-to-end invariants THROUGH `solve`: fr tracks tile the content box
//! exactly (minus gaps), placed children never overlap and never escape
//! the container, spans cover the right extent, and degenerate specs
//! (zero tracks, over-wide spans) stay safe.

use abstracttui::base::Rect;
use abstracttui::layout::{Display, LayoutTree, Style, Track};
use abstracttui::testing::Rng;

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x.max(b.x) < a.right().min(b.right()) && a.y.max(b.y) < a.bottom().min(b.bottom())
}

/// fr tracks (+ fixed cells) tile the content width EXACTLY: the sum of
/// column extents plus inter-column gaps equals the container width,
/// whenever the fixed tracks + gaps fit. Random specs, random widths.
#[test]
fn grid_columns_tile_width_exactly() {
    let mut rng = Rng::new(0x0062_17D0);
    for _ in 0..400 {
        let ncols = 1 + rng.below(6);
        let gap = rng.below(4) as i32;
        let w = 4 + rng.below(120) as i32;
        let h = 4 + rng.below(20) as i32;

        let mut cols = Vec::new();
        for _ in 0..ncols {
            if rng.below(3) == 0 {
                cols.push(Track::Cells(1 + rng.below(10) as i32));
            } else {
                cols.push(Track::Fr(0.5 + rng.below(30) as f32 / 10.0));
            }
        }
        let has_fr = cols.iter().any(|t| matches!(t, Track::Fr(_)));

        let mut tree = LayoutTree::new();
        let root = tree.add(Style::default().grid(cols.clone(), vec![]).gap(gap));
        // One child per column so each column has content and gets a rect.
        let mut ids = Vec::new();
        for _ in 0..ncols {
            let id = tree.add(Style::default());
            tree.add_child(root, id);
            ids.push(id);
        }
        let container = Rect::new(0, 0, w, h);
        abstracttui::layout::solve(&mut tree, root, container);

        let rects: Vec<Rect> = ids.iter().map(|&id| tree.rect(id)).collect();
        // No two cells overlap.
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(
                    !overlaps(rects[i], rects[j]),
                    "grid cells overlap: {:?} {:?}",
                    rects[i],
                    rects[j]
                );
            }
        }
        // Non-fr extent (fixed cols) + gaps that fit => fr columns
        // absorb the rest and the row tiles exactly. Percent/Auto tracks
        // are excluded from this random population (only Cells + Fr are
        // generated above), but the match is exhaustive for safety.
        let fixed: i32 = cols
            .iter()
            .map(|t| match t {
                Track::Cells(c) => (*c).max(0),
                Track::Percent(_) | Track::Auto | Track::Fr(_) => 0,
            })
            .sum();
        let gaps_total = gap * (ncols as i32 - 1).max(0);
        let fits = w >= fixed + gaps_total;
        if has_fr && fits {
            let widths: i32 = rects.iter().map(|r| r.w).sum();
            assert_eq!(
                widths + gaps_total,
                w,
                "fr columns must tile width exactly: cols={cols:?} w={w} gap={gap}"
            );
            // Columns are contiguous left-to-right separated by exactly gap.
            let mut sorted = rects.clone();
            sorted.sort_by_key(|r| r.x);
            for pair in sorted.windows(2) {
                assert_eq!(pair[1].x - pair[0].right(), gap, "wrong inter-column gap");
            }
        }
        // Containment holds when the tracks fit; fixed tracks wider than
        // the container legitimately overflow (grid does not shrink fixed
        // tracks), so containment is asserted only in the fit case.
        if fits {
            for r in &rects {
                assert!(
                    r.x >= 0 && r.y >= 0 && r.right() <= w && r.bottom() <= h,
                    "grid cell escapes container while fitting: {r:?} in {container:?}"
                );
            }
        }
    }
}

/// Column spans cover the summed extent of their tracks plus the gaps
/// BETWEEN them, and a spanning child still fits the container.
#[test]
fn grid_col_span_covers_tracks_plus_internal_gaps() {
    let gap = 2;
    let mut tree = LayoutTree::new();
    let root = tree.add(
        Style::default()
            .grid(
                vec![Track::Cells(10), Track::Cells(10), Track::Cells(10)],
                vec![],
            )
            .gap(gap),
    );
    let wide = tree.add(Style::default().col_span(2));
    let solo = tree.add(Style::default());
    tree.add_child(root, wide);
    tree.add_child(root, solo);
    abstracttui::layout::solve(&mut tree, root, Rect::new(0, 0, 34, 4));
    let rw = tree.rect(wide);
    // Two 10-cell tracks + one internal gap = 22.
    assert_eq!(rw.w, 22, "col_span(2) over 10+gap(2)+10 = 22, got {rw:?}");
    assert_eq!(rw.x, 0);
    // The solo child lands in the third column, past both spanned tracks.
    let rs = tree.rect(solo);
    assert_eq!(rs.x, 24, "solo should start at track 3 (10+2+10+2)");
    assert_eq!(rs.w, 10);
}

/// Degenerate: an EMPTY column spec behaves as one full-width column;
/// children stack into that single column (each its own row).
#[test]
fn grid_zero_tracks_is_one_full_width_column() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::default().grid(vec![], vec![]));
    let a = tree.add(Style::default().h(2));
    let b = tree.add(Style::default().h(2));
    tree.add_child(root, a);
    tree.add_child(root, b);
    abstracttui::layout::solve(&mut tree, root, Rect::new(0, 0, 30, 10));
    let (ra, rb) = (tree.rect(a), tree.rect(b));
    assert_eq!(ra.w, 30, "single implicit column spans full width");
    assert_eq!(rb.w, 30);
    assert!(
        !overlaps(ra, rb),
        "stacked rows must not overlap: {ra:?} {rb:?}"
    );
    assert!(
        ra.bottom() <= rb.y || rb.bottom() <= ra.y,
        "rows must be vertically disjoint"
    );
}

/// Degenerate: a col_span WIDER than the grid clamps to the column count
/// (never panics, never escapes).
#[test]
fn grid_overwide_span_clamps_safely() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::default().grid(vec![Track::Fr(1.0), Track::Fr(1.0)], vec![]));
    let huge = tree.add(Style::default().col_span(99));
    tree.add_child(root, huge);
    abstracttui::layout::solve(&mut tree, root, Rect::new(0, 0, 20, 4));
    let r = tree.rect(huge);
    assert!(
        r.right() <= 20,
        "over-span must clamp within the container: {r:?}"
    );
    assert!(r.w > 0, "clamped span still has width");
}

/// Determinism: identical grid + container => identical rects.
#[test]
fn grid_solve_is_deterministic() {
    let build = || {
        let mut tree = LayoutTree::new();
        let root = tree.add(
            Style::default()
                .grid(
                    vec![Track::Fr(1.0), Track::Cells(6), Track::Fr(2.0)],
                    vec![Track::Cells(3)],
                )
                .gap(1)
                .cross_gap(1),
        );
        let ids: Vec<_> = (0..6)
            .map(|i| {
                let s = if i % 2 == 0 {
                    Style::default()
                } else {
                    Style::default().col_span(2)
                };
                let id = tree.add(s);
                tree.add_child(root, id);
                id
            })
            .collect();
        abstracttui::layout::solve(&mut tree, root, Rect::new(0, 0, 41, 13));
        ids.iter().map(|&id| tree.rect(id)).collect::<Vec<_>>()
    };
    assert_eq!(build(), build(), "grid layout must be deterministic");
    let _ = Display::Flex;
}
