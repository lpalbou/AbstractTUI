//! `FeedState::sync` tests: the scripted-fold path pins (append-only
//! never rebuilds, mid-list changes rebuild, tail visibility appends)
//! and the pixel-parity bar — a sync-driven feed renders EXACTLY what
//! a hand-pushed feed renders for the same end state, across reorder,
//! mid-list update, burst append and full replace.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::base::{Point, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{flush_effects, run_due_timers, Signal};
use crate::ui::{BufferCanvas, Element, UiTree};
use crate::widgets::itest_util::{mount_widget, render};
use crate::widgets::{Feed, FeedItem, FeedState, SyncSpec};

/// The scripted source row: id = identity, rev = fingerprint,
/// hidden = visibility, text = pixels.
#[derive(Clone)]
struct Msg {
    id: String,
    rev: u64,
    hidden: bool,
    text: String,
}

fn msg(id: &str, text: &str) -> Msg {
    Msg {
        id: id.to_string(),
        rev: 0,
        hidden: false,
        text: text.to_string(),
    }
}

/// One full settle (the feed_tests recipe): effects -> layout -> draw
/// (width discovery) -> due timers (geometry sync) -> effects -> draw.
fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let _ = render(tree, size);
    run_due_timers(std::time::Instant::now());
    flush_effects();
    tree.layout();
    render(tree, size)
}

/// What `mount_synced` hands back for driving + inspection.
type SyncedMount = (
    crate::reactive::RootScope,
    UiTree,
    Signal<Vec<Msg>>,
    FeedState,
    Rc<Cell<usize>>,
);

/// The holder cell tests capture mount-created state through.
type StateHolder = Rc<RefCell<Option<(Signal<Vec<Msg>>, FeedState)>>>;

/// Mount a feed synced to a fresh `Signal<Vec<Msg>>`; returns the
/// source signal and a render-call counter (the cost pin instrument).
fn mount_synced(size: Size) -> SyncedMount {
    let renders = Rc::new(Cell::new(0usize));
    let holder: StateHolder = Rc::new(RefCell::new(None));
    let (h, r) = (holder.clone(), renders.clone());
    let (root, mut tree) = mount_widget(size, move |cx| {
        let items: Signal<Vec<Msg>> = cx.signal(Vec::new());
        let feed = FeedState::new(cx);
        feed.sync(
            cx,
            items,
            SyncSpec::new(
                |m: &Msg| m.id.to_string(),
                |m| m.rev,
                move |m| {
                    r.set(r.get() + 1);
                    FeedItem::text(m.text.clone())
                },
            )
            .visible(|m| !m.hidden),
        );
        *h.borrow_mut() = Some((items, feed.clone()));
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).view(cx))
            .build()
    });
    let (items, feed) = holder.borrow().clone().expect("state captured");
    let _ = settle(&mut tree, size);
    (root, tree, items, feed, renders)
}

/// Hand-pushed reference feed for the parity bar.
fn mount_reference(size: Size) -> (crate::reactive::RootScope, UiTree, FeedState) {
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        *h.borrow_mut() = Some(feed.clone());
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).view(cx))
            .build()
    });
    let feed = holder.borrow().clone().expect("state captured");
    (root, tree, feed)
}

fn assert_same_pixels(a: &BufferCanvas, b: &BufferCanvas, size: Size, label: &str) {
    for y in 0..size.h {
        for x in 0..size.w {
            assert_eq!(
                a.cell(Point::new(x, y)),
                b.cell(Point::new(x, y)),
                "{label}: divergence at ({x},{y}):\nsynced: {:?}\nhand:   {:?}",
                a.row_text(y),
                b.row_text(y)
            );
        }
    }
}

/// Parity harness: drive the synced feed to `list`, hand-push the same
/// visible end state, compare every cell.
fn assert_parity(label: &str, list: Vec<Msg>) {
    let size = Size::new(26, 14);
    let (root_a, mut tree_a, items, _feed, _r) = mount_synced(size);
    items.set(list.clone());
    let canvas_a = settle(&mut tree_a, size);

    let (root_b, mut tree_b, reference) = mount_reference(size);
    for m in list.iter().filter(|m| !m.hidden) {
        reference.push(m.id.clone(), FeedItem::text(m.text.clone()));
    }
    let canvas_b = settle(&mut tree_b, size);

    assert_same_pixels(&canvas_a, &canvas_b, size, label);
    root_a.dispose();
    root_b.dispose();
}

#[test]
fn append_only_folds_take_the_push_path_and_never_rebuild() {
    let size = Size::new(24, 12);
    let (root, mut tree, items, feed, renders) = mount_synced(size);

    items.set(vec![msg("a", "alpha"), msg("b", "beta")]);
    flush_effects();
    assert_eq!(renders.get(), 2, "initial fill renders each item once");
    assert_eq!(feed.len(), 2);

    // Ten appends, one at a time: each run renders ONLY the new item.
    for i in 0..10 {
        items.update(|v| v.push(msg(&format!("k{i}"), &format!("line {i}"))));
        flush_effects();
    }
    assert_eq!(
        renders.get(),
        12,
        "append-only fold must never re-render shown items (no rebuild)"
    );
    assert_eq!(feed.len(), 12);
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("alpha"));
    root.dispose();
}

#[test]
fn fingerprint_change_updates_in_place_without_rebuild() {
    let size = Size::new(24, 12);
    let (root, mut tree, items, feed, renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta"), msg("c", "gamma")]);
    flush_effects();
    assert_eq!(renders.get(), 3);

    // Mid-list content change: bump the fingerprint, keep order.
    items.update(|v| {
        v[1].rev = 1;
        v[1].text = "BETA2".into();
    });
    flush_effects();
    assert_eq!(renders.get(), 4, "one changed fingerprint = one render");
    assert_eq!(feed.len(), 3, "update in place, not append");
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(dump.iter().any(|r| r.contains("BETA2")), "{dump:#?}");
    assert!(!dump.iter().any(|r| r.contains("beta")), "old text gone");
    root.dispose();
}

#[test]
fn mid_list_insert_rebuilds_exactly_once() {
    let size = Size::new(24, 12);
    let (root, _tree, items, feed, renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("c", "gamma")]);
    flush_effects();
    assert_eq!(renders.get(), 2);

    items.update(|v| v.insert(1, msg("b", "beta")));
    flush_effects();
    // Rebuild: every visible item re-renders exactly once (2 + 3).
    assert_eq!(renders.get(), 5, "mid-list insert = one whole rebuild");
    assert_eq!(feed.len(), 3);
    root.dispose();
}

#[test]
fn visibility_flips_mid_list_rebuild_and_tail_flips_append() {
    let size = Size::new(24, 12);
    let (root, _tree, items, feed, renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta"), msg("c", "gamma")]);
    flush_effects();
    assert_eq!(renders.get(), 3);

    // Hide a mid-list item: push order breaks -> rebuild (2 renders).
    items.update(|v| v[1].hidden = true);
    flush_effects();
    assert_eq!(renders.get(), 5, "mid-list hide rebuilds the window");
    assert_eq!(feed.len(), 2);

    // A hidden TAIL item becoming visible is an append, not a rebuild.
    items.update(|v| {
        let mut d = msg("d", "delta");
        d.hidden = true;
        v.push(d);
    });
    flush_effects();
    assert_eq!(renders.get(), 5, "hidden tail item renders nothing");
    items.update(|v| v[3].hidden = false);
    flush_effects();
    assert_eq!(renders.get(), 6, "tail-only visibility flip appends");
    assert_eq!(feed.len(), 3);
    root.dispose();
}

#[test]
fn shrink_and_reorder_take_the_rebuild_path() {
    let size = Size::new(24, 12);
    let (root, _tree, items, feed, renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta"), msg("c", "gamma")]);
    flush_effects();
    assert_eq!(renders.get(), 3);

    // Tail removal: same key prefix but shorter -> must rebuild
    // (the feed cannot remove).
    items.update(|v| {
        v.pop();
    });
    flush_effects();
    assert_eq!(renders.get(), 5, "shrink rebuilds (2 items re-render)");
    assert_eq!(feed.len(), 2);

    // Reorder: prefix keys mismatch -> rebuild.
    items.update(|v| v.swap(0, 1));
    flush_effects();
    assert_eq!(renders.get(), 7, "reorder rebuilds");
    assert_eq!(feed.len(), 2);
    root.dispose();
}

/// The visibility-mirror bar: for every mutation class the synced feed
/// must render the EXACT cells a hand-pushed feed renders for the same
/// visible end state.
#[test]
fn parity_reorder_midlist_update_burst_append_full_replace() {
    // Reorder.
    assert_parity(
        "reorder",
        vec![msg("b", "second first"), msg("a", "first second")],
    );
    // Mid-list update end state.
    let mut updated = vec![msg("a", "alpha"), msg("b", "changed body"), msg("c", "g")];
    updated[1].rev = 3;
    assert_parity("mid-list update", updated);
    // Burst append end state (drive through the signal in two steps to
    // exercise the append path, then compare).
    let size = Size::new(26, 14);
    let (root_a, mut tree_a, items, _f, _r) = mount_synced(size);
    items.set(vec![msg("a", "alpha")]);
    flush_effects();
    items.update(|v| {
        for i in 0..6 {
            v.push(msg(&format!("b{i}"), &format!("burst {i}")));
        }
    });
    let canvas_a = settle(&mut tree_a, size);
    let (root_b, mut tree_b, reference) = mount_reference(size);
    reference.push("a", FeedItem::text("alpha"));
    for i in 0..6 {
        reference.push(format!("b{i}"), FeedItem::text(format!("burst {i}")));
    }
    let canvas_b = settle(&mut tree_b, size);
    assert_same_pixels(&canvas_a, &canvas_b, size, "burst append");
    root_a.dispose();
    root_b.dispose();
    // Full replace (every key different).
    let size = Size::new(26, 14);
    let (root_a, mut tree_a, items, _f, _r) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta")]);
    flush_effects();
    items.set(vec![msg("x", "new one"), msg("y", "new two")]);
    let canvas_a = settle(&mut tree_a, size);
    let (root_b, mut tree_b, reference) = mount_reference(size);
    reference.push("x", FeedItem::text("new one"));
    reference.push("y", FeedItem::text("new two"));
    let canvas_b = settle(&mut tree_b, size);
    assert_same_pixels(&canvas_a, &canvas_b, size, "full replace");
    root_a.dispose();
    root_b.dispose();
}

#[test]
fn hidden_items_never_reach_the_feed_and_parity_holds_with_filter() {
    let mut list = vec![msg("a", "alpha"), msg("b", "beta"), msg("c", "gamma")];
    list[1].hidden = true;
    assert_parity("hidden mid-list", list);
}

/// The one-writer self-heal (cycle-2 review C-1): a manual `push` onto
/// a synced feed is DETECTED at the next drain (mutation counter) and
/// healed with a rebuild — the stray item is evicted, the feed equals
/// the source again, and fast paths resume afterwards (the heal is a
/// one-shot repair, not a permanent rebuild storm).
#[test]
fn foreign_push_between_drains_self_heals_with_a_rebuild() {
    let size = Size::new(24, 12);
    let (root, mut tree, items, feed, renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha"), msg("b", "beta")]);
    flush_effects();
    assert_eq!(renders.get(), 2);

    // The violation: the app writes past the bridge.
    feed.push("stray", FeedItem::text("stray row"));
    assert_eq!(feed.len(), 3, "precondition: the stray landed");

    // Next drain is an APPEND-ONLY source change — before the guard
    // this stayed on the fast path forever and the stray survived.
    items.update(|v| v.push(msg("c", "gamma")));
    flush_effects();
    assert_eq!(feed.len(), 3, "self-heal rebuilt: 3 mirrored, 0 stray");
    assert_eq!(
        renders.get(),
        5,
        "the heal is one full rebuild (2 + 3 renders)"
    );
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(
        !dump.iter().any(|r| r.contains("stray row")),
        "stray write evicted at the next drain: {dump:#?}"
    );

    // A clean append after the heal takes the fast path again.
    items.update(|v| v.push(msg("d", "delta")));
    flush_effects();
    assert_eq!(renders.get(), 6, "fast paths resume after the heal");
    assert_eq!(feed.len(), 4);
    root.dispose();
}

/// The worst shape from the review (C-1): the app manually pushes a
/// key the source appends LATER. Un-healed, the bridge's `push` for
/// that key lands as a replace-in-place at the old index, so feed
/// order diverges from source order permanently while `shown` claims
/// they agree. The self-heal rebuild restores source order.
#[test]
fn foreign_push_of_a_future_source_key_heals_to_source_order() {
    let size = Size::new(24, 12);
    let (root, mut tree, items, feed, _renders) = mount_synced(size);
    items.set(vec![msg("a", "alpha")]);
    flush_effects();

    // Foreign write of key "c" — a key the source will append later.
    feed.push("c", FeedItem::text("premature gamma"));

    // The source then appends b and c: without the heal, "c" would be
    // replaced in place at index 1 and the feed would read a, c, b.
    items.update(|v| {
        v.push(msg("b", "beta"));
        v.push(msg("c", "gamma"));
    });
    let canvas = settle(&mut tree, size);
    assert_eq!(feed.len(), 3);
    let rows: Vec<i32> = ["a", "b", "c"]
        .iter()
        .map(|k| feed.row_of(k).expect("mirrored key"))
        .collect();
    assert!(
        rows[0] < rows[1] && rows[1] < rows[2],
        "feed order equals source order after the heal: {rows:?}"
    );
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(
        dump.iter().any(|r| r.contains("gamma")),
        "the source's render won, not the foreign content: {dump:#?}"
    );
    assert!(
        !dump.iter().any(|r| r.contains("premature")),
        "foreign content evicted: {dump:#?}"
    );
    root.dispose();
}

/// Measured (report evidence, perf-suite convention): one sync run
/// folding a 1k burst append into a feed already mirroring 10k items —
/// the diff walks 11k fingerprints and renders exactly 1k new items.
///
/// ```sh
/// cargo test --release --lib perf_sync_burst -- --ignored --nocapture
/// ```
// The borrow-based source door (first-app/0282) — a child module in a
// sibling file (<600-line budget) so it shares this rig through
// `super::*` with zero visibility churn.
#[path = "feed_sync_with_tests.rs"]
mod sync_with;

#[test]
#[ignore]
fn perf_sync_burst_1k_into_10k() {
    let size = Size::new(40, 12);
    let m = crate::testing::time_median("sync burst 1k into 10k", 1, 5, 1, |_| {
        let (root, mut tree, items, feed, renders) = mount_synced(size);
        items.set(
            (0..10_000)
                .map(|i| msg(&format!("k{i}"), "seed row"))
                .collect(),
        );
        flush_effects();
        let _ = settle(&mut tree, size);
        assert_eq!(renders.get(), 10_000);
        let start = std::time::Instant::now();
        items.update(|v| {
            for i in 0..1_000 {
                v.push(msg(&format!("b{i}"), "burst row"));
            }
        });
        flush_effects();
        eprintln!("  burst fold alone: {:?}", start.elapsed());
        assert_eq!(renders.get(), 11_000, "burst renders only the 1k new items");
        assert_eq!(feed.len(), 11_000);
        root.dispose();
    });
    eprintln!("{}", m.report());
    if !cfg!(debug_assertions) {
        m.assert_under(std::time::Duration::from_secs(3));
    }
}
