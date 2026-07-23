//! Signal transitions: animate a signal's changes instead of jumping.
//!
//! `animate(cx, source, easing, duration)` returns a FOLLOWER signal
//! that chases `source`: every source change starts (or retargets) a
//! tween from the follower's CURRENT value, advanced once per frame by
//! the app loop's frame-task pump. UI reads the follower exactly like
//! any signal — a `Dyn` reading it re-renders per animation frame,
//! which is precisely the billing the damage contract wants (an
//! animation is a sequence of frame requests, §4).
//!
//! Zero idle cost: the task list empties when every transition lands;
//! no timers, no threads — each pending task re-requests one frame.
//!
//! Built on `anim::{Tween, Easing, Lerp}` (RENDER's shipped primitives);
//! when a dedicated `anim::Transition` type lands this helper becomes a
//! thin adapter over it (request filed) — the reactive-side surface
//! (`animate`, `run_frame_tasks`) stays as is.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::anim::{Easing, Lerp, Tween};

use super::runtime::with_rt;
use super::scheduler::request_frame;
use super::scope::Scope;
use super::signal::Signal;

struct Flight<T: Lerp> {
    tween: Tween<T>,
    /// Stamped by the FIRST frame that advances this flight, so the
    /// animation timeline aligns to frames (and tests drive it with
    /// synthetic instants — no real-clock coupling).
    started: Option<Instant>,
    target: T,
}

struct AnimState<T: Lerp> {
    flight: Option<Flight<T>>,
    /// A frame task for this follower is currently registered.
    task_live: bool,
}

/// Follow `source` through eased transitions. The returned signal is
/// owned by `cx` (dies with the component); `T` needs `Lerp` (anim's
/// interpolation vocabulary) and `PartialEq` (equality cut-off keeps
/// settled frames free).
pub fn animate<T>(cx: Scope, source: Signal<T>, easing: Easing, duration: Duration) -> Signal<T>
where
    T: Lerp + PartialEq + 'static,
{
    let out = cx.signal(source.get_untracked());
    let state: Rc<RefCell<AnimState<T>>> = Rc::new(RefCell::new(AnimState {
        flight: None,
        task_live: false,
    }));

    cx.effect_labeled("reactive-animate", move || {
        let target = source.get(); // tracked: retarget on every change
        let current = out.get_untracked();
        if target == current {
            // Nothing to chase; a mid-flight snap-back lands the flight.
            state.borrow_mut().flight = None;
            return;
        }
        {
            let mut st = state.borrow_mut();
            st.flight = Some(Flight {
                tween: Tween::new(current, target, duration).with_easing(easing),
                started: None,
                target,
            });
            if !st.task_live {
                st.task_live = true;
                let state_for_task = state.clone();
                register_frame_task(move |now| {
                    let mut st = state_for_task.borrow_mut();
                    let Some(flight) = st.flight.as_mut() else {
                        st.task_live = false;
                        return false; // landed or cancelled: drop the task
                    };
                    let started = *flight.started.get_or_insert(now);
                    let elapsed = now.saturating_duration_since(started);
                    if flight.tween.is_finished(elapsed) {
                        let target = flight.target;
                        st.flight = None;
                        st.task_live = false;
                        drop(st); // release before re-entering the runtime
                        out.set_if_changed(target);
                        false // settled: no further frames from this task
                    } else {
                        let sample = flight.tween.sample(elapsed);
                        drop(st);
                        out.set_if_changed(sample);
                        request_frame(); // keep the loop paced while flying
                        true
                    }
                });
            }
        }
        request_frame();
    });
    out
}

/// Register a per-frame callback (false = done, drop it). Crate-internal:
/// `animate` and the meter ballistics (widgets/meter, media-av 0620) are
/// the consumers — a PUBLIC frame-task surface is games/0710's decision,
/// deliberately not preempted here.
pub(crate) fn register_frame_task(task: impl FnMut(Instant) -> bool + 'static) {
    with_rt(|rt| rt.frame_tasks.push(Box::new(task)));
}

/// Advance all pending frame tasks (the app loop calls this once per
/// frame in phase U, with the frame's clock reading). Returns how many
/// tasks remain in flight — 0 means animations are settled and idle can
/// be truly idle.
pub fn run_frame_tasks(now: Instant) -> usize {
    // Take the list out so tasks can register new tasks (a completed
    // transition triggering another) without aliasing the runtime borrow.
    let mut tasks = with_rt(|rt| std::mem::take(&mut rt.frame_tasks));
    tasks.retain_mut(|task| task(now));
    with_rt(|rt| {
        // New tasks registered DURING the run come first next frame —
        // order is irrelevant (tasks are independent), append is cheap.
        let newly = std::mem::take(&mut rt.frame_tasks);
        rt.frame_tasks = tasks;
        rt.frame_tasks.extend(newly);
        rt.frame_tasks.len()
    })
}

/// Pending frame-task count (loop pacing: > 0 means schedule a frame).
pub fn frame_tasks_pending() -> usize {
    with_rt(|rt| rt.frame_tasks.len())
}

/// Run `f` once on the UI thread after `delay` (a one-shot timer —
/// toast dismissal, debounce). Timers do NOT frame-pace: the loop
/// sleeps until [`next_timer_deadline`] and fires due timers in phase U
/// (`run_due_timers`), so a pending timer costs zero wakeups until due.
/// For a repeating, cancellable cadence use [`super::interval`].
pub fn after(delay: Duration, f: impl FnOnce() + 'static) {
    let _ = arm_timer_at(Instant::now() + delay, f);
    // Wake a possibly-blocked loop so it recomputes its sleep deadline.
    request_frame();
}

/// Arm a one-shot at an ABSOLUTE deadline; returns the entry's id for
/// [`cancel_timer`]. Internal: `after` (fire-and-forget) and `interval`
/// (re-arming + cancellation) are the public faces. Does NOT wake the
/// loop — callers decide (an interval re-arming inside phase U needs no
/// wake; a fresh arm from user code does).
pub(crate) fn arm_timer_at(deadline: Instant, f: impl FnOnce() + 'static) -> u64 {
    with_rt(|rt| {
        rt.next_timer_id += 1;
        let id = rt.next_timer_id;
        rt.timers.push(super::runtime::TimerEntry {
            deadline,
            id,
            f: Box::new(f),
        });
        id
    })
}

/// Remove a pending timer by id BEFORE it fires. True if an entry was
/// removed; false when it already fired (or was already cancelled) —
/// ids are never reused, so this can never remove a stranger's timer.
/// The callback is dropped outside the runtime borrow (its captures'
/// `Drop` may re-enter the runtime).
pub(crate) fn cancel_timer(id: u64) -> bool {
    let removed: Option<super::runtime::TimerEntry> = with_rt(|rt| {
        let at = rt.timers.iter().position(|e| e.id == id)?;
        Some(rt.timers.swap_remove(at))
    });
    removed.is_some()
}

/// The clock reading the CURRENT `run_due_timers` pass fires with —
/// `None` outside one. Re-arming callbacks (intervals) derive their next
/// deadline from this, so injected test clocks stay authoritative.
pub(crate) fn timer_fire_now() -> Option<Instant> {
    with_rt(|rt| rt.timer_now)
}

/// Earliest pending timer deadline — the loop's idle sleep bound.
pub fn next_timer_deadline() -> Option<Instant> {
    with_rt(|rt| rt.timers.iter().map(|e| e.deadline).min())
}

/// Fire every timer whose deadline passed (phase U). Returns fired count.
pub fn run_due_timers(now: Instant) -> usize {
    // Take due entries out first: a firing timer may register new timers
    // or re-enter the runtime.
    let due: Vec<Box<dyn FnOnce()>> = with_rt(|rt| {
        let mut fired = Vec::new();
        let mut i = 0;
        while i < rt.timers.len() {
            if rt.timers[i].deadline <= now {
                fired.push(rt.timers.swap_remove(i).f);
            } else {
                i += 1;
            }
        }
        // Publish the pass clock for re-arming callbacks; cleared by the
        // guard below even if a callback panics (a stale value would
        // corrupt every later interval's timeline).
        rt.timer_now = Some(now);
        fired
    });
    struct FireGuard;
    impl Drop for FireGuard {
        fn drop(&mut self) {
            super::runtime::clear_timer_now();
        }
    }
    let _guard = FireGuard;
    let count = due.len();
    for f in due {
        f();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{create_root, flush_effects, take_frame_request};

    #[test]
    fn follower_chases_source_and_settles() {
        let (_root, ()) = create_root(|cx| {
            let source = cx.signal(0.0f32);
            let follower = animate(cx, source, Easing::Linear, Duration::from_millis(100));
            assert_eq!(follower.get_untracked(), 0.0);
            assert_eq!(frame_tasks_pending(), 0, "no change, no task");

            source.set(10.0);
            flush_effects();
            assert_eq!(frame_tasks_pending(), 1, "transition in flight");
            assert!(take_frame_request(), "a frame was requested");

            // Drive with synthetic frame times: t0 anchors the flight.
            let t0 = Instant::now();
            run_frame_tasks(t0); // stamps start, samples t=0
            assert_eq!(follower.get_untracked(), 0.0);
            run_frame_tasks(t0 + Duration::from_millis(50));
            let mid = follower.get_untracked();
            assert!(mid > 3.0 && mid < 7.0, "midpoint-ish: {mid}");
            assert!(take_frame_request(), "in-flight frames keep pacing");
            let left = run_frame_tasks(t0 + Duration::from_millis(150));
            assert_eq!(follower.get_untracked(), 10.0, "lands exactly on target");
            assert_eq!(left, 0, "settled: task dropped, idle is idle again");
        });
    }

    #[test]
    fn retarget_mid_flight_starts_from_current_value() {
        let (_root, ()) = create_root(|cx| {
            let source = cx.signal(0.0f32);
            let follower = animate(cx, source, Easing::Linear, Duration::from_millis(100));
            source.set(10.0);
            flush_effects();
            let t0 = Instant::now();
            run_frame_tasks(t0);
            run_frame_tasks(t0 + Duration::from_millis(50)); // ~5.0
            let mid = follower.get_untracked();
            source.set(0.0); // reverse!
            flush_effects();
            // New flight: from ~mid back to 0, fresh timeline.
            let t1 = t0 + Duration::from_millis(60);
            run_frame_tasks(t1); // stamps new start
            run_frame_tasks(t1 + Duration::from_millis(100));
            assert_eq!(follower.get_untracked(), 0.0);
            assert!(mid > 0.0, "was mid-flight when retargeted");
            assert_eq!(frame_tasks_pending(), 0);
        });
    }

    #[test]
    fn identical_target_never_registers_a_task() {
        let (_root, ()) = create_root(|cx| {
            let source = cx.signal(5i32);
            let follower = animate(cx, source, Easing::Linear, Duration::from_millis(50));
            source.set(5); // no-op change
            flush_effects();
            assert_eq!(frame_tasks_pending(), 0, "equal target: zero work");
            assert_eq!(follower.get_untracked(), 5);
        });
    }
}
