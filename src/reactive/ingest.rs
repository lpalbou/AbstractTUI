//! Bounded, coalescing event ingestion (live-data 0020): the DATA lane
//! for producers that can flood.
//!
//! [`WakeHandle::post`](super::WakeHandle::post) is the unbounded
//! control lane — correct for low-rate messages, unbounded by contract.
//! A flooding backend (tool-result chunks, a bursty hub) needs three
//! things the raw lane deliberately lacks, and this helper adds exactly
//! those:
//!
//! - **A capacity**: memory is bounded by `capacity` per stage (one
//!   producer-side transit buffer + the retained `Signal<Vec<T>>`
//!   window — ≤ 2×capacity values total, ever).
//! - **An explicit overflow policy** ([`OverflowPolicy`]): what happens
//!   at the bound is the APP's stated choice, never an accident.
//! - **Labeled honesty** ([`IngestStats`] as a signal): every dropped or
//!   coalesced value is COUNTED and renderable ("1.2k shown · 34
//!   dropped"). Silent loss is the failure mode this exists to prevent
//!   — surface nonzero drops to the user, the labeled-degradation
//!   convention.
//!
//! Wake cost: one posted drain closure and one waker invocation per
//! drain cycle no matter how many values arrive (helper-level dedup via
//! a scheduled flag, engine-level dedup in `RemoteShared::notify`).
//!
//! ## Why there is NO `Block` policy
//!
//! Blocking the producer until the UI catches up is deliberately not
//! offered. The consumer here is the UI thread draining at frame
//! cadence: a producer parked on it inherits every UI stall — a user
//! holding a scrollbar, a suspended terminal (Ctrl+Z), a modal — as
//! unbounded latency on the producer's OWN resources (sockets, locks,
//! wakeup storms on resume), a priority inversion with a deadlock
//! surface (a blocked producer can never answer the cancellation the UI
//! is about to send it). Producers that must not lose data apply flow
//! control UPSTREAM (stop reading the socket; the transport pushes
//! back) — pausing the READ is safe, pausing against the UI is not.
//! Choose a capacity that covers honest bursts and let the counter
//! testify when it was not enough.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::scheduler::{wake_handle, WakeHandle};
use super::scope::Scope;
use super::signal::Signal;

/// A [`OverflowPolicy::Coalesce`] fold: `fold(&mut newest, incoming)`.
/// `Send + Sync` because transit overflow folds on the producer thread.
pub type CoalesceFn<T> = Arc<dyn Fn(&mut T, T) + Send + Sync>;

/// What happens when a bounded buffer is full and one more value
/// arrives. Chosen at construction; applied identically on the transit
/// buffer (producer side) and the retained window (UI side).
pub enum OverflowPolicy<T> {
    /// Ring semantics: evict the OLDEST value, admit the new one. The
    /// feed/log shape (the newest tail is the truth); evictions count
    /// as `dropped`.
    DropOldest,
    /// Keep the head: refuse the NEW value once full. The "first N
    /// events" capture shape; refusals count as `dropped`.
    DropNewest,
    /// Merge the new value into the NEWEST buffered one with the fold
    /// function (`fold(&mut newest, incoming)`). The progress/presence
    /// shape — superseded values merge instead of vanishing; merges
    /// count as `coalesced`, never `dropped`. The fold runs on the
    /// PRODUCER thread for transit overflow (keep it cheap and
    /// panic-free) and on the UI thread for window overflow.
    ///
    /// **A panicking fold degrades, labeled — it never poisons.** The
    /// fold owns the incoming value, so a panic destroys that value
    /// mid-merge; the panic is caught at the call site (the buffer
    /// mutex stays healthy, later sends keep working), the lost value
    /// counts as `dropped`, the event counts as
    /// [`IngestStats::fold_panics`], and the merge target keeps
    /// whatever state the fold left it in (it was half-merged by USER
    /// code; synthesizing a clean state would be dishonest). True
    /// "retry as DropOldest" is impossible without `T: Clone` — the
    /// value is already gone. Render `fold_panics` like `dropped`:
    /// nonzero means your fold has a bug.
    Coalesce(CoalesceFn<T>),
}

impl<T> OverflowPolicy<T> {
    /// Sugar for [`OverflowPolicy::Coalesce`] without the `Arc` noise.
    pub fn coalesce(fold: impl Fn(&mut T, T) + Send + Sync + 'static) -> OverflowPolicy<T> {
        OverflowPolicy::Coalesce(Arc::new(fold))
    }
}

impl<T> Clone for OverflowPolicy<T> {
    fn clone(&self) -> Self {
        match self {
            OverflowPolicy::DropOldest => OverflowPolicy::DropOldest,
            OverflowPolicy::DropNewest => OverflowPolicy::DropNewest,
            OverflowPolicy::Coalesce(f) => OverflowPolicy::Coalesce(f.clone()),
        }
    }
}

impl<T> std::fmt::Debug for OverflowPolicy<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OverflowPolicy::DropOldest => "DropOldest",
            OverflowPolicy::DropNewest => "DropNewest",
            OverflowPolicy::Coalesce(_) => "Coalesce(..)",
        })
    }
}

/// Cumulative ingestion accounting, surfaced as `Signal<IngestStats>`
/// and updated on the UI thread, atomically with the window, at every
/// drain. Invariant (while the scope lives): every sent value lands in
/// exactly one VALUE bucket — `delivered + dropped + coalesced` =
/// values sent (`fold_panics` counts EVENTS, not values: a panicked
/// fold's value is already inside `dropped`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct IngestStats {
    /// Values ADMITTED to the retained window — they became readable by
    /// the UI. A `DropOldest` window later aging one out (newer values
    /// pushed it off the ring) does NOT retro-count it as dropped:
    /// scrolling away after being shown is retention churn, not loss.
    pub delivered: u64,
    /// Values that never became observable: transit overflow, window
    /// refusals (`DropNewest`), batch overflow beyond the window
    /// (`DropOldest`). Render this number when nonzero; silent dropping
    /// is not acceptable.
    pub dropped: u64,
    /// Values merged into a survivor by [`OverflowPolicy::Coalesce`] —
    /// represented, not lost.
    pub coalesced: u64,
    /// Times a `Coalesce` fold PANICKED (labeled degradation, cycle-2
    /// hardening): the panic was caught (no mutex poison, later sends
    /// unaffected), the incoming value was lost mid-merge (counted in
    /// `dropped`), the merge target keeps the fold's partial state.
    /// Nonzero = the app's fold function has a bug; render it.
    pub fold_panics: u64,
}

struct BoundedShared<T> {
    wake: WakeHandle,
    queue: Mutex<VecDeque<T>>,
    capacity: usize,
    policy: OverflowPolicy<T>,
    /// One drain closure in flight per cycle (helper-level wake dedup).
    drain_scheduled: AtomicBool,
    /// Transit-stage counters (producer side); folded into the stats
    /// signal by the next drain.
    dropped: AtomicU64,
    coalesced: AtomicU64,
    fold_panics: AtomicU64,
    /// Values that reached a drain after the scope died (inert).
    dead_sends: AtomicU64,
    events: Signal<Vec<T>>,
    stats: Signal<IngestStats>,
}

/// `Clone + Send` producer handle for the bounded lane. Never blocks,
/// never fails; overflow follows the constructed [`OverflowPolicy`] and
/// is counted. Inert after the owning scope dies (memory stays bounded
/// by the transit capacity; [`BoundedSender::dead_sends`] counts).
pub struct BoundedSender<T> {
    shared: Arc<BoundedShared<T>>,
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        BoundedSender {
            shared: self.shared.clone(),
        }
    }
}

/// Run one coalesce fold with the panic firewall: a panicking USER fold
/// must not unwind through our mutex guard (poison would kill every
/// later send with an opaque lock panic — the exact failure this
/// degrades away from). On panic the incoming value is gone (the fold
/// owned it), `target` keeps the fold's partial state, and the caller
/// counts one dropped value + one fold_panics event. `AssertUnwindSafe`
/// is sound here: `target` is user data we hand back to user code
/// either way, and OUR invariants (deque structure, counters) are
/// maintained entirely outside the closure.
fn run_fold<T>(fold: &CoalesceFn<T>, target: &mut T, value: T) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fold(target, value))).is_ok()
}

/// Push `value` onto the TRANSIT deque under `policy`/`capacity`; count
/// into `dropped`/`coalesced`/`fold_panics`. A transit eviction is a
/// real loss (the evicted value was never observable by the UI), so
/// every overflow here is counted.
fn admit_transit<T>(
    buf: &mut VecDeque<T>,
    value: T,
    capacity: usize,
    policy: &OverflowPolicy<T>,
    dropped: &mut u64,
    coalesced: &mut u64,
    fold_panics: &mut u64,
) {
    if buf.len() < capacity {
        buf.push_back(value);
        return;
    }
    match policy {
        OverflowPolicy::DropOldest => {
            buf.pop_front();
            buf.push_back(value);
            *dropped += 1;
        }
        OverflowPolicy::DropNewest => {
            *dropped += 1;
        }
        OverflowPolicy::Coalesce(fold) => {
            let newest = buf.back_mut().expect("capacity >= 1: full is non-empty");
            if run_fold(fold, newest, value) {
                *coalesced += 1;
            } else {
                *dropped += 1; // the value died inside the fold
                *fold_panics += 1;
            }
        }
    }
}

impl<T: Send + 'static> BoundedSender<T> {
    /// Deliver one value. Applies the overflow policy to the transit
    /// buffer immediately (bounded memory even if the UI never drains)
    /// and schedules at most ONE drain closure per cycle.
    pub fn send(&self, value: T) {
        let shared = &self.shared;
        {
            let mut queue = shared.queue.lock().expect("bounded-source queue");
            let (mut d, mut c, mut p) = (0u64, 0u64, 0u64);
            admit_transit(
                &mut queue,
                value,
                shared.capacity,
                &shared.policy,
                &mut d,
                &mut c,
                &mut p,
            );
            if d > 0 {
                shared.dropped.fetch_add(d, Ordering::Relaxed);
            }
            if c > 0 {
                shared.coalesced.fetch_add(c, Ordering::Relaxed);
            }
            if p > 0 {
                shared.fold_panics.fetch_add(p, Ordering::Relaxed);
            }
        } // lock released BEFORE posting: the drain also takes it
        if !shared.drain_scheduled.swap(true, Ordering::AcqRel) {
            let shared = shared.clone();
            self.shared.wake.post(move || drain(&shared));
        }
    }

    /// Values that reached a drain after the owning scope died (inert
    /// sends — nothing applied, nothing counted in the stats signal,
    /// which died with the scope).
    pub fn dead_sends(&self) -> u64 {
        self.shared.dead_sends.load(Ordering::Relaxed)
    }
}

/// The UI-thread half of the bounded lane: fold the transit batch into
/// the retained window and publish stats — one signal write each, inside
/// one `batch` (one effect flush per drain).
fn drain<T: 'static>(shared: &Arc<BoundedShared<T>>) {
    // Clear BEFORE taking the batch (the drain_posted discipline): a
    // producer pushing after the take schedules a fresh drain; one
    // pushing before it is included here.
    shared.drain_scheduled.store(false, Ordering::Release);
    let batch: VecDeque<T> = {
        let mut queue = shared.queue.lock().expect("bounded-source queue");
        std::mem::take(&mut *queue)
    }; // lock released before ANY signal write (effects may send again)
    if batch.is_empty() {
        return; // racing schedule delivered an empty drain: harmless
    }
    if !shared.events.is_alive() {
        shared
            .dead_sends
            .fetch_add(batch.len() as u64, Ordering::Relaxed);
        return;
    }
    let (mut delivered, mut dropped, mut coalesced, mut fold_panics) = (0u64, 0u64, 0u64, 0u64);
    super::runtime::batch(|| {
        shared.events.update(|vec| {
            let cap = shared.capacity;
            let incoming = batch.len();
            match &shared.policy {
                OverflowPolicy::DropOldest => {
                    // Ring: final window = the last `cap` of (window +
                    // batch). Batch items beyond `cap` can never fit —
                    // never observable, counted dropped. Pre-existing
                    // items aging off the front WERE shown: retention
                    // churn, deliberately uncounted (see IngestStats).
                    let unfittable = incoming.saturating_sub(cap);
                    dropped += unfittable as u64;
                    delivered += (incoming - unfittable) as u64;
                    vec.extend(batch.into_iter().skip(unfittable));
                    let aged = vec.len().saturating_sub(cap);
                    vec.drain(..aged);
                }
                OverflowPolicy::DropNewest => {
                    // Keep the head: admit into remaining room, refuse
                    // the rest (never observable, counted dropped).
                    let room = cap.saturating_sub(vec.len());
                    let admitted = room.min(incoming);
                    delivered += admitted as u64;
                    dropped += (incoming - admitted) as u64;
                    vec.extend(batch.into_iter().take(admitted));
                }
                OverflowPolicy::Coalesce(fold) => {
                    for value in batch {
                        if vec.len() < cap {
                            vec.push(value);
                            delivered += 1;
                        } else if run_fold(fold, vec.last_mut().expect("cap >= 1"), value) {
                            coalesced += 1;
                        } else {
                            dropped += 1; // the value died inside the fold
                            fold_panics += 1;
                        }
                    }
                }
            }
        });
        // Fold transit-stage counters (producer side) into the same
        // publication so the UI reads ONE consistent stats value.
        dropped += shared.dropped.swap(0, Ordering::Relaxed);
        coalesced += shared.coalesced.swap(0, Ordering::Relaxed);
        fold_panics += shared.fold_panics.swap(0, Ordering::Relaxed);
        if shared.stats.is_alive() {
            shared.stats.update(|s| {
                s.delivered += delivered;
                s.dropped += dropped;
                s.coalesced += coalesced;
                s.fold_panics += fold_panics;
            });
        }
    });
}

/// Bounded ingestion binding: like
/// [`channel_source`](super::source::channel_source) but with a
/// capacity, an explicit [`OverflowPolicy`] and honest accounting.
///
/// Returns `(sender, events, stats)`:
/// - `sender` — `Clone + Send`, hand to producer threads;
/// - `events: Signal<Vec<T>>` — the retained window, never more than
///   `capacity` items (the window IS the retention: older history is
///   the app's business, and with `DropOldest` its eviction is what the
///   counter reports);
/// - `stats: Signal<IngestStats>` — cumulative delivered / dropped /
///   coalesced, updated atomically with the window (render `dropped`
///   when nonzero — labeled degradation, never silent).
///
/// Both signals are owned by `cx` and die with it; the sender then
/// turns inert (bounded transit memory, counted sends, never UB).
///
/// # Panics
///
/// `capacity == 0` panics loudly: a zero-capacity window can admit
/// nothing and every send would be a silent drop — FIX: size the window
/// for the burst you accept, and pick the policy that says what
/// overflow means.
pub fn bounded_source<T: Send + 'static>(
    cx: Scope,
    capacity: usize,
    policy: OverflowPolicy<T>,
) -> (BoundedSender<T>, Signal<Vec<T>>, Signal<IngestStats>) {
    assert!(
        capacity >= 1,
        "abstracttui reactive: bounded_source capacity must be >= 1 — a zero-capacity \
         window admits nothing. FIX: size capacity for the burst you accept (it is also \
         the retained-window length) and choose the OverflowPolicy that names what \
         overflow should mean"
    );
    let events = cx.signal(Vec::new());
    let stats = cx.signal(IngestStats::default());
    let sender = BoundedSender {
        shared: Arc::new(BoundedShared {
            wake: wake_handle(),
            queue: Mutex::new(VecDeque::with_capacity(capacity.min(4096))),
            capacity,
            policy,
            drain_scheduled: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            coalesced: AtomicU64::new(0),
            fold_panics: AtomicU64::new(0),
            dead_sends: AtomicU64::new(0),
            events,
            stats,
        }),
    };
    (sender, events, stats)
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
