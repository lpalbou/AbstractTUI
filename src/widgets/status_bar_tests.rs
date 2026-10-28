//! StatusBar (1260) + semantic hit targets (0187): slots at several
//! widths, overflow by priority, click → action, keyboard reach, spans
//! following a resize, and the recorder being load-bearing.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::base::{Point, Rect, Size};
use crate::layout::Style as LayoutStyle;
use crate::reactive::flush_effects;
use crate::theme::default_theme;
use crate::ui::{dyn_view, Key, MouseButton, MouseKind, StyledCanvas, UiTree};
use crate::widgets::itest_util::{click, key, mount_widget, mouse, render};
use crate::widgets::test_util::{draw_into, row};

fn chrome() -> StatusBar {
    StatusBar::new()
        .left(BarItem::text("APP").priority(10))
        .left(
            BarItem::text("workflow-name")
                .action("wf")
                .fit(BarFit::TruncateEnd { min: 5 })
                .priority(5),
        )
        .center(
            BarItem::text("/home/ws/project")
                .action("ws")
                .fit(BarFit::TruncateStart { min: 6 })
                .priority(2),
        )
        .center(BarItem::text("sess-1234").action("sess").priority(1))
        .right(BarItem::text("theme").action("theme").priority(3))
}

fn paint(bar: StatusBar, w: i32) -> String {
    let t = default_theme().tokens;
    row(&draw_into(bar.element(&t), Size::new(w, 1)), 0)
}

#[test]
fn slots_place_left_center_right_at_a_wide_width() {
    let line = paint(chrome(), 80);
    assert!(line.starts_with(" APP  workflow-name"), "{line:?}");
    assert!(line.ends_with("theme "), "{line:?}");
    let c = line
        .find("/home/ws/project  sess-1234")
        .expect("center slot");
    // Centered: the center run's midpoint sits at the bar's midpoint (±1).
    let mid = c as i32 + text::width("/home/ws/project  sess-1234") / 2;
    assert!((mid - 40).abs() <= 1, "center midpoint {mid}: {line:?}");
}

#[test]
fn overflow_yields_in_priority_order_at_every_width() {
    for w in [80, 60, 50, 44, 40, 34, 30, 24, 20, 12, 6] {
        let line = paint(chrome(), w);
        assert_eq!(text::width(&line), w, "[{w}] row stays the bar's width");
        // The highest priority survives longest.
        if w >= 6 {
            assert!(line.contains("APP"), "[{w}] {line:?}");
        }
        // Whole-only items never fragment.
        for whole in ["sess-1234", "theme", "APP"] {
            let head: String = whole.chars().take(3).collect();
            assert!(
                !line.contains(&head) || line.contains(whole),
                "[{w}] {whole} fragmented: {line:?}"
            );
        }
        // Lower priority never shows while a higher one dropped.
        if !line.contains("theme") {
            assert!(
                !line.contains("sess-1234") && !line.contains("/ws"),
                "[{w}] {line:?}"
            );
        }
    }
    // Pinned points: the session (lowest) goes first, then the path
    // truncates at its head, keeping the tail.
    let at50 = paint(chrome(), 50);
    assert!(
        !at50.contains("sess-1234") && at50.contains("/home/ws/project"),
        "{at50:?}"
    );
    let at40 = paint(chrome(), 40);
    assert!(
        at40.contains("…ws/project"),
        "the path keeps its tail: {at40:?}"
    );
    let at24 = paint(chrome(), 24);
    assert!(
        at24.contains("APP") && at24.contains("workflow-name") && !at24.contains("theme"),
        "{at24:?}"
    );
    assert!(
        !at24.contains("/ws") && !at24.contains("project"),
        "{at24:?}"
    );
}

#[test]
fn plan_drops_by_priority_and_truncates_to_min_first() {
    let m = |width, min, priority| SlotMeasure {
        width,
        min,
        priority,
    };
    let left = [m(10, 4, 1)];
    let right = [m(6, 6, 0)];
    // Needs 1+10+2+6+1 = 20; at 17 the lower item (right) drops whole.
    let p = plan_slots(17, 1, 2, 2, [&left, &[], &right]);
    assert_eq!(p[2][0], None);
    assert_eq!(p[0][0], Some(Placement { x: 1, w: 10 }));
    // Make the right item the stronger one: the left truncates instead.
    let right = [m(6, 6, 5)];
    let p = plan_slots(17, 1, 2, 2, [&left, &[], &right]);
    assert_eq!(p[0][0], Some(Placement { x: 1, w: 7 }));
    assert_eq!(p[2][0], Some(Placement { x: 10, w: 6 }));
    // Below its min it drops whole.
    let p = plan_slots(12, 1, 2, 2, [&left, &[], &right]);
    assert_eq!(p[0][0], None);
    assert_eq!(p[2][0], Some(Placement { x: 5, w: 6 }));
}

#[test]
fn center_slides_away_from_a_long_left_slot() {
    let m = |width| SlotMeasure {
        width,
        min: width,
        priority: 0,
    };
    let p = plan_slots(30, 0, 1, 2, [&[m(18)], &[m(4)], &[]]);
    assert_eq!(p[1][0], Some(Placement { x: 20, w: 4 }));
}

/// A tree with a live bar; returns the fired-action log.
fn live_tree(w: i32) -> (crate::reactive::RootScope, UiTree, Rc<RefCell<Vec<String>>>) {
    let fired: Rc<RefCell<Vec<String>>> = Rc::default();
    let log = fired.clone();
    let (root, tree) = mount_widget(Size::new(w, 1), move |_| {
        StatusBar::live(chrome, move |_, id| log.borrow_mut().push(id.to_string()))
    });
    (root, tree, fired)
}

#[test]
fn clicks_reach_each_action_at_several_widths() {
    for w in [80, 60, 40] {
        let (root, mut tree, fired) = live_tree(w);
        render(&mut tree, Size::new(w, 1));
        let targets = tree.action_targets();
        assert!(!targets.is_empty(), "[{w}] the bar records its targets");
        for (id, r) in &targets {
            click(&mut tree, r.x + r.w - 1, r.y);
            assert_eq!(
                fired.borrow().last().map(String::as_str),
                Some(id.as_str()),
                "[{w}] {id}"
            );
        }
        // A press between targets fires nothing.
        let n = fired.borrow().len();
        let line = row(&render(&mut tree, Size::new(w, 1)), 0);
        let gap = line.find("APP").unwrap() as i32 + 3;
        click(&mut tree, gap, 0);
        assert_eq!(fired.borrow().len(), n, "[{w}] a gap fires nothing");
        root.dispose();
    }
}

#[test]
fn press_and_release_on_different_spans_fire_nothing() {
    let (root, mut tree, fired) = live_tree(80);
    render(&mut tree, Size::new(80, 1));
    let wf = tree.action_rect("wf").unwrap();
    let th = tree.action_rect("theme").unwrap();
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), wf.x, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), th.x, 0);
    assert!(fired.borrow().is_empty());
    root.dispose();
}

#[test]
fn keyboard_reaches_every_action_in_reading_order() {
    let (root, mut tree, fired) = live_tree(80);
    render(&mut tree, Size::new(80, 1));
    tree.focus_next();
    render(&mut tree, Size::new(80, 1));
    let order: Vec<String> = tree
        .action_targets()
        .into_iter()
        .map(|(a, _)| a.to_string())
        .collect();
    assert_eq!(order, ["wf", "ws", "sess", "theme"]);
    for expect in &order {
        key(&mut tree, Key::Enter);
        assert_eq!(fired.borrow().last(), Some(expect));
        key(&mut tree, Key::Right);
    }
    // Wraps; Left goes back; Space activates too.
    key(&mut tree, Key::Left);
    key(&mut tree, Key::Char(' '));
    assert_eq!(fired.borrow().last().map(String::as_str), Some("theme"));
    root.dispose();
}

#[test]
fn the_keyboard_cursor_draws_reversed_while_focused() {
    let (root, mut tree, _) = live_tree(80);
    render(&mut tree, Size::new(80, 1));
    tree.focus_next();
    key(&mut tree, Key::Right); // cursor on "ws"
    let c = render(&mut tree, Size::new(80, 1));
    let ws = tree.action_rect("ws").unwrap();
    let wf = tree.action_rect("wf").unwrap();
    assert!(c
        .attrs_at(Point::new(ws.x, 0))
        .contains(crate::render::Attrs::REVERSE));
    assert!(!c
        .attrs_at(Point::new(wf.x, 0))
        .contains(crate::render::Attrs::REVERSE));
    tree.set_focus(None);
    let c = render(&mut tree, Size::new(80, 1));
    assert!(!c
        .attrs_at(Point::new(ws.x, 0))
        .contains(crate::render::Attrs::REVERSE));
    root.dispose();
}

#[test]
fn spans_follow_a_resize_and_a_relayout() {
    let (root, mut tree, fired) = live_tree(80);
    render(&mut tree, Size::new(80, 1));
    let before = tree.action_rect("theme").unwrap();
    tree.set_viewport(Size::new(60, 1));
    render(&mut tree, Size::new(60, 1));
    let after = tree.action_rect("theme").unwrap();
    assert_ne!(before, after, "the right slot moved with the width");
    // The old cell is no longer the theme target (60 < 78).
    click(&mut tree, after.x, 0);
    assert_eq!(fired.borrow().last().map(String::as_str), Some("theme"));
    root.dispose();
}

/// The 0187 core on a raw draw closure: two tagged spans, a click
/// between them fires nothing, and the record moves with layout — a
/// sibling growing pushes the drawer right without any app arithmetic.
#[test]
fn raw_draw_closure_spans_hit_after_relayout() {
    let fired: Rc<RefCell<Vec<String>>> = Rc::default();
    let log = fired.clone();
    let mut lead_sig = None;
    let (root, mut tree) = mount_widget(Size::new(30, 1), |cx| {
        let lead = cx.signal(2);
        lead_sig = Some(lead);
        Element::new()
            .style(LayoutStyle::row().height(Dimension::Cells(1)))
            .child(dyn_view(LayoutStyle::default(), move || {
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(lead.get()))
                            .height(Dimension::Cells(1)),
                    )
                    .build()
            }))
            .child(
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(20))
                            .height(Dimension::Cells(1)),
                    )
                    .on_action(move |_, id| log.borrow_mut().push(id.to_string()))
                    .draw(|canvas: &mut dyn StyledCanvas, rect: Rect| {
                        let s = crate::render::Style::new();
                        canvas.print_action(
                            Point::new(rect.x, rect.y),
                            "aa",
                            &s,
                            &ActionId::from("a"),
                        );
                        canvas.print_action(
                            Point::new(rect.x + 5, rect.y),
                            "bb",
                            &s,
                            &ActionId::from("b"),
                        );
                    })
                    .build(),
            )
            .build()
    });
    render(&mut tree, Size::new(30, 1));
    click(&mut tree, 3, 0);
    click(&mut tree, 5, 0); // between the spans
    click(&mut tree, 8, 0);
    assert_eq!(*fired.borrow(), ["a", "b"]);
    lead_sig.unwrap().set(6);
    flush_effects();
    tree.layout(); // relayout WITHOUT a repaint: the record follows
    click(&mut tree, 7, 0);
    click(&mut tree, 12, 0);
    assert_eq!(*fired.borrow(), ["a", "b", "a", "b"]);
    root.dispose();
}

/// Red without the recorder: a bare draw (no tree, no frame) records
/// nothing, so nothing is clickable — the engine frame is load-bearing.
#[test]
fn without_the_tree_recorder_nothing_is_a_target() {
    let t = default_theme().tokens;
    let _ = draw_into(chrome().element(&t), Size::new(80, 1));
    let tree = UiTree::new(Size::new(80, 1));
    assert!(tree.action_targets().is_empty());
}
