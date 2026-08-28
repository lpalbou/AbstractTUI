//! A grid child's margins come out of its cell.
//!
//! `Style::margin()` used to be a silent no-op on a grid child: the call
//! compiled, nothing threw, and the value was discarded — the whole suite
//! was byte-identical with and without it, because nothing observed it in
//! either direction. Flex has always honoured margins; grid now does too.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{solve, Align, Edges, LayoutId, LayoutTree, Style, Track};

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x.max(b.x) < a.right().min(b.right()) && a.y.max(b.y) < a.bottom().min(b.bottom())
}

#[test]
fn a_margined_grid_child_is_inset_on_every_edge() {
    let mut tree = LayoutTree::new();
    // An explicit row track, so the cell is the whole container and the
    // arithmetic is about margins rather than about Auto row sizing.
    let root = tree.add(Style::default().grid(vec![Track::Fr(1.0)], vec![Track::Fr(1.0)]));
    let kid = tree.add(Style::default().margin(Edges::all(3)));
    tree.add_child(root, kid);
    solve(&mut tree, root, Rect::new(0, 0, 40, 10));

    assert_eq!(
        tree.rect(kid),
        Rect::new(3, 3, 34, 4),
        "the child fills its cell LESS its own margins"
    );
}

#[test]
fn margins_separate_neighbouring_grid_children() {
    let mut tree = LayoutTree::new();
    let root = tree.add(
        Style::default()
            .grid(vec![Track::Fr(1.0), Track::Fr(1.0)], vec![])
            .gap(2),
    );
    let a = tree.add(Style::default().margin(Edges::all(1)));
    let b = tree.add(Style::default().margin(Edges::all(1)));
    tree.add_child(root, a);
    tree.add_child(root, b);
    solve(&mut tree, root, Rect::new(0, 0, 40, 10));

    let (ra, rb) = (tree.rect(a), tree.rect(b));
    assert!(!overlaps(ra, rb), "{ra:?} overlaps {rb:?}");
    // Cell gap 2 plus one margin from each side.
    assert_eq!(rb.x - ra.right(), 4, "separation = gap + both margins");
}

#[test]
fn an_auto_track_is_wide_enough_for_content_plus_margins() {
    // Auto sizes to the child's intrinsic requirement. That requirement
    // is the MARGIN box: a track sized to the content alone would leave
    // the margins nowhere to go.
    let measured = |margin: i32| {
        let mut tree = LayoutTree::new();
        let root = tree.add(
            Style::default()
                .grid(vec![Track::Auto, Track::Fr(1.0)], vec![])
                .align_items(Align::Start),
        );
        let kid = tree.add_leaf(
            Style::default().margin(Edges::hv(margin, 0)),
            Box::new(|_: Size| Size::new(10, 1)),
        );
        tree.add_child(root, kid);
        let filler = tree.add(Style::default());
        tree.add_child(root, filler);
        solve(&mut tree, root, Rect::new(0, 0, 40, 10));
        tree.rect(kid)
    };

    let bare = measured(0);
    let marg = measured(4);
    assert_eq!(bare.w, 10, "auto track fits the content");
    assert_eq!(
        marg.w, 10,
        "the child still gets its full content width — the TRACK grew to \
         make room for the margins rather than the child being squeezed"
    );
    assert_eq!(marg.x, 4, "inset by the leading margin");
}

// The row-height counterpart of the Auto-track test above. Kept separate
// because the column and row passes are separate code, and a single test
// covering both would let one of them regress silently.
#[test]
fn an_auto_row_is_tall_enough_for_content_plus_margins() {
    let measured = |margin: i32| {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::default().grid(vec![Track::Fr(1.0)], vec![]));
        let kid = tree.add_leaf(
            Style::default().margin(Edges::hv(0, margin)),
            Box::new(|_: Size| Size::new(8, 3)),
        );
        tree.add_child(root, kid);
        solve(&mut tree, root, Rect::new(0, 0, 40, 20));
        tree.rect(kid)
    };

    assert_eq!(measured(0).h, 3, "auto row fits the content");
    let marg = measured(2);
    assert_eq!(
        marg.h, 3,
        "the child keeps its full content height — the ROW grew for the \
         margins instead of the child being crushed"
    );
    assert_eq!(marg.y, 2, "inset by the leading margin");
}

#[test]
fn alignment_happens_inside_the_margin_box() {
    // Note `align_self`, not the container's `align_items`: grid reads
    // only the per-child override today. That asymmetry with flex is a
    // separate gap and is not what this test is about.
    let aligned = |align: Align| {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::default().grid(vec![Track::Fr(1.0)], vec![Track::Fr(1.0)]));
        let kid = tree.add(
            Style::default()
                .margin(Edges::all(2))
                .align_self(align)
                .w(10)
                .h(2),
        );
        tree.add_child(root, kid);
        solve(&mut tree, root, Rect::new(0, 0, 40, 10));
        tree.rect(kid)
    };

    // Cell is 40x10; the margin box is 36x6 at (2,2).
    assert_eq!(aligned(Align::Start), Rect::new(2, 2, 10, 2));
    assert_eq!(
        aligned(Align::End),
        Rect::new(28, 6, 10, 2),
        "flush to the margin edge, not the cell edge"
    );
    assert_eq!(aligned(Align::Center), Rect::new(15, 4, 10, 2));
}

#[test]
fn margins_larger_than_the_cell_clamp_instead_of_going_negative() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::default().grid(vec![Track::Fr(1.0)], vec![]));
    let kid: LayoutId = tree.add(Style::default().margin(Edges::all(50)));
    tree.add_child(root, kid);
    solve(&mut tree, root, Rect::new(0, 0, 10, 6));

    let r = tree.rect(kid);
    assert!(r.w >= 0 && r.h >= 0, "no negative extent: {r:?}");
    assert_eq!((r.w, r.h), (0, 0), "crushed to nothing, not inverted");
}
