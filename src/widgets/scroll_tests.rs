//! Scroll tests: wheel/keys/clip and drag (v1), plus the 0130 wave —
//! measured content extent (no hint), hint-wins, follow-tail
//! disengage/re-arm, and the leftover-not-content default basis (the
//! 0240 modal-overflow follow-up).

use super::*;
use crate::base::{Point, Size};
use crate::layout::Style as LayoutStyle;
use crate::theme::default_theme;
use crate::ui::{text, Element, Key, MouseButton, MouseKind, UiTree};
use crate::widgets::itest_util::{key, mount_widget, mouse, render, settle};
use crate::widgets::{Feed, FeedItem, FeedState};
use std::cell::RefCell;
use std::rc::Rc;

/// 1-column-wide content: 20 numbered rows. Shared with the sibling
/// `scroll_extent_tests.rs` (pub(super), the feed_tests precedent).
pub(super) fn tall_content() -> (View, i32) {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..20 {
        col = col.child(text(format!("row {i}")));
    }
    (col.build(), 20)
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

/// The field report, as an assertion: "I am unsure the scrollbar is
/// clickable/draggable at all". Grabbing the thumb must move NOTHING —
/// the gesture takes hold, and only the drag that follows scrolls. The
/// pre-seam handler mapped the press row straight onto an offset with a
/// travel range that did not match the thumb's, so the content jumped
/// (up to 300 rows on a long transcript) and the thumb slid out from
/// under the cursor before the drag began.
#[test]
fn a_press_on_the_thumb_moves_nothing() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, off) = mount_bar(size, content, t, None);
    off.set(45);
    let canvas = render(&mut tree, size);
    let bar_x = (size.w - 1) as usize;
    let thumb_top = (0..size.h)
        .find(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█'))
        .expect("thumb drawn");
    mouse(
        &mut tree,
        MouseKind::Down(MouseButton::Left),
        bar_x as i32,
        thumb_top,
    );
    assert_eq!(
        off.get_untracked(),
        45,
        "a press ON the thumb is a grab, not a jump"
    );
    let canvas = render(&mut tree, size);
    assert_eq!(
        (0..size.h).find(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█')),
        Some(thumb_top),
        "and the thumb stays exactly where the eye left it"
    );
}

/// The drag itself: the thumb travels with the pointer, row for row,
/// instead of somewhere proportional-but-elsewhere.
#[test]
fn a_thumb_drag_keeps_the_thumb_under_the_pointer() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, off) = mount_bar(size, content, t, None);
    off.set(45);
    let canvas = render(&mut tree, size);
    let bar_x = (size.w - 1) as usize;
    let thumb_top = (0..size.h)
        .find(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█'))
        .expect("thumb drawn");
    mouse(
        &mut tree,
        MouseKind::Down(MouseButton::Left),
        bar_x as i32,
        thumb_top,
    );
    mouse(
        &mut tree,
        MouseKind::Drag(MouseButton::Left),
        bar_x as i32,
        thumb_top + 2,
    );
    assert!(off.get_untracked() > 45, "the drag scrolled forward");
    let canvas = render(&mut tree, size);
    assert_eq!(
        (0..size.h).find(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█')),
        Some(thumb_top + 2),
        "the thumb followed the pointer exactly two rows"
    );
    // The drag survives the pointer leaving the strip (pointer capture).
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 0, size.h - 1);
    assert_eq!(
        off.get_untracked(),
        90,
        "off-strip drag still steers, clamped"
    );
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 0, size.h - 1);
    assert_eq!(off.get_untracked(), 90, "release commits, never snaps back");
}

/// A press on bare TRACK teleports (the macOS convention) and lands the
/// thumb UNDER the pointer, so the drag that follows starts where the
/// eye already is.
#[test]
fn a_track_press_lands_the_thumb_under_the_pointer() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, off) = mount_bar(size, content, t, None);
    let bar_x = (size.w - 1) as usize;
    mouse(
        &mut tree,
        MouseKind::Down(MouseButton::Left),
        bar_x as i32,
        8,
    );
    assert!(off.get_untracked() > 0, "the track press moved the content");
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.row_text(8).chars().nth(bar_x),
        Some('█'),
        "row 8 was pressed, row 8 wears the thumb"
    );
}

/// The strip declares itself a DRAG ZONE (first-app/1335), so screen
/// select mode stands down over it and the thumb keeps its gesture.
/// Content columns must not: a list of text stays selectable.
#[test]
fn the_strip_is_a_drag_zone_and_the_content_is_not() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, _off) = mount_bar(size, content, t, None);
    settle(&mut tree, size);
    let bar_x = size.w - 1;
    assert!(
        tree.press_probe_at(Point::new(bar_x, 3)).drag_owner,
        "the strip owns drags"
    );
    assert!(
        !tree.press_probe_at(Point::new(1, 3)).drag_owner,
        "content text stays selectable"
    );
}

/// An invisible target owns nothing: with `scrollbar_auto_hide` and
/// content that fits, the strip claims neither the offset nor the
/// gesture — a selection starting there is still a selection.
#[test]
fn an_auto_hidden_strip_is_not_a_drag_zone() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..3 {
        col = col.child(text(format!("row {i}")));
    }
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(col.build())
            .content_size(10, 3)
            .scrollbar_auto_hide(true)
            .element(cx, t)
            .build()
    });
    settle(&mut tree, size);
    assert!(
        !tree.press_probe_at(Point::new(size.w - 1, 2)).drag_owner,
        "a hidden bar must not swallow a selection"
    );
}

/// A `Drag` that arrives with no grab of ours belongs to somebody
/// else's gesture. Two sources: terminals emit motion-with-button after
/// a lost `Up`, and (before first-app/1335 taught the selection layer to
/// stand down here) a claimed screen-text selection cancelled the
/// strip's capture mid-gesture. It must never teleport the offset.
#[test]
fn a_bare_drag_without_a_press_never_steers() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, off) = mount_bar(size, content, t, None);
    off.set(45);
    let bar_x = size.w - 1;
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), bar_x, 2);
    assert_eq!(off.get_untracked(), 45, "no grab, no steering");
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), bar_x, 9);
    assert_eq!(off.get_untracked(), 45);
}

/// `scrollbar_width` widens the gutter for apps with room for a
/// comfortable mouse target. The strip is RESERVED, so the cells come
/// out of the content — and the whole strip is grabbable.
#[test]
fn scrollbar_width_widens_the_gutter_and_stays_grabbable() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, off) = mount_bar(size, content, t, Some(2));
    let canvas = render(&mut tree, size);
    let row0 = canvas.row_text(0);
    assert_eq!(row0.chars().nth(10), Some('█'), "{row0:?}");
    assert_eq!(row0.chars().nth(11), Some('█'), "the thumb spans the strip");
    // The LEFT column of the widened strip is a live target too.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 10, 8);
    assert!(off.get_untracked() > 0, "the widened column steers");
}

/// The affordance: the thumb takes accent ink while the pointer is on
/// the strip, and keeps it for the whole drag (the pointer leaves the
/// strip constantly mid-drag). Without it the one bar you CAN grab was
/// the one that never answered the pointer — the "is this even
/// clickable?" half of the field report.
#[test]
fn the_thumb_lights_under_the_pointer_and_stays_lit_while_dragging() {
    let t = &default_theme().tokens;
    let size = Size::new(12, 10);
    let (content, _) = tall_content();
    let (_root, mut tree, _off) = mount_bar(size, content, t, None);
    let bar_x = size.w - 1;
    let ink_at = |tree: &mut UiTree, y: i32| {
        render(tree, size)
            .cell(crate::base::Point::new(bar_x, y))
            .map(|(_, fg, _)| fg)
    };
    let cold = ink_at(&mut tree, 0).expect("thumb cell");
    mouse(&mut tree, MouseKind::Move, bar_x, 0);
    let hot = ink_at(&mut tree, 0).expect("thumb cell");
    assert_ne!(cold, hot, "the strip answers the pointer");
    // A drag whose pointer wanders off the strip keeps the ink.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 0, 3);
    let dragging = ink_at(&mut tree, 3).expect("thumb cell");
    assert_eq!(dragging, hot, "still lit mid-drag, off-strip");
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 0, 3);
    assert_eq!(
        ink_at(&mut tree, 3),
        Some(cold),
        "and cools when the gesture ends away from the strip"
    );
}

/// Mount a `Scroll` with a bound offset over 100 hinted rows: a 10-row
/// viewport, a 3-row thumb, 7 rows of travel — coarse enough to expose a
/// mapping that is off by a cell, which the 4-row rigs above cannot see.
fn mount_bar(
    size: Size,
    content: View,
    t: &crate::theme::TokenSet,
    width: Option<i32>,
) -> (crate::reactive::RootScope, UiTree, Signal<i32>) {
    let cell: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let out = cell.clone();
    let (root, tree) = mount_widget(size, |cx| {
        let off = cx.signal(0i32);
        *out.borrow_mut() = Some(off);
        let mut s = Scroll::new(content).content_size(10, 100).offset_y(off);
        if let Some(w) = width {
            s = s.scrollbar_width(w);
        }
        s.element(cx, t).build()
    });
    let off = cell.borrow().expect("offset bound at mount");
    (root, tree, off)
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
fn frozen_follow_tail_holds_its_rows_and_repins_on_thaw() {
    // 1300: a live screen selection freezes the tail so the cells under
    // the drag are the cells the copy reads. The freeze is the widgets-
    // layer half — driven here directly, wired to `app::selection` in
    // `tests/adv_selection.rs`.
    let size = Size::new(16, 4);
    let (root, mut tree, rig) = mount_follow_feed(size);
    for i in 0..10 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(3).contains("line 9"), "pinned to the tail");
    // Text columns only: the scrollbar thumb SHOULD move while frozen —
    // content grew below the viewport and the bar reports that honestly.
    let body = |s: String| s.chars().take((size.w - 1) as usize).collect::<String>();
    let held: Vec<String> = (0..4).map(|y| body(canvas.row_text(y))).collect();

    // Frozen: appends land below the viewport, the visible rows do not
    // move, and `follow` stays ARMED (a freeze is not a disengage — the
    // app's "following" chrome must not flicker for the drag).
    super::freeze_follow_tail(true);
    rig.feed.push("m10", FeedItem::text("line 10"));
    rig.feed.push("m11", FeedItem::text("line 11"));
    let canvas = settle(&mut tree, size);
    assert_eq!(
        (0..4).map(|y| body(canvas.row_text(y))).collect::<Vec<_>>(),
        held,
        "a frozen tail must not move under the selection"
    );
    assert!(
        rig.follow.get_untracked(),
        "freezing must not disengage follow"
    );

    // Thawed: one settle turn re-pins to the tail AS IT STANDS NOW —
    // the frozen appends are not replayed, the view jumps to live.
    super::freeze_follow_tail(false);
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(3).contains("line 11"),
        "thaw re-pins to the live tail: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
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

#[test]
fn plain_arrow_up_with_follow_at_tail_moves_one_row_not_to_head() {
    // agora-tui plain_arrow harness: one ↑ must not snap to list head when
    // follow was pinned at the tail (the app-side class lives in custom
    // scroll_target; this pins engine Scroll behavior).
    let size = Size::new(16, 4);
    let (root, mut tree, rig) = mount_bound_feed(size);
    for i in 0..20 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let canvas = settle(&mut tree, size);
    assert!(rig.follow.get_untracked(), "starts following at the tail");
    let at_tail = rig.offset.get_untracked();
    assert!(at_tail > 0, "tail pin must leave a non-zero offset");
    assert!(
        canvas.row_text(3).contains("line 19"),
        "tail visible: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );

    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Up);
    let canvas = settle(&mut tree, size);
    let after = rig.offset.get_untracked();
    assert!(!rig.follow.get_untracked(), "plain ↑ disengages follow");
    assert_eq!(after, at_tail - 1, "one content row up, not a head snap");
    assert!(
        !canvas.row_text(0).contains("line 0"),
        "viewport must not jump to the list head: {:?}",
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

/// The thumb has a usable FLOOR (first-app/1320). The exact proportion
/// of a long transcript rounds to zero — the old `clamp(1, h)` then drew
/// a single glyph, a dot the eye cannot find or follow. Field report
/// (abstractcode-tui, 2026-08-19): "the scrollbar is too small".
#[test]
fn scrollbar_thumb_never_shrinks_to_a_dot() {
    let size = Size::new(16, 20);
    let (root, mut tree, rig) = mount_follow_feed(size);
    // 2000 rows in a 20-row pane: the proportional thumb is 20*20/2000
    // = 0 cells.
    for i in 0..2000 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let canvas = settle(&mut tree, size);
    let bar_x = (size.w - 1) as usize;
    let thumb_rows = (0..size.h)
        .filter(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█'))
        .count();
    assert!(
        thumb_rows >= 3,
        "a 2000-row buffer must still draw a findable thumb, got {thumb_rows} rows"
    );
    assert!(
        thumb_rows < size.h as usize,
        "...and it must keep room to travel, got {thumb_rows} of {}",
        size.h
    );
    root.dispose();
}

/// The floor never eats the track: content that FITS still fills the
/// bar (that is the honest "nothing to scroll" answer), and a tiny
/// viewport yields rather than covering its own travel room.
#[test]
fn scrollbar_thumb_floor_yields_to_short_tracks() {
    let size = Size::new(16, 3);
    let (root, mut tree, rig) = mount_follow_feed(size);
    for i in 0..200 {
        rig.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    let canvas = settle(&mut tree, size);
    let bar_x = (size.w - 1) as usize;
    let thumb_rows = (0..size.h)
        .filter(|&y| canvas.row_text(y).chars().nth(bar_x) == Some('█'))
        .count();
    assert!(
        (1..size.h as usize).contains(&thumb_rows),
        "a 3-row track keeps at least one row of travel, got {thumb_rows}"
    );
    root.dispose();
}
