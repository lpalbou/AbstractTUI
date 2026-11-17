//! `TextArea` side strokes: off by default (text from the first column),
//! `strokes(true)` brings the frame back, and every geometry consumer (paint, wrap width, caret anchor)
//! agreeing on that one frame. A `#[path]` sibling of `textarea.rs`.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::itest_util::{key, mount_widget, render, settle, type_str};

const W: i32 = 8;

fn mount(
    strokes: bool,
    inset: (i32, i32),
) -> (crate::reactive::RootScope, crate::ui::UiTree, TextAreaState) {
    let t = default_theme().tokens;
    let slot: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let out = slot.clone();
    let (root, tree) = mount_widget(Size::new(16, 4), move |cx| {
        let state = TextAreaState::new(cx);
        *slot.borrow_mut() = Some(state.clone());
        Element::new()
            .style(LayoutStyle::column())
            .child(
                TextArea::new()
                    .state(&state)
                    .strokes(strokes)
                    .inset(inset.0, inset.1)
                    .placeholder("describe a task")
                    .placeholder_while_focused(true)
                    .rows(1, 4)
                    .layout(LayoutStyle::default().width(Dimension::Cells(W)))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let state = out.borrow_mut().take().expect("mounted");
    (root, tree, state)
}

fn has_stroke(canvas: &crate::ui::BufferCanvas, rows: i32) -> bool {
    (0..rows).any(|y| canvas.row_text(y).contains(['▐', '▌']))
}

#[test]
fn the_default_paints_no_strokes() {
    let t = default_theme().tokens;
    let size = Size::new(16, 2);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        TextArea::new().placeholder("hint").element(cx, &t).build()
    });
    let canvas = render(&mut tree, size);
    assert!(!has_stroke(&canvas, 2), "{:?}", canvas.row_text(0));
    assert!(
        canvas.row_text(0).starts_with("hint"),
        "{:?}",
        canvas.row_text(0)
    );
}

#[test]
fn strokes_true_brings_the_frame_back() {
    let (_root, mut tree, _) = mount(true, (0, 0));
    let canvas = render(&mut tree, Size::new(16, 4));
    assert_eq!(canvas.cell(Point::new(0, 0)).unwrap().0, '▐');
    assert_eq!(canvas.cell(Point::new(W - 1, 0)).unwrap().0, '▌');
}

#[test]
fn strokes_off_paints_no_frame_and_the_caret_owns_column_zero() {
    let size = Size::new(16, 4);
    let theme = default_theme();
    let (_root, mut tree, state) = mount(false, (0, 0));
    key(&mut tree, Key::Tab);
    let canvas = settle(&mut tree, size);
    assert!(
        !has_stroke(&canvas, 4),
        "no side stroke: {:?}",
        canvas.row_text(0)
    );
    assert_eq!(
        canvas.cell(Point::new(0, 0)).unwrap().2,
        theme.tokens.cursor,
        "the caret block sits in the first column"
    );
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().0,
        'd',
        "hint one past the caret"
    );
    assert_eq!(state.caret_cell().get_untracked(), Some(Point::new(0, 0)));

    type_str(&mut tree, "hi");
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(0).starts_with("hi"),
        "{:?}",
        canvas.row_text(0)
    );
    assert_eq!(state.caret_cell().get_untracked(), Some(Point::new(2, 0)));
}

#[test]
fn strokes_off_wraps_at_the_full_width() {
    let size = Size::new(16, 4);
    let (_root, mut tree, state) = mount(false, (0, 0));
    key(&mut tree, Key::Tab);
    type_str(&mut tree, "abcdefghij");
    let canvas = settle(&mut tree, size);
    // All W columns carry text (with strokes only W - 2 would).
    assert!(
        canvas.row_text(0).starts_with("abcdefgh"),
        "{:?}",
        canvas.row_text(0)
    );
    assert!(
        canvas.row_text(1).starts_with("ij"),
        "{:?}",
        canvas.row_text(1)
    );
    assert_eq!(state.caret_cell().get_untracked(), Some(Point::new(2, 1)));
}

#[test]
fn strokes_off_keeps_the_inset() {
    let size = Size::new(16, 4);
    let (_root, mut tree, state) = mount(false, (2, 0));
    key(&mut tree, Key::Tab);
    type_str(&mut tree, "x");
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(0).starts_with("  x"),
        "{:?}",
        canvas.row_text(0)
    );
    assert_eq!(state.caret_cell().get_untracked(), Some(Point::new(3, 0)));
}
