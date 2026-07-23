//! TextArea placeholder-clipping tests (first-app/0284), a `#[path]`
//! sibling of `textarea.rs` (file budget — `textarea_tests.rs` is at
//! capacity): both placeholder branches must clip to the widget's
//! INTERIOR columns. Draw closures clip to damage regions, not element
//! rects, so the old unbounded hint print overwrote the widget's own
//! right `▌` stroke and escaped the rect entirely at narrow widths.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::itest_util::{key, mount_widget, render};

/// An 8-wide TextArea inside a 16-wide canvas: columns 8.. are foreign
/// ground the placeholder must never touch.
fn mount_narrow_area(
    canvas: Size,
    widget_w: i32,
    while_focused: bool,
) -> (crate::reactive::RootScope, crate::ui::UiTree) {
    let t = default_theme().tokens;
    // The tree root always fills the viewport, so the width-constrained
    // widget mounts as a CHILD of a plain container.
    mount_widget(canvas, move |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                TextArea::new()
                    .placeholder("a very long hint")
                    .placeholder_while_focused(while_focused)
                    .layout(
                        LayoutStyle::default()
                            .width(Dimension::Cells(widget_w))
                            .height(Dimension::Cells(1)),
                    )
                    .element(cx, &t)
                    .build(),
            )
            .build()
    })
}

#[test]
fn unfocused_placeholder_clips_inside_the_frame() {
    let size = Size::new(16, 1);
    let (_root, mut tree) = mount_narrow_area(size, 8, false);
    let canvas = render(&mut tree, size);
    // Interior = columns 1..=6 (tw = 6): 5 content columns + ellipsis.
    assert_eq!(canvas.cell(Point::new(6, 0)).unwrap().0, '…');
    assert_eq!(
        canvas.cell(Point::new(7, 0)).unwrap().0,
        '▌',
        "the widget's own right stroke survives the hint"
    );
    assert!(
        canvas.row_text(0).chars().skip(8).all(|c| c == ' '),
        "hint escaped the widget rect: {:?}",
        canvas.row_text(0)
    );
}

#[test]
fn focused_placeholder_clips_and_keeps_caret_and_stroke() {
    let size = Size::new(16, 1);
    let theme = default_theme();
    let (_root, mut tree) = mount_narrow_area(size, 8, true);
    key(&mut tree, Key::Tab); // the autofocused-composer state
    let canvas = render(&mut tree, size);
    // Caret block still owns the first interior cell (0291 contract).
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().2,
        theme.tokens.cursor
    );
    // Hint starts one past the caret and clips one short of the
    // stroke (tw - 1 = 5 columns of room).
    assert_eq!(canvas.cell(Point::new(6, 0)).unwrap().0, '…');
    assert_eq!(canvas.cell(Point::new(7, 0)).unwrap().0, '▌');
    assert!(
        canvas.row_text(0).chars().skip(8).all(|c| c == ' '),
        "focused hint escaped the widget rect: {:?}",
        canvas.row_text(0)
    );
}

#[test]
fn width_three_placeholder_degrades_to_a_bare_ellipsis() {
    let size = Size::new(8, 1);
    let (_root, mut tree) = mount_narrow_area(size, 3, true);
    // Unfocused, tw = 1: a lone ellipsis between intact strokes.
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.cell(Point::new(0, 0)).unwrap().0, '▐');
    assert_eq!(canvas.cell(Point::new(1, 0)).unwrap().0, '…');
    assert_eq!(canvas.cell(Point::new(2, 0)).unwrap().0, '▌');
    assert!(canvas.row_text(0).chars().skip(3).all(|c| c == ' '));
    // Focused: the while-focused hint has no room (`tw > 1` guard) —
    // caret only, strokes intact, still nothing beyond the rect.
    key(&mut tree, Key::Tab);
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.cell(Point::new(2, 0)).unwrap().0, '▌');
    assert!(canvas.row_text(0).chars().skip(3).all(|c| c == ' '));
}
