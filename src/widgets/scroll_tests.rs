//! Scroll tests: wheel/keys/clip and drag (v1), plus the 0130 wave —
//! measured content extent (no hint), hint-wins, follow-tail
//! disengage/re-arm, and the leftover-not-content default basis (the
//! 0240 modal-overflow follow-up).

use super::*;
use crate::base::Size;
use crate::layout::Style as LayoutStyle;
use crate::reactive::{flush_effects, run_due_timers};
use crate::theme::default_theme;
use crate::ui::{text, BufferCanvas, Element, Key, MouseButton, MouseKind, UiTree};
use crate::widgets::itest_util::{key, mount_widget, mouse, render};
use crate::widgets::{Feed, FeedItem, FeedState};
use std::cell::RefCell;
use std::rc::Rc;

/// 1-column-wide content: 20 numbered rows.
fn tall_content() -> (View, i32) {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..20 {
        col = col.child(text(format!("row {i}")));
    }
    (col.build(), 20)
}

/// Settle the deferred geometry loop: draw (probes record), fire due
/// timers (probes publish), flush (pins apply), repeat until quiet.
/// Mirrors what consecutive `Driver::turn`s do in a real app.
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

#[test]
fn wheel_and_keys_scroll_and_clip() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let (content, h) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, h)
            .element(cx, t)
            .build()
    });
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).starts_with("row 0"));
    assert!(!canvas.row_text(3).contains("row 7"), "clipped to viewport");
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1); // +3
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).starts_with("row 3"),
        "{:?}",
        canvas.row_text(0)
    );
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down); // +1
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).starts_with("row 4"));
    key(&mut tree, Key::End);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(3).starts_with("row 19"),
        "clamped to bottom"
    );
}

#[test]
fn scrolled_away_content_is_not_hit_testable() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let (content, h) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, h)
            .element(cx, t)
            .build()
    });
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    tree.layout();
    // "row 0"'s text instance now sits ABOVE the viewport (negative
    // y). A hit at (2, 0) must resolve inside the visible content,
    // never to a node whose solved rect is scrolled out.
    let hit = tree.hit_test(crate::base::Point::new(2, 0)).expect("hit");
    let r = tree.rect_of(hit);
    assert!(r.y >= 0, "hit a scrolled-away instance at {r:?}");
}

#[test]
fn nested_scrolls_route_the_wheel_to_the_nearest() {
    // RT3-4's shape: an inner scroll inside an outer scroll's content.
    // A wheel over the inner must move ONLY the inner offset.
    let t = &default_theme().tokens;
    let size = Size::new(40, 14);
    type OffsetPair = (crate::reactive::Signal<i32>, crate::reactive::Signal<i32>);
    let holders: Rc<RefCell<Option<OffsetPair>>> = Rc::new(RefCell::new(None));
    let h2 = holders.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let outer_y = cx.signal(0i32);
        let inner_y = cx.signal(0i32);
        *h2.borrow_mut() = Some((outer_y, inner_y));
        let (inner_content, _) = tall_content();
        let inner = Scroll::new(inner_content)
            .content_size(30, 50)
            .offset_y(inner_y)
            .layout(
                LayoutStyle::default()
                    .width(crate::layout::Dimension::Cells(34))
                    .height(crate::layout::Dimension::Cells(6)),
            )
            .element(cx, t)
            .build();
        let (outer_rows, _) = tall_content();
        let content = Element::new()
            .style(LayoutStyle::column())
            .child(inner)
            .child(outer_rows)
            .build();
        Scroll::new(content)
            .content_size(36, 100)
            .offset_y(outer_y)
            .layout(LayoutStyle::default().grow(1.0))
            .element(cx, t)
            .build()
    });
    tree.layout();
    let (outer_y, inner_y) = holders.borrow().expect("signals");
    mouse(&mut tree, MouseKind::Move, 5, 2);
    mouse(&mut tree, MouseKind::ScrollDown, 5, 2);
    assert!(
        inner_y.get_untracked() > 0,
        "inner consumes: {}",
        inner_y.get_untracked()
    );
    assert_eq!(outer_y.get_untracked(), 0, "outer must not double-scroll");
    // Below the inner widget: the outer takes it.
    let inner_before = inner_y.get_untracked();
    mouse(&mut tree, MouseKind::Move, 5, 12);
    mouse(&mut tree, MouseKind::ScrollDown, 5, 12);
    assert!(
        outer_y.get_untracked() > 0,
        "outer takes the wheel outside inner"
    );
    assert_eq!(inner_y.get_untracked(), inner_before);
}

#[test]
fn scrollbar_drag_jumps_the_offset() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let (content, h) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, h)
            .element(cx, t)
            .build()
    });
    // The bar is the last column; drag the thumb to the bottom.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 11, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 11, 3);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 11, 3);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).starts_with("row 16"),
        "drag to bottom = max offset: {:?}",
        canvas.row_text(0)
    );
}

// ---------------------------------------------------------------------------
// 0130: measured extent (no hint) + hint-wins.
// ---------------------------------------------------------------------------

#[test]
fn measured_extent_scrolls_to_the_true_last_row_without_a_hint() {
    // No content_size: the solver measures the mounted column (20 text
    // leaves), the probe publishes it, and End reaches the true bottom.
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| Scroll::new(content).element(cx, t).build());
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).starts_with("row 0"));
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::End);
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(3).starts_with("row 19"),
        "End must reach the measured bottom: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    // And the clamp is exact: one more wheel-down changes nothing.
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(3).starts_with("row 19"), "clamped");
}

#[test]
fn content_size_hint_wins_over_measurement() {
    // 20 measurable rows, but the hint says 30: End scrolls into the
    // hint's blank overhang (offset 26), which is only reachable if the
    // HINT governed the clamp — a measured extent (20) would stop at 16
    // with "row 19" on the bottom row.
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(content)
            .content_size(10, 30)
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::End);
    let canvas = settle(&mut tree, size);
    assert!(
        !canvas.row_text(3).contains("row"),
        "End under a 30-row hint must scroll past the 20 real rows: {:?}",
        canvas.row_text(3)
    );
}

#[test]
fn default_layout_takes_leftover_not_content_basis() {
    // The 0240 modal-overflow class: a default-layout Scroll beside a
    // fixed one-row sibling in a tight column must leave that row
    // visible (basis 0 = the scroll takes LEFTOVER, exerting no
    // overflow pressure that would shrink the sibling to zero).
    let t = &default_theme().tokens;
    let size = Size::new(12, 6);
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(crate::layout::Dimension::Percent(1.0))
                    .height(crate::layout::Dimension::Cells(6)),
            )
            .child(Scroll::new(content).element(cx, t).build())
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BUTTON"))
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    assert!(
        (0..6).any(|y| canvas.row_text(y).contains("BUTTON")),
        "fixed sibling row must survive beside a default-layout Scroll:\n{:?}",
        (0..6).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    assert!(
        canvas.row_text(0).starts_with("row 0"),
        "scroll still shows its content head"
    );
}

// ---------------------------------------------------------------------------
// 0130: follow-tail disengage / re-arm / jump (the transcript idiom).
// ---------------------------------------------------------------------------

struct FollowRig {
    feed: FeedState,
    follow: crate::reactive::Signal<bool>,
}

fn mount_follow_feed(size: Size) -> (crate::reactive::RootScope, UiTree, FollowRig) {
    let holder: Rc<RefCell<Option<FollowRig>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let feed = FeedState::new(cx);
        let follow = cx.signal(true);
        *h.borrow_mut() = Some(FollowRig {
            feed: feed.clone(),
            follow,
        });
        Scroll::new(Feed::new(&feed).gap(0).view(cx))
            .follow_tail(follow)
            .element(cx, &t)
            .build()
    });
    let rig = holder.borrow_mut().take().expect("rig captured");
    (root, tree, rig)
}

#[test]
fn follow_tail_pins_growth_disengages_on_wheel_and_rearms_at_bottom() {
    let size = Size::new(16, 4);
    let (root, mut tree, rig) = mount_follow_feed(size);
    for i in 0..10 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(3).contains("line 9"),
        "pinned to the tail after growth: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    assert!(rig.follow.get_untracked(), "still following at the bottom");

    // Wheel up: releases the tail; growth no longer moves the view.
    mouse(&mut tree, MouseKind::ScrollUp, 2, 1);
    assert!(!rig.follow.get_untracked(), "scroll-up must disengage");
    let canvas = settle(&mut tree, size);
    let held = canvas.row_text(0);
    rig.feed.push("m10", FeedItem::text("line 10"));
    let canvas = settle(&mut tree, size);
    assert_eq!(
        canvas.row_text(0),
        held,
        "growth must not move a disengaged view"
    );

    // Wheel back down to the bottom edge: re-arms; growth pins again.
    // (oy sits at 3; the tail is now 11 rows so max is 7 — two wheel
    // steps of +3 reach it, the second clamping onto the edge.)
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    assert!(
        !rig.follow.get_untracked(),
        "mid-content wheel-down must not re-arm early"
    );
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    assert!(rig.follow.get_untracked(), "bottom edge must re-arm");
    rig.feed.push("m11", FeedItem::text("line 11"));
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(3).contains("line 11"),
        "re-armed follow pins the new tail: {:?}",
        canvas.row_text(3)
    );
    root.dispose();
}

#[test]
fn app_can_force_follow_to_jump_to_latest() {
    let size = Size::new(16, 4);
    let (root, mut tree, rig) = mount_follow_feed(size);
    for i in 0..12 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let _ = settle(&mut tree, size);
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Home); // to the top: disengaged
    assert!(!rig.follow.get_untracked());
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("line 0"));

    rig.follow.set(true); // the "jump to latest ↓" affordance
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(3).contains("line 11"),
        "forcing the signal must jump to the tail: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// 0281 (first-app): offset repair when content shrinks under a bound
// offset — the details-fold / session-switch void state.
// ---------------------------------------------------------------------------

struct BoundRig {
    feed: FeedState,
    offset: crate::reactive::Signal<i32>,
    follow: crate::reactive::Signal<bool>,
}

/// The consumer shape: external offset + follow signals both bound.
fn mount_bound_feed(size: Size) -> (crate::reactive::RootScope, UiTree, BoundRig) {
    let holder: Rc<RefCell<Option<BoundRig>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, tree) = mount_widget(size, move |cx| {
        let t = default_theme().tokens;
        let feed = FeedState::new(cx);
        let offset = cx.signal(0i32);
        let follow = cx.signal(true);
        *h.borrow_mut() = Some(BoundRig {
            feed: feed.clone(),
            offset,
            follow,
        });
        Scroll::new(Feed::new(&feed).gap(0).view(cx))
            .offset_y(offset)
            .follow_tail(follow)
            .element(cx, &t)
            .build()
    });
    let rig = holder.borrow_mut().take().expect("rig captured");
    (root, tree, rig)
}

#[test]
fn shrink_below_offset_reclamps_and_repaints_without_a_gesture() {
    let size = Size::new(16, 4);
    let (root, mut tree, rig) = mount_bound_feed(size);
    for i in 0..20 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let _ = settle(&mut tree, size);
    // Disengage: the user reads scrollback at a held offset.
    mouse(&mut tree, MouseKind::ScrollUp, 2, 1);
    assert!(!rig.follow.get_untracked());
    let held = rig.offset.get_untracked();
    assert!(held > 0, "reading scrollback at {held}");

    // Session switch: the content is replaced wholesale, far below the
    // held offset — the wrapper lands fully above the clip (the state
    // where an unflagged extent probe starves). The engine must
    // re-clamp and repaint content with NO gesture.
    rig.feed.clear();
    rig.feed.push("n0", FeedItem::text("new 0"));
    rig.feed.push("n1", FeedItem::text("new 1"));
    let canvas = settle(&mut tree, size);
    assert_eq!(
        rig.offset.get_untracked(),
        0,
        "offset repaired to the new max_off"
    );
    assert!(
        canvas.row_text(0).contains("new 0"),
        "pane repaints content immediately:\n{:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    assert!(
        !rig.follow.get_untracked(),
        "a repair is not a gesture: follow stays disengaged"
    );

    // Growth after the repair: a disengaged, in-range offset is never
    // touched (max_off only grows — live streaming must not fight a
    // reading user).
    for i in 0..10 {
        rig.feed
            .push(format!("g{i}"), FeedItem::text(format!("grown {i}")));
    }
    let canvas = settle(&mut tree, size);
    assert_eq!(rig.offset.get_untracked(), 0, "growth keeps the offset");
    assert!(canvas.row_text(0).contains("new 0"), "view held on growth");
    root.dispose();
}

#[test]
fn restored_offset_survives_startup_measurement() {
    // An app restoring a session may write the offset BEFORE the first
    // frame measures anything: the repair must stay inert until the
    // extent is real (the (0,0) unmeasured sentinel), never snap a
    // valid restored offset to 0.
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let holder: Rc<RefCell<Option<crate::reactive::Signal<i32>>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let offset = cx.signal(12i32); // restored scroll position
        *h.borrow_mut() = Some(offset);
        Scroll::new(content).offset_y(offset).element(cx, t).build()
    });
    let offset = holder.borrow().expect("signal");
    let canvas = settle(&mut tree, size);
    assert_eq!(offset.get_untracked(), 12, "restored offset kept");
    assert!(
        canvas.row_text(0).starts_with("row 12"),
        "{:?}",
        canvas.row_text(0)
    );
}

#[test]
fn viewport_growth_reclamps_a_hint_mode_offset() {
    // Hint mode has no measurement, but the repair still covers it:
    // a taller viewport shrinks max_off under a bottom-held offset.
    let t = &default_theme().tokens;
    let size = Size::new(12, 4);
    let holder: Rc<RefCell<Option<crate::reactive::Signal<i32>>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (content, _) = tall_content();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let offset = cx.signal(26i32); // bottom under a 30-row hint
        *h.borrow_mut() = Some(offset);
        Scroll::new(content)
            .content_size(10, 30)
            .offset_y(offset)
            .element(cx, t)
            .build()
    });
    let offset = holder.borrow().expect("signal");
    let _ = settle(&mut tree, size);
    assert_eq!(offset.get_untracked(), 26, "in range at 4 rows");
    let tall = Size::new(12, 12);
    tree.set_viewport(tall);
    let _ = settle(&mut tree, tall);
    assert_eq!(
        offset.get_untracked(),
        18,
        "viewport growth re-clamps: 30 - 12"
    );
}

#[test]
fn follow_tail_repins_across_resize() {
    // The width-change row-count case: wrapped content re-typesets on
    // resize, the extent changes, and an engaged follow keeps the tail.
    let size = Size::new(24, 5);
    let (root, mut tree, rig) = mount_follow_feed(size);
    for i in 0..8 {
        rig.feed.push(
            format!("m{i}"),
            FeedItem::text(format!("message {i} with words that wrap when narrow")),
        );
    }
    let canvas = settle(&mut tree, size);
    assert!(
        (0..5).any(|y| canvas.row_text(y).contains("message 7")
            || canvas.row_text(y).contains("wrap when narrow")),
        "tail visible before resize"
    );
    assert!(rig.follow.get_untracked());

    tree.set_viewport(Size::new(14, 5));
    let narrow = Size::new(14, 5);
    let canvas = settle(&mut tree, narrow);
    let dump: Vec<String> = (0..5).map(|y| canvas.row_text(y)).collect();
    assert!(
        dump.iter().any(|r| r.contains("narrow")),
        "tail (last item's wrapped end) still pinned after resize:\n{dump:#?}"
    );
    assert!(rig.follow.get_untracked(), "resize must not disengage");
    root.dispose();
}
