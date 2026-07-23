//! Scroll tests for the 0260/0850 enablers: `extent_signal` (the
//! app-visible content extent) and `scrollbar_auto_hide` (no bar
//! while the content fits). Sibling of `scroll_tests.rs` for the
//! file-size discipline; shares its settle helper.

use std::cell::RefCell;
use std::rc::Rc;

use super::tests::{settle, tall_content};
use super::*;
use crate::base::Size;
use crate::layout::Style as LayoutStyle;
use crate::theme::default_theme;
use crate::ui::{text, Element, MouseButton, MouseKind, View};
use crate::widgets::itest_util::{mount_widget, mouse};

/// Test slot for capturing the bound extent signal out of the mount.
type ExtentSlot = Rc<RefCell<Option<crate::reactive::Signal<(i32, i32)>>>>;

#[test]
fn extent_signal_reports_the_measured_content_and_the_hint() {
    // Measured mode: the solver's answer lands in the bound signal one
    // settle turn after the first draw discovers it.
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let holder: ExtentSlot = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let extent = cx.signal((0i32, 0i32));
        *h.borrow_mut() = Some(extent);
        Scroll::new(content)
            .extent_signal(extent)
            .element(cx, t)
            .build()
    });
    let extent = holder.borrow().expect("signal");
    let _ = settle(&mut tree, size);
    assert_eq!(
        extent.get_untracked().1,
        20,
        "measured height published to the bound signal"
    );

    // Hint mode: the hint lands verbatim at build (nothing measured).
    let holder2: ExtentSlot = Rc::new(RefCell::new(None));
    let h2 = holder2.clone();
    let (content, _) = tall_content();
    let (_root2, mut tree2) = mount_widget(size, move |cx| {
        let extent = cx.signal((0i32, 0i32));
        *h2.borrow_mut() = Some(extent);
        Scroll::new(content)
            .content_size(10, 30)
            .extent_signal(extent)
            .element(cx, t)
            .build()
    });
    let extent2 = holder2.borrow().expect("signal");
    let _ = settle(&mut tree2, size);
    assert_eq!(extent2.get_untracked(), (10, 30), "hint lands verbatim");
}

#[test]
fn scrollbar_auto_hide_hides_on_fit_shows_on_overflow_and_inerts_drags() {
    // Fitting content (3 rows in a 4-row viewport): the bar column is
    // reserved but paints bare ground; a drag on the strip must not
    // move the offset (invisible targets are inert).
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let short: View = {
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..3 {
            col = col.child(text(format!("row {i}")));
        }
        col.build()
    };
    let holder: Rc<RefCell<Option<crate::reactive::Signal<i32>>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let offset = cx.signal(0i32);
        *h.borrow_mut() = Some(offset);
        Scroll::new(short)
            .offset_y(offset)
            .scrollbar_auto_hide(true)
            .element(cx, t)
            .build()
    });
    let offset = holder.borrow().expect("signal");
    let canvas = settle(&mut tree, size);
    for y in 0..4 {
        let cell = canvas.cell(crate::base::Point::new(11, y)).map(|c| c.0);
        assert_eq!(cell, Some(' '), "bar column blank on fit at y={y}");
    }
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 11, 3);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 11, 3);
    assert_eq!(offset.get_untracked(), 0, "hidden bar must not steer");

    // Overflowing content: the bar shows and drags steer, exactly as
    // without the flag.
    let (content, _) = tall_content();
    let (_root2, mut tree2) = mount_widget(size, move |cx| {
        Scroll::new(content)
            .scrollbar_auto_hide(true)
            .element(cx, t)
            .build()
    });
    let canvas = settle(&mut tree2, size);
    let bar_cells: String = (0..4)
        .filter_map(|y| canvas.cell(crate::base::Point::new(11, y)).map(|c| c.0))
        .collect();
    assert!(
        bar_cells.contains('┃'),
        "thumb visible on overflow: {bar_cells:?}"
    );
    mouse(&mut tree2, MouseKind::Down(MouseButton::Left), 11, 3);
    mouse(&mut tree2, MouseKind::Up(MouseButton::Left), 11, 3);
    let canvas = settle(&mut tree2, size);
    assert!(
        canvas.row_text(0).starts_with("row 16"),
        "visible bar still steers: {:?}",
        canvas.row_text(0)
    );
}
