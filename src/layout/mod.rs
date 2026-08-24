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

pub use solve::{measure, resolve_subtree, solve};
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
    use std::cell::RefCell;
    use std::rc::Rc;

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

    /// 0240 follow-up #3: a DECLARED fixed-size child crushed to zero
    /// by overflow pressure names itself (debug builds); the documented
    /// opt-out (any explicit min, incl. min 0) and shrink(0.0) stay
    /// silent. Debug-gated: release builds record nothing by design.
    #[cfg(debug_assertions)]
    #[test]
    fn zero_collapse_emits_a_debug_notice_once_with_opt_outs() {
        // 600-cell child + two one-row children in a 6-row column: the
        // fixed rows lose their single cell (weights 600:1:1).
        let (mut tree, root, ids) = tree_with(
            &[
                Style::default().h(600),
                Style::default().h(1),
                Style::default().h(1).min_h(0), // author opt-out
            ],
            Style::column().h(6),
        );
        solve(&mut tree, root, Rect::new(0, 0, 10, 6));
        assert_eq!(tree.rect(ids[1]).h, 0, "the watched row was crushed");
        let notices = tree.take_collapse_notices();
        assert_eq!(
            notices.len(),
            1,
            "opted-out row must stay silent: {notices:?}"
        );
        assert!(
            notices[0].contains("1 cells") && notices[0].contains("height"),
            "notice names the declared size and axis: {}",
            notices[0]
        );
        // Once per node: a re-solve does not repeat the report.
        solve(&mut tree, root, Rect::new(0, 0, 10, 6));
        assert!(tree.take_collapse_notices().is_empty(), "reported once");

        // shrink(0.0) children hold their extent: nothing to report.
        let (mut t2, r2, i2) = tree_with(
            &[Style::default().h(600), Style::default().h(1).shrink(0.0)],
            Style::column().h(6),
        );
        solve(&mut t2, r2, Rect::new(0, 0, 10, 6));
        assert_eq!(t2.rect(i2[1]).h, 1, "shrink 0 holds the row");
        assert!(t2.take_collapse_notices().is_empty());
    }

    /// 2026-07-22 dashboard incident: `dyn` views mint FRESH layout
    /// nodes every regeneration, so per-node dedup re-reported the same
    /// collapsed row on every data tick (notice spam). Dedup keys on
    /// the SITUATION (parent rect, axis, declared, child index) — a
    /// rebuilt child in the same geometry stays silent; a resize is a
    /// new situation and reports again.
    #[cfg(debug_assertions)]
    #[test]
    fn zero_collapse_dedup_survives_dyn_regeneration() {
        let styles = [Style::default().h(600), Style::default().h(1)];
        let (mut tree, root, ids) = tree_with(&styles, Style::column().h(6));
        solve(&mut tree, root, Rect::new(0, 0, 10, 6));
        assert_eq!(tree.take_collapse_notices().len(), 1, "first report");

        // Simulate a dyn regeneration: remove and re-add the children
        // (fresh generational keys), identical styles and geometry.
        for id in &ids {
            tree.remove(*id);
        }
        let _new_ids: Vec<LayoutId> = styles
            .iter()
            .map(|s| {
                let id = tree.add(s.clone());
                tree.add_child(root, id);
                id
            })
            .collect();
        solve(&mut tree, root, Rect::new(0, 0, 10, 6));
        assert!(
            tree.take_collapse_notices().is_empty(),
            "same situation with fresh node keys must not re-report"
        );

        // A resize IS a new situation: the report fires once more.
        solve(&mut tree, root, Rect::new(0, 0, 12, 7));
        let after_resize = tree.take_collapse_notices();
        assert_eq!(after_resize.len(), 1, "resize reports the new geometry");
    }

    /// 1330: the other end of the zero-collapse class. A child that
    /// REFUSES to shrink inside a column shorter than it is solved past
    /// the parent's box and paints there — where the next sibling
    /// overpaints it. The engine keeps the rect truthful and says so.
    #[cfg(debug_assertions)]
    #[test]
    fn main_overflow_emits_a_debug_notice_once() {
        // The composer shape: a 3-row box around a 4-row widget that
        // declared shrink(0.0). The widget keeps its 4 rows; row 4 lands
        // outside the parent.
        let (mut tree, root, ids) =
            tree_with(&[Style::default().h(4).shrink(0.0)], Style::column().h(3));
        solve(&mut tree, root, Rect::new(0, 0, 20, 3));
        assert_eq!(tree.rect(ids[0]).h, 4, "shrink 0 holds its rows");
        assert!(
            tree.rect(ids[0]).bottom() > tree.rect(root).bottom(),
            "and therefore overflows the parent: {:?} vs {:?}",
            tree.rect(ids[0]),
            tree.rect(root)
        );
        let notices = tree.take_collapse_notices();
        assert_eq!(notices.len(), 1, "the overflow names itself: {notices:?}");
        assert!(
            notices[0].contains("4 rows") && notices[0].contains("3-row"),
            "notice names wanted and available: {}",
            notices[0]
        );
        // Once per situation, exactly like the zero-collapse notice.
        solve(&mut tree, root, Rect::new(0, 0, 20, 3));
        assert!(tree.take_collapse_notices().is_empty(), "reported once");

        // A parent that CLIPS has said what it wants done with the
        // surplus: no notice.
        let (mut t2, r2, _) = tree_with(
            &[Style::default().h(4).shrink(0.0)],
            Style::column().h(3).clip(),
        );
        solve(&mut t2, r2, Rect::new(0, 0, 20, 3));
        assert!(
            t2.take_collapse_notices().is_empty(),
            "a clipping parent is silent"
        );

        // Rows stay silent: a label wider than its cell is the most
        // ordinary condition in a terminal UI.
        let (mut t3, r3, _) = tree_with(&[Style::default().w(40).shrink(0.0)], Style::row().w(20));
        solve(&mut t3, r3, Rect::new(0, 0, 20, 1));
        assert!(
            t3.take_collapse_notices().is_empty(),
            "row overflow is not reported"
        );

        // A child that CAN give is not an overflow: it just shrinks.
        let (mut t4, r4, i4) = tree_with(&[Style::default().h(4)], Style::column().h(3));
        solve(&mut t4, r4, Rect::new(0, 0, 20, 3));
        assert_eq!(t4.rect(i4[0]).h, 3, "ordinary shrink");
        assert!(
            t4.take_collapse_notices().is_empty(),
            "shrinking is what shrink means"
        );
    }

    #[test]
    fn measure_query_answers_without_assigning_rects() {
        // The 0130 size query: a column of measured leaves answers its
        // stacked intrinsic height at the given width; no rect changes.
        let mut tree = LayoutTree::new();
        let col = tree.add(Style::column());
        let a = tree.add_leaf(Style::default(), Box::new(|_| Size::new(8, 2)));
        let b = tree.add_leaf(Style::default(), Box::new(|_| Size::new(6, 3)));
        tree.add_child(col, a);
        tree.add_child(col, b);
        let m = measure(&tree, col, Size::new(30, 100));
        assert_eq!(m, Size::new(8, 5), "sum main axis, max cross: {m:?}");
        assert_eq!(tree.rect(col), Rect::ZERO, "measure must not solve");
        // An explicit dimension wins over content.
        tree.set_style(col, Style::column().h(9));
        assert_eq!(measure(&tree, col, Size::new(30, 100)).h, 9);
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
    fn a_margined_child_is_measured_inside_the_box_it_will_be_solved_to() {
        // A content-sized child is SOLVED to the content box less its
        // own margins, so that is the width its measure callback must
        // be asked at. Asking at the full width makes a wrapping leaf
        // answer for a line length it will never be drawn at, and it
        // is drawn a row short — the placement pass and the intrinsic
        // pass disagreeing about the same box.
        let asked: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&asked);
        // 70 columns of text: 2 rows at width 40, 3 rows at width 34.
        let wrap70 = move |avail: Size| {
            log.borrow_mut().push(avail.w);
            let w = avail.w.max(1);
            Size::new(w, (70 + w - 1) / w)
        };

        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column());
        let leaf = tree.add_leaf(Style::default().margin(Edges::all(3)), Box::new(wrap70));
        tree.add_child(root, leaf);
        solve(&mut tree, root, Rect::new(0, 0, 40, 20));

        let widths = asked.borrow().clone();
        // The check must be able to fail for the right reason: a leaf
        // that was never measured would satisfy every assertion below
        // about what it was measured AT.
        assert!(
            !widths.is_empty(),
            "the measure callback must be consulted at all"
        );
        let r = tree.rect(leaf);
        assert_eq!((r.x, r.w), (3, 34), "solved inside its own margins: {r:?}");
        assert!(
            widths.iter().all(|&w| w == r.w),
            "measured at {widths:?} but solved to {} — the leaf answers for a width it is never drawn at",
            r.w
        );
        assert_eq!(
            r.h, 3,
            "70 columns wrap to 3 rows at width 34 (2 at 40): {r:?}"
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

    /// Solve a scroll column of `rows` measured leaves into a viewport
    /// `visible` rows tall, and answer how many leaves the solver asked
    /// to measure themselves.
    fn measures_for(rows: i32, visible: i32) -> usize {
        let asked = Rc::new(RefCell::new(0usize));
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column().scroll());
        for _ in 0..rows {
            let counter = Rc::clone(&asked);
            let leaf = tree.add_leaf(
                Style::default(),
                Box::new(move |_avail: Size| {
                    *counter.borrow_mut() += 1;
                    Size::new(20, 1)
                }),
            );
            tree.add_child(root, leaf);
        }
        solve(&mut tree, root, Rect::new(0, 0, 40, visible));
        let n = *asked.borrow();
        n
    }

    #[test]
    fn layout_cost_follows_retained_rows_not_visible_ones() {
        // The cost model behind the "scrolling a long feed is slow"
        // class of complaint, measured rather than asserted. Paint
        // culls off-screen nodes (`ui/draw.rs` skips a rect that does
        // not intersect the clip); LAYOUT deliberately does not —
        // `Overflow`'s own doc says "layout itself NEVER clips (solved
        // rects stay truthful)". So a scroll container pays for every
        // row it RETAINS, however few it SHOWS, and `scroll()` buys
        // nothing here: it is draw/hit metadata, not a solver hint.
        //
        // This guard exists to be the number that decision is made on,
        // and to go RED the day the solver learns to skip rows outside
        // its container — which is what makes it a measurement and not
        // a restatement of the doc comment.
        let tall_short_viewport = measures_for(400, 10);
        let tall_full_viewport = measures_for(400, 400);
        let short = measures_for(10, 10);

        // Non-vacuity first: the counter tracks real children, so a
        // later `0` or a constant cannot pass as a finding.
        assert_eq!(
            short, 10,
            "ten visible rows must cost ten measures, or this counter \
             is measuring nothing"
        );
        assert_eq!(
            tall_short_viewport, 400,
            "a 400-row scroll column showing 10 rows still measured \
             {tall_short_viewport} leaves"
        );
        assert_eq!(
            tall_short_viewport, tall_full_viewport,
            "shrinking the viewport 40x changed the solver's work from \
             {tall_full_viewport} to {tall_short_viewport} measures — if \
             this ever differs, layout has started culling and the \
             comment above is stale"
        );
    }

    #[test]
    fn a_leaf_in_a_content_sized_card_is_measured_more_than_once_per_solve() {
        // Where the per-row cost actually goes.
        //
        // This comment used to say "one intrinsic pass + one placement
        // pass". The NUMBER was right and the mechanism was not, which
        // made it teach the wrong cost model — worth correcting rather
        // than leaving, because the model is what a caller uses to
        // decide how to nest.
        //
        // Both calls here are BASIS-path calls, from two successive
        // Auto-sized ancestors: each one recurses through
        // `intrinsic_size` to find its own content size, and each
        // recursion reaches the leaf. The placement re-measure the old
        // comment named is real (`solve.rs`, the `cross_size` fold) but
        // it sits behind `match align { Stretch => cross_avail, _ =>
        // intrinsic_size(..) }`, and `Align::Stretch` is the DEFAULT —
        // so it does not fire in this shape at all. Give the card
        // `align_items(Start)` and the count here goes to 3.
        //
        // The real model is therefore: **1 + the number of Auto-sized
        // ancestors between the leaf and the root**, pinned across
        // depths in
        // `ui::tests::the_solver_asks_once_per_auto_ancestor_which_is_what_makes_the_memo_worth_having`.
        // Every wrapper `Element` between a card and its text re-measures
        // the whole subtree beneath it. For a real `ViewNode::Text` that
        // callback is `text::measure`, wrap-aware and not cheap (see
        // `solve_cost_table`: 48x the box arithmetic around it).
        //
        // This stays at 2 after the per-leaf memo landed, and that is
        // correct rather than a miss: the memo lives in `ui::mount`'s
        // text-leaf closure, so a raw `LayoutTree` built here has none.
        // What this guard measures is the SOLVER's demand; what the memo
        // changes is how much of that demand reaches `text::measure`.
        let asked = Rc::new(RefCell::new(0usize));
        let counter = Rc::clone(&asked);
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column().scroll());
        // No explicit height: the card must ask its child how tall it
        // is, which is what a feed row looks like.
        let card = tree.add(Style::column().padding(Edges::all(1)).clip());
        let body = tree.add_leaf(
            Style::default(),
            Box::new(move |_avail: Size| {
                *counter.borrow_mut() += 1;
                Size::new(60, 2)
            }),
        );
        tree.add_child(root, card);
        tree.add_child(card, body);
        solve(&mut tree, root, Rect::new(0, 0, 80, 40));

        let per_solve = *asked.borrow();
        assert!(
            per_solve > 0,
            "the leaf was never measured, so this counts nothing"
        );
        assert_eq!(
            per_solve, 2,
            "two Auto-sized ancestors (root and card) means two basis-path \
             measures of this leaf; it is now {per_solve}. If this rose, \
             every text leaf under this shape got more expensive; if it \
             fell to 1, a pass was removed and solve_cost_table should be \
             re-run"
        );

        // The placement re-measure the old comment credited these two
        // calls to. It is real, and it is a THIRD call — so naming it as
        // one of the two above was wrong on its own terms.
        let asked = Rc::new(RefCell::new(0usize));
        let counter = Rc::clone(&asked);
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column().scroll());
        let card = tree.add(
            Style::column()
                .padding(Edges::all(1))
                .align_items(Align::Start)
                .clip(),
        );
        let body = tree.add_leaf(
            Style::default(),
            Box::new(move |_avail: Size| {
                *counter.borrow_mut() += 1;
                Size::new(60, 2)
            }),
        );
        tree.add_child(root, card);
        tree.add_child(card, body);
        solve(&mut tree, root, Rect::new(0, 0, 80, 40));
        assert_eq!(
            *asked.borrow(),
            3,
            "with a non-Stretch align the cross-axis re-measure fires and adds a THIRD \
             call. If this equals the Stretch count, that fold has stopped running and \
             the model above is wrong again"
        );
    }

    /// The magnitude behind the guard above, reproducible with
    /// `cargo test --release --lib solve_cost_table -- --ignored
    /// --nocapture`.
    ///
    /// This is an INSTRUMENT, not a guard: wall clock cannot be
    /// asserted on without flaking, so the falsifiable claim lives in
    /// `layout_cost_follows_retained_rows_not_visible_ones` and this
    /// only prints what that shape costs. It is in the tree rather
    /// than in a message so the number can be re-measured on the
    /// machine that doubts it, which a quoted millisecond cannot be.
    ///
    /// Each row is three nodes — a clipped card with padding wrapping a
    /// title and a body leaf — because a one-leaf row understates a
    /// real feed and would make the engine look cheaper than it is.
    ///
    /// It prints TWO columns, and the split is the whole point. A leaf
    /// answering a constant isolates the solver's own box arithmetic;
    /// a leaf answering through `text::measure` — the engine's one
    /// width authority, and what every real `ViewNode::Text` gets in
    /// `ui/mount.rs` — carries wrap-aware measurement with it. Timing
    /// only the first would attribute the whole per-row cost to the
    /// solver, which is exactly the mistake this instrument exists to
    /// prevent.
    #[test]
    #[ignore = "timing instrument; run --release with --nocapture"]
    fn solve_cost_table() {
        const BODY: &str = "the seam you flagged is real and the transit guard \
                            I mentioned is in the same file, a few lines below \
                            the one you quoted back at me";

        fn build(rows: i32, real_text: bool) -> (LayoutTree, LayoutId) {
            let mut tree = LayoutTree::new();
            let root = tree.add(Style::column().scroll());
            for _ in 0..rows {
                let card = tree.add(Style::column().padding(Edges::all(1)).clip());
                let (title, body) = if real_text {
                    (
                        tree.add_leaf(
                            Style::default(),
                            Box::new(|avail| crate::text::measure("agora-tui", avail)),
                        ),
                        tree.add_leaf(
                            Style::default(),
                            Box::new(|avail| crate::text::measure(BODY, avail)),
                        ),
                    )
                } else {
                    (
                        tree.add_leaf(Style::default(), Box::new(|_| Size::new(9, 1))),
                        tree.add_leaf(Style::default(), Box::new(|_| Size::new(60, 2))),
                    )
                };
                tree.add_child(root, card);
                tree.add_child(card, title);
                tree.add_child(card, body);
            }
            (tree, root)
        }

        fn time_solve(rows: i32, real_text: bool) -> (usize, std::time::Duration) {
            let viewport = Rect::new(0, 0, 80, 40);
            let (mut tree, root) = build(rows, real_text);
            // One warm solve first: the first pass over a fresh tree
            // pays allocation the steady state does not.
            solve(&mut tree, root, viewport);
            let start = std::time::Instant::now();
            for _ in 0..10 {
                // Dirty the root so each pass is a real re-solve and
                // not an incremental no-op.
                tree.set_style(root, Style::column().scroll());
                solve(&mut tree, root, viewport);
            }
            (tree.len(), start.elapsed() / 10)
        }

        println!("viewport 80x40, every row retained, 10 re-solves averaged");
        println!("rows  nodes   box arithmetic   with text::measure");
        let mut widest = std::time::Duration::ZERO;
        for rows in [50, 100, 200, 400, 800] {
            let (nodes, bare) = time_solve(rows, false);
            let (_, texty) = time_solve(rows, true);
            println!("{rows:4}  {nodes:5}   {bare:>14?}   {texty:>18?}");
            widest = widest.max(texty);
        }
        assert!(
            widest > std::time::Duration::ZERO,
            "the largest solve took no measurable time, so this \
             instrument measured nothing"
        );
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
