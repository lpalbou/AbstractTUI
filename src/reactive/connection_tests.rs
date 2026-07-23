//! Connection lifecycle + backoff tests (live-data 0040), split file
//! (`#[path]`-included as `connection::tests`). The machine is driven
//! headless: posted reports apply through `drain_posted` (the app
//! loop's phase U), retries fire through `run_due_timers` with
//! explicit clocks — the interval suite's discipline.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::*;
use crate::reactive::{create_root, drain_posted, next_timer_deadline, run_due_timers};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ------------------------------------------------------------- backoff

#[test]
fn backoff_ceiling_grows_monotone_to_the_cap() {
    let mut b = Backoff::new(ms(500), Duration::from_secs(30)).seeded(1);
    let mut ceilings = Vec::new();
    for _ in 0..12 {
        ceilings.push(b.ceiling());
        b.next_delay();
    }
    assert_eq!(ceilings[0], ms(500), "attempt 0 draws under the base");
    assert_eq!(ceilings[1], ms(1000));
    assert_eq!(ceilings[2], ms(2000));
    assert!(
        ceilings.windows(2).all(|w| w[0] <= w[1]),
        "monotone growth: {ceilings:?}"
    );
    assert_eq!(
        ceilings.last(),
        Some(&Duration::from_secs(30)),
        "saturates at the cap"
    );
    assert_eq!(b.attempt(), 12, "attempt counter grew monotonically");
}

#[test]
fn backoff_draws_stay_within_the_jitter_bounds() {
    // Full jitter: every draw lands in [0, ceiling-at-draw], for any
    // seed; the cap bounds everything. And the jitter is REAL — a
    // seeded run must not collapse to one repeated value.
    for seed in [1u64, 7, 42, 0xDEAD_BEEF, u64::MAX] {
        let mut b = Backoff::new(ms(500), Duration::from_secs(30)).seeded(seed);
        let mut distinct = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let bound = b.ceiling();
            let d = b.next_delay();
            assert!(d <= bound, "seed {seed}: {d:?} exceeds ceiling {bound:?}");
            assert!(d <= Duration::from_secs(30), "cap bounds every draw");
            distinct.insert(d);
        }
        assert!(
            distinct.len() > 8,
            "seed {seed}: draws must vary (full jitter), got {distinct:?}"
        );
    }
}

#[test]
fn backoff_reset_returns_to_base() {
    let mut b = Backoff::default().seeded(3);
    for _ in 0..6 {
        b.next_delay();
    }
    assert_eq!(b.ceiling(), Duration::from_secs(30), "deep in the schedule");
    b.reset();
    assert_eq!(b.attempt(), 0);
    assert_eq!(b.ceiling(), ms(500), "reset re-bases the schedule");
    assert!(b.next_delay() <= ms(500));
}

#[test]
fn backoff_zero_base_never_panics() {
    // Degenerate but legal: a zero base draws zero until the doubling
    // ...which never leaves zero — the schedule is then "retry now",
    // the app's explicit choice.
    let mut b = Backoff::new(Duration::ZERO, Duration::ZERO).seeded(9);
    assert_eq!(b.next_delay(), Duration::ZERO);
    assert_eq!(b.next_delay(), Duration::ZERO);
}

// ------------------------------------------------------- the machine

/// Rig: a connection whose dial only RECORDS (call count + the minted
/// reporter); tests drive outcomes by hand through the reporters.
struct Rig {
    conn: Connection,
    log: Rc<RefCell<Vec<ConnState>>>,
    dials: Rc<RefCell<Vec<ConnectionEvents>>>,
}

fn rig(cx: crate::reactive::Scope) -> Rig {
    let dials: Rc<RefCell<Vec<ConnectionEvents>>> = Default::default();
    let d2 = dials.clone();
    let conn = connection(cx, Backoff::default().seeded(11), move |events| {
        d2.borrow_mut().push(events)
    });
    let log: Rc<RefCell<Vec<ConnState>>> = Default::default();
    let l2 = log.clone();
    let state = conn.state();
    cx.effect(move || l2.borrow_mut().push(state.get()));
    Rig { conn, log, dials }
}

impl Rig {
    fn reporter(&self, attempt: usize) -> ConnectionEvents {
        self.dials.borrow()[attempt].clone()
    }
    fn dial_count(&self) -> usize {
        self.dials.borrow().len()
    }
}

#[test]
fn state_sequence_golden_under_scripted_failures() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        assert_eq!(r.dial_count(), 1, "birth dial is immediate");

        // fail #1 -> reconnect (attempt 1, jitter under the base).
        r.reporter(0).failed("socket reset");
        drain_posted();
        // The armed retry fires -> dial #2 -> connected -> degraded ->
        // fail again (schedule RESET by the connect) -> close.
        let deadline = next_timer_deadline().expect("retry armed");
        run_due_timers(deadline);
        assert_eq!(r.dial_count(), 2);
        r.reporter(1).connected();
        drain_posted();
        r.reporter(1).degraded("catching up");
        drain_posted();
        r.reporter(1).failed("stream died");
        drain_posted();
        r.conn.close();

        let log = r.log.borrow();
        // Golden shape: every transition, in order, exactly once.
        assert_eq!(log.len(), 7, "golden length: {log:?}");
        assert_eq!(log[0], ConnState::Connecting, "birth");
        let ConnState::Reconnecting { attempt, next_in } = &log[1] else {
            panic!("log[1] = {:?}", log[1]);
        };
        assert_eq!(*attempt, 1);
        assert!(*next_in <= ms(500), "first retry draws under the base");
        assert_eq!(log[2], ConnState::Connecting, "retry dialed");
        assert_eq!(log[3], ConnState::Connected);
        assert_eq!(log[4], ConnState::Degraded("catching up".into()));
        let ConnState::Reconnecting { attempt, .. } = &log[5] else {
            panic!("log[5] = {:?}", log[5]);
        };
        assert_eq!(*attempt, 1, "the connect RESET the schedule");
        assert_eq!(log[6], ConnState::Closed);

        assert_eq!(
            next_timer_deadline(),
            None,
            "close cancelled the second retry"
        );
    });
    root.dispose();
}

#[test]
fn degraded_from_connecting_counts_as_impaired_connect() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        r.reporter(0).degraded("read-only replica");
        drain_posted();
        assert_eq!(
            r.conn.state().get_untracked(),
            ConnState::Degraded("read-only replica".into())
        );
        // The schedule reset like a connect: the next failure is
        // attempt 1 again.
        r.reporter(0).failed("dropped");
        drain_posted();
        assert!(matches!(
            r.conn.state().get_untracked(),
            ConnState::Reconnecting { attempt: 1, .. }
        ));
    });
    root.dispose();
}

#[test]
fn cancel_mid_reconnect_close_removes_the_timer_entirely() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        r.reporter(0).failed("boom");
        drain_posted();
        assert!(next_timer_deadline().is_some(), "retry armed");
        r.conn.close();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Closed);
        assert!(r.conn.is_closed());
        assert_eq!(
            next_timer_deadline(),
            None,
            "cancel must REMOVE the armed entry — a dead connection may \
             not bound the idle sleep"
        );
        assert_eq!(run_due_timers(Instant::now() + ms(60_000)), 0);
        assert_eq!(r.dial_count(), 1, "no further dial, ever");
        r.conn.close(); // idempotent
        r.conn.retry_now(); // closed: no-op
        assert_eq!(r.dial_count(), 1);
    });
    root.dispose();
}

#[test]
fn scope_disposal_mid_reconnect_cancels_and_closes() {
    let mut kept = None;
    let (root, ()) = create_root(|cx| {
        let child = cx.child();
        let r = rig(child);
        r.reporter(0).failed("boom");
        drain_posted();
        assert!(next_timer_deadline().is_some(), "retry armed");
        kept = Some((r.conn.clone(), r.reporter(0)));
        child.dispose();
    });
    let (conn, reporter) = kept.expect("kept");
    assert!(conn.is_closed(), "disposal closes");
    assert_eq!(next_timer_deadline(), None, "disposal removed the timer");
    assert!(reporter.is_closed(), "workers observe the stop condition");
    // Late reports are inert and counted — never a panic on the dead
    // signal (the source-lane discipline).
    reporter.connected();
    drain_posted();
    assert_eq!(reporter.stale_reports(), 1);
    root.dispose();
}

#[test]
fn stale_attempt_reports_are_inert_and_counted() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        let old = r.reporter(0);
        assert!(old.is_current());
        old.failed("first death");
        drain_posted();
        assert!(!old.is_current(), "accepting the failure superseded it");
        let deadline = next_timer_deadline().expect("retry armed");
        run_due_timers(deadline);
        assert_eq!(r.dial_count(), 2);
        // The zombie worker from attempt #1 reports success LATE: the
        // machine must not flip attempt #2's Connecting.
        old.connected();
        drain_posted();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Connecting);
        assert_eq!(old.stale_reports(), 1);
        // The live attempt's report still lands.
        let live = r.reporter(1);
        assert!(live.is_current());
        live.connected();
        drain_posted();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Connected);
    });
    root.dispose();
}

#[test]
fn transport_clean_close_is_terminal() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        r.reporter(0).connected();
        drain_posted();
        r.reporter(0).closed(); // server said goodbye
        drain_posted();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Closed);
        assert_eq!(next_timer_deadline(), None, "clean close retries NOTHING");
        r.reporter(0).failed("late noise");
        drain_posted();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Closed);
        assert_eq!(r.reporter(0).stale_reports(), 1);
    });
    root.dispose();
}

#[test]
fn retry_now_skips_the_wait() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        r.conn.retry_now(); // Connecting: no-op (attempt in flight)
        assert_eq!(r.dial_count(), 1);
        r.reporter(0).failed("boom");
        drain_posted();
        assert!(next_timer_deadline().is_some());
        r.conn.retry_now();
        assert_eq!(r.dial_count(), 2, "dialed immediately");
        assert_eq!(r.conn.state().get_untracked(), ConnState::Connecting);
        assert_eq!(
            next_timer_deadline(),
            None,
            "the armed one-shot was consumed by the manual retry"
        );
    });
    root.dispose();
}

#[test]
fn dial_may_close_the_connection_reentrantly() {
    // A dial that decides "we're done" (fatal config, auth refused)
    // and closes from INSIDE the dial call must not deadlock on the
    // machine's own cells or restore a dropped closure.
    let (root, ()) = create_root(|cx| {
        let conn_slot: Rc<RefCell<Option<Connection>>> = Default::default();
        let cs = conn_slot.clone();
        let dials = Rc::new(std::cell::Cell::new(0u32));
        let d2 = dials.clone();
        let conn = connection(cx, Backoff::default().seeded(5), move |events| {
            d2.set(d2.get() + 1);
            if d2.get() == 2 {
                // The RETRY dial gives up for good.
                cs.borrow().as_ref().expect("stored").close();
            } else {
                events.failed("try again");
            }
        });
        *conn_slot.borrow_mut() = Some(conn.clone());
        drain_posted(); // the birth dial's failure applies
        let deadline = next_timer_deadline().expect("retry armed");
        run_due_timers(deadline); // retry dial closes re-entrantly
        assert_eq!(dials.get(), 2);
        assert_eq!(conn.state().get_untracked(), ConnState::Closed);
        assert_eq!(next_timer_deadline(), None);
        conn.retry_now(); // closed: dial fn is GONE, not restorable
        assert_eq!(dials.get(), 2);
    });
    root.dispose();
}

#[test]
fn zero_idle_cost_when_closed() {
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        r.reporter(0).failed("boom");
        drain_posted();
        r.conn.close();
        // The three idle meters the loop bills by: no timers, no
        // posted work, no frame tasks. A closed connection is FREE.
        assert_eq!(next_timer_deadline(), None, "no armed timer");
        assert_eq!(drain_posted(), 0, "no pending posted jobs");
        assert_eq!(crate::reactive::frame_tasks_pending(), 0, "no frame tasks");
        // And nothing EVER runs again: far-future clock fires nothing.
        assert_eq!(run_due_timers(Instant::now() + ms(3_600_000)), 0);
        assert_eq!(r.dial_count(), 1);
        assert_eq!(r.log.borrow().last(), Some(&ConnState::Closed));
    });
    root.dispose();
}

#[test]
fn reporters_are_send_and_the_handle_stays_ui_side() {
    // Compile-time contract: workers get the reporter, never the
    // Connection (Signal handles are Copy+Send but the machine is
    // UI-thread-owned — the reporter is the sanctioned crossing).
    fn assert_send<T: Send>() {}
    assert_send::<ConnectionEvents>();
    // A worker thread reporting through the posted lane end-to-end:
    let (root, ()) = create_root(|cx| {
        let r = rig(cx);
        let events = r.reporter(0);
        let t = std::thread::spawn(move || {
            events.connected();
        });
        t.join().expect("worker");
        drain_posted();
        assert_eq!(r.conn.state().get_untracked(), ConnState::Connected);
    });
    root.dispose();
}
