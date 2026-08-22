//! Does the margin deduction reach the WRAP and GRID paths?
//!
//! `intrinsic_size` is called from the single-line flex path, the wrap
//! path and the grid path. The rule it enforces — measure a child inside
//! the box it will actually be SOLVED to, i.e. less its own margins — has
//! to hold at every one of them, or the same wrapping leaf answers a row
//! count for a line length it is never drawn at.
//!
//! `control_flex_column_is_tall_enough` is the baseline on the path that
//! already had it. If the control passes and a wrap/grid case fails, the
//! gap is in that path and not in the fixture.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{measure, solve, Align, Edges, LayoutId, LayoutTree, Style, Track};

/// 120 columns of text: rows = ceil(120 / line_width), at least 1.
fn wrapping_leaf(tree: &mut LayoutTree, style: Style) -> LayoutId {
    tree.add_leaf(
        style,
        Box::new(|inner: Size| {
            let w = inner.w.max(1);
            Size::new(inner.w, ((120 + w - 1) / w).max(1))
        }),
    )
}

fn side_margins() -> Style {
    Style::default().margin(Edges::hv(3, 0))
}

/// Rows the leaf actually needs at the width it was solved to.
fn needed_rows(w: i32) -> i32 {
    (120 + w.max(1) - 1) / w.max(1)
}

fn assert_tall_enough(tree: &LayoutTree, leaf: LayoutId, path: &str) {
    let l = tree.rect(leaf);
    let needs = needed_rows(l.w);
    assert!(
        l.h >= needs,
        "{path}: leaf {l:?} is solved to {} columns, which wraps to \
         {needs} rows, but it was given only {}",
        l.w,
        l.h
    );
}

// The control: the single-line flex path, same shape.
#[test]
fn control_flex_column_is_tall_enough() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::column());
    let leaf = wrapping_leaf(&mut tree, side_margins());
    tree.add_child(root, leaf);
    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    assert_eq!(tree.rect(leaf).w, 34, "control: solved width");
    assert_tall_enough(&tree, leaf, "flex");
}

// Same shape with wrap enabled: the line-breaking basis comes from the
// wrap path instead, and is likewise never re-derived.
#[test]
fn wrap_column_child_is_tall_enough_for_its_own_wrap() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::column().wrap());
    let leaf = wrapping_leaf(&mut tree, side_margins());
    tree.add_child(root, leaf);
    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    assert_tall_enough(&tree, leaf, "wrap column");
}

// Non-stretch cross alignment takes the per-child cross measure rather
// than the all-stretch line fallback — a different call site.
#[test]
fn wrap_row_child_is_tall_enough_with_align_start() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::row().wrap().align_items(Align::Start));
    let leaf = wrapping_leaf(&mut tree, side_margins());
    tree.add_child(root, leaf);
    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    assert_tall_enough(&tree, leaf, "wrap row align-start");
}

// GRID is a different case and this pins which one it is. A grid child's
// box is its CELL — grid never reduces it by the child's margins — so
// measuring at the full cell size is self-consistent and the truncation
// class does not arise. What that leaves open is a separate question:
// whether grid should honour margins at all. This asserts only the
// invariant, so it stays true either way and goes red if a future margin
// implementation lands on one side of the pair and not the other.
#[test]
fn grid_child_is_tall_enough_for_its_own_wrap() {
    let mut tree = LayoutTree::new();
    let root = tree.add(
        Style::default()
            .grid(vec![Track::Auto], vec![])
            .align_items(Align::Start),
    );
    let leaf = wrapping_leaf(&mut tree, side_margins());
    tree.add_child(root, leaf);
    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    assert_tall_enough(&tree, leaf, "grid");
}

// The public `measure` query on a wrap container.
#[test]
fn intrinsic_measure_of_a_wrap_container_deducts_child_margins() {
    let mut tree = LayoutTree::new();
    let wrapper = tree.add(Style::column().wrap());
    let leaf = wrapping_leaf(&mut tree, side_margins());
    tree.add_child(wrapper, leaf);

    let m = measure(&tree, wrapper, Size::new(40, 20));
    assert_eq!(m.h, 4, "wrap container measured height");
}
