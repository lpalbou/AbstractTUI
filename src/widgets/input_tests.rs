//! TextInput tests (split file for the size budget — same
//! crate-private module as `input.rs` via `#[path]`): editing model,
//! selection, paste, submit, placeholder, and the masked mode's
//! draw + `access_value` redaction (the 0510 §masked leak surface).

use super::*;
use crate::base::{Point, Size};
use crate::theme::default_theme;
use crate::ui::{Key, Mods, UiEvent, UiTree};
use crate::widgets::itest_util::{key, key_mod, mount_widget, render, type_str};

fn focused_input(size: Size) -> (crate::reactive::RootScope, UiTree, Signal<String>) {
    let t = &default_theme().tokens;
    let holder: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let value = cx.signal(String::new());
        *h.borrow_mut() = Some(value);
        TextInput::new()
            .value(value)
            .placeholder("type here")
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab); // focus the field
    let sig = holder.borrow().expect("value signal");
    (root, tree, sig)
}

/// Disposal-safety law (backlog 0297): TextInput finishes every signal
/// write (value, caret) before `notify` runs the user callback, so
/// `on_submit`/`on_change` may dispose the input's scope synchronously.
/// Audited clean at filing; pinned for both arms.
#[test]
fn callbacks_may_dispose_the_inputs_scope() {
    let t = &default_theme().tokens;
    // on_submit (Enter) disposes — the submit-and-close dialog.
    let mut tree = UiTree::new(Size::new(16, 1));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let view = TextInput::new()
            .on_submit(move |_| modal_cx.dispose())
            .element(modal_cx, t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab); // focus
    type_str(&mut tree, "hi");
    key(&mut tree, Key::Enter); // submit -> dispose, mid-dispatch
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();

    // on_change (first keystroke) disposes.
    let mut tree = UiTree::new(Size::new(16, 1));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let view = TextInput::new()
            .on_change(move |_| modal_cx.dispose())
            .element(modal_cx, t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Char('q')); // edit -> on_change -> dispose
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

#[test]
fn typing_inserts_and_renders_inside_the_frame() {
    let theme = default_theme();
    let size = Size::new(16, 1);
    let (_root, mut tree, value) = focused_input(size);
    type_str(&mut tree, "hello");
    assert_eq!(value.get_untracked(), "hello");
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().0,
        'h',
        "text starts after the stroke"
    );
    assert!(canvas.row_text(0).contains("hello"));
    // Focused frame wears border_focus (§3.2 bordered row).
    assert_eq!(
        canvas.cell(Point::new(0, 0)).unwrap().1,
        theme.tokens.border_focus
    );
}

#[test]
fn backspace_delete_home_end_word_jump() {
    let size = Size::new(20, 1);
    let (_root, mut tree, value) = focused_input(size);
    type_str(&mut tree, "one two three");
    key(&mut tree, Key::Backspace); // "one two thre"
    assert_eq!(value.get_untracked(), "one two thre");
    key_mod(&mut tree, Key::Left, Mods::ALT); // to start of "thre"
    key(&mut tree, Key::Delete); // "one two hre"
    assert_eq!(value.get_untracked(), "one two hre");
    key(&mut tree, Key::Home);
    key(&mut tree, Key::Delete);
    assert_eq!(value.get_untracked(), "ne two hre");
    key(&mut tree, Key::End);
    type_str(&mut tree, "!");
    assert_eq!(value.get_untracked(), "ne two hre!");
}

#[test]
fn selection_replaces_on_type_and_renders_selected_style() {
    let theme = default_theme();
    let t = &theme.tokens;
    let size = Size::new(20, 1);
    let (_root, mut tree, value) = focused_input(size);
    type_str(&mut tree, "abcdef");
    key(&mut tree, Key::Home);
    key_mod(&mut tree, Key::Right, Mods::SHIFT);
    key_mod(&mut tree, Key::Right, Mods::SHIFT); // select "ab"
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.cell(Point::new(1, 0)).unwrap().2, t.selection_bg);
    type_str(&mut tree, "X"); // replaces the selection
    assert_eq!(value.get_untracked(), "Xcdef");
}

#[test]
fn paste_goes_in_whole_and_scrolls_to_cursor() {
    let size = Size::new(8, 1);
    let (_root, mut tree, value) = focused_input(size);
    tree.dispatch(&UiEvent::Paste("pasted line\nwith break".into()));
    assert_eq!(value.get_untracked(), "pasted line with break");
    // Cursor is at the end; the visible window must include it: the
    // start of the text is scrolled out of the frame.
    let canvas = render(&mut tree, size);
    assert_ne!(
        canvas.cell(Point::new(1, 0)).unwrap().0,
        'p',
        "scrolled: {:?}",
        canvas.row_text(0)
    );
}

#[test]
fn submit_fires_with_current_value() {
    let t = &default_theme().tokens;
    let submitted: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let s2 = submitted.clone();
    let (_root, mut tree) = mount_widget(Size::new(16, 1), move |cx| {
        TextInput::new()
            .on_submit(move |v| s2.borrow_mut().push(v.to_string()))
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab);
    type_str(&mut tree, "ok");
    key(&mut tree, Key::Enter);
    assert_eq!(*submitted.borrow(), vec!["ok".to_string()]);
}

#[test]
fn placeholder_shows_only_unfocused_empty() {
    let size = Size::new(16, 1);
    let t = &default_theme().tokens;
    let (_root, mut tree) = mount_widget(size, |cx| {
        TextInput::new()
            .placeholder("type here")
            .element(cx, t)
            .build()
    });
    let theme = default_theme();
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).contains("type here"));
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().1,
        theme.tokens.text_faint,
        "placeholder ink is text_faint (§3)"
    );
    key(&mut tree, Key::Tab); // focus hides placeholder, shows cursor
    let canvas = render(&mut tree, size);
    assert!(!canvas.row_text(0).contains("type here"));
}

/// A masked field mounted with a probe on its value signal.
fn masked_input(size: Size) -> (crate::reactive::RootScope, UiTree, Signal<String>) {
    let t = &default_theme().tokens;
    let holder: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let value = cx.signal(String::new());
        *h.borrow_mut() = Some(value);
        TextInput::new()
            .value(value)
            .placeholder("api key")
            .masked(true)
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab); // focus the field
    let sig = holder.borrow().expect("value signal");
    (root, tree, sig)
}

/// 0510 §masked, the leak surface: with a masked field populated,
/// NEITHER the rendered screen NOR the accessibility snapshot text may
/// contain a plaintext fragment — both export bullets, one per
/// grapheme cluster (a ZWJ family is ONE bullet).
#[test]
fn masked_field_leaks_no_plaintext_through_screen_or_semantic_tree() {
    let size = Size::new(24, 1);
    let (_root, mut tree, value) = masked_input(size);
    // "secret" (6 clusters) + a ZWJ astronaut (1 cluster, width 2).
    tree.dispatch(&UiEvent::Paste("secret\u{1F9D1}\u{200D}\u{1F680}".into()));
    assert_eq!(value.get_untracked(), "secret\u{1F9D1}\u{200D}\u{1F680}");
    let canvas = render(&mut tree, size);
    let row = canvas.row_text(0);
    assert!(!row.contains("secret"), "plaintext on screen: {row:?}");
    assert!(!row.contains('\u{1F9D1}'), "emoji on screen: {row:?}");
    assert_eq!(
        row.matches('\u{2022}').count(),
        7,
        "one bullet per grapheme cluster: {row:?}"
    );
    let a11y = tree.accessibility_tree_text();
    assert!(
        !a11y.contains("secret"),
        "plaintext in the semantic tree: {a11y}"
    );
    assert!(
        a11y.contains(&"\u{2022}".repeat(7)),
        "access_value exports cluster-count bullets: {a11y}"
    );
    // The label (placeholder) is not secret and still announces.
    assert!(a11y.contains("api key"));
}

/// Cycle-3 review F7: in masked mode, Alt+arrow word jumps treat the
/// whole value as ONE word — `word_step` over the real text would park
/// the caret on the secret's word boundaries, revealing word count and
/// word lengths through cursor motion. The cursor lands at 0 / len
/// (proven by where typed text goes, the strongest position oracle),
/// Shift+Alt extends the selection over the whole field, and the
/// unmasked control keeps true word-boundary jumps byte-identical.
#[test]
fn masked_word_jump_treats_whole_value_as_one_word() {
    let size = Size::new(24, 1);
    let (_root, mut tree, value) = masked_input(size);
    tree.dispatch(&UiEvent::Paste("one two three".into()));
    // Alt+Left from the end: cursor == 0, NOT the start of "three".
    key_mod(&mut tree, Key::Left, Mods::ALT);
    type_str(&mut tree, "X");
    assert_eq!(value.get_untracked(), "Xone two three", "cursor was at 0");
    // Alt+Right: cursor == len.
    key_mod(&mut tree, Key::Right, Mods::ALT);
    type_str(&mut tree, "Y");
    assert_eq!(
        value.get_untracked(),
        "Xone two threeY",
        "cursor was at len"
    );
    // Shift+Alt+Left selects the WHOLE value (one word): typing
    // replaces everything — Home/End semantics, shift included.
    key_mod(&mut tree, Key::Left, Mods::SHIFT | Mods::ALT);
    type_str(&mut tree, "Z");
    assert_eq!(value.get_untracked(), "Z", "whole-field selection replaced");
    // Unmasked control: the word jump still stops at word boundaries
    // (byte-identical pre-F7 behavior).
    let (_r2, mut plain_tree, plain_value) = focused_input(size);
    plain_tree.dispatch(&UiEvent::Paste("one two three".into()));
    key_mod(&mut plain_tree, Key::Left, Mods::ALT); // to start of "three"
    type_str(&mut plain_tree, "X");
    assert_eq!(plain_value.get_untracked(), "one two Xthree");
}

/// Masking is presentation-only: the editing model (cluster-atomic
/// backspace, selection replace, paste) behaves byte-identically to an
/// unmasked field, and the bound signal holds the real text throughout.
#[test]
fn masked_editing_stays_cluster_atomic_and_value_stays_real() {
    let size = Size::new(24, 1);
    let (_root, mut tree, value) = masked_input(size);
    tree.dispatch(&UiEvent::Paste("ab\u{1F9D1}\u{200D}\u{1F680}cd".into()));
    key(&mut tree, Key::Backspace); // "ab🧑‍🚀c"
    assert_eq!(value.get_untracked(), "ab\u{1F9D1}\u{200D}\u{1F680}c");
    key(&mut tree, Key::Backspace); // "ab🧑‍🚀"
    key(&mut tree, Key::Backspace); // whole ZWJ family in ONE backspace
    assert_eq!(value.get_untracked(), "ab");
    type_str(&mut tree, "XY");
    assert_eq!(value.get_untracked(), "abXY");
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.row_text(0).matches('\u{2022}').count(), 4);
    // Unmasked control: access_value exports plaintext as before.
    let t = &default_theme().tokens;
    let (_r2, mut plain_tree) = mount_widget(size, move |cx| {
        let v = cx.signal(String::from("visible"));
        TextInput::new().value(v).element(cx, t).build()
    });
    assert!(plain_tree.accessibility_tree_text().contains("visible"));
}
