//! PageHost unit tests (`#[path]` sibling of `page_host.rs`, file-size
//! discipline): mount/unmount lifecycle, the state-ownership recipe,
//! controlled/uncontrolled activation, the 0297 disposal law, chord
//! navigation (both wire spellings), digit opt-in, capture priority
//! over modifier-blind scrollables, focus re-anchoring, bar geometry
//! (windowing/truncation/stickiness), damage containment, a11y.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::ui::{text, KeyEvent, UiTree};
use crate::widgets::itest_util::{click, key, key_mod, mount_widget, render, type_str};
use crate::widgets::{Button, Scroll, TextInput};

/// Settle the deferred geometry loop (the scroll suite's recipe):
/// draw (probes record), fire due timers (probes publish), flush,
/// re-layout — what consecutive `Driver::turn`s do in a real app.
fn settle_geometry(tree: &mut UiTree, size: Size) -> crate::ui::BufferCanvas {
    crate::reactive::flush_effects();
    tree.layout();
    let mut canvas = render(tree, size);
    for _ in 0..4 {
        let fired = crate::reactive::run_due_timers(std::time::Instant::now());
        crate::reactive::flush_effects();
        tree.layout();
        canvas = render(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

fn three_pages(cx: Scope, builds: Rc<RefCell<Vec<&'static str>>>) -> Element {
    let t = &default_theme().tokens;
    let (b1, b2, b3) = (builds.clone(), builds.clone(), builds);
    PageHost::new()
        .page("one", "One", move |_| {
            b1.borrow_mut().push("one");
            text("PAGE ONE")
        })
        .page("two", "Two", move |_| {
            b2.borrow_mut().push("two");
            text("PAGE TWO")
        })
        .page("three", "Three", move |_| {
            b3.borrow_mut().push("three");
            text("PAGE THREE")
        })
        .element(cx, t)
}

#[test]
fn pages_mount_lazily_and_dispose_on_switch() {
    let size = Size::new(30, 5);
    let builds: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let b = builds.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| three_pages(cx, b).build());
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE ONE"));
    assert_eq!(*builds.borrow(), vec!["one"], "inactive pages never built");
    assert!(
        canvas.row_text(1).contains('▔'),
        "active tab wears the border_focus cell strip"
    );

    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE TWO"));
    assert_eq!(*builds.borrow(), vec!["one", "two"]);

    key_mod(&mut tree, Key::PageUp, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("PAGE ONE"),
        "switch back disposes and rebuilds"
    );
    assert_eq!(*builds.borrow(), vec!["one", "two", "one"]);
}

/// THE state recipe: durable state in app-owned signals OUTSIDE the
/// builders survives switches; the page subtree itself is rebuilt.
#[test]
fn state_outside_builders_survives_switches() {
    let size = Size::new(40, 6);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        // App-owned: outlives every page generation.
        let draft = cx.signal(String::new());
        PageHost::new()
            .page("edit", "Edit", move |gcx| {
                TextInput::new().value(draft).element(gcx, &t).build()
            })
            .page("other", "Other", |_| text("OTHER"))
            .element(cx, &t)
            .build()
    });
    key(&mut tree, Key::Tab); // bar
    key(&mut tree, Key::Tab); // the input inside the page
    type_str(&mut tree, "hi");
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("OTHER"));
    key_mod(&mut tree, Key::PageUp, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("hi"),
        "draft survived the round trip via the app-owned signal:\n{}",
        canvas.row_text(2)
    );
}

#[test]
fn controlled_signal_drives_pages_and_external_writes_skip_on_change() {
    let size = Size::new(30, 5);
    let changes: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let c = changes.clone();
    let mut probe = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let page = cx.signal("b".to_string());
        probe = Some(page);
        PageHost::new()
            .page("a", "Aaa", |_| text("PAGE A"))
            .page("b", "Bbb", |_| text("PAGE B"))
            .active(page)
            .on_change(move |id| c.borrow_mut().push(id.to_string()))
            .element(cx, &t)
            .build()
    });
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("PAGE B"),
        "controlled signal picks the start page"
    );
    probe.unwrap().set("a".to_string());
    crate::reactive::flush_effects();
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE A"));
    assert!(
        changes.borrow().is_empty(),
        "external writes are not host-driven switches"
    );
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    assert_eq!(*changes.borrow(), vec!["b".to_string()]);
}

#[test]
fn uncontrolled_initial_is_honored_and_unknown_controlled_id_folds_to_first() {
    let size = Size::new(30, 5);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        PageHost::new()
            .page("a", "Aaa", |_| text("PAGE A"))
            .page("b", "Bbb", |_| text("PAGE B"))
            .initial("b")
            .element(cx, &t)
            .build()
    });
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE B"));

    let (_root2, mut tree2) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let page = cx.signal("ghost".to_string());
        PageHost::new()
            .page("a", "Aaa", |_| text("PAGE A"))
            .page("b", "Bbb", |_| text("PAGE B"))
            .active(page)
            .element(cx, &t)
            .build()
    });
    let canvas = render(&mut tree2, size);
    assert!(
        canvas.row_text(2).contains("PAGE A"),
        "an unknown id folds to the first page"
    );
}

/// Disposal-safety law (0297): the active write lands BEFORE
/// `on_change`, so the callback may dispose the host's scope.
#[test]
fn on_change_may_dispose_the_host_scope() {
    let t = &default_theme().tokens;
    let mut tree = UiTree::new(Size::new(30, 5));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let host_cx = cx.child();
        let view = PageHost::new()
            .page("one", "One", |_| text("P1"))
            .page("two", "Two", |_| text("P2"))
            .on_change(move |_| host_cx.dispose())
            .element(host_cx, t)
            .build();
        tree.mount(host_cx, view);
    });
    tree.layout();
    // Bar segments: " One " = cols 0..5, gap, " Two " starts at col 6.
    click(&mut tree, 7, 0);
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

#[test]
fn click_selects_tabs_and_bar_arrows_cycle_with_wrap() {
    let size = Size::new(30, 5);
    let builds: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let b = builds.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| three_pages(cx, b).build());
    // " One " (0..5), " Two " (6..11), " Three " (12..19).
    click(&mut tree, 7, 0);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE TWO"));
    // Click focused the bar; arrows now cycle, wrapping both ways.
    key(&mut tree, Key::Right);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE THREE"));
    key(&mut tree, Key::Right);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE ONE"), "Right wraps");
    key(&mut tree, Key::Left);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE THREE"), "Left wraps");
}

/// Chords are container-reserved (Capture phase): a focused Scroll —
/// which matches PageUp/PageDown MODIFIER-BLIND — cannot eat
/// Ctrl+PgDn, while plain PgDn still scrolls the content.
#[test]
fn ctrl_chords_beat_a_focused_scroll_and_plain_paging_stays_content_side() {
    let size = Size::new(30, 8);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        PageHost::new()
            .page("feed", "Feed", move |gcx| {
                let mut col = Element::new().style(LayoutStyle::column());
                for i in 0..40 {
                    col = col.child(text(format!("line{i:02}")));
                }
                Scroll::new(col.build()).view(gcx)
            })
            .page("other", "Other", |_| text("PAGE OTHER"))
            .element(cx, &t)
            .build()
    });
    let canvas = settle_geometry(&mut tree, size); // extent probe publishes
    assert!(canvas.row_text(2).contains("line00"));
    key(&mut tree, Key::Tab); // bar
    key(&mut tree, Key::Tab); // the Scroll inside the page

    key(&mut tree, Key::PageDown); // plain: the content scrolls
    let canvas = settle_geometry(&mut tree, size);
    assert!(
        !canvas.row_text(2).contains("line00"),
        "plain PgDn belongs to the content:\n{}",
        canvas.row_text(2)
    );
    assert!(
        canvas.row_text(2).contains("line"),
        "still on the feed page"
    );

    key_mod(&mut tree, Key::PageDown, Mods::CTRL); // chord: the host switches
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("PAGE OTHER"),
        "Ctrl+PgDn is container-reserved:\n{}",
        canvas.row_text(2)
    );
}

/// After a chord switch disposed the focused node, focus re-anchors on
/// the host root — the NEXT chord must not be dead (the 0230 class).
/// The host is deliberately mounted UNDER a wrapper so a dropped focus
/// would route keys to the tree root, off the host's path.
#[test]
fn chords_stay_alive_after_the_focused_node_died_with_its_page() {
    let size = Size::new(30, 6);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let host = PageHost::new()
            .page("one", "One", move |gcx| {
                Element::new()
                    .style(LayoutStyle::column())
                    .child(Button::new("inside").element(gcx, &t).build())
                    .build()
            })
            .page("two", "Two", |_| text("PAGE TWO"))
            .page("three", "Three", |_| text("PAGE THREE"))
            .element(cx, &t)
            .build();
        Element::new()
            .style(LayoutStyle::column())
            .child(host)
            .build()
    });
    key(&mut tree, Key::Tab); // bar
    key(&mut tree, Key::Tab); // the button INSIDE page one
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE TWO"));
    // The button died with page one; without the re-anchor this chord
    // would target the tree root and never reach the host.
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("PAGE THREE"),
        "second chord still routes through the host:\n{}",
        canvas.row_text(2)
    );
}

/// Custom letter chords fold both wire spellings to one registration:
/// legacy bakes Shift into the char (`Char('L')`), kitty reports the
/// base key + SHIFT (`Char('l')`+SHIFT) — both must fire; the
/// unshifted letter must not.
#[test]
fn letter_chords_fire_on_both_wire_spellings() {
    let size = Size::new(30, 5);
    let builds: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let b = builds.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let (b1, b2, b3) = (b.clone(), b.clone(), b.clone());
        PageHost::new()
            .page("one", "One", move |_| {
                b1.borrow_mut().push("one");
                text("PAGE ONE")
            })
            .page("two", "Two", move |_| {
                b2.borrow_mut().push("two");
                text("PAGE TWO")
            })
            .page("three", "Three", move |_| {
                b3.borrow_mut().push("three");
                text("PAGE THREE")
            })
            .chords(
                &[KeyChord::plain(Key::Char('H'))],
                &[KeyChord::plain(Key::Char('L'))],
            )
            .element(cx, &t)
            .build()
    });
    // Legacy spelling: the shift is baked into the character.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Char('L'))));
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE TWO"));
    // Kitty spelling: base key + SHIFT.
    tree.dispatch(&UiEvent::Key(KeyEvent::new(Key::Char('l'), Mods::SHIFT)));
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE THREE"));
    // The unshifted letter means 'l', never 'L' — no switch.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Char('l'))));
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE THREE"));
}

#[test]
fn number_jump_is_opt_in_and_yields_to_a_focused_input() {
    let size = Size::new(40, 6);
    // OFF by default: digits do nothing.
    let (_r1, mut tree) = mount_widget(size, move |cx| {
        let b = Rc::new(RefCell::new(Vec::new()));
        three_pages(cx, b).build()
    });
    key(&mut tree, Key::Char('2'));
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(2).contains("PAGE ONE"),
        "digits are the app's"
    );

    // Opted in: digits jump — but a focused TextInput keeps them.
    let mut probe = None;
    let (_r2, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let draft = cx.signal(String::new());
        probe = Some(draft);
        PageHost::new()
            .page("edit", "Edit", move |gcx| {
                TextInput::new().value(draft).element(gcx, &t).build()
            })
            .page("two", "Two", |_| text("PAGE TWO"))
            .number_jump(true)
            .element(cx, &t)
            .build()
    });
    key(&mut tree, Key::Tab); // bar focused: digits are free
    key(&mut tree, Key::Char('2'));
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("PAGE TWO"));
    key(&mut tree, Key::Char('1')); // jump back
    key(&mut tree, Key::Tab); // bar -> input
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Char('2'));
    let canvas = render(&mut tree, size);
    assert!(
        !canvas.row_text(2).contains("PAGE TWO"),
        "a focused input consumes digits"
    );
    assert_eq!(probe.unwrap().get_untracked(), "2", "the digit was typed");
}

#[test]
fn badges_render_reactively_without_remounting_the_page() {
    let size = Size::new(40, 5);
    let builds: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let b = builds.clone();
    let mut probe = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let count = cx.signal(0u32);
        probe = Some(count);
        let b1 = b.clone();
        PageHost::new()
            .page("inbox", "Inbox", move |_| {
                b1.borrow_mut().push("inbox");
                text("PAGE INBOX")
            })
            .page("done", "Done", |_| text("PAGE DONE"))
            .badge("inbox", move || {
                let n = count.get();
                (n > 0).then(|| n.to_string())
            })
            .element(cx, &t)
            .build()
    });
    let canvas = render(&mut tree, size);
    assert!(!canvas.row_text(0).contains('3'));
    assert_eq!(builds.borrow().len(), 1);
    probe.unwrap().set(3);
    crate::reactive::flush_effects();
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).contains('3'),
        "badge renders in the bar:\n{}",
        canvas.row_text(0)
    );
    assert_eq!(
        builds.borrow().len(),
        1,
        "a badge change repaints the bar only — the page never remounts"
    );
}

#[test]
fn a11y_bar_reports_tabs_role_position_and_badge() {
    let size = Size::new(40, 5);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        PageHost::new()
            .page("inbox", "Inbox", |_| text("P1"))
            .page("done", "Done", |_| text("P2"))
            .badge("inbox", || Some("9".to_string()))
            .element(cx, &t)
            .build()
    });
    tree.layout();
    let snap = tree.accessibility_tree();
    let bar = snap.find(crate::ui::Role::Tabs).expect("bar annotated");
    assert_eq!(bar.value.as_deref(), Some("Inbox (1/2) [9]"));
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    let snap = tree.accessibility_tree();
    let bar = snap.find(crate::ui::Role::Tabs).expect("bar annotated");
    assert_eq!(bar.value.as_deref(), Some("Done (2/2)"));
}

#[test]
fn switch_damage_stays_inside_the_host_region() {
    let size = Size::new(40, 8);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let builds = Rc::new(RefCell::new(Vec::new()));
        Element::new()
            .style(LayoutStyle::column())
            .child(text("HEADER OUTSIDE THE HOST"))
            .child(three_pages(cx, builds).build())
            .build()
    });
    let _ = render(&mut tree, size);
    let _ = tree.take_damage(); // drain the mount damage
    click(&mut tree, 7, 1); // " Two " in the bar (bar rows are y=1..2)
    tree.layout();
    let damage = tree.take_damage();
    assert!(!damage.is_empty(), "the switch must damage something");
    for r in &damage {
        assert!(
            r.y >= 1,
            "damage leaked above the host into the header row: {r:?} (all: {damage:?})"
        );
    }
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).contains("HEADER"));
    assert!(canvas.row_text(3).contains("PAGE TWO"));
}

#[test]
fn oversized_titles_truncate_with_an_ellipsis() {
    let size = Size::new(20, 4);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        PageHost::new()
            .page("a", "an extremely long page title", |_| text("P1"))
            .page("b", "second", |_| text("P2"))
            .element(cx, &t)
            .build()
    });
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).contains('…'),
        "clamped title wears the ellipsis:\n{}",
        canvas.row_text(0)
    );
}

#[test]
fn overflow_indicators_page_by_click_and_mark_hidden_sides() {
    let size = Size::new(24, 5);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let mut host = PageHost::new();
        for (id, title) in [
            ("p1", "Page"),
            ("p2", "Pond"),
            ("p3", "Palm"),
            ("p4", "Pier"),
            ("p5", "Peak"),
            ("p6", "Pine"),
        ] {
            host = host.page(id, title, move |_| text(format!("BODY {id}")));
        }
        host.element(cx, &t).build()
    });
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).contains('›'), "hidden tabs on the right");
    assert!(!canvas.row_text(0).contains('‹'), "nothing hidden left yet");
    // The right indicator zone advances to the next page.
    for expect in ["BODY p2", "BODY p3", "BODY p4"] {
        click(&mut tree, 23, 0);
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(2).contains(expect),
            "expected {expect}:\n{}",
            canvas.row_text(2)
        );
    }
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).contains('‹'),
        "window slid — hidden tabs on the left:\n{}",
        canvas.row_text(0)
    );
    click(&mut tree, 0, 0); // the left zone goes back
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("BODY p3"));
}

#[test]
fn empty_host_renders_and_ignores_navigation() {
    let size = Size::new(20, 4);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        PageHost::new().number_jump(true).element(cx, &t).build()
    });
    let _ = render(&mut tree, size);
    key_mod(&mut tree, Key::PageDown, Mods::CTRL);
    key(&mut tree, Key::Char('1'));
    click(&mut tree, 3, 0);
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.row_text(2).trim(), "");
}

// plan_bar geometry pins — sibling file for the size budget (reaches
// the private `bar` module through `super::super`).
#[path = "page_host_bar_tests.rs"]
mod bar_geometry;
