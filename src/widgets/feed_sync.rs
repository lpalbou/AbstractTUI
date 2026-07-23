//! `FeedState::sync` — the diffing bridge from a `Signal<Vec<T>>`
//! source of truth to the keyed, append-only feed (backlog 0104).
//!
//! Feed order is PUSH order, so a key may only be appended when it
//! lands at the tail; everything that violates push order (mid-list
//! insert, removal, reorder, a visibility flip before the tail) takes
//! the rebuild path — `clear()` + re-push, the documented rebuild-on-
//! shrink policy, owned HERE so consumers stop re-implementing it
//! slightly wrong. The fast paths stay the default paths: appended
//! tail keys `push` (O(1)), changed fingerprints `update` in place.
//!
//! Child module of `feed` (file-size discipline). Tests live in
//! `feed_sync_tests.rs`.
//!
//! OWNER: CONTENT (app-widgets wave).

use crate::reactive::{Effect, Scope, Signal};

use super::{FeedItem, FeedState};

/// How a synced feed derives identity, change, visibility and pixels
/// from one source item. Construct with [`SyncSpec::new`], add the
/// optional visibility filter with [`SyncSpec::visible`]:
///
/// ```ignore
/// feed.sync(cx, messages, SyncSpec::new(
///     |m: &Msg| m.id.clone(),          // identity (stable, unique)
///     |m| m.rev,                        // cheap change fingerprint
///     |m| FeedItem::markdown(&m.text),  // pixels, built on change only
/// ).visible(|m| !m.hidden));
/// ```
///
/// The FINGERPRINT is the change detector: any `PartialEq` value that
/// changes whenever the rendered output would (a revision counter, a
/// content hash, a small tuple). Fingerprints are the only per-item
/// work on unchanged items — `render` runs only for new keys, changed
/// fingerprints, and rebuilds.
///
/// FLOAT FINGERPRINTS MUST COMPARE BY BITS (cycle-2 review C-2): the
/// change test is `PartialEq`, and IEEE `NaN != NaN` — a fingerprint
/// that is ever `NaN` compares unequal to ITSELF, so the item
/// re-renders + re-typesets on EVERY drain with nothing changed
/// (pixels stay correct; the "render runs only on change" promise
/// silently degrades to "every source change"). Use the bit pattern
/// instead: `|m| m.progress.to_bits()` (`f32::to_bits`/`f64::to_bits`,
/// or a newtype whose `PartialEq` compares bits). More generally the
/// fingerprint type should be totally equal (reflexive: `a == a`
/// always) — integer, string and tuple fingerprints all are.
pub struct SyncSpec<T, Fp = u64> {
    key: Box<dyn Fn(&T) -> String>,
    fingerprint: Box<dyn Fn(&T) -> Fp>,
    visible: Option<VisibleFn<T>>,
    render: Box<dyn Fn(&T) -> FeedItem>,
}

/// The optional visibility predicate's slot shape (clippy-visible name).
type VisibleFn<T> = Box<dyn Fn(&T) -> bool>;

impl<T, Fp> SyncSpec<T, Fp> {
    /// Identity + change detection + rendering — the required three.
    /// Keys must be unique per visible snapshot (they are feed-item
    /// identities: a duplicate key REPLACES, exactly like
    /// [`FeedState::push`]).
    pub fn new(
        key: impl Fn(&T) -> String + 'static,
        fingerprint: impl Fn(&T) -> Fp + 'static,
        render: impl Fn(&T) -> FeedItem + 'static,
    ) -> SyncSpec<T, Fp> {
        SyncSpec {
            key: Box::new(key),
            fingerprint: Box::new(fingerprint),
            visible: None,
            render: Box::new(render),
        }
    }

    /// Optional visibility filter: hidden items never reach the feed.
    /// This closure is the ONE truth for visibility — the old
    /// "mirror predicate must stay byte-exact with the renderer"
    /// consumer obligation dissolves into it. A flip on the tail item
    /// appends/rebuilds honestly; a flip before the tail rebuilds.
    pub fn visible(mut self, f: impl Fn(&T) -> bool + 'static) -> SyncSpec<T, Fp> {
        self.visible = Some(Box::new(f));
        self
    }
}

impl FeedState {
    /// Mirror a `Signal<Vec<T>>` into this feed, diffing by key (the
    /// fold-shaped consumer's bridge — backlog 0104). Runs immediately
    /// and on every source change, inside a labeled effect owned by
    /// `cx`; returns the [`Effect`] (dispose it to stop syncing —
    /// otherwise it dies with the scope).
    ///
    /// Diff semantics, in order:
    /// - source keys that keep the previous VISIBLE order as a prefix:
    ///   changed fingerprints -> [`FeedState::update`] in place, new
    ///   tail keys -> [`FeedState::push`] (the O(1) hot path — an
    ///   append-only fold never rebuilds, test-pinned);
    /// - anything else (shrink, removal, reorder, mid-list insert or
    ///   visibility flip) -> the REBUILD path: [`FeedState::clear`] +
    ///   re-push every visible item, because feed order is push order
    ///   and the feed is append-only.
    ///
    /// REBUILD COST, named (cycle-2 review C-3): a rebuild re-renders
    /// and re-typesets EVERY visible item, so a source that reorders
    /// on every change (a most-recent-first sort, a live-resorted
    /// leaderboard) pays O(visible) renders per drain, forever. For
    /// feeds ordered by mutable rank, sync a STABLE order and sort at
    /// render time — or accept O(visible) per change knowingly.
    ///
    /// Contracts: the synced feed has ONE writer — this bridge.
    /// Manual `push`/`update`/`stream_*`/`clear` on a synced feed is a
    /// contract violation the bridge DETECTS AND SELF-HEALS (cycle-2
    /// review C-1): every item mutation bumps a feed-internal counter;
    /// a drain that finds the counter moved past its own record takes
    /// the rebuild path, restoring the feed to exactly the source's
    /// visible order — stray items are evicted, order-divergence
    /// (a manually-pushed key the source appends LATER would land as
    /// a replace-in-place at the old index) is repaired. The heal is
    /// a safety net, not a feature: the foreign content stays on
    /// screen until the NEXT source change arrives, and that drain
    /// pays a full O(visible) rebuild. The
    /// `render`/`key`/`fingerprint`/`visible` closures receive `&T`
    /// borrowed from inside the source signal's cell — they must not
    /// read the SOURCE signal reactively (same rule as
    /// [`Signal::update`]); reading other signals is fine but adds
    /// them to the sync effect's dependencies.
    pub fn sync<T: 'static, Fp: PartialEq + 'static>(
        &self,
        cx: Scope,
        items: Signal<Vec<T>>,
        spec: SyncSpec<T, Fp>,
    ) -> Effect {
        let feed = self.clone();
        // The mirror bookkeeping: the visible (key, fingerprint)
        // sequence the feed currently shows, in push order.
        let mut shown: Vec<(String, Fp)> = Vec::new();
        // The one-writer detector (C-1): the feed's mutation count as
        // this bridge last left it. `None` until the first drain — the
        // contract begins when the bridge attaches, so pre-attach
        // pushes are not its business (the first drain appends after
        // them exactly as before).
        let mut synced_mutations: Option<u64> = None;
        cx.effect_labeled("feed.sync", move || {
            items.with(|list| {
                let SyncSpec {
                    key,
                    fingerprint,
                    visible,
                    render,
                } = &spec;
                let is_visible = |item: &T| visible.as_ref().is_none_or(|f| f(item));

                // Self-heal check (C-1): the counter moved past this
                // bridge's own record — someone else wrote to the feed
                // between drains. The `shown` bookkeeping no longer
                // describes the feed, so the only honest move is the
                // rebuild path. One u64 compare on every drain.
                let foreign =
                    synced_mutations.is_some_and(|recorded| recorded != feed.mutation_count());

                // Pass 1 — order check against the shown prefix. Keys
                // are the only per-item probe here; fingerprints are
                // compared in pass 2 only when the order holds.
                let mut vis_count = 0usize;
                let mut prefix_holds = true;
                for item in list.iter().filter(|i| is_visible(i)) {
                    if vis_count < shown.len() && key(item) != shown[vis_count].0 {
                        prefix_holds = false;
                        break;
                    }
                    vis_count += 1;
                }
                let fast = !foreign && prefix_holds && vis_count >= shown.len();

                if fast {
                    // Fast path: in-place updates + tail appends.
                    for (at, item) in list.iter().filter(|i| is_visible(i)).enumerate() {
                        if at < shown.len() {
                            let fp = fingerprint(item);
                            if fp != shown[at].1 {
                                feed.update(&shown[at].0, render(item));
                                shown[at].1 = fp;
                            }
                        } else {
                            let k = key(item);
                            feed.push(k.clone(), render(item));
                            shown.push((k, fingerprint(item)));
                        }
                    }
                } else {
                    // Rebuild path: push order broke, the list shrank,
                    // or a foreign write desynced the mirror — the
                    // append-only feed rebuilds whole (strays evicted,
                    // order restored to source order).
                    feed.clear();
                    shown.clear();
                    for item in list.iter().filter(|i| is_visible(i)) {
                        let k = key(item);
                        feed.push(k.clone(), render(item));
                        shown.push((k, fingerprint(item)));
                    }
                }
                // Record the counter AFTER this drain's own writes:
                // any bump past this value before the next drain is a
                // foreign mutation by construction.
                synced_mutations = Some(feed.mutation_count());
            });
        })
    }
}

#[cfg(test)]
#[path = "feed_sync_tests.rs"]
mod tests;
