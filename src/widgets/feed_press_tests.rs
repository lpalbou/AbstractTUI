//! Feed item-press tests (field-agora 0850): the row -> item hit-info
//! seam (`FeedState::item_at_row`) and the `Feed::on_item_press`
//! wiring — real tree, real dispatch, real typeset geometry. Sibling
//! of `feed_tests.rs` for the file-size discipline.

use std::cell::RefCell;
use std::rc::Rc;

use super::tests::{mount_feed, settle};
use super::*;
use crate::base::Size;
use crate::ui::UiTree;
use crate::widgets::itest_util::{click, mount_widget};

/// Two items with known typeset heights: "a" = 1 row, "b" = 2 rows,
/// default gap 1. Content rows: 0 = a[0], 1 = gap, 2 = b[0], 3 = b[1].
fn seed(feed: &FeedState) {
    feed.push("a", FeedItem::text("alpha"));
    feed.push("b", FeedItem::text("b one\nb two"));
}

#[test]
fn item_at_row_maps_rows_and_refuses_gaps_and_void() {
    let size = Size::new(24, 8);
    let (root, mut tree, feed) = mount_feed(size);
    seed(&feed);
    let _ = settle(&mut tree, size); // width discovery: heights exist now
    assert_eq!(feed.item_at_row(0), Some(("a".into(), 0)));
    assert_eq!(feed.item_at_row(1), None, "the gap row belongs to no one");
    assert_eq!(feed.item_at_row(2), Some(("b".into(), 0)));
    assert_eq!(feed.item_at_row(3), Some(("b".into(), 1)));
    assert_eq!(feed.item_at_row(4), None, "past the tail");
    assert_eq!(feed.item_at_row(-1), None, "negative rows");
    root.dispose();
}

#[test]
fn item_at_row_is_none_before_the_first_draw_discovers_a_width() {
    // The warmup contract (same as total_rows/row_of): heights are 0
    // until typeset, so no row maps to an item — honest, not rounded.
    let (root, ()) = crate::reactive::create_root(|cx| {
        let feed = FeedState::new(cx);
        seed(&feed);
        assert_eq!(feed.item_at_row(0), None);
    });
    root.dispose();
}

#[test]
fn on_item_press_reports_key_and_row_within_item() {
    let size = Size::new(24, 8);
    let log: Rc<RefCell<Vec<(String, i32)>>> = Rc::default();
    let sink = log.clone();
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        seed(&feed);
        *h.borrow_mut() = Some(feed.clone());
        crate::ui::Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Feed::new(&feed)
                    .on_item_press(move |key, row| sink.borrow_mut().push((key.into(), row)))
                    .view(cx),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // a, row 0
    click(&mut tree, 2, 1); // gap: nothing
    click(&mut tree, 2, 2); // b, row 0
    click(&mut tree, 2, 3); // b, row 1
    click(&mut tree, 2, 6); // void below the tail: nothing
    assert_eq!(
        log.borrow().as_slice(),
        &[("a".into(), 0), ("b".into(), 0), ("b".into(), 1)],
        "presses map to (key, row_within_item); gaps and void are silent"
    );
    root.dispose();
}

#[test]
fn press_callback_may_mutate_the_feed_reentrantly() {
    // The state borrow ends inside item_at_row, so the callback can
    // write the SAME FeedState (the click-to-toggle recipe re-pushes
    // the pressed item) without a RefCell panic.
    let size = Size::new(24, 8);
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        seed(&feed);
        *h.borrow_mut() = Some(feed.clone());
        let inner = feed.clone();
        crate::ui::Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Feed::new(&feed)
                    .on_item_press(move |key, _| {
                        inner.push(key, FeedItem::text("toggled\nopen"));
                    })
                    .view(cx),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // re-push "a" from inside the press
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(
        dump.iter().any(|r| r.contains("toggled")),
        "reentrant mutation rendered:\n{dump:#?}"
    );
    root.dispose();
}

/// Disposal-safety law (backlog 0297): the Feed finishes no bookkeeping
/// after the callback, so `on_item_press` may dispose the Feed's scope
/// synchronously (a press that closes the surrounding pane).
#[test]
fn on_item_press_may_dispose_the_feeds_scope() {
    let size = Size::new(24, 8);
    let mut tree = UiTree::new(size);
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::default();
    let h = holder.clone();
    let (root, ()) = crate::reactive::create_root(|cx| {
        let pane_cx = cx.child();
        let feed = FeedState::new(pane_cx);
        seed(&feed);
        *h.borrow_mut() = Some(feed.clone());
        let view = crate::ui::Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Feed::new(&feed)
                    .on_item_press(move |_, _| pane_cx.dispose())
                    .view(pane_cx),
            )
            .build();
        tree.mount(pane_cx, view);
    });
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0);
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}
