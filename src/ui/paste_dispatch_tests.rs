//! Capture-phase paste routing (`Element::on_paste`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Size;
use crate::layout::{Dimension, LayoutStyle};
use crate::reactive::create_root;
use crate::theme::default_theme;
use crate::ui::{Element, UiEvent, UiTree};
use crate::widgets::{List, PasteAction, TextInput};

#[test]
fn capture_on_paste_runs_before_focused_editor() {
    let t = default_theme().tokens;
    let intercepted: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let log = intercepted.clone();
    let mut tree = UiTree::new(Size::new(40, 6));
    let (_root, ()) = create_root(|cx| {
        let value = cx.signal(String::new());
        let view = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .on_paste(move |text| {
                log.borrow_mut().push(text.to_string());
                PasteAction::Consume
            })
            .child(List::of(["sidebar"]).element(cx, &t).build())
            .child(TextInput::new().value(value).element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    tree.layout();
    tree.focus_first();
    // Focus lands on the list or input — tab to the input.
    tree.dispatch(&UiEvent::Key(crate::ui::KeyEvent::plain(
        crate::ui::Key::Tab,
    )));
    let before = tree.dispatch(&UiEvent::Paste("/tmp/file.txt".into()));
    assert!(before, "root capture consumed paste");
    assert_eq!(*intercepted.borrow(), vec!["/tmp/file.txt".to_string()]);
}

#[test]
fn capture_insert_lets_focus_target_handle_paste() {
    let t = default_theme().tokens;
    let mut tree = UiTree::new(Size::new(30, 4));
    let value_probe = Rc::new(RefCell::new(None));
    let probe = value_probe.clone();
    let (_root, ()) = create_root(|cx| {
        let value = cx.signal(String::new());
        *probe.borrow_mut() = Some(value);
        let view = Element::new()
            .on_paste(|_| PasteAction::Insert)
            .child(TextInput::new().value(value).element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    tree.layout();
    tree.focus_first();
    assert!(tree.dispatch(&UiEvent::Paste("hello".into())));
    let value = value_probe.borrow().unwrap();
    assert_eq!(value.get_untracked(), "hello");
}
