//! Connection lifecycle + jittered reconnect (live-data 0040): the
//! NAMED shape for "a networked app survives drops" — a state machine
//! the UI renders honestly, and the retry clock the engine owns.
//!
//! ## What the engine does NOT do
//!
//! No network I/O, no sockets, no TLS, no threads: the app supplies the
//! DIAL function (0050 owns the transport decision; ureq-class blocking
//! HTTP in a worker thread is field-proven meanwhile). The engine owns
//! what every consumer was hand-rolling around that transport: the
//! state vocabulary, the jittered backoff schedule, the retry timer,
//! and cancellation — riding the existing timer machinery
//! ([`after`](super::after)-class one-shots: an armed retry costs zero
//! wakeups until due), dying with the owning scope like everything
//! else.
//!
//! ## The shape
//!
//! ```
//! use abstracttui::reactive::{connection, create_root, drain_posted, Backoff, ConnState};
//!
//! let (root, ()) = create_root(|cx| {
//!     let conn = connection(cx, Backoff::default(), move |events| {
//!         // Spawn your transport attempt (spawn_worker + your dial);
//!         // report through `events` from any thread. Here: in-process.
//!         events.connected();
//!     });
//!     assert_eq!(conn.state().get_untracked(), ConnState::Connecting);
//!     drain_posted(); // the app loop's phase U does this every turn
//!     assert_eq!(conn.state().get_untracked(), ConnState::Connected);
//!     conn.close();
//!     assert_eq!(conn.state().get_untracked(), ConnState::Closed);
//! });
//! root.dispose();
//! ```
//!
//! The UI renders `conn.state()` like any signal (badge, status line,
//! dimmed panes); while offline the loop stays parked — the only clock
//! is the one armed retry. A visible countdown is an ordinary
//! [`interval`](super::interval), billed as such; never a poll loop.
//!
//! ## Why FULL jitter
//!
//! A fleet of clients backing off `base × 2^n` with no jitter retries
//! in LOCKSTEP after a hub restart — every wave arrives together (the
//! thundering herd), and the first consumer's hand-roll (linear ×
//! consecutive_errors, capped, NO jitter) has exactly that failure
//! mode. Full jitter draws uniformly from `[0, min(cap, base × 2^n)]`,
//! decorrelating the fleet; the un-jittered ceiling still grows
//! exponentially, so pressure on a dead endpoint still decays.
//!
//! OWNER: FIXNET (live-data 0040).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::animate::{arm_timer_at, cancel_timer, timer_fire_now};
use super::scheduler::{request_frame, wake_handle, WakeHandle};
use super::scope::Scope;
use super::signal::Signal;

/// The connection lifecycle a UI renders. Deliberately EXHAUSTIVE:
/// these five states are the complete transport-agnostic vocabulary —
/// apps match on all of them and render each honestly. Growing this
/// enum would be a breaking change this crate refuses (0040's contract:
/// the state model must not absorb any one transport's semantics).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnState {
    /// A dial attempt is in flight (the initial one, or a retry's).
    Connecting,
    /// The transport is up and healthy.
    Connected,
    /// Up but impaired, with the app's reason ("catching up",
    /// "read-only replica", "stale > 30s") — rendered, never hidden.
    Degraded(String),
    /// Down; the engine holds one armed one-shot. `attempt` is the
    /// number of the UPCOMING dial (1 = first retry), `next_in` the
    /// jittered delay that was drawn for it — render "retry #2 in
    /// 1.4s" from these two fields.
    Reconnecting { attempt: u32, next_in: Duration },
    /// Terminal: closed by the app ([`Connection::close`]), by the
    /// transport ([`ConnectionEvents::closed`]), or by scope disposal.
    /// Nothing is armed, nothing will run again — zero idle cost.
    Closed,
}

/// Jittered exponential backoff (pure — no I/O, no clocks): FULL
/// jitter over a doubling ceiling, the AWS-shape retry schedule with
/// the agora client's parameters as defaults (base 500ms, ×2, cap 30s;
/// reset on success).
///
/// ```
/// use abstracttui::reactive::Backoff;
/// use std::time::Duration;
///
/// let mut b = Backoff::default().seeded(7); // seeded: deterministic tests
/// let first = b.next_delay();
/// assert!(first <= Duration::from_millis(500), "draw within [0, base]");
/// b.next_delay();
/// assert_eq!(b.attempt(), 2);
/// assert_eq!(b.ceiling(), Duration::from_secs(2), "500ms doubled twice");
/// b.reset();
/// assert_eq!(b.attempt(), 0);
/// ```
#[derive(Clone, Debug)]
pub struct Backoff {
    base: Duration,
    cap: Duration,
    attempt: u32,
    rng: u64,
}

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff::new(Duration::from_millis(500), Duration::from_secs(30))
    }
}

impl Backoff {
    /// A backoff schedule with ceiling `min(cap, base × 2^attempt)`.
    /// Seeded from wall-clock + address entropy so two processes
    /// starting together still decorrelate; use [`Backoff::seeded`]
    /// for deterministic tests.
    pub fn new(base: Duration, cap: Duration) -> Backoff {
        Backoff {
            base,
            cap,
            attempt: 0,
            rng: entropy_seed(),
        }
    }

    /// Replace the jitter seed (deterministic sequences for tests).
    pub fn seeded(mut self, seed: u64) -> Backoff {
        self.rng = seed | 1; // xorshift must not start at 0
        self
    }

    /// The un-jittered bound the NEXT [`Backoff::next_delay`] draws
    /// under: `min(cap, base × 2^attempt)`. Monotone in `attempt`,
    /// saturating at `cap` — the honest "how hard are we backing off"
    /// number.
    pub fn ceiling(&self) -> Duration {
        let mut c = self.base.min(self.cap);
        for _ in 0..self.attempt {
            if c >= self.cap {
                return self.cap;
            }
            c = c.saturating_mul(2).min(self.cap);
        }
        c
    }

    /// Draw the next delay — uniform in `[0, ceiling()]` (full jitter)
    /// — and advance the attempt counter. Named `next_delay`, not
    /// `next`: this is a schedule draw, not an `Iterator` (clippy's
    /// `should_implement_trait` would read a `next()` here as one).
    pub fn next_delay(&mut self) -> Duration {
        let bound = self.ceiling();
        self.attempt = self.attempt.saturating_add(1);
        self.draw(bound)
    }

    /// Retries consumed since the last reset.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Back to the base schedule — call on success (the connection
    /// machine does this on `connected()`).
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Uniform draw in `[0, bound]` from the crate's xorshift64 shape
    /// (the particles PRNG). Modulo bias is immaterial at jitter scale
    /// (bound ≪ 2^64).
    fn draw(&mut self, bound: Duration) -> Duration {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let nanos = bound.as_nanos().min(u64::MAX as u128) as u64;
        if nanos == 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.rng % (nanos + 1))
    }
}

/// Wall-clock + ASLR entropy: enough to decorrelate fleet members
/// (the whole point of jitter); never used where determinism matters
/// (tests seed explicitly).
fn entropy_seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() << 32) ^ u64::from(d.subsec_nanos()))
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let addr = &t as *const _ as u64;
    (t ^ addr.rotate_left(32)) | 1
}

/// UI-thread machine internals, reachable from posted applies through
/// a `Signal<Rc<RefCell<CoreUi>>>` (the handle is `Copy + Send`; the
/// value never leaves the UI thread — the source-lane crossing rule).
struct CoreUi {
    backoff: Backoff,
    /// The armed retry one-shot (cancel on close/disposal) — kept
    /// OUTSIDE the arena so the disposal cleanup can reach it even
    /// while sibling nodes are mid-teardown.
    timer: Rc<std::cell::Cell<Option<u64>>>,
    /// Taken OUT while user code runs (a dial that closes the
    /// connection re-entrantly must not hit a borrowed slot); put back
    /// only while the connection is still open.
    dial: Option<Box<dyn FnMut(ConnectionEvents)>>,
}

struct Shared {
    wake: WakeHandle,
    /// The live attempt generation. Reporters carry the generation
    /// they were minted with; events from older attempts are STALE —
    /// a slow worker from attempt N must not flip attempt N+1's state.
    gen: AtomicU64,
    closed: AtomicBool,
    /// Reports that arrived after close/disposal or from a stale
    /// attempt (inert — nothing applied). Producer-side observability,
    /// the `dead_sends` convention.
    stale_reports: AtomicU64,
}

/// `Clone + Send` reporter handed to each dial attempt: the transport
/// worker calls these from any thread; they apply on the UI thread in
/// the next phase U (posted-jobs lane — ordered, coalesced per frame,
/// inert after close/disposal or when a NEWER attempt superseded this
/// one).
#[derive(Clone)]
pub struct ConnectionEvents {
    shared: Arc<Shared>,
    /// The attempt this reporter belongs to (staleness check).
    gen: u64,
    state: Signal<ConnState>,
    core: Signal<Rc<RefCell<CoreUi>>>,
}

impl ConnectionEvents {
    /// The transport is up: → `Connected`, backoff resets.
    pub fn connected(&self) {
        self.post(Report::Connected);
    }

    /// Up but impaired: → `Degraded(reason)`. Accepted from
    /// `Connecting` too (a connection that came up impaired counts as
    /// up: backoff resets).
    pub fn degraded(&self, reason: impl Into<String>) {
        self.post(Report::Degraded(reason.into()));
    }

    /// The attempt (or the live connection) died: → `Reconnecting`
    /// with a jittered delay; the engine arms the retry.
    pub fn failed(&self, reason: impl Into<String>) {
        self.post(Report::Failed(reason.into()));
    }

    /// CLEAN close from the transport side (server said goodbye, EOF
    /// by design): → `Closed`, terminal — no retry. An unexpected drop
    /// that should reconnect is [`ConnectionEvents::failed`].
    pub fn closed(&self) {
        self.post(Report::Closed);
    }

    /// Worker stop condition: true once the connection closed (any
    /// generation) — poll it in your read loop so a worker never
    /// outlives the session.
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    /// True while THIS reporter's attempt is the live one — false once
    /// a failure was accepted and a newer attempt took over (a slow
    /// worker can use this to stop early instead of racing the retry).
    pub fn is_current(&self) -> bool {
        !self.is_closed() && self.shared.gen.load(Ordering::Acquire) == self.gen
    }

    /// Reports that applied NOTHING (after close/disposal, or from a
    /// superseded attempt). Nonzero usually means a worker missed its
    /// stop signal.
    pub fn stale_reports(&self) -> u64 {
        self.shared.stale_reports.load(Ordering::Relaxed)
    }

    fn post(&self, report: Report) {
        let shared = self.shared.clone();
        let gen = self.gen;
        let state = self.state;
        let core = self.core;
        self.shared.wake.post(move || {
            // Staleness is decided ON the UI thread (authoritative
            // order): scope death, terminal close, or a newer attempt
            // all make this report inert-and-counted.
            if !state.is_alive()
                || shared.closed.load(Ordering::Acquire)
                || shared.gen.load(Ordering::Acquire) != gen
            {
                shared.stale_reports.fetch_add(1, Ordering::Relaxed);
                return;
            }
            apply_report(state, core, &shared, report);
        });
    }
}

enum Report {
    Connected,
    Degraded(String),
    Failed(String),
    Closed,
}

/// Apply one accepted (live-generation) report — UI thread only.
fn apply_report(
    state: Signal<ConnState>,
    core: Signal<Rc<RefCell<CoreUi>>>,
    shared: &Arc<Shared>,
    report: Report,
) {
    let core_rc = core.get_untracked();
    match report {
        Report::Connected => {
            core_rc.borrow_mut().backoff.reset();
            state.set(ConnState::Connected);
        }
        Report::Degraded(reason) => {
            // Degraded-from-Connecting counts as "came up impaired":
            // the dial SUCCEEDED, so the schedule resets like a
            // connect. From Connected/Degraded it is a health update.
            core_rc.borrow_mut().backoff.reset();
            state.set(ConnState::Degraded(reason));
        }
        Report::Failed(_reason) => {
            // Accepting the failure supersedes this attempt: bump the
            // generation FIRST so any further reports from the same
            // (or an older) worker are stale by construction.
            shared.gen.fetch_add(1, Ordering::AcqRel);
            let (attempt, next_in) = {
                let mut c = core_rc.borrow_mut();
                let next_in = c.backoff.next_delay();
                (c.backoff.attempt(), next_in)
            };
            state.set(ConnState::Reconnecting { attempt, next_in });
            arm_retry(state, core, shared.clone(), &core_rc, next_in);
        }
        Report::Closed => close_now(state, &core_rc, shared),
    }
}

/// Arm the retry one-shot. Rides the runtime timer heap (zero wakeups
/// until due); the id is recorded for cancellation (close, disposal,
/// retry-now). Inside a timer pass the injected clock stays
/// authoritative (the interval precedent).
fn arm_retry(
    state: Signal<ConnState>,
    core: Signal<Rc<RefCell<CoreUi>>>,
    shared: Arc<Shared>,
    core_rc: &Rc<RefCell<CoreUi>>,
    next_in: Duration,
) {
    let now = timer_fire_now().unwrap_or_else(Instant::now);
    let timer_slot = core_rc.borrow().timer.clone();
    let id = arm_timer_at(now + next_in, move || {
        if shared.closed.load(Ordering::Acquire) || !state.is_alive() {
            return; // belt: cancellation already removed us normally
        }
        dial_now(state, core, &shared);
    });
    timer_slot.set(Some(id));
    // Wake a possibly-parked loop so its sleep bound includes the new
    // deadline (the `after` rule). One coalesced wake per state change
    // — which damaged the UI anyway.
    request_frame();
}

/// Start one dial attempt: → `Connecting`, mint the reporter for the
/// CURRENT generation, run the app's dial fn (taken out/put back — a
/// dial closing the connection re-entrantly must not hit a borrowed
/// slot, and a closed connection drops the closure instead of
/// restoring it).
fn dial_now(state: Signal<ConnState>, core: Signal<Rc<RefCell<CoreUi>>>, shared: &Arc<Shared>) {
    let core_rc = core.get_untracked();
    core_rc.borrow().timer.set(None);
    // Equality cut-off: the birth dial finds the signal already at
    // `Connecting` (its initial value) — no phantom notification.
    let _ = state.set_if_changed(ConnState::Connecting);
    let events = ConnectionEvents {
        shared: shared.clone(),
        gen: shared.gen.load(Ordering::Acquire),
        state,
        core,
    };
    let taken = core_rc.borrow_mut().dial.take();
    if let Some(mut dial) = taken {
        dial(events);
        if !shared.closed.load(Ordering::Acquire) {
            let mut c = core_rc.borrow_mut();
            if c.dial.is_none() {
                c.dial = Some(dial);
            }
        }
    }
}

/// Terminal close — UI thread. Cancels the armed retry, drops the dial
/// fn (its captures free now, not at scope death), writes `Closed`
/// while the signal still lives. Idempotent.
fn close_now(state: Signal<ConnState>, core_rc: &Rc<RefCell<CoreUi>>, shared: &Arc<Shared>) {
    if shared.closed.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Some(id) = core_rc.borrow().timer.take() {
        let _ = cancel_timer(id);
    }
    core_rc.borrow_mut().dial = None;
    if state.is_alive() {
        state.set(ConnState::Closed);
    }
}

/// UI-side handle to a [`connection`]. NOT `Send` — the machine lives
/// on the UI thread; workers get [`ConnectionEvents`].
#[derive(Clone)]
pub struct Connection {
    state: Signal<ConnState>,
    core: Signal<Rc<RefCell<CoreUi>>>,
    core_rc: Rc<RefCell<CoreUi>>,
    shared: Arc<Shared>,
}

impl Connection {
    /// The lifecycle signal — render it like any other. Owned by the
    /// creating scope; dies with it.
    pub fn state(&self) -> Signal<ConnState> {
        self.state
    }

    /// Close from the UI side (quit, "disconnect" button): terminal
    /// `Closed`, armed retry cancelled, dial fn dropped. Idempotent.
    pub fn close(&self) {
        close_now(self.state, &self.core_rc, &self.shared);
    }

    /// True once closed (by either side or by scope disposal).
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    /// Skip the wait: while `Reconnecting`, cancel the armed one-shot
    /// and dial immediately (the "retry now" button every reconnect UI
    /// grows). No-op in every other state — an in-flight attempt is
    /// not restarted, a closed connection stays closed.
    pub fn retry_now(&self) {
        if self.is_closed() || !self.state.is_alive() {
            return;
        }
        if !matches!(self.state.get_untracked(), ConnState::Reconnecting { .. }) {
            return;
        }
        if let Some(id) = self.core_rc.borrow().timer.take() {
            let _ = cancel_timer(id);
        }
        dial_now(self.state, self.core, &self.shared);
    }
}

/// Start a connection lifecycle owned by `cx`: state goes `Connecting`
/// and `dial` runs once immediately, then once per retry the engine
/// schedules (jittered per `backoff`). The dial fn runs ON THE UI
/// THREAD and must return quickly — spawn blocking transport work
/// (`spawn_worker`) and report through the given [`ConnectionEvents`]
/// from there. Scope disposal cancels the armed retry and closes the
/// connection (workers observe `is_closed`); reports arriving after
/// that are inert and counted.
pub fn connection(
    cx: Scope,
    backoff: Backoff,
    dial: impl FnMut(ConnectionEvents) + 'static,
) -> Connection {
    let state = cx.signal(ConnState::Connecting);
    let core_rc = Rc::new(RefCell::new(CoreUi {
        backoff,
        timer: Rc::new(std::cell::Cell::new(None)),
        dial: Some(Box::new(dial)),
    }));
    let core = cx.signal(core_rc.clone());
    let shared = Arc::new(Shared {
        wake: wake_handle(),
        gen: AtomicU64::new(1),
        closed: AtomicBool::new(false),
        stale_reports: AtomicU64::new(0),
    });
    // Disposal = close: the timer id and dial fn live OUTSIDE the
    // arena (plain Rc), so this cleanup never races sibling-node
    // teardown; the state write is guarded (the signal may already be
    // gone — `Closed` is then implied by everything being gone).
    {
        let core_rc = core_rc.clone();
        let shared = shared.clone();
        cx.on_cleanup(move || close_now(state, &core_rc, &shared));
    }
    dial_now(state, core, &shared);
    Connection {
        state,
        core,
        core_rc,
        shared,
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
