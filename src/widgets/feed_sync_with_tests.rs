//! `FeedState::sync_with` tests (first-app/0282): the borrow-based
//! source door for items living INSIDE a larger reactive shape.
//! Since `sync` now delegates to `sync_with`, the whole
//! `feed_sync_tests` suite already pins the shared drain core — these
//! pin what only the new door can express: a fold-shaped source whose
//! stats-only writes must render nothing (the fingerprint path), a
//! focus-selected nested projection over two signals, and the
//! one-writer self-heal reached through the borrow read.
//!
//! Child module of `feed_sync_tests` (shares its `Msg`/`settle` rig).

use super::*;

/// The 0282 evidence shape: items are ONE FIELD of a larger state
/// struct whose siblings (stats here) mutate under the SAME signal —
/// splitting items out would be surgery through the app's most
/// contract-laden type, and a clone-mirror would copy the vec on
/// every stats write.
struct Fold {
    items: Vec<Msg>,
    stats: u64,
}

/// A conversation whose transcript is NESTED state — the sync source
/// is a focus-dependent projection, not any signal that exists.
struct Convo {
    items: Vec<Msg>,
}

/// What the fold mount hands back for driving + inspection.
type FoldMount = (
    crate::reactive::RootScope,
    UiTree,
    Signal<Fold>,
    FeedState,
    Rc<Cell<usize>>,
);

/// The holder cell fold mounts capture state through.
type FoldHolder = Rc<RefCell<Option<(Signal<Fold>, FeedState)>>>;

/// Mount a feed synced through `sync_with` to a fresh `Signal<Fold>`;
/// returns the fold signal and the render-call counter (the cost pin
/// instrument, exactly the `mount_synced` shape).
fn mount_fold(size: Size) -> FoldMount {
    let renders = Rc::new(Cell::new(0usize));
    let holder: FoldHolder = Rc::new(RefCell::new(None));
    let (h, r) = (holder.clone(), renders.clone());
    let (root, mut tree) = mount_widget(size, move |cx| {
        let fold: Signal<Fold> = cx.signal(Fold {
            items: Vec::new(),
            stats: 0,
        });
        let feed = FeedState::new(cx);
        feed.sync_with(
            cx,
            // The borrow door: items handed over in place from inside
            // the fold signal's cell — zero copies, `fold` tracked.
            move |read| fold.with(|f| read(&f.items)),
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
        *h.borrow_mut() = Some((fold, feed.clone()));
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).view(cx))
            .build()
    });
    let (fold, feed) = holder.borrow_mut().take().expect("state captured");
    let _ = settle(&mut tree, size);
    (root, tree, fold, feed, renders)
}

#[test]
fn fold_shaped_source_syncs_and_stats_only_writes_render_nothing() {
    let size = Size::new(24, 12);
    let (root, mut tree, fold, feed, renders) = mount_fold(size);

    fold.update(|f| {
        f.items = vec![msg("a", "alpha"), msg("b", "beta")];
        f.stats += 1;
    });
    flush_effects();
    assert_eq!(renders.get(), 2, "initial fill renders each item once");
    assert_eq!(feed.len(), 2);

    // The whole point of the borrow door: a STATS-ONLY write on the
    // one fold signal re-runs the drain (unavoidable — one signal),
    // but the fingerprint walk finds nothing changed and renders
    // NOTHING. A clone-mirror adapter would have copied the vec here;
    // the render counter is the honest instrument either way.
    for _ in 0..5 {
        fold.update(|f| f.stats += 1);
        flush_effects();
    }
    assert_eq!(
        renders.get(),
        2,
        "stats-only writes must not re-render items (fingerprint path)"
    );
    assert_eq!(feed.len(), 2, "no rebuild either");

    // An append THROUGH the fold stays the O(1) fast path: exactly one
    // new render, no rebuild of the shown prefix.
    fold.update(|f| {
        f.stats += 1;
        f.items.push(msg("c", "gamma"));
    });
    flush_effects();
    assert_eq!(renders.get(), 3, "append renders only the new item");
    assert_eq!(feed.len(), 3);

    // And a fingerprint bump inside the fold updates in place.
    fold.update(|f| {
        f.items[1].rev = 1;
        f.items[1].text = "BETA2".into();
    });
    flush_effects();
    assert_eq!(renders.get(), 4, "one changed fingerprint = one render");
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(dump.iter().any(|r| r.contains("alpha")), "{dump:#?}");
    assert!(dump.iter().any(|r| r.contains("BETA2")), "{dump:#?}");
    assert!(dump.iter().any(|r| r.contains("gamma")), "{dump:#?}");
    root.dispose();
}

#[test]
fn focus_switched_nested_source_rebuilds_on_focus_and_stays_fast_within() {
    let size = Size::new(24, 12);
    let renders = Rc::new(Cell::new(0usize));
    type Held = (Signal<Vec<Convo>>, Signal<usize>, FeedState);
    let holder: Rc<RefCell<Option<Held>>> = Rc::new(RefCell::new(None));
    let (h, r) = (holder.clone(), renders.clone());
    let (root, _tree) = mount_widget(size, move |cx| {
        let convos: Signal<Vec<Convo>> = cx.signal(vec![
            Convo {
                items: vec![msg("c0-a", "zero alpha")],
            },
            Convo {
                items: vec![msg("c1-x", "one ex"), msg("c1-y", "one why")],
            },
        ]);
        let focus: Signal<usize> = cx.signal(0usize);
        let feed = FeedState::new(cx);
        feed.sync_with(
            cx,
            // The focus-dependent projection: the source is "the
            // focused convo's nested items" — BOTH signals become
            // dependencies of the sync effect.
            move |read| {
                let at = focus.get();
                convos.with(|cs| read(&cs[at].items));
            },
            SyncSpec::new(
                |m: &Msg| m.id.to_string(),
                |m| m.rev,
                move |m| {
                    r.set(r.get() + 1);
                    FeedItem::text(m.text.clone())
                },
            ),
        );
        *h.borrow_mut() = Some((convos, focus, feed.clone()));
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).view(cx))
            .build()
    });
    let (convos, focus, feed) = holder.borrow_mut().take().expect("state captured");
    flush_effects();
    assert_eq!(renders.get(), 1, "focused convo 0 renders its one item");
    assert_eq!(feed.len(), 1);

    // Appends within the focused convo take the fast path.
    convos.update(|cs| cs[0].items.push(msg("c0-b", "zero beta")));
    flush_effects();
    assert_eq!(renders.get(), 2, "in-focus append is O(1)");
    assert_eq!(feed.len(), 2);

    // Switching focus swaps the whole visible list (different keys):
    // the rebuild path, driven purely by the focus signal.
    focus.set(1);
    flush_effects();
    assert_eq!(renders.get(), 4, "focus switch rebuilds to the other convo");
    assert_eq!(feed.len(), 2);
    assert!(feed.row_of("c1-x").is_some(), "convo 1 items shown");
    assert!(feed.row_of("c0-a").is_none(), "convo 0 items gone");

    // A background write to the UNfocused convo re-runs the drain
    // (same convos signal) but the focused projection is unchanged —
    // nothing renders, nothing rebuilds.
    convos.update(|cs| cs[0].items.push(msg("c0-c", "zero gamma")));
    flush_effects();
    assert_eq!(renders.get(), 4, "unfocused-convo writes render nothing");

    // And the focused convo keeps its fast path after the switch.
    convos.update(|cs| cs[1].items.push(msg("c1-z", "one zed")));
    flush_effects();
    assert_eq!(renders.get(), 5, "post-switch append is O(1)");
    assert_eq!(feed.len(), 3);
    root.dispose();
}

#[test]
fn self_heal_still_fires_through_the_borrow_door() {
    let size = Size::new(24, 12);
    let (root, mut tree, fold, feed, renders) = mount_fold(size);
    fold.update(|f| f.items = vec![msg("a", "alpha"), msg("b", "beta")]);
    flush_effects();
    assert_eq!(renders.get(), 2);

    // The violation: the app writes past the bridge.
    feed.push("stray", FeedItem::text("stray row"));
    assert_eq!(feed.len(), 3, "precondition: the stray landed");

    // Next drain arrives through the fold — an append-only change that
    // would stay on the fast path if the one-writer detector did not
    // ride the shared drain core.
    fold.update(|f| f.items.push(msg("c", "gamma")));
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

    // Fast paths resume after the heal, exactly like the sync door.
    fold.update(|f| f.items.push(msg("d", "delta")));
    flush_effects();
    assert_eq!(renders.get(), 6, "fast paths resume after the heal");
    assert_eq!(feed.len(), 4);
    root.dispose();
}
