//! TextArea disposal-safety tests (backlog 0297, the 0250 ruling
//! engine-wide) — split sibling of textarea_tests.rs for the file
//! budget, `#[path]`-included as `textarea::disposal_tests`.

use super::{TextArea, TextAreaState};
use crate::base::Size;
use crate::theme::default_theme;
use crate::ui::{Key, UiTree};
use crate::widgets::itest_util::{key, type_str};

/// Disposal-safety law (backlog 0297): the TextArea completes ALL of
/// its signal writes — buffer, caret, caret-cell publish — before
/// `on_submit` runs, so the callback may dispose the TextArea's scope
/// synchronously (the submit-and-close composer). Before the fix the
/// post-callback caret publish read the just-disposed caret signal
/// and panicked.
#[test]
fn on_submit_may_dispose_the_textareas_scope() {
    let size = Size::new(12, 4);
    let t = &default_theme().tokens;
    let submitted: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let s2 = submitted.clone();
    let mut tree = UiTree::new(size);
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let state = TextAreaState::new(modal_cx);
        let view = TextArea::new()
            .state(&state)
            .on_submit(move |v| {
                s2.borrow_mut().push(v.to_string());
                modal_cx.dispose();
            })
            .element(modal_cx, t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab); // focus
    type_str(&mut tree, "hi");
    key(&mut tree, Key::Enter); // submit -> dispose, mid-dispatch
    assert_eq!(submitted.borrow().as_slice(), ["hi".to_string()]);
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

/// Same law, `on_change` arm: an edit callback that closes its panel
/// (filter-as-you-type panes) is legal — the caret publish that used
/// to run after it is now widget bookkeeping done BEFORE.
#[test]
fn on_change_may_dispose_the_textareas_scope() {
    let size = Size::new(12, 4);
    let t = &default_theme().tokens;
    let mut tree = UiTree::new(size);
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let state = TextAreaState::new(modal_cx);
        let view = TextArea::new()
            .state(&state)
            .on_change(move |_| modal_cx.dispose())
            .element(modal_cx, t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab); // focus
    key(&mut tree, Key::Char('q')); // edit -> on_change -> dispose
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}
