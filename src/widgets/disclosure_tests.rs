//! Disclosure tests: fold/unfold in both state modes, toggle surfaces
//! (click, Enter/Space-when-focused), the body cap + scroll
//! engagement, title/detail row geometry, remount-on-expand, a11y and
//! the disposal-safety law — real tree, real dispatch, real typeset.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::base::{Point, Size};
use crate::reactive::{flush_effects, run_due_timers};
use crate::theme::default_theme;
use crate::ui::{text, BufferCanvas, UiTree};
use crate::widgets::itest_util::{click, key, mount_widget, mouse, render};

/// Settle the deferred geometry loop (feed width fixup + scroll extent
/// probe): draw, fire due timers, flush, repeat until quiet — the
/// scroll_tests recipe.
fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let mut canvas = render(tree, size);
    for _ in 0..4 {
        let fired = run_due_timers(std::time::Instant::now());
        flush_effects();
        tree.layout();
        canvas = render(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

fn dump(canvas: &BufferCanvas, h: i32) -> Vec<String> {
    (0..h).map(|y| canvas.row_text(y)).collect()
}

fn twelve_lines() -> String {
    (0..12)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn starts_folded_by_default_and_click_on_the_title_toggles() {
    let t = default_theme().tokens;
    let size = Size::new(28, 8);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("build log", "hidden body line")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(top.contains('▸') && top.contains("build log"), "{top:?}");
    assert!(
        !dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("hidden body")),
        "folded by default: the body must not render"
    );

    click(&mut tree, 3, 0); // the title row
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains('▾'), "glyph flips open");
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("hidden body")),
        "unfolded: the body renders:\n{:#?}",
        dump(&canvas, size.h)
    );

    click(&mut tree, 3, 0);
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains('▸'), "second click re-folds");
    assert!(!dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("hidden body")));
    root.dispose();
}

#[test]
fn initially_unfolded_shows_the_body_at_mount() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("notes", "open from the start")
                    .initially_folded(false)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains('▾'));
    assert!(dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("open from the start")));
    root.dispose();
}

#[test]
fn controlled_signal_drives_the_card_and_receives_toggles() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let holder: Rc<RefCell<Option<crate::reactive::Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let folded = cx.signal(false); // app policy: start expanded
        *h.borrow_mut() = Some(folded);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("card", "controlled body")
                    .folded(folded)
                    .initially_folded(true) // ignored: the signal wins
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let folded = holder.borrow().expect("signal");
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("controlled body")),
        "the signal's value IS the state (initially_folded ignored)"
    );

    folded.set(true); // app-side fold (toggle-all), no gesture
    let canvas = settle(&mut tree, size);
    assert!(!dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("controlled body")));

    click(&mut tree, 3, 0); // gesture writes the app's signal back
    flush_effects();
    assert!(!folded.get_untracked(), "click unfolds through the signal");
    root.dispose();
}

#[test]
fn enter_and_space_toggle_only_while_focused() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("keys", "key body").element(cx, &t).build())
            .build()
    });
    let _ = settle(&mut tree, size);
    key(&mut tree, Key::Enter); // nothing focused: inert
    let canvas = settle(&mut tree, size);
    assert!(!dump(&canvas, size.h).iter().any(|r| r.contains("key body")));

    key(&mut tree, Key::Tab); // the header is the card's tab stop
    key(&mut tree, Key::Enter);
    let canvas = settle(&mut tree, size);
    assert!(dump(&canvas, size.h).iter().any(|r| r.contains("key body")));

    key(&mut tree, Key::Char(' '));
    let canvas = settle(&mut tree, size);
    assert!(!dump(&canvas, size.h).iter().any(|r| r.contains("key body")));
    root.dispose();
}

#[test]
fn on_toggle_reports_the_new_state_after_the_write() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    // (callback arg, state read inside the callback): equal pairs prove
    // the write landed BEFORE the callback (the 0297 ordering).
    let log: Rc<RefCell<Vec<(bool, bool)>>> = Rc::default();
    let sink = log.clone();
    let holder: Rc<RefCell<Option<crate::reactive::Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let folded = cx.signal(true);
        *h.borrow_mut() = Some(folded);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("t", "b")
                    .folded(folded)
                    .on_toggle(move |now| sink.borrow_mut().push((now, folded.get_untracked())))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    click(&mut tree, 3, 0); // unfold
    click(&mut tree, 3, 0); // fold
    assert_eq!(
        log.borrow().as_slice(),
        &[(false, false), (true, true)],
        "arg = the NEW folded state; the signal already holds it"
    );
    root.dispose();
}

/// Disposal-safety law (backlog 0297): the fold state is written before
/// `on_toggle`, so the callback may dispose the card's scope.
#[test]
fn on_toggle_may_dispose_the_disclosures_scope() {
    let t = default_theme().tokens;
    let mut tree = UiTree::new(Size::new(28, 6));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let card_cx = cx.child();
        let view = Disclosure::text("t", "b")
            .on_toggle(move |_| card_cx.dispose())
            .element(card_cx, &t)
            .build();
        tree.mount(card_cx, view);
    });
    tree.layout();
    click(&mut tree, 3, 0); // toggle -> on_toggle -> dispose
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

#[test]
fn max_body_rows_caps_the_region_and_the_body_scrolls_with_a_bar() {
    let t = default_theme().tokens;
    let size = Size::new(28, 10);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("long", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(4)
                    .element(cx, &t)
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(rows[1].contains("line 0"), "{rows:#?}");
    assert!(
        rows[4].contains("line 3"),
        "cap shows 4 body rows: {rows:#?}"
    );
    assert!(
        rows[5].contains("BELOW"),
        "the card ends after header + 4 capped rows: {rows:#?}"
    );
    // The scrollbar thumb is visible in the body's right column
    // (12 rows in a 4-row viewport = overflow).
    let bar_col = size.w - 2; // body host has a 1-cell right pad
    let bar: String = (1..5)
        .filter_map(|y| canvas.cell(Point::new(bar_col, y)).map(|c| c.0))
        .collect();
    assert!(bar.contains('┃'), "thumb visible on overflow: {bar:?}");

    // The wheel scrolls the BODY (nearest scroll container), +3 rows.
    mouse(&mut tree, crate::ui::MouseKind::ScrollDown, 4, 2);
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[1].contains("line 3"),
        "wheel scrolled the body: {rows:#?}"
    );
    assert!(rows[5].contains("BELOW"), "the card's extent never moved");
    root.dispose();
}

#[test]
fn short_body_takes_its_natural_height_and_hides_the_bar() {
    let t = default_theme().tokens;
    let size = Size::new(28, 8);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("short", "one\ntwo")
                    .initially_folded(false)
                    .max_body_rows(8)
                    .element(cx, &t)
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[1].contains("one") && rows[2].contains("two"),
        "{rows:#?}"
    );
    assert!(
        rows[3].contains("BELOW"),
        "capped region shrinks to the 2-row content (limited-to, not padded-to): {rows:#?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains('┃') || r.contains('│')),
        "no scrollbar while the body fits: {rows:#?}"
    );
    root.dispose();
}

#[test]
fn max_body_rows_zero_means_unbounded_natural_height() {
    let t = default_theme().tokens;
    let size = Size::new(28, 16);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("all", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(0)
                    .element(cx, &t)
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[12].contains("line 11"),
        "all 12 rows render: {rows:#?}"
    );
    assert!(rows[13].contains("BELOW"), "{rows:#?}");
    assert!(
        !rows.iter().any(|r| r.contains('┃')),
        "no scroll chrome when uncapped: {rows:#?}"
    );
    root.dispose();
}

#[test]
fn title_truncates_and_the_detail_drops_when_the_row_is_tight() {
    let t = default_theme().tokens;
    // Roomy: the detail renders whole, right-aligned; the title
    // truncates into the remainder.
    let size = Size::new(18, 3);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("a very long disclosure title")
                    .detail("99+")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(top.contains('…'), "title truncates: {top:?}");
    assert!(top.contains("99+"), "detail renders whole: {top:?}");
    root.dispose();

    // Tight: fewer than 4 title cells would remain — the detail drops,
    // the title keeps the run.
    let size = Size::new(10, 3);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("a very long disclosure title")
                    .detail("99+")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(
        !top.contains("99+"),
        "detail drops, never crushes the title: {top:?}"
    );
    assert!(top.contains('…'), "{top:?}");
    root.dispose();
}

#[test]
fn folded_body_unmounts_and_every_unfold_remounts() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let builds: Rc<Cell<u32>> = Rc::default();
    let counter = builds.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("lazy")
                    .body(move |_| {
                        counter.set(counter.get() + 1);
                        text("built body")
                    })
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    assert_eq!(builds.get(), 0, "folded at mount: the body never builds");

    click(&mut tree, 3, 0);
    let canvas = settle(&mut tree, size);
    assert_eq!(builds.get(), 1, "first expand builds once");
    assert!(dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("built body")));

    click(&mut tree, 3, 0);
    let canvas = settle(&mut tree, size);
    assert_eq!(builds.get(), 1, "folding builds nothing");
    assert!(
        !dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("built body")),
        "folded = unmounted"
    );

    click(&mut tree, 3, 0);
    let _ = settle(&mut tree, size);
    assert_eq!(builds.get(), 2, "re-expand REMOUNTS (fresh generation)");
    root.dispose();
}

#[test]
fn markdown_body_renders_the_doc_vocabulary() {
    let t = default_theme().tokens;
    let size = Size::new(30, 6);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::markdown("notes", "**bold** move")
                    .initially_folded(false)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("bold move")),
        "markdown typeset through the shared recipe:\n{:#?}",
        dump(&canvas, size.h)
    );
    root.dispose();
}

#[test]
fn access_reports_region_button_label_and_fold_state() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("Deploy log", "body")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    let a11y = tree.accessibility_tree_text();
    assert!(a11y.contains("region \"Deploy log\""), "{a11y}");
    assert!(
        a11y.contains("button \"Deploy log\" = \"collapsed\""),
        "{a11y}"
    );
    click(&mut tree, 3, 0);
    flush_effects();
    let a11y = tree.accessibility_tree_text();
    assert!(
        a11y.contains("button \"Deploy log\" = \"expanded\""),
        "{a11y}"
    );
    root.dispose();
}

#[test]
fn focused_header_shows_a_visible_affordance() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("focus me", "body").element(cx, &t).build())
            .build()
    });
    let _ = settle(&mut tree, size);
    key(&mut tree, Key::Tab); // focus the header
    flush_effects();
    tree.layout();
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "the focused title row must differ visibly (selection pair)"
    );
    root.dispose();
}
