//! A wrap line must be tall enough for the children that STRETCH into it.
//!
//! `wrap.rs` computes each line's cross extent as the max of its members'
//! cross sizes, then lets `Align::Stretch` members fill it. The gap this
//! pins: a Stretch member's own content contributed NOTHING to that max,
//! so the line was only ever as tall as its non-stretch members — and
//! `Stretch` is the DEFAULT `align_items`, so this is the ordinary case
//! rather than an exotic one.
//!
//! CSS resolves the same cycle by sizing the line from every item's
//! HYPOTHETICAL cross size (what it would be if it did not stretch) and
//! only then stretching. That is what the fix does.
//!
//! Written as a fixture rather than folded into the `adv_layout.rs`
//! populations because it names one mechanism; the population that would
//! have caught it is added there separately.

use abstracttui::base::{Rect, Size};
use abstracttui::layout::{solve, Align, LayoutTree, Style};

/// Rows a `chars`-long string wraps to at `width`.
fn wrapped_rows(chars: i32, width: i32) -> i32 {
    let w = width.max(1);
    ((chars + w - 1) / w).max(1)
}

/// A paragraph: it would LIKE `pref` columns, takes fewer if offered
/// fewer, and is as tall as `chars` needs at whatever width it ends up
/// with. The preferred width is what lets it share a wrap line with a
/// sibling instead of monopolising one.
fn para(
    tree: &mut LayoutTree,
    style: Style,
    chars: i32,
    pref: i32,
) -> abstracttui::layout::LayoutId {
    tree.add_leaf(
        style,
        Box::new(move |inner: Size| {
            let w = pref.min(inner.w).max(1);
            Size::new(w, wrapped_rows(chars, w))
        }),
    )
}

/// A stretching paragraph sharing a wrap line with a one-row sibling is
/// given the whole line, and the line is sized for the paragraph.
#[test]
fn a_wrap_line_is_tall_enough_for_its_stretching_paragraph() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::row().wrap().gap(1));
    // Explicit height => a non-stretch member. Before the fix this child
    // alone decided the line's height.
    let chip = tree.add(Style::default().w(4).h(1));
    tree.add_child(root, chip);
    let text = para(&mut tree, Style::default(), 90, 20);
    tree.add_child(root, text);

    solve(&mut tree, root, Rect::new(0, 0, 30, 40));

    let r = tree.rect(text);
    let needs = wrapped_rows(90, r.w);
    assert!(
        r.h >= needs,
        "stretching paragraph solved to {r:?} — {} columns wraps to \
         {needs} rows, but the line gave it {}",
        r.w,
        r.h
    );
    // And the short sibling still fills the (now taller) line, which is
    // what Stretch means: this is not "the line grew and nobody used it".
    assert_eq!(tree.rect(chip).h, 1, "explicit height wins over the line");
}

/// The same shape with the paragraph pinned to `Align::Start`: its cross
/// size was always measured, so this passed before the fix too. It is the
/// control — if BOTH tests move together the fixture is wrong, not the
/// solver.
#[test]
fn control_a_non_stretch_paragraph_was_already_measured() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::row().wrap().gap(1).align_items(Align::Start));
    let chip = tree.add(Style::default().w(4).h(1));
    tree.add_child(root, chip);
    let text = para(&mut tree, Style::default(), 90, 20);
    tree.add_child(root, text);

    solve(&mut tree, root, Rect::new(0, 0, 30, 40));

    let r = tree.rect(text);
    let needs = wrapped_rows(90, r.w);
    assert!(
        r.h >= needs,
        "non-stretch paragraph solved to {r:?} — {} columns wraps to \
         {needs} rows, but it was given {}",
        r.w,
        r.h
    );
}

/// The second half of the same gap: with EVERY member stretching, the old
/// code fell back to an intrinsic measure at the container's full width
/// rather than at the width the child was actually solved to — the stale
/// estimate the solver docs warn about, one layer down.
///
/// The paragraph is alone on its line and wider than it: it shrinks, so
/// the width it renders at is genuinely narrower than the container.
#[test]
fn an_all_stretch_line_measures_at_the_solved_width_not_the_container() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::row().wrap());
    // Side margins take the child off the container width: solved to
    // 30 - 4 = 26 columns while the container offers 30.
    let text = para(
        &mut tree,
        Style::default().margin(abstracttui::layout::Edges::hv(2, 0)),
        90,
        60,
    );
    tree.add_child(root, text);

    solve(&mut tree, root, Rect::new(0, 0, 30, 40));

    let r = tree.rect(text);
    let needs = wrapped_rows(90, r.w);
    assert!(
        r.h >= needs,
        "all-stretch line: paragraph solved to {r:?} — {} columns wraps \
         to {needs} rows, but the line gave it {} (measured at the \
         container width, not the solved one?)",
        r.w,
        r.h
    );
}

/// A column-direction wrap reaches the same code by the other axis: the
/// line's cross extent is a WIDTH there. Same invariant, different axis,
/// so a fix that only handles rows does not pass this.
#[test]
fn a_column_wrap_line_is_wide_enough_for_its_stretching_child() {
    let mut tree = LayoutTree::new();
    let root = tree.add(Style::column().wrap().gap(1));
    let chip = tree.add(Style::default().w(2).h(3));
    tree.add_child(root, chip);
    // In a column the main axis is height: this leaf wants 6 rows and
    // 18 columns, and must not be squeezed to the chip's 2 columns.
    let block = tree.add_leaf(Style::default(), Box::new(|_inner: Size| Size::new(18, 6)));
    tree.add_child(root, block);

    solve(&mut tree, root, Rect::new(0, 0, 40, 20));

    let r = tree.rect(block);
    assert!(
        r.w >= 18,
        "column-wrap line: block wants 18 columns, got {:?}",
        r
    );
}
