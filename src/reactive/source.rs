//! Async data-source → Signal binding (live-data 0010): the NAMED shape
//! for "a background thread produces values, the UI reads a signal".
//!
//! ## The ownership rule (one sentence)
//!
//! The reactive graph is single-threaded: background threads never touch
//! signals — the only sanctioned crossing is a closure posted through
//! [`WakeHandle`], and these helpers are that crossing with a name.
//! (Enforcement: a wrong-thread signal access is a named panic,
//! `runtime::MSG_WRONG_THREAD` — never silent aliasing.)
//!
//! ## Two flavors
//!
//! - [`channel_source`] — append-buffer binding (the chat/feed shape):
//!   EVERY sent value lands, in per-sender order, into a
//!   `Signal<Vec<T>>`.
//! - [`latest_source`] — latest-value binding (progress/telemetry
//!   shape): between two UI drains only the NEWEST value applies;
//!   intermediates coalesce at the source by design.
//!
//! Both are UNBOUNDED control-lane helpers built on
//! [`WakeHandle::post`]; a producer that can flood should use the
//! bounded lane ([`super::ingest::bounded_source`]) which adds capacity,
//! an explicit overflow policy and a labeled drop counter.
//!
//! ## Guarantees (inherited from the posted-jobs queue)
//!
//! - **Ordered delivery**: one sender's values apply in send order
//!   (FIFO queue; cross-sender order is lock-acquisition order).
//! - **Frame semantics**: a burst of sends coalesces into one wake and
//!   one frame; a send landing mid-frame applies next frame, exactly
//!   once (damage contract §2).
//! - **Disposal safety**: a sender outliving the owning scope is INERT
//!   — sends after disposal apply nothing (the stale-handle discipline:
//!   the generational arena answers "gone", never UB) and are counted
//!   on the sender ([`SourceSender::dead_sends`]).
//! - **Zero idle cost**: a quiet sender costs nothing — no timers, no
//!   polling; the loop stays parked in its blocking read.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::scheduler::{wake_handle, WakeHandle};
use super::scope::Scope;
use super::signal::Signal;

/// How a [`SourceSender`] delivers into its signal.
enum Mode<T> {
    /// Every value posts one apply-closure: push onto `Signal<Vec<T>>`.
    Channel { target: Signal<Vec<T>> },
    /// Newest value wins: a slot the producer overwrites plus ONE
    /// pending apply-closure per drain cycle (`scheduled` dedups).
    Latest {
        slot: Mutex<Option<T>>,
        scheduled: AtomicBool,
        target: Signal<T>,
    },
}

struct SourceShared<T> {
    wake: WakeHandle,
    mode: Mode<T>,
    /// Values that reached the UI thread and found the signal disposed
    /// (inert sends). For `latest_source` each coalesced batch counts
    /// once — its intermediates were superseded while alive too.
    dead_sends: AtomicU64,
}

/// `Clone + Send` producer handle: hand it to a thread (or several —
/// clones share the same target) and call [`SourceSender::send`]. Never
/// blocks, never fails; after the owning scope dies it turns inert (see
/// module docs). Values must be `Send` — they cross to the UI thread.
pub struct SourceSender<T> {
    shared: Arc<SourceShared<T>>,
}

// Manual impl: `derive(Clone)` would demand `T: Clone` needlessly.
impl<T> Clone for SourceSender<T> {
    fn clone(&self) -> Self {
        SourceSender {
            shared: self.shared.clone(),
        }
    }
}

impl<T: Send + 'static> SourceSender<T> {
    /// Deliver one value to the UI thread. Fire-and-forget by contract:
    /// ordered (per sender), applied in the next phase U, inert after
    /// the owning scope's disposal.
    pub fn send(&self, value: T) {
        match &self.shared.mode {
            Mode::Channel { target } => {
                let target = *target;
                let shared = self.shared.clone();
                self.shared.wake.post(move || {
                    if target.is_alive() {
                        target.update(|vec| vec.push(value));
                    } else {
                        shared.dead_sends.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
            Mode::Latest {
                slot,
                scheduled,
                target,
            } => {
                *slot.lock().expect("latest-source slot") = Some(value);
                // One apply per drain cycle: first writer schedules, the
                // rest only overwrite the slot (coalescing at source).
                if !scheduled.swap(true, Ordering::AcqRel) {
                    let target = *target;
                    let shared = self.shared.clone();
                    self.shared.wake.post(move || {
                        let Mode::Latest {
                            slot, scheduled, ..
                        } = &shared.mode
                        else {
                            unreachable!("latest sender carries latest mode");
                        };
                        // Clear BEFORE take: a producer writing after the
                        // take schedules a fresh apply; one writing before
                        // it is folded into this take. A racing schedule
                        // may deliver an empty (None) apply — harmless.
                        scheduled.store(false, Ordering::Release);
                        let value = slot.lock().expect("latest-source slot").take();
                        if let Some(value) = value {
                            if target.is_alive() {
                                target.set(value);
                            } else {
                                shared.dead_sends.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                }
            }
        }
    }

    /// How many values arrived on the UI thread AFTER the owning scope
    /// died (inert sends — nothing applied). Producer-side observability
    /// for the disposal contract; a nonzero count usually means the
    /// producer missed its stop signal.
    pub fn dead_sends(&self) -> u64 {
        self.shared.dead_sends.load(Ordering::Relaxed)
    }
}

/// Append-buffer binding (the chat/feed shape): every value a worker
/// [`send`s](SourceSender::send) is pushed — on the UI thread, in send
/// order — onto the returned `Signal<Vec<T>>`, which is owned by `cx`
/// and dies with it.
///
/// The buffer is UNBOUNDED on both lanes (posted queue + retained Vec):
/// this is the low-rate control shape. Flooding producers belong on
/// [`super::ingest::bounded_source`]. Retention is the app's business —
/// a UI that shows the tail should render a slice, or use the bounded
/// helper whose capacity IS the retained window.
///
/// ```
/// use abstracttui::reactive::{channel_source, create_root, drain_posted};
///
/// let (root, ()) = create_root(|cx| {
///     let (tx, events) = channel_source::<u32>(cx);
///     let t = std::thread::spawn(move || {
///         for n in 0..5 {
///             tx.send(n);
///         }
///     });
///     t.join().expect("producer");
///     drain_posted(); // the app loop's phase U does this every turn
///     assert_eq!(events.get_untracked(), vec![0, 1, 2, 3, 4]);
/// });
/// root.dispose();
/// ```
pub fn channel_source<T: Send + 'static>(cx: Scope) -> (SourceSender<T>, Signal<Vec<T>>) {
    let target = cx.signal(Vec::new());
    let sender = SourceSender {
        shared: Arc::new(SourceShared {
            wake: wake_handle(),
            mode: Mode::Channel { target },
            dead_sends: AtomicU64::new(0),
        }),
    };
    (sender, target)
}

/// Latest-value binding (progress/telemetry shape): the returned
/// `Signal<T>` follows the NEWEST sent value; values superseded between
/// two UI drains never apply (coalesced at the source — that is the
/// contract, not a loss). One posted closure per drain cycle regardless
/// of send rate. The signal starts at `initial` and is owned by `cx`.
///
/// ```
/// use abstracttui::reactive::{create_root, drain_posted, latest_source};
///
/// let (root, ()) = create_root(|cx| {
///     let (tx, progress) = latest_source(cx, 0u8);
///     let t = std::thread::spawn(move || {
///         for pct in [10u8, 40, 90, 100] {
///             tx.send(pct); // burst: intermediates coalesce
///         }
///     });
///     t.join().expect("producer");
///     drain_posted();
///     assert_eq!(progress.get_untracked(), 100);
/// });
/// root.dispose();
/// ```
pub fn latest_source<T: Send + 'static>(cx: Scope, initial: T) -> (SourceSender<T>, Signal<T>) {
    let target = cx.signal(initial);
    let sender = SourceSender {
        shared: Arc::new(SourceShared {
            wake: wake_handle(),
            mode: Mode::Latest {
                slot: Mutex::new(None),
                scheduled: AtomicBool::new(false),
                target,
            },
            dead_sends: AtomicU64::new(0),
        }),
    };
    (sender, target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{create_root, drain_posted};

    #[test]
    fn channel_delivers_every_value_in_send_order() {
        let (root, ()) = create_root(|cx| {
            let (tx, events) = channel_source::<u32>(cx);
            let t = std::thread::spawn(move || {
                for n in 0..100 {
                    tx.send(n);
                }
            });
            t.join().expect("producer");
            drain_posted();
            let got = events.get_untracked();
            assert_eq!(got.len(), 100);
            assert!(got.windows(2).all(|w| w[0] < w[1]), "order preserved");
        });
        root.dispose();
    }

    #[test]
    fn channel_preserves_per_sender_order_across_concurrent_senders() {
        let (root, ()) = create_root(|cx| {
            let (tx, events) = channel_source::<(u8, u32)>(cx);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let mk = |id: u8, tx: SourceSender<(u8, u32)>, b: Arc<std::sync::Barrier>| {
                std::thread::spawn(move || {
                    b.wait();
                    for n in 0..500 {
                        tx.send((id, n));
                    }
                })
            };
            let a = mk(0, tx.clone(), barrier.clone());
            let b = mk(1, tx, barrier);
            a.join().expect("sender a");
            b.join().expect("sender b");
            drain_posted();
            events.with_untracked(|got| {
                assert_eq!(got.len(), 1000);
                let mut next = [0u32; 2];
                for &(id, n) in got.iter() {
                    assert_eq!(n, next[id as usize], "sender {id} out of order");
                    next[id as usize] += 1;
                }
            });
        });
        root.dispose();
    }

    #[test]
    fn latest_coalesces_bursts_to_the_newest_value() {
        let (root, ()) = create_root(|cx| {
            let (tx, latest) = latest_source(cx, 0u32);
            let t = std::thread::spawn(move || {
                for n in 1..=1000 {
                    tx.send(n);
                }
            });
            t.join().expect("producer");
            drain_posted();
            assert_eq!(latest.get_untracked(), 1000, "newest value wins");
            // Exactly one pending apply existed for the whole burst: a
            // second drain has nothing left to run.
            assert_eq!(drain_posted(), 0, "burst coalesced into one job");
        });
        root.dispose();
    }

    #[test]
    fn latest_reschedules_after_each_drain() {
        let (root, ()) = create_root(|cx| {
            let (tx, latest) = latest_source(cx, 0u32);
            tx.send(1);
            drain_posted();
            assert_eq!(latest.get_untracked(), 1);
            tx.send(2); // new cycle: must schedule a fresh apply
            drain_posted();
            assert_eq!(latest.get_untracked(), 2);
        });
        root.dispose();
    }

    #[test]
    fn sends_after_scope_disposal_are_inert_and_counted() {
        let mut handles = None;
        let (root, ()) = create_root(|cx| {
            let child = cx.child();
            let (tx, events) = channel_source::<u32>(child);
            tx.send(1);
            drain_posted();
            assert_eq!(events.get_untracked(), vec![1]);
            child.dispose();
            handles = Some((tx, events));
        });
        let (tx, events) = handles.expect("handles");
        // The signal is gone; the sender must stay safe and honest.
        tx.send(2);
        tx.send(3);
        drain_posted();
        assert!(!events.is_alive());
        assert_eq!(events.try_get_untracked(), None);
        assert_eq!(tx.dead_sends(), 2, "inert sends are counted");
        root.dispose();
    }

    #[test]
    fn latest_sends_after_disposal_are_inert_and_counted_once_per_cycle() {
        let mut handles = None;
        let (root, ()) = create_root(|cx| {
            let child = cx.child();
            let (tx, latest) = latest_source(child, 0u32);
            child.dispose();
            handles = Some((tx, latest));
        });
        let (tx, latest) = handles.expect("handles");
        tx.send(7);
        tx.send(8); // coalesces with 7 — one survivor
        drain_posted();
        assert!(!latest.is_alive());
        assert_eq!(tx.dead_sends(), 1, "the coalesced survivor is counted");
        root.dispose();
    }
}
