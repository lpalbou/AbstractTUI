//! TextArea `on_paste` intercept tests (backlog first-app/0273) —
//! split sibling for the file budget, `#[path]`-included as
//! `textarea::paste_tests`. Driver-level wire-byte coverage lives in
//! `tests/wave_attachments.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use super::{TextArea, TextAreaState};
use crate::base::Size;
use crate::theme::default_theme;
use crate::ui::{Key, UiEvent, UiTree};
use crate::widgets::itest_util::{key, mount_widget, type_str};
use crate::widgets::PasteAction;

const SIZE: Size = Size::new(24, 5);

struct Rig {
    root: crate::reactive::RootScope,
    tree: UiTree,
    state: TextAreaState,
    seen: Rc<RefCell<Vec<String>>>,
    changes: Rc<RefCell<Vec<String>>>,
}

fn rig(action: PasteAction) -> Rig {
    let seen: Rc<RefCell<Vec<String>>> = Default::default();
    let changes: Rc<RefCell<Vec<String>>> = Default::default();
    let (s2, c2) = (seen.clone(), changes.clone());
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let (root, mut tree) = mount_widget(SIZE, move |cx| {
        let t = &default_theme().tokens;
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        TextArea::new()
            .state(&state)
            .on_change(move |v| c2.borrow_mut().push(v.to_string()))
            .on_paste(move |text| {
                s2.borrow_mut().push(text.to_string());
                action
            })
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab); // focus
    let state = holder.borrow().clone().expect("state");
    Rig {
        root,
        tree,
        state,
        seen,
        changes,
    }
}

#[test]
fn hook_sees_the_raw_paste_text_before_any_insertion() {
    let mut r = rig(PasteAction::Consume);
    r.tree
        .dispatch(&UiEvent::Paste("/a/one.txt\r\nline".into()));
    // RAW: line endings NOT normalized on the way to the hook.
    assert_eq!(r.seen.borrow().as_slice(), ["/a/one.txt\r\nline"]);
    r.root.dispose();
}

#[test]
fn consume_inserts_nothing_and_touches_no_state() {
    let mut r = rig(PasteAction::Consume);
    type_str(&mut r.tree, "hi");
    r.state.push_history("prior entry");
    let caret_before = r.state.caret_byte();
    let changes_before = r.changes.borrow().len();
    r.tree.dispatch(&UiEvent::Paste("/tmp/dropped.png".into()));
    assert_eq!(r.state.text(), "hi", "nothing inserted");
    assert_eq!(r.state.caret_byte(), caret_before, "caret untouched");
    assert_eq!(r.state.history_len(), 1, "history untouched");
    assert_eq!(
        r.changes.borrow().len(),
        changes_before,
        "no on_change for a consumed paste"
    );
    assert_eq!(r.seen.borrow().len(), 1, "hook fired exactly once");
    r.root.dispose();
}

#[test]
fn insert_path_is_byte_identical_to_an_unhooked_textarea() {
    // Hooked-Insert twin…
    let mut hooked = rig(PasteAction::Insert);
    hooked
        .tree
        .dispatch(&UiEvent::Paste("first\r\nsecond\rthird".into()));
    // …against the pre-0273 widget shape (no hook bound).
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let plain_changes: Rc<RefCell<Vec<String>>> = Default::default();
    let pc2 = plain_changes.clone();
    let (root, mut plain_tree) = mount_widget(SIZE, move |cx| {
        let t = &default_theme().tokens;
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        TextArea::new()
            .state(&state)
            .on_change(move |v| pc2.borrow_mut().push(v.to_string()))
            .element(cx, t)
            .build()
    });
    key(&mut plain_tree, Key::Tab);
    plain_tree.dispatch(&UiEvent::Paste("first\r\nsecond\rthird".into()));
    let plain_state = holder.borrow().clone().expect("state");

    assert_eq!(hooked.state.text(), plain_state.text());
    assert_eq!(hooked.state.text(), "first\nsecond\nthird");
    assert_eq!(hooked.state.caret_byte(), plain_state.caret_byte());
    assert_eq!(
        *hooked.changes.borrow(),
        *plain_changes.borrow(),
        "same on_change sequence"
    );
    root.dispose();
    hooked.root.dispose();
}

#[test]
fn hook_disposing_the_scope_is_safe_on_both_arms() {
    for action in [PasteAction::Consume, PasteAction::Insert] {
        let mut tree = UiTree::new(SIZE);
        let (root, ()) = crate::reactive::create_root(|cx| {
            let modal_cx = cx.child();
            let state = TextAreaState::new(modal_cx);
            let t = &default_theme().tokens;
            let view = TextArea::new()
                .state(&state)
                .on_paste(move |_| {
                    // The paste closes the composer (attachment modal
                    // handoff). Insert-after-dispose must be treated
                    // as consumed, never a dead-signal panic.
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
