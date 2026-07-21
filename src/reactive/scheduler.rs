//! Scheduler bridge: frame requests toward the render loop and wakeups
//! from other threads toward the (single-threaded) reactive graph.
//!
//! Division of labor:
//! - The reactive graph runs ONLY on its owning thread. Effects are queued
//!   and flushed there (see `runtime::flush_effects`).
//! - Other threads (timers, IO, decode workers) never touch the graph;
//!   they `post` closures through a [`WakeHandle`] and trigger the app's
//!   waker (typically a self-pipe write) so the event loop stops blocking
//!   in `poll`. The main loop then runs `drain_posted()` — the closures
//!   execute on the UI thread with full runtime access.
//! - Frame pacing: UI computations that damage the screen call
//!   [`request_frame`]; the app draws once per wakeup, so any number of
//!   damage events coalesce into one frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::runtime::with_rt;

/// The one frame-request trait, now owned by `base` (cycle-2 unification
/// of the cycle-1 local duplicates; `anim` re-exports the same trait).
/// Re-exported here so cycle-1 call sites keep compiling unchanged.
pub use crate::base::FrameRequester;

type PostedJob = Box<dyn FnOnce() + Send>;

/// State shared with other threads. Kept deliberately tiny: a job queue,
/// a wake flag, and the waker callback. Everything else is thread-local.
pub(crate) struct RemoteShared {
    posted: Mutex<Vec<PostedJob>>,
    woken: AtomicBool,
    waker: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl RemoteShared {
    pub(crate) fn new() -> Self {
        RemoteShared {
            posted: Mutex::new(Vec::new()),
            woken: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }

    fn notify(&self) {
        self.woken.store(true, Ordering::Release);
        // Snapshotting the callback under the lock, invoking outside it,
        // would risk racing an unset; the waker is set once at app start
        // and is cheap (self-pipe write), so invoking under the lock is
        // simpler and safe (it must never call back into WakeHandle).
        if let Some(waker) = self.waker.lock().expect("waker lock").as_ref() {
            waker();
        }
    }
}

/// Cloneable, `Send + Sync` handle other threads use to schedule work on
/// the UI thread. The closure crosses the thread boundary; the reactive
/// graph does not.
#[derive(Clone)]
pub struct WakeHandle {
    shared: Arc<RemoteShared>,
}

impl WakeHandle {
    /// Wake the UI loop without posting work (e.g. "data ready, come poll").
    pub fn wake(&self) {
        self.shared.notify();
    }

    /// Queue `f` to run on the UI thread at the next `drain_posted`, then
    /// wake the loop. This is how a timer thread sets a signal: the set
    /// happens on the UI thread, inside the closure.
    pub fn post(&self, f: impl FnOnce() + Send + 'static) {
        self.shared
            .posted
            .lock()
            .expect("posted lock")
            .push(Box::new(f));
        self.shared.notify();
    }
}

/// Obtain a wake handle bound to the current thread's runtime.
pub fn wake_handle() -> WakeHandle {
    WakeHandle {
        shared: with_rt(|rt| rt.remote.clone()),
    }
}

/// Install the callback `WakeHandle::wake/post` fire from any thread
/// (typically: write one byte to a self-pipe the poll loop watches).
pub fn set_wake_callback(f: impl Fn() + Send + Sync + 'static) {
    let shared = with_rt(|rt| rt.remote.clone());
    *shared.waker.lock().expect("waker lock") = Some(Box::new(f));
}

/// True if a wake arrived since the last `drain_posted`.
pub fn wake_pending() -> bool {
    with_rt(|rt| rt.remote.clone())
        .woken
        .load(Ordering::Acquire)
}

/// Run all posted closures ON THE CALLING (UI) THREAD — this is the
/// thread-affinity contract (RT1-15b): a closure posted from a timer or
/// IO worker executes with full runtime access precisely because it runs
/// here, not where it was posted. Never call from a non-UI thread.
/// Returns how many ran. Clears the wake flag BEFORE running jobs so a
/// post that lands mid-drain re-flags and the loop knows to come back.
pub fn drain_posted() -> usize {
    let shared = with_rt(|rt| rt.remote.clone());
    shared.woken.store(false, Ordering::Release);
    let jobs: Vec<PostedJob> = {
        let mut posted = shared.posted.lock().expect("posted lock");
        std::mem::take(&mut *posted)
    };
    let count = jobs.len();
    for job in jobs {
        job(); // runs with full runtime access; may set signals, flush, etc.
    }
    count
}

/// Install the frame requester (the app loop). `Rc` because requests can
/// re-enter through user draw code; the callback itself must be cheap.
pub fn set_frame_requester(requester: std::rc::Rc<dyn FrameRequester>) {
    with_rt(|rt| rt.frame_requester = Some(requester));
}

/// Ask for a repaint. Coalesced: only the first request between two
/// `take_frame_request` calls reaches the `FrameRequester`, so a storm of
/// damaged regions costs one wakeup.
pub fn request_frame() {
    let requester = with_rt(|rt| {
        if rt.frame_requested {
            None
        } else {
            rt.frame_requested = true;
            rt.frame_requester.clone()
        }
    });
    if let Some(r) = requester {
        r.request_frame(); // outside the borrow: may re-enter the runtime
    }
}

/// Consume the pending frame request (the app calls this once per frame).
pub fn take_frame_request() -> bool {
    with_rt(|rt| std::mem::take(&mut rt.frame_requested))
}

/// Spawn a background worker whose PANIC is reported instead of silently
/// killing the thread (RT1-15b). Default Rust behavior for a panicking
/// worker is thread death + an app symptom of "images silently stopped
/// loading"; here the panic message is posted back to the UI thread and
/// surfaces through [`super::diag::take_worker_failures`] /
/// [`super::diag::diagnostics`] as a LABELED app error.
///
/// Must be called from the UI thread (it captures that thread's wake
/// handle). The worker itself may not touch the reactive graph — it
/// posts closures instead, like any other thread.
pub fn spawn_worker(
    label: &'static str,
    f: impl FnOnce() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let handle = wake_handle();
    std::thread::Builder::new()
        .name(format!("abstracttui-worker-{label}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            if let Err(payload) = result {
                let msg = panic_text(payload.as_ref());
                let text = format!("background worker '{label}' died: {msg}");
                handle.post(move || super::diag::record_worker_failure(text));
            }
        })
        .expect("spawn worker thread")
}

/// Best-effort extraction of a panic payload's message.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn posted_work_runs_on_draining_thread() {
        let handle = wake_handle();
        let hits = Arc::new(AtomicUsize::new(0));
        let h2 = hits.clone();
        std::thread::spawn(move || {
            h2.fetch_add(1, Ordering::SeqCst);
            handle.post(move || {
                // Runs on the draining thread, not the posting thread.
            });
        })
        .join()
        .expect("thread");
        assert!(wake_pending());
        assert_eq!(drain_posted(), 1);
        assert!(!wake_pending());
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn frame_requests_coalesce() {
        struct Counter(Arc<AtomicUsize>);
        impl FrameRequester for Counter {
            fn request_frame(&self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        set_frame_requester(std::rc::Rc::new(Counter(calls.clone())));
        let _ = take_frame_request(); // reset any state from other tests
        request_frame();
        request_frame();
        request_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "requests must coalesce");
        assert!(take_frame_request());
        assert!(!take_frame_request());
        request_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(take_frame_request());
    }
}
