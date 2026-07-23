//! TextInput `on_paste` intercept tests (backlog first-app/0273) —
//! split sibling for the file budget, `#[path]`-included as
//! `input::paste_tests`. TextArea's twin cases live in
//! `textarea_paste_tests.rs`; the two suites pin UNIFORM semantics.

use std::cell::RefCell;
use std::rc::Rc;

use super::TextInput;
use crate::base::Size;
use crate::reactive::Signal;
use crate::theme::default_theme;
use crate::ui::{Key, UiEvent, UiTree};
use crate::widgets::itest_util::{key, mount_widget, type_str};
use crate::widgets::PasteAction;

const SIZE: Size = Size::new(24, 3);

/// (root, tree, value signal, hook-seen pastes, inserted values).
type FieldRig = (
    crate::reactive::RootScope,
    UiTree,
    Signal<String>,
    Rc<RefCell<Vec<String>>>,
    Rc<RefCell<Vec<String>>>,
);

fn field(masked: bool, action: PasteAction) -> FieldRig {
    let seen: Rc<RefCell<Vec<String>>> = Default::default();
    let changes: Rc<RefCell<Vec<String>>> = Default::default();
    let (s2, c2) = (seen.clone(), changes.clone());
    let holder: Rc<RefCell<Option<Signal<String>>>> = Default::default();
    let h2 = holder.clone();
    let (root, mut tree) = mount_widget(SIZE, move |cx| {
        let t = &default_theme().tokens;
        let value = cx.signal(String::new());
        *h2.borrow_mut() = Some(value);
        TextInput::new()
            .value(value)
            .masked(masked)
            .on_change(move |v| c2.borrow_mut().push(v.to_string()))
            .on_paste(move |text| {
                s2.borrow_mut().push(text.to_string());
                action
            })
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab); // focus
    let value = holder.borrow().expect("value signal");
    (root, tree, value, seen, changes)
}

#[test]
fn consume_inserts_nothing_no_change_fires_hook_sees_raw_text() {
    let (root, mut tree, value, seen, changes) = field(false, PasteAction::Consume);
    type_str(&mut tree, "q");
    let changes_before = changes.borrow().len();
    tree.dispatch(&UiEvent::Paste("/a/My File.txt\n".into()));
    // RAW text reaches the hook — the fold-to-spaces is Insert-only.
    assert_eq!(seen.borrow().as_slice(), ["/a/My File.txt\n"]);
    assert_eq!(value.get_untracked(), "q", "nothing inserted");
    assert_eq!(changes.borrow().len(), changes_before, "no on_change");
    root.dispose();
}

#[test]
fn insert_path_is_byte_identical_to_an_unhooked_input() {
    let (root, mut tree, value, _seen, changes) = field(false, PasteAction::Insert);
    tree.dispatch(&UiEvent::Paste("two\nlines".into()));
    // Unhooked twin (pre-0273 shape).
    let holder: Rc<RefCell<Option<Signal<String>>>> = Default::default();
    let h2 = holder.clone();
    let plain_changes: Rc<RefCell<Vec<String>>> = Default::default();
    let pc2 = plain_changes.clone();
    let (plain_root, mut plain_tree) = mount_widget(SIZE, move |cx| {
        let t = &default_theme().tokens;
        let v = cx.signal(String::new());
        *h2.borrow_mut() = Some(v);
        TextInput::new()
            .value(v)
            .on_change(move |s| pc2.borrow_mut().push(s.to_string()))
            .element(cx, t)
            .build()
    });
    key(&mut plain_tree, Key::Tab);
    plain_tree.dispatch(&UiEvent::Paste("two\nlines".into()));
    let plain_value = holder.borrow().expect("value");

    assert_eq!(value.get_untracked(), plain_value.get_untracked());
    assert_eq!(value.get_untracked(), "two lines", "single-line fold kept");
    assert_eq!(*changes.borrow(), *plain_changes.borrow());
    plain_root.dispose();
    root.dispose();
}

#[test]
fn masked_field_still_fires_the_hook_and_can_block_pastes() {
    // The documented password-field use: Consume unconditionally.
    let (root, mut tree, value, seen, _changes) = field(true, PasteAction::Consume);
    tree.dispatch(&UiEvent::Paste("hunter2".into()));
    assert_eq!(seen.borrow().as_slice(), ["hunter2"], "hook fired masked");
    assert_eq!(value.get_untracked(), "", "paste blocked");
    // And a masked field that ALLOWS pasting still inserts.
    root.dispose();
    let (root, mut tree, value, _seen, _changes) = field(true, PasteAction::Insert);
    tree.dispatch(&UiEvent::Paste("hunter2".into()));
    assert_eq!(value.get_untracked(), "hunter2");
    root.dispose();
}

#[test]
fn hook_disposing_the_scope_is_safe_on_both_arms() {
    for action in [PasteAction::Consume, PasteAction::Insert] {
        let mut tree = UiTree::new(SIZE);
        let (root, ()) = crate::reactive::create_root(|cx| {
            let modal_cx = cx.child();
            let t = &default_theme().tokens;
            let view = TextInput::new()
                .on_paste(move |_| {
                    modal_cx.dispose();
                    action
                })
                .element(modal_cx, t)
                .build();
            tree.mount(modal_cx, view);
        });
        tree.layout();
        key(&mut tree, Key::Tab);
        tree.dispatch(&UiEvent::Paste("/tmp/x".into()));
        assert_eq!(
            tree.instance_count(),
            0,
            "{action:?}: subtree unmounted by dispose, no panic"
        );
        root.dispose();
    }
}
