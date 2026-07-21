//! Wave regression suite (STABILITY seat): backlog items 0220 / 0230 /
//! 0240 plus the 0170 rulings-subset compile guarantees (ADR-0003's
//! `Capabilities` constructor contract). Everything here drives the
//! PUBLIC API only — this file compiles as a separate crate, exactly
//! like a downstream application, which is the point: it pins the
//! surfaces applications depend on.

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::{Dimension, LayoutStyle};
use abstracttui::reactive::{create_root, flush_effects, Scope};
use abstracttui::term::{Capabilities, EnterOptions, GraphicsCaps, KittyFlags, MouseMode};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{
    dyn_view_scoped, text, BufferCanvas, Element, Key, KeyChord, KeyEvent, UiEvent, UiTree,
};
use abstracttui::widgets::TextInput;

use std::cell::RefCell;
use std::rc::Rc;

/// Fixed capabilities so the host environment never leaks into the
/// assertions — built through the ADR-0003 constructor, as any
/// downstream test must once `Capabilities` is `#[non_exhaustive]`.
fn test_config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: Some(EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: KittyFlags(0),
        }),
        probe: false,
    }
}

/// App shell + captured scope for tests that open modals at runtime.
fn app_with_scope(size: Size) -> (App, Rc<RefCell<Option<Scope>>>) {
    let mut app = App::new(size);
    let scope_holder: Rc<RefCell<Option<Scope>>> = Rc::new(RefCell::new(None));
    let sh = scope_holder.clone();
    app.mount(move |cx| {
        *sh.borrow_mut() = Some(cx);
        Element::new().child(text("underneath")).build()
    })
    .expect("mount");
    (app, scope_holder)
}

// ---------------------------------------------------------------------
// 0220 — autofocus inside a dyn_view regeneration must not panic the
// reactive runtime, and focus must land on the node.
// ---------------------------------------------------------------------

/// The exact first-app composition: a theme-keyed `dyn_view_scoped`
/// wrapping a `TextInput` marked `.autofocus()`. Before the fix this
/// panicked ("dependency cycle") at mount: the dyn-subtree autofocus
/// path delivered FocusIn INSIDE the running dyn computation, and the
/// input's `focus_signal` write re-entered the running node via the
/// effect flush.
#[test]
fn autofocus_inside_dyn_view_regeneration_mounts_and_focuses() {
    let tokens = abstracttui::theme::default_theme().tokens;
    let mut tree = UiTree::new(Size::new(30, 4));
    let mut probe = None;
    let (root, ()) = create_root(|cx| {
        let theme_key = cx.signal(0u32);
        probe = Some(theme_key);
        let view = Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view_scoped(
                LayoutStyle::default().height(Dimension::Cells(1)),
                move |scx| {
                    let _generation = theme_key.get(); // tracked: regen on change
                    TextInput::new()
                        .placeholder("composer")
                        .element(scx, &tokens)
                        .autofocus()
                        .build()
                },
            ))
            .build();
        // Initial mount: the autofocus node lives inside a dyn whose
        // effect runs nested in this mount — the panic fired here too.
        tree.mount(cx, view);
    });
    tree.layout();
    let before = tree
        .focused()
        .expect("initial mount delivers dyn-subtree autofocus");

    // The regeneration path: a theme-key write rebuilds the dyn subtree
    // (new instances, new autofocus node) inside a flush. Focus must
    // land on the REGENERATED node once the frame settles — without a
    // reactive-runtime panic.
    probe.expect("signal captured").set(1);
    flush_effects();
    tree.layout(); // frame phase L: parked autofocus delivers here
    let after = tree.focused().expect("focus landed after regeneration");
    assert_ne!(
        before, after,
        "focus points at the regenerated input, not the disposed one"
    );
    root.dispose();
}

/// Several regenerations in a row: every generation ends focused on a
/// LIVE instance (typing works), never on a disposed corpse.
#[test]
fn repeated_dyn_regenerations_keep_autofocus_deterministic() {
    let tokens = abstracttui::theme::default_theme().tokens;
    let mut tree = UiTree::new(Size::new(30, 4));
    let mut probe = None;
    let (root, ()) = create_root(|cx| {
        let key = cx.signal(0u32);
        probe = Some(key);
        let view = Element::new()
            .child(dyn_view_scoped(LayoutStyle::default(), move |scx| {
                let _k = key.get();
                TextInput::new().element(scx, &tokens).autofocus().build()
            }))
            .build();
        tree.mount(cx, view);
    });
    let key = probe.expect("signal");
    for generation in 1..=3u32 {
        key.set(generation);
        flush_effects();
        tree.layout();
        assert!(
            tree.focused().is_some(),
            "generation {generation} ends focused"
        );
        // The focused node is alive and receiving: a typed character is
        // consumed by the input's own key handler.
        let consumed = tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Char('x'))));
        assert!(consumed, "generation {generation} input consumes keys");
    }
    root.dispose();
}

// ---------------------------------------------------------------------
// 0230 — modal content shortcuts live from frame one.
// ---------------------------------------------------------------------

/// The full production path (bytes -> driver -> overlay routing): a
/// modal whose content root carries a shortcut and NO focusable
/// children. The chord must fire without the user pressing Tab first.
#[test]
fn modal_content_shortcut_fires_without_tab() {
    let size = Size::new(30, 8);
    let (mut app, scope_holder) = app_with_scope(size);
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");

    let fired: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let f2 = fired.clone();
    let cx = scope_holder.borrow().expect("scope");
    let _modal = Modal::open(&overlays, cx, size, Size::new(20, 4), move |_| {
        Element::new()
            .shortcut(KeyChord::plain(Key::Char('a')), move |_| {
                *f2.borrow_mut() += 1;
            })
            .child(text("approve? [a]"))
            .build()
    });
    driver.turn(&mut app, &mut term).expect("modal frame");

    // Frame one: the chord goes straight to the modal tree — no Tab.
    term.push_input(b"a");
    driver.turn(&mut app, &mut term).expect("chord turn");
    assert_eq!(
        *fired.borrow(),
        1,
        "content-root shortcut fired without Tab"
    );
}

/// A modal with an autofocus input: the input is focused from frame one
/// (typing works immediately) AND content-root shortcuts still fire —
/// the root stays on the dispatch path.
#[test]
fn modal_autofocus_wins_and_root_shortcuts_stay_reachable() {
    let tokens = abstracttui::theme::default_theme().tokens;
    let size = Size::new(30, 8);
    let (app, scope_holder) = app_with_scope(size);
    let overlays = app.overlays();

    let escaped: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let e2 = escaped.clone();
    let value_probe: Rc<RefCell<Option<abstracttui::reactive::Signal<String>>>> =
        Rc::new(RefCell::new(None));
    let v2 = value_probe.clone();
    let cx = scope_holder.borrow().expect("scope");
    let modal = Modal::open(&overlays, cx, size, Size::new(24, 5), move |mcx| {
        let value = mcx.signal(String::new());
        *v2.borrow_mut() = Some(value);
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                *e2.borrow_mut() += 1;
            })
            .child(text("name:"))
            .child(
                TextInput::new()
                    .value(value)
                    .element(mcx, &tokens)
                    .autofocus()
                    .build(),
            )
            .build()
    });

    let mut tree = modal.layer().tree().expect("modal layer holds a tree");
    // The autofocus input owns the keyboard from frame one...
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Char('x'))));
    let typed = value_probe
        .borrow()
        .expect("value signal captured")
        .get_untracked();
    assert_eq!(typed, "x", "typing reaches the autofocused input");
    // ...and the content root's Escape still resolves via the path.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Escape)));
    assert_eq!(
        *escaped.borrow(),
        1,
        "root shortcut reachable while the input is focused"
    );
}

// ---------------------------------------------------------------------
// 0240 — overflowing modal content must not silently erase fixed rows
// (the popups half: declared fixed sizes are floors inside modal trees).
// ---------------------------------------------------------------------

/// The first-app probe: a small modal whose column holds a title row, an
/// overflowing middle, a button row and a hint row. Before the fix the
/// button and hint rows solved to ZERO height — invisible controls in an
/// approval dialog. The floor keeps every declared fixed row visible;
/// the grow middle absorbs the loss.
#[test]
fn modal_fixed_rows_survive_content_overflow() {
    let size = Size::new(30, 8);
    let (app, scope_holder) = app_with_scope(size);
    let overlays = app.overlays();

    let cx = scope_holder.borrow().expect("scope");
    // 30x6 modal, 1-cell panel padding: 4 content rows for a column
    // asking title(1) + middle(grow, 40-line content) + buttons(1) +
    // hint(1) — overflow pressure by construction.
    let modal = Modal::open(&overlays, cx, size, Size::new(30, 6), |_| {
        let tall = (0..40).map(|i| format!("arg line {i}")).collect::<Vec<_>>();
        let fixed_row = |label: &str| {
            Element::new()
                .style(LayoutStyle::default().height(Dimension::Cells(1)))
                .child(text(label))
                .build()
        };
        Element::new()
            .style(LayoutStyle::column())
            .child(fixed_row("Approve tool?"))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .child(text(tall.join("\n")))
                    .build(),
            )
            .child(fixed_row("[a]pprove [d]eny"))
            .child(fixed_row("esc closes"))
            .build()
    });

    let mut tree = modal.layer().tree().expect("modal layer holds a tree");
    let mut canvas = BufferCanvas::new(Size::new(30, 6));
    tree.draw(&mut canvas);
    let screen: Vec<String> = (0..6).map(|y| canvas.row_text(y)).collect();
    let flat = screen.join("\n");
    for needle in ["Approve tool?", "[a]pprove [d]eny", "esc closes"] {
        assert!(
            flat.contains(needle),
            "fixed row {needle:?} must stay visible; screen:\n{flat}"
        );
    }
}

/// Opt-out honored: an author who explicitly allows a row to vanish
/// (`min_h(0)`) keeps that behavior — the floor only applies where the
/// author said nothing.
#[test]
fn modal_fixed_row_floor_respects_explicit_min() {
    let size = Size::new(30, 8);
    let (app, scope_holder) = app_with_scope(size);
    let overlays = app.overlays();

    let cx = scope_holder.borrow().expect("scope");
    let modal = Modal::open(&overlays, cx, size, Size::new(30, 6), |_| {
        let tall = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>();
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    // Explicit opt-out: this row MAY collapse.
                    .style(LayoutStyle::default().height(Dimension::Cells(1)).min_h(0))
                    .child(text("collapsible"))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .child(text(tall.join("\n")))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().height(Dimension::Cells(1)))
                    .child(text("kept row"))
                    .build(),
            )
            .build()
    });

    let mut tree = modal.layer().tree().expect("modal tree");
    let mut canvas = BufferCanvas::new(Size::new(30, 6));
    tree.draw(&mut canvas);
    let flat: String = (0..6).map(|y| canvas.row_text(y) + "\n").collect();
    assert!(
        !flat.contains("collapsible"),
        "explicit min_h(0) keeps the author's collapse; screen:\n{flat}"
    );
    assert!(
        flat.contains("kept row"),
        "unopted fixed row stays visible; screen:\n{flat}"
    );
}

// ---------------------------------------------------------------------
// 0170 rulings subset — ADR-0003: Capabilities/GraphicsCaps are
// non_exhaustive; downstream construction goes through `with`.
// ---------------------------------------------------------------------

/// Downstream-style construction: adding a capability field must never
/// break this test (this file compiles as a separate crate, exactly
/// like an application).
#[test]
fn capabilities_construct_via_with_and_grow_without_breakage() {
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.unicode_ok = true;
    });
    assert!(caps.truecolor && caps.colors_256 && caps.unicode_ok);
    assert!(caps.deferred_wrap, "unset fields keep their defaults");

    let gfx = GraphicsCaps::with(|g| {
        g.kitty_graphics = true;
    });
    assert!(gfx.kitty_graphics && !gfx.sixel);

    // The read side stays plain field access, and detection still works.
    let from_env = Capabilities::detect_env_with(&|_| None);
    assert!(from_env.dumb, "empty environment is honestly dumb");
}
