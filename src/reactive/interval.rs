//! Recurring time source (live-data 0070): [`interval`] beside
//! [`after`](super::after) — the engine-owned version of the
//! self-rescheduling `after` recursion every dashboard hand-rolls, with
//! the cancellation story that recursion never has.
//!
//! ## Contract
//!
//! - Fires `f` on the UI thread, in phase U, riding the existing timer
//!   heap: ONE pending one-shot per interval, re-armed after each fire.
//!   Timers do NOT frame-pace — between fires an armed interval costs
//!   zero wakeups (the loop sleeps until the deadline), and a fire that
//!   damages nothing renders nothing.
//! - **Drift policy: fixed-delay.** The next deadline is `fire time +
//!   period`, taken from the LOOP's clock (injected in tests). Periodic
//!   UI work wants steadiness, not punctuality debt: after a suspend or
//!   a stall of N periods the interval fires ONCE and resumes its
//!   cadence — missed ticks coalesce, there is no catch-up storm. Under
//!   load the period is therefore a MINIMUM, and long-run tick counts
//!   drift below wall-time/period; a job that must know real elapsed
//!   time reads its own clock inside `f`.
//! - **Cancellation**: [`IntervalHandle::cancel`] (idempotent, usable
//!   from inside `f`) removes the pending timer immediately — after it
//!   returns, `f` never runs again and the loop never wakes for it.
//!   The owning scope's disposal cancels the same way, so a dead pane's
//!   poller CANNOT keep ticking by accident (the 0070 leak). Dropping
//!   the handle does NOT cancel — the scope owns the interval's
//!   lifetime, the handle is just the early-off switch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::animate::{arm_timer_at, cancel_timer, timer_fire_now};
use super::scheduler::request_frame;
use super::scope::Scope;

struct IntervalState {
    cancelled: Cell<bool>,
    /// Id of the currently armed one-shot (None between cancel and
    /// nothing — an armed, live interval always holds Some).
    pending: Cell<Option<u64>>,
}

impl IntervalState {
    fn cancel(&self) {
        // Idempotent: the first call disarms, repeats are no-ops.
        self.cancelled.set(true);
        if let Some(id) = self.pending.take() {
            // False = the entry already fired this very pass (we are
            // being cancelled from inside a callback); the `cancelled`
            // flag above stops its re-arm.
            let _ = cancel_timer(id);
        }
    }
}

/// Cancellation handle for [`interval`]. Cheap to clone (share it with
/// the closures that decide when to stop). See the module docs for the
/// drop semantics: dropping does NOT cancel; scope disposal does.
#[derive(Clone)]
pub struct IntervalHandle {
    state: Rc<IntervalState>,
}

impl IntervalHandle {
    /// Stop the interval now: the pending timer is removed, `f` never
    /// runs again, the loop never wakes for it again. Idempotent; safe
    /// from inside the interval's own callback (no re-arm follows).
    pub fn cancel(&self) {
        self.state.cancel();
    }

    /// True once cancelled (explicitly or by scope disposal).
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.get()
    }
}

/// Run `f` on the UI thread every `period`, starting one period from
/// now. Owned by `cx`: scope disposal cancels; the returned
/// [`IntervalHandle`] cancels earlier. Fixed-delay drift policy, missed
/// ticks coalesce — see the module contract.
///
/// ```
/// use abstracttui::reactive::{create_root, interval, run_due_timers};
/// use std::time::{Duration, Instant};
/// use std::{cell::Cell, rc::Rc};
///
/// let (root, ()) = create_root(|cx| {
///     let fires = Rc::new(Cell::new(0u32));
///     let f2 = fires.clone();
///     let handle = interval(cx, Duration::from_millis(100), move || {
///         f2.set(f2.get() + 1);
///     });
///     let t0 = Instant::now();
///     // The app loop does this each turn; tests drive time by hand.
///     run_due_timers(t0 + Duration::from_millis(150)); // 1st fire
///     run_due_timers(t0 + Duration::from_millis(260)); // 2nd fire
///     assert_eq!(fires.get(), 2);
///     handle.cancel();
///     run_due_timers(t0 + Duration::from_secs(10)); // never again
///     assert_eq!(fires.get(), 2);
/// });
/// root.dispose();
/// ```
///
/// # Panics
///
/// A zero `period` panics loudly: it would re-arm a due timer every
/// turn and spin the loop — FIX: the smallest honest cadence for
/// terminal UI work is milliseconds; for per-frame work use the frame
/// lane (`reactive::animate` / frame tasks), which is paced by the
/// loop, not by a timer.
pub fn interval(cx: Scope, period: Duration, f: impl FnMut() + 'static) -> IntervalHandle {
    assert!(
        !period.is_zero(),
        "abstracttui reactive: interval period must be nonzero — a zero period re-arms a \
         due timer every turn and spins the loop. FIX: pick a real cadence (milliseconds \
         up), or use the frame-task lane (reactive::animate) for per-frame work"
    );
    let state = Rc::new(IntervalState {
        cancelled: Cell::new(false),
        pending: Cell::new(None),
    });
    // FnMut shared between successive one-shot arms (each arm consumes
    // an FnOnce; the RefCell lets every generation borrow the same f).
    let callback: Rc<RefCell<dyn FnMut()>> = Rc::new(RefCell::new(f));
    arm(state.clone(), callback, period, Instant::now() + period);
    // Scope disposal is the safety net: a pane's interval dies with the
    // pane, cleanups run on the UI thread (LIFO, outside the borrow).
    {
        let state = state.clone();
        cx.on_cleanup(move || state.cancel());
    }
    // Wake a possibly-parked loop so it recomputes its sleep bound to
    // include the new deadline (same rule as `after`).
    request_frame();
    IntervalHandle { state }
}

/// Arm ONE one-shot for this interval generation and record its id.
fn arm(
    state: Rc<IntervalState>,
    callback: Rc<RefCell<dyn FnMut()>>,
    period: Duration,
    deadline: Instant,
) {
    let st = state.clone();
    let id = arm_timer_at(deadline, move || {
        if st.cancelled.get() {
            // Belt: cancel_timer already removed us in the normal path;
            // this covers a cancel landing in the same fire pass.
            return;
        }
        (callback.borrow_mut())();
        if st.cancelled.get() {
            // f cancelled itself: no re-arm.
            st.pending.set(None);
            return;
        }
        // Fixed-delay re-arm from the LOOP's fire clock (injected in
        // tests): missed periods coalesced into the one fire above.
        let now = timer_fire_now().unwrap_or_else(Instant::now);
        arm(st.clone(), callback.clone(), period, now + period);
    });
    state.pending.set(Some(id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{create_root, next_timer_deadline, run_due_timers};

    /// Milliseconds shorthand for readable timelines.
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn fires_once_per_elapsed_period_with_steady_clock() {
        let (root, ()) = create_root(|cx| {
            let fires = Rc::new(Cell::new(0u32));
            let f2 = fires.clone();
            let _handle = interval(cx, ms(100), move || f2.set(f2.get() + 1));
            let t0 = Instant::now();
            assert_eq!(run_due_timers(t0 + ms(50)), 0, "not due yet");
            assert_eq!(run_due_timers(t0 + ms(110)), 1);
            assert_eq!(run_due_timers(t0 + ms(215)), 1, "re-armed from fire time");
            assert_eq!(run_due_timers(t0 + ms(320)), 1);
            assert_eq!(fires.get(), 3);
        });
        root.dispose();
    }

    #[test]
    fn missed_ticks_coalesce_into_one_fire() {
        let (root, ()) = create_root(|cx| {
            let fires = Rc::new(Cell::new(0u32));
            let f2 = fires.clone();
            let _handle = interval(cx, ms(100), move || f2.set(f2.get() + 1));
            let t0 = Instant::now();
            // Ten periods pass in one gap (a suspend): ONE fire, then
            // cadence resumes from the fire time — no catch-up storm.
            assert_eq!(run_due_timers(t0 + ms(1050)), 1, "coalesced fire");
            assert_eq!(fires.get(), 1);
            assert_eq!(
                run_due_timers(t0 + ms(1051)),
                0,
                "no storm: next fire is a full period after the last"
            );
            assert_eq!(run_due_timers(t0 + ms(1160)), 1, "cadence resumed");
            assert_eq!(fires.get(), 2);
        });
        root.dispose();
    }

    #[test]
    fn cancel_between_fires_removes_the_pending_timer_entirely() {
        let (root, ()) = create_root(|cx| {
            let fires = Rc::new(Cell::new(0u32));
            let f2 = fires.clone();
            let handle = interval(cx, ms(100), move || f2.set(f2.get() + 1));
            let t0 = Instant::now();
            run_due_timers(t0 + ms(110));
            assert_eq!(fires.get(), 1);
            handle.cancel();
            assert!(handle.is_cancelled());
            assert_eq!(
                next_timer_deadline(),
                None,
                "cancel must remove the armed entry — a dead interval may \
                 not bound the idle sleep"
            );
            assert_eq!(run_due_timers(t0 + ms(10_000)), 0);
            assert_eq!(fires.get(), 1);
            handle.cancel(); // idempotent
        });
        root.dispose();
    }

    #[test]
    fn cancel_from_inside_the_callback_stops_the_rearm() {
        let (root, ()) = create_root(|cx| {
            let fires = Rc::new(Cell::new(0u32));
            let handle_slot: Rc<RefCell<Option<IntervalHandle>>> = Rc::new(RefCell::new(None));
            let f2 = fires.clone();
            let hs = handle_slot.clone();
            let handle = interval(cx, ms(100), move || {
                f2.set(f2.get() + 1);
                if f2.get() == 2 {
                    hs.borrow().as_ref().expect("handle stored").cancel();
                }
            });
            *handle_slot.borrow_mut() = Some(handle);
            let t0 = Instant::now();
            run_due_timers(t0 + ms(110));
            run_due_timers(t0 + ms(220)); // fires + self-cancels
            assert_eq!(fires.get(), 2);
            assert_eq!(next_timer_deadline(), None, "no re-arm after self-cancel");
            assert_eq!(run_due_timers(t0 + ms(10_000)), 0);
            assert_eq!(fires.get(), 2);
        });
        root.dispose();
    }

    #[test]
    fn scope_disposal_cancels_the_interval() {
        let (root, ()) = create_root(|cx| {
            let child = cx.child();
            let fires = Rc::new(Cell::new(0u32));
            let f2 = fires.clone();
            let handle = interval(child, ms(100), move || f2.set(f2.get() + 1));
            let t0 = Instant::now();
            run_due_timers(t0 + ms(110));
            assert_eq!(fires.get(), 1);
            child.dispose();
            assert!(handle.is_cancelled(), "disposal flips the handle state");
            assert_eq!(next_timer_deadline(), None, "disposal removed the entry");
            assert_eq!(run_due_timers(t0 + ms(10_000)), 0);
            assert_eq!(fires.get(), 1);
        });
        root.dispose();
    }

    #[test]
    fn dropping_the_handle_does_not_cancel() {
        let (root, ()) = create_root(|cx| {
            let fires = Rc::new(Cell::new(0u32));
            let f2 = fires.clone();
            drop(interval(cx, ms(100), move || f2.set(f2.get() + 1)));
            let t0 = Instant::now();
            run_due_timers(t0 + ms(110));
            assert_eq!(
                fires.get(),
                1,
                "the scope owns the lifetime, not the handle"
            );
        });
        root.dispose();
    }

    #[test]
    #[should_panic(expected = "interval period must be nonzero")]
    fn zero_period_panics_loudly() {
        let (_root, ()) = create_root(|cx| {
            let _ = interval(cx, Duration::ZERO, || {});
        });
    }
}
