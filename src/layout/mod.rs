//! Layout: flexbox-style solver over the component tree (direction,
//! grow/shrink/basis, gap, padding, margin, min/max, percent, absolute),
//! with text measurement callbacks. Pure and deterministic: integer
//! cells, largest-remainder rounding (children tile containers exactly),
//! and subtree re-solve for incremental updates.
//!
//! Owner: REACT. Scope notes: `auto` margins and percent insets remain
//! out (cycle candidates if widgets need them). Cycle 6 added flex WRAP
//! (`Style::wrap`, `cross_gap`), a track GRID (`Display::Grid` with
//! `Cells`/`Fr` tracks, col/row spans, gaps) and `Overflow` semantics
//! (`Visible`/`Clip`/`Scroll` — `Scroll` is the wheel-routing hint).

mod flex_math;
mod grid;
mod solve;
mod style;
mod tree;
mod wrap;

pub use solve::{resolve_subtree, solve};
pub use style::{
    Align, Dimension, Direction, Display, Edges, Inset, Justify, Overflow, Position, Style, Track,
};
pub use tree::{LayoutId, LayoutTree, MeasureFn};

/// THE user-facing name for [`Style`] (the prelude exports it this way):
/// `LayoutStyle` is box geometry (direction/size/gap/overflow),
/// `render::Style` is paint (colors/attrs). Two `Style` types confuse
/// every newcomer; the alias keeps imports self-describing.
pub type LayoutStyle = Style;

/// Largest-remainder integer distribution — shared with crate-internal
/// consumers that tile spans outside the box model (table columns).
pub(crate) use flex_math::distribute;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Rect, Size};

    fn tree_with(styles: &[Style], root_style: Style) -> (LayoutTree, LayoutId, Vec<LayoutId>) {
        let mut tree = LayoutTree::new();
        let root = tree.add(root_style);
        let ids: Vec<LayoutId> = styles
            .iter()
            .map(|s| {
                let id = tree.add(s.clone());
                tree.add_child(root, id);
                id
            })
            .collect();
        (tree, root, ids)
    }

    #[test]
    fn wrap_breaks_lines_and_each_line_tiles() {
        // 5 fixed 4-wide children in a 10-wide wrapping row: lines of
        // 2/2/1; each line starts at x=0; lines stack with cross_gap.
        let styles: Vec<Style> = (0..5)
            .map(|_| {
                Style::default()
                    .width(Dimension::Cells(4))
                    .height(Dimension::Cells(1))
            })
            .collect();
        let (mut tree, root, ids) = tree_with(&styles, Style::row().wrap().cross_gap(1));
        solve(&mut tree, root, Rect::new(0, 0, 10, 10));
        let r: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(r[0], Rect::new(0, 0, 4, 1));
        assert_eq!(r[1], Rect::new(4, 0, 4, 1));
        assert_eq!(r[2], Rect::new(0, 2, 4, 1), "wraps to line 2 (cross_gap 1)");
        assert_eq!(r[3], Rect::new(4, 2, 4, 1));
        assert_eq!(r[4], Rect::new(0, 4, 4, 1));
    }

    #[test]
    fn wrap_lines_distribute_grow_independently() {
        // Line breaks happen on BASIS (CSS hypothetical main size):
        // 4+4 fits a 10-wide line, the third child wraps. Each line
        // then grows ITS members over ITS OWN leftover — line 1 tiles
        // 5/5, line 2's pair tiles 5/5 below, never one global pool.
        let styles: Vec<Style> = (0..4)
            .map(|_| {
                Style::default()
                    .basis(Dimension::Cells(4))
                    .grow(1.0)
                    .height(Dimension::Cells(1))
            })
            .collect();
        let (mut tree, root, ids) = tree_with(&styles, Style::row().wrap());
        solve(&mut tree, root, Rect::new(0, 0, 10, 8));
        let r: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(r[0].w + r[1].w, 10, "line 1 tiles: {r:?}");
        assert_eq!(r[2].w + r[3].w, 10, "line 2 tiles: {r:?}");
        assert_eq!((r[0].w, r[1].w), (5, 5), "grow splits the line: {r:?}");
        assert_eq!(r[0].y, r[1].y);
        assert!(r[2].y > r[0].y, "second line below the first");
    }

    #[test]
    fn wrap_oversized_child_gets_its_own_line() {
        let styles = vec![
            Style::default()
                .width(Dimension::Cells(15))
                .height(Dimension::Cells(1))
                .shrink(0.0),
            Style::default()
                .width(Dimension::Cells(3))
                .height(Dimension::Cells(1)),
        ];
        let (mut tree, root, ids) = tree_with(&styles, Style::row().wrap());
        solve(&mut tree, root, Rect::new(0, 0, 10, 5));
        assert_eq!(tree.rect(ids[0]).y, 0);
        assert_eq!(
            tree.rect(ids[1]).y,
            1,
            "oversized child owns line 1; next child wraps"
        );
    }

    #[test]
    fn grid_places_row_major_with_spans_and_gaps() {
        // 3 columns (fixed 4, 1fr, 1fr) in width 14, gap 1: fr cols get
        // (14 - 4 - 2 gaps)/2 = 4 each. Child 1 spans 2 cols.
        let styles = vec![
            Style::default().height(Dimension::Cells(1)),
            Style::default().col_span(2).height(Dimension::Cells(1)),
            Style::default().height(Dimension::Cells(1)),
            Style::default().height(Dimension::Cells(1)),
        ];
        let (mut tree, root, ids) = tree_with(
            &styles,
            Style::default()
                .grid(
                    vec![Track::Cells(4), Track::Fr(1.0), Track::Fr(1.0)],
                    vec![],
                )
                .gap(1)
                .cross_gap(0),
        );
        solve(&mut tree, root, Rect::new(0, 0, 14, 10));
        let r: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(r[0], Rect::new(0, 0, 4, 1), "col 0: {r:?}");
        // Child 1 spans cols 1-2: width 4 + 1 (gap) + 4 = 9, at x 5.
        assert_eq!(r[1], Rect::new(5, 0, 9, 1), "span 2: {r:?}");
        // Child 2 no longer fits row 0 -> row 1 col 0; child 3 follows.
        assert_eq!(r[2].y, r[0].y + 1);
        assert_eq!(r[2].x, 0);
        assert_eq!(r[3].x, 5);
    }

    #[test]
    fn grid_fr_rows_share_leftover_height_exactly() {
        let styles = vec![Style::default(), Style::default(), Style::default()];
        let (mut tree, root, ids) = tree_with(
            &styles,
            Style::default().grid(
                vec![Track::Fr(1.0)],
                vec![Track::Cells(2), Track::Fr(1.0), Track::Fr(1.0)],
            ),
        );
        solve(&mut tree, root, Rect::new(0, 0, 8, 11));
        let r: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(r[0].h, 2);
        assert_eq!(r[1].h + r[2].h, 9, "fr rows tile the leftover: {r:?}");
        assert!((r[1].h - r[2].h).abs() <= 1, "largest-remainder split");
        assert_eq!(r[0].w, 8, "single fr column fills the width");
    }

    #[test]
    fn grid_row_span_occupies_and_displaces() {
        // 2 cols; child 0 spans 2 rows in col 0 -> children 1,2 fill col
        // 1 of rows 0,1; child 3 lands at row 2 col 0.
        let styles = vec![
            Style::default().row_span(2).height(Dimension::Cells(4)),
            Style::default().height(Dimension::Cells(2)),
            Style::default().height(Dimension::Cells(2)),
            Style::default().height(Dimension::Cells(2)),
        ];
        let (mut tree, root, ids) = tree_with(
            &styles,
            Style::default().grid(vec![Track::Fr(1.0), Track::Fr(1.0)], vec![]),
        );
        solve(&mut tree, root, Rect::new(0, 0, 10, 12));
        let r: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(r[0].x, 0);
        assert_eq!(r[1].x, 5, "col 1: {r:?}");
        assert_eq!(r[1].y, r[0].y);
        assert_eq!(r[2].x, 5, "row 1 col 0 is occupied by the span: {r:?}");
        assert_eq!(r[3].x, 0, "row 2 col 0 free again: {r:?}");
        assert!(r[3].y >= r[0].bottom(), "below the spanning child: {r:?}");
    }

    #[test]
    fn wrap_property_children_never_overlap_and_stay_left_aligned() {
        // Randomized: fixed-width children in a wrapping row never
        // overlap and every line starts at the content left edge.
        let mut rng = 0xD1B54A32D192ED03u64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for _ in 0..60 {
            let n = (next() % 8) as usize + 1;
            let container_w = (next() % 30) as i32 + 2;
            let styles: Vec<Style> = (0..n)
                .map(|_| {
                    Style::default()
                        .width(Dimension::Cells((next() % 9) as i32 + 1))
                        .height(Dimension::Cells(1))
                        .shrink(0.0)
                })
                .collect();
            let (mut tree, root, ids) = tree_with(&styles, Style::row().wrap());
            solve(&mut tree, root, Rect::new(0, 0, container_w, 50));
            let rects: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
            for (i, a) in rects.iter().enumerate() {
                for b in rects.iter().skip(i + 1) {
                    assert!(
                        !a.intersects(*b) || a.is_empty() || b.is_empty(),
                        "overlap: {a:?} vs {b:?} in w={container_w}"
                    );
                }
            }
            let mut seen_rows = std::collections::BTreeMap::new();
            for r in &rects {
                let first_x = seen_rows.entry(r.y).or_insert(r.x);
                assert!(*first_x <= r.x);
            }
        }
    }

    #[test]
    fn row_grow_distributes_rounding_largest_remainder() {
        // The charter case: 3 growing children in width 10 -> 4/3/3.
        let (mut tree, root, ids) = tree_with(
            &[
                Style::default().grow(1.0),
                Style::default().grow(1.0),
                Style::default().grow(1.0),
            ],
            Style::row(),
        );
        solve(&mut tree, root, Rect::new(0, 0, 10, 2));
        let rects: Vec<Rect> = ids.iter().map(|id| tree.rect(*id)).collect();
        assert_eq!(rects[0], Rect::new(0, 0, 4, 2));
        assert_eq!(rects[1], Rect::new(4, 0, 3, 2));
        assert_eq!(rects[2], Rect::new(7, 0, 3, 2));
        let total: i32 = rects.iter().map(|r| r.w).sum();
        assert_eq!(total, 10, "no lost or invented columns");
    }

    #[test]
    fn column_grow_fills_height_exactly() {
        let (mut tree, root, ids) = tree_with(
            &[Style::default().grow(1.0), Style::default().grow(2.0)],
            Style::column(),
        );
        solve(&mut tree, root, Rect::new(0, 0, 8, 9));
        assert_eq!(tree.rect(ids[0]), Rect::new(0, 0, 8, 3));
        assert_eq!(tree.rect(ids[1]), Rect::new(0, 3, 8, 6));
    }

    #[test]
    fn padding_and_gap_math() {
        let (mut tree, root, ids) = tree_with(
            &[Style::default().w(3).h(1), Style::default().w(2).h(1)],
            Style::row().gap(2).padding(Edges::all(1)),
        );
        solve(&mut tree, root, Rect::new(0, 0, 20, 5));
        // Content box starts at (1,1); second child = 1 + 3 + gap 2 = 6.
        assert_eq!(tree.rect(ids[0]), Rect::new(1, 1, 3, 1));
        assert_eq!(tree.rect(ids[1]), Rect::new(6, 1, 2, 1));
    }

    #[test]
    fn margins_offset_flow() {
        let (mut tree, root, ids) = tree_with(
            &[Style::default().w(4).h(1).margin(Edges {
                left: 2,
                right: 1,
                top: 1,
                bottom: 0,
            })],
            Style::row(),
        );
        solve(&mut tree, root, Rect::new(0, 0, 20, 4));
        assert_eq!(tree.rect(ids[0]), Rect::new(2, 1, 4, 1));
    }

    #[test]
    fn percent_resolves_against_parent_content_box() {
        let (mut tree, root, ids) = tree_with(
            &[Style::default().width(Dimension::Percent(0.5)).h(1)],
            Style::row().padding(Edges::hv(2, 0)), // content w = 20 - 4 = 16
        );
        solve(&mut tree, root, Rect::new(0, 0, 20, 3));
        assert_eq!(tree.rect(ids[0]).w, 8, "50% of the 16-cell content box");
        assert_eq!(tree.rect(ids[0]).x, 2);
    }

    #[test]
    fn min_max_clamps_redistribute() {
        let (mut tree, root, ids) = tree_with(
            &[
                Style::default().grow(1.0).max_w(3),
                Style::default().grow(1.0),
            ],
            Style::row(),
        );
        solve(&mut tree, root, Rect::new(0, 0, 12, 1));
        assert_eq!(tree.rect(ids[0]).w, 3, "max clamps");
        assert_eq!(tree.rect(ids[1]).w, 9, "freed space redistributes");
        // Shrink honoring min:
        let (mut tree2, root2, ids2) = tree_with(
            &[Style::default().w(8).min_w(6), Style::default().w(8)],
            Style::row(),
        );
        solve(&mut tree2, root2, Rect::new(0, 0, 10, 1));
        assert_eq!(tree2.rect(ids2[0]).w, 6, "shrink stops at min");
        assert_eq!(tree2.rect(ids2[1]).w, 4);
    }

    #[test]
    fn justify_and_space_between() {
        // Center: 2 fixed children (3+3=6) in 12 -> lead offset 3.
        let (mut tree, root, ids) = tree_with(
            &[Style::default().w(3).h(1), Style::default().w(3).h(1)],
            Style::row().justify(Justify::Center),
        );
        solve(&mut tree, root, Rect::new(0, 0, 12, 1));
        assert_eq!(tree.rect(ids[0]).x, 3);
        assert_eq!(tree.rect(ids[1]).x, 6);
        // SpaceBetween: leftover 6 into 1 slot.
        let (mut t2, r2, i2) = tree_with(
            &[Style::default().w(3).h(1), Style::default().w(3).h(1)],
            Style::row().justify(Justify::SpaceBetween),
        );
        solve(&mut t2, r2, Rect::new(0, 0, 12, 1));
        assert_eq!(t2.rect(i2[0]).x, 0);
        assert_eq!(t2.rect(i2[1]).x, 9, "pushed to the far edge");
        // SpaceBetween rounding: leftover 7 into 2 slots -> 4 then 3.
        let (mut t3, r3, i3) = tree_with(
            &[
                Style::default().w(1).h(1),
                Style::default().w(1).h(1),
                Style::default().w(1).h(1),
            ],
            Style::row().justify(Justify::SpaceBetween),
        );
        solve(&mut t3, r3, Rect::new(0, 0, 10, 1));
        assert_eq!(t3.rect(i3[0]).x, 0);
        assert_eq!(t3.rect(i3[1]).x, 5, "first slot gets the larger share");
        assert_eq!(t3.rect(i3[2]).x, 9);
    }

    #[test]
    fn align_and_stretch_cross_axis() {
        let (mut tree, root, ids) = tree_with(
            &[
                Style::default().w(2), // stretch (default)
                Style::default().w(2).h(1).align_self(Align::Center),
                Style::default().w(2).h(1).align_self(Align::End),
            ],
            Style::row(),
        );
        solve(&mut tree, root, Rect::new(0, 0, 10, 5));
        assert_eq!(tree.rect(ids[0]).h, 5, "stretch fills the cross axis");
        assert_eq!(tree.rect(ids[1]).y, 2, "center = (5-1)/2");
        assert_eq!(tree.rect(ids[2]).y, 4, "end pins to the bottom");
    }

    #[test]
    fn absolute_positioning_insets() {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row().padding(Edges::all(1)));
        let pinned = tree.add(Style {
            position: Position::Absolute,
            inset: Inset {
                left: None,
                right: Some(1),
                top: Some(0),
                bottom: None,
            },
            ..Style::default().w(4).h(1)
        });
        let stretched = tree.add(Style {
            position: Position::Absolute,
            inset: Inset {
                left: Some(1),
                right: Some(1),
                top: Some(1),
                bottom: Some(1),
            },
            ..Style::default()
        });
        tree.add_child(root, pinned);
        tree.add_child(root, stretched);
        solve(&mut tree, root, Rect::new(0, 0, 20, 10));
        // Content box: (1,1,18,8). Right-pinned: x = 1+18-1-4 = 14.
        assert_eq!(tree.rect(pinned), Rect::new(14, 1, 4, 1));
        // Both insets, auto size: fills content box minus insets.
        assert_eq!(tree.rect(stretched), Rect::new(2, 2, 16, 6));
    }

    #[test]
    fn measure_callback_drives_leaf_size() {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column());
        let text = tree.add_leaf(
            Style::default(),
            Box::new(|_avail: Size| Size::new(11, 2)), // e.g. "hello world" wrapped
        );
        tree.add_child(root, text);
        solve(&mut tree, root, Rect::new(0, 0, 40, 12));
        let r = tree.rect(text);
        assert_eq!(
            (r.w, r.h),
            (40, 2),
            "row width stretches, height from measure"
        );
    }

    #[test]
    fn nested_containers_solve_recursively() {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column());
        let bar = tree.add(Style::row().h(1));
        let body = tree.add(Style::row().grow(1.0));
        let left = tree.add(Style::default().w(10));
        let main = tree.add(Style::default().grow(1.0));
        tree.add_child(root, bar);
        tree.add_child(root, body);
        tree.add_child(body, left);
        tree.add_child(body, main);
        solve(&mut tree, root, Rect::new(0, 0, 80, 24));
        assert_eq!(tree.rect(bar), Rect::new(0, 0, 80, 1));
        assert_eq!(tree.rect(body), Rect::new(0, 1, 80, 23));
        assert_eq!(tree.rect(left), Rect::new(0, 1, 10, 23));
        assert_eq!(tree.rect(main), Rect::new(10, 1, 70, 23));
    }

    #[test]
    fn intrinsic_content_sizes_containers() {
        // A column whose height is content-driven inside a row.
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row().align_items(Align::Start));
        let card = tree.add(Style::column().gap(1).padding(Edges::all(1)).w(10));
        let a = tree.add_leaf(Style::default(), Box::new(|_| Size::new(4, 1)));
        let b = tree.add_leaf(Style::default(), Box::new(|_| Size::new(4, 2)));
        tree.add_child(root, card);
        tree.add_child(card, a);
        tree.add_child(card, b);
        solve(&mut tree, root, Rect::new(0, 0, 30, 20));
        // Height = padding 2 + 1 + gap 1 + 2 = 6.
        assert_eq!(tree.rect(card).h, 6);
    }

    #[test]
    fn removal_invalidates_subtree_ids() {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::row());
        let child = tree.add(Style::default());
        let grand = tree.add(Style::default());
        tree.add_child(root, child);
        tree.add_child(child, grand);
        assert_eq!(tree.len(), 3);
        tree.remove(child);
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_alive(child));
        assert!(!tree.is_alive(grand), "subtree removal is recursive");
        assert!(tree.is_alive(root));
        assert!(tree.children(root).is_empty(), "parent list is detached");
    }
}
