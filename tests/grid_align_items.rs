//! A grid container's `align_items` reaches its children.
//!
//! Flex resolves a child's alignment as `align_self` falling back to the
//! container's `align_items`. Grid used to hardcode that fallback to
//! `Stretch`, so `Style::align_items()` on a grid container was a silent
//! no-op: it compiled, threw nothing, and was discarded.

use abstracttui::base::Rect;
use abstracttui::layout::{solve, Align, LayoutTree, Style, Track};

/// One 40x10 cell holding a 10x2 child. Returns the child's rect.
fn cell_child(items: Align, self_: Option<Align>) -> Rect {
    let mut tree = LayoutTree::new();
    let root = tree.add(
        Style::default()
            .grid(vec![Track::Fr(1.0)], vec![Track::Fr(1.0)])
            .align_items(items),
    );
    let mut kid = Style::default().w(10).h(2);
    if let Some(a) = self_ {
        kid = kid.align_self(a);
    }
    let kid = tree.add(kid);
    tree.add_child(root, kid);
    solve(&mut tree, root, Rect::new(0, 0, 40, 10));
    tree.rect(kid)
}

#[test]
fn container_align_items_positions_grid_children() {
    assert_eq!(cell_child(Align::Start, None), Rect::new(0, 0, 10, 2));
    assert_eq!(cell_child(Align::Center, None), Rect::new(15, 4, 10, 2));
    assert_eq!(cell_child(Align::End, None), Rect::new(30, 8, 10, 2));
}

#[test]
fn a_childs_align_self_still_overrides_the_container() {
    assert_eq!(
        cell_child(Align::End, Some(Align::Start)),
        Rect::new(0, 0, 10, 2),
        "align_self wins over the container's align_items"
    );
    assert_eq!(
        cell_child(Align::Start, Some(Align::End)),
        Rect::new(30, 8, 10, 2),
        "and in the other direction"
    );
}

#[test]
fn stretch_remains_the_default_when_the_container_says_nothing() {
    // Style::default() is already align_items: Stretch, so a grid that
    // never mentions alignment must be bit-for-bit what it always was:
    // the child fills its cell.
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::default().grid(vec![Track::Fr(1.0)], vec![Track::Fr(1.0)]));
    let kid = tree.add(Style::default());
    tree.add_child(root, kid);
    solve(&mut tree, root, Rect::new(0, 0, 40, 10));

    assert_eq!(tree.rect(kid), Rect::new(0, 0, 40, 10), "fills the cell");
}

#[test]
fn grid_resolves_alignment_the_same_way_flex_does() {
    // The property that was actually broken: not "grid centres things"
    // but "grid and flex agree on where a child's alignment comes from".
    for items in [Align::Start, Align::Center, Align::End] {
        let mut tree = LayoutTree::new();
        let root = tree.add(Style::column().align_items(items));
        let kid = tree.add(Style::default().w(10).h(2));
        tree.add_child(root, kid);
        solve(&mut tree, root, Rect::new(0, 0, 40, 10));
        let flex_x = tree.rect(kid).x;

        let grid_x = cell_child(items, None).x;
        assert_eq!(
            grid_x, flex_x,
            "grid and flex disagree on align_items {items:?}"
        );
    }
}
