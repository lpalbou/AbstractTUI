//! Activation acceptance (backlog 0250, the ruling recorded in
//! reviews/study/platform-on-appkits.md §"The 0250 ruling"), driven
//! through the REAL pipeline: `app::Driver` frames against a
//! `CaptureTerm`, SGR mouse bytes + key bytes in, modeled VT screen
//! out.
//!
//! Pins, by ruling clause:
//! - clause 1: arrows/clicks MOVE the selection (`on_select` is a
//!   notification) — the dashboard-shaped nav List keeps working with
//!   no activation bound;
//! - clause 2: Enter activates (always), Space activates in a List (no
//!   toggle meaning), a click on the ALREADY-selected row activates
//!   while a click on an unselected row only selects — no double-click
//!   synthesis anywhere;
//! - compatibility: an activation-less List leaves Enter to the app's
//!   own shortcuts (the pre-0250 field workaround keeps working);
//! - 0510 §masked at the wire: a masked TextInput never puts plaintext
//!   on the VT screen or into the accessibility snapshot.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{current_theme, App, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;

fn config() -> RunConfig {
    RunConfig {
        // ADR-0003: `Capabilities` is `#[non_exhaustive]` — downstream
        // construction goes through `with`.
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

/// Drive turns until idle (bounded).
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

#[test]
fn sgr_click_selects_then_activates_and_enter_space_activate() {
    let size = Size::new(20, 6);
    let mut app = App::new(size);
    let activated: Rc<RefCell<Vec<usize>>> = Default::default();
    let a2 = activated.clone();
    let sel_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let s2 = sel_holder.clone();
    app.mount(move |cx| {
        let sel = cx.signal(0usize);
        *s2.borrow_mut() = Some(sel);
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(
                List::of(["alpha", "beta", "gamma", "delta"])
                    .selection(sel)
                    .on_activate(move |i| a2.borrow_mut().push(i))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let sel = sel_holder.borrow().expect("selection probe");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Click row 2 ("gamma", SGR is 1-based): an UNSELECTED row — the
    // click selects and must NOT activate (ruling clause 2).
    term.push_input(b"\x1b[<0;2;3M");
    term.push_input(b"\x1b[<0;2;3m");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(sel.get_untracked(), 2, "click moved the selection");
    assert!(
        activated.borrow().is_empty(),
        "click on an unselected row must not activate: {:?}",
        activated.borrow()
    );
    // The moved highlight is REAL frame content on the modeled screen.
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    assert_eq!(
        term.screen().cell(0, 2).unwrap().paint.bg,
        Some(sel_bg),
        "selected row wears the selection ground"
    );

    // Click the SAME row again: already selected -> activation.
    term.push_input(b"\x1b[<0;2;3M");
    term.push_input(b"\x1b[<0;2;3m");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        *activated.borrow(),
        vec![2],
        "click-when-selected activates"
    );

    // Enter activates, always (the one deterministic key automation
    // can inject — ruling clause 6).
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*activated.borrow(), vec![2, 2]);

    // Space activates in a List: no toggle meaning claims it (clause 2).
    term.push_input(b" ");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*activated.borrow(), vec![2, 2, 2]);
    assert_eq!(sel.get_untracked(), 2, "activation never moves selection");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn nav_list_without_on_activate_leaves_enter_to_app_shortcuts() {
    // The dashboard sidebar shape (selection signal + focus_signal, no
    // callbacks) must be byte-identical to pre-0250 behavior: arrows
    // move the selection, and Enter falls through to the app's own
    // shortcut — the field workaround pattern stays valid.
    let size = Size::new(24, 8);
    let mut app = App::new(size);
    let confirmed: Rc<RefCell<Vec<usize>>> = Default::default();
    let c2 = confirmed.clone();
    let sel_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let s2 = sel_holder.clone();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let nav = cx.signal(0usize);
        let nav_focus = cx.signal(false);
        *s2.borrow_mut() = Some(nav);
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .shortcut(KeyChord::plain(Key::Enter), move |_ctx| {
                // Root-level confirm reading the selection signal — the
                // documented pre-activation composition.
                c2.borrow_mut().push(nav.get_untracked());
            })
            .child(
                Block::new()
                    .title("nav")
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(
                        List::of(["overview", "traffic", "sessions", "logs"])
                            .selection(nav)
                            .focus_signal(nav_focus)
                            .layout(LayoutStyle::default().grow(1.0))
                            .element(cx, &t)
                            .build(),
                    )
                    .element(&t)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let nav = sel_holder.borrow().expect("selection probe");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Tab focuses the nav List (first focusable), then arrows move the
    // selection (clause 1) and never confirm.
    term.push_input(b"\t");
    term.push_input(b"\x1b[B");
    term.push_input(b"\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(nav.get_untracked(), 2, "Down moved the nav selection");
    assert!(confirmed.borrow().is_empty(), "movement never confirms");

    // Enter is NOT consumed by the activation-less List: the root
    // shortcut hears it and confirms the current selection.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        *confirmed.borrow(),
        vec![2],
        "Enter reached the app shortcut through the List"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn masked_input_never_leaks_plaintext_through_wire_or_semantic_tree() {
    // 0510 §masked at the REAL boundary: typed bytes land in the value
    // signal, while the VT screen and the accessibility snapshot (the
    // export the control-plane band ships off-process) carry bullets
    // only.
    let size = Size::new(24, 3);
    let mut app = App::new(size);
    let value_holder: Rc<RefCell<Option<Signal<String>>>> = Default::default();
    let v2 = value_holder.clone();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let value = cx.signal(String::new());
        *v2.borrow_mut() = Some(value);
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(text("api key:"))
            .child(
                TextInput::new()
                    .value(value)
                    .masked(true)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let value = value_holder.borrow().expect("value probe");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\t"); // focus the field
    term.push_input(b"hunter2");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        value.get_untracked(),
        "hunter2",
        "the app owns the real text"
    );

    let dump = term.screen().to_text();
    assert!(!dump.contains("hunter2"), "plaintext on the wire: {dump}");
    assert!(
        dump.matches('\u{2022}').count() >= 7,
        "bullets on screen (7 clusters + cursor cell): {dump}"
    );
    let a11y = app.tree().accessibility_tree_text();
    assert!(
        !a11y.contains("hunter2"),
        "plaintext in the semantic tree: {a11y}"
    );
    assert!(a11y.contains(&"\u{2022}".repeat(7)));

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}
