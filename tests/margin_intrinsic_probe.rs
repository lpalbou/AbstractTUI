//! Does the intrinsic pass deduct a child's OWN margins?
//!
//! A wrapping leaf answers a row count for the line length it is given.
//! If the intrinsic pass hands it the full content width while placement
//! will actually solve it to `width - its own margins`, the row count is
//! for a wrap that never happens.
//!
//! Two sites implement this, and each is pinned here by exactly one test:
//! `solve.rs` `intrinsic_size` (the container aggregation) and the
//! flex-basis estimate in `layout_children_of`. Reverting either turns
//! its own test red plus the invariant test at the bottom.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{measure, solve, Edges, LayoutId, LayoutTree, Style};

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

// The flex-basis estimate. In a column the child's MAIN axis is its
// height, taken from the basis and never re-derived during placement —
// unlike the cross axis, which placement re-measures at an already
// margin-deducted `cross_avail`. That asymmetry is why this shape can
// see the intrinsic pass being wrong and a width assertion cannot.
#[test]
fn column_child_height_accounts_for_its_own_side_margins() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::column());
    let leaf = wrapping_leaf(&mut tree, Style::default().margin(Edges::hv(3, 0)));
    tree.add_child(root, leaf);

    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    let r = tree.rect(leaf);
    // Solved to 40 - 3 - 3 = 34 columns wide, so 120 cols wrap to 4 rows.
    assert_eq!(r.w, 34, "leaf width");
    assert_eq!(
        r.h, 4,
        "leaf must be as tall as its OWN wrap: measured at the full 40 \
         it answers 3 rows, and the 4th row of text has nowhere to go"
    );
}

// The aggregation branch of `intrinsic_size`, reachable through the
// public `measure` query with no solve at all.
#[test]
fn intrinsic_measure_of_a_container_deducts_its_childs_margins() {
    let mut tree = LayoutTree::new();
    let wrapper = tree.add(Style::column());
    let leaf = wrapping_leaf(&mut tree, Style::default().margin(Edges::hv(3, 0)));
    tree.add_child(wrapper, leaf);

    let m = measure(&tree, wrapper, Size::new(40, 20));
    assert_eq!(
        m.h, 4,
        "the wrapper is as tall as its child wraps at 34, not at 40"
    );
}

// The consequence both sites exist to prevent, stated as the invariant
// rather than as one shape: a content-sized box must be tall enough for
// what it wraps to AT THE WIDTH IT WAS SOLVED TO.
//
// Worth knowing before writing a cheaper check here: the child does NOT
// escape the wrapper when this breaks. Flex shrink absorbs the shortfall,
// so containment holds and the last row is silently truncated instead. A
// `child within parent` assertion — the shape the random-tree invariants
// in `adv_layout.rs` use — cannot see this class at all.
#[test]
fn a_margined_child_is_tall_enough_for_its_own_wrap() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::column());
    let wrapper = tree.add(Style::column());
    let leaf = wrapping_leaf(&mut tree, Style::default().margin(Edges::hv(3, 0)));
    tree.add_child(root, wrapper);
    tree.add_child(wrapper, leaf);

    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    let w = tree.rect(wrapper);
    let l = tree.rect(leaf);
    let needs = (120 + l.w - 1) / l.w.max(1);
    assert!(
        l.h >= needs,
        "leaf {l:?} is solved to {} columns, which wraps to {needs} rows, \
         but it was given only {} — the last row is truncated, not \
         overflowed: it sits inside wrapper {w:?}",
        l.w,
        l.h
    );
}
