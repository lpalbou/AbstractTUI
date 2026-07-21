//! `TerminalWaker`: a cheap cross-thread handle that interrupts a blocking
//! `Terminal::read`, making the read return [`super::TermRead::Wake`].
//!
//! OWNER: KERNEL. This is REACT's loop primitive (cycle-1 request 6): the
//! reactive scheduler's `set_wake_callback` closure calls `wake()` whenever
//! a job is posted from another thread, so the event loop never needs a
//! tick timer to notice cross-thread work.
//!
//! Contract:
//! - `wake()` is cheap, non-blocking, panic-free and callable from any
//!   thread (including while a `read` is mid-poll on the terminal thread).
//! - Wakes COALESCE: any number of `wake()` calls between two reads
//!   guarantee at least one `TermRead::Wake`, not one per call. Consumers
//!   drain all pending work per wake (the reactive scheduler already does).
//! - A waker outliving its terminal is harmless: `wake()` becomes a no-op
//!   against a closed channel (platform notes in the backends).
//!
//! The concrete transport is platform-owned (unix: a dedicated self-pipe
//! polled beside the tty; windows: an auto-reset event in the wait set).
//! This type is deliberately just an `Arc<dyn Fn>` so test terminals
//! (REDTEAM's CaptureTerm) can mint one from any closure via
//! [`TerminalWaker::new`].

use std::fmt;
use std::sync::Arc;

/// Cross-thread wake handle for a blocking [`crate::term::Terminal`]
/// read; see the module docs for the coalescing contract.
///
/// ```
/// use abstracttui::term::TerminalWaker;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use std::sync::Arc;
///
/// // Scripted terminals (and the reactive scheduler) mint wakers from
/// // any thread-safe closure; platform terminals hand out their own.
/// let hits = Arc::new(AtomicUsize::new(0));
/// let h = hits.clone();
/// let waker = TerminalWaker::new(move || { h.fetch_add(1, Ordering::SeqCst); });
///
/// let for_thread = waker.clone(); // Clone + Send + Sync
/// std::thread::spawn(move || for_thread.wake()).join().unwrap();
/// assert_eq!(hits.load(Ordering::SeqCst), 1);
/// ```
#[derive(Clone)]
pub struct TerminalWaker {
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl TerminalWaker {
    /// Wrap any thread-safe closure. Platform backends pass their pipe
    /// write / event signal; test terminals pass whatever their scripted
    /// clock needs.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        TerminalWaker {
            wake: Arc::new(wake),
        }
    }

    /// Interrupt the terminal's blocking read (coalescing, see module doc).
    pub fn wake(&self) {
        (self.wake)();
    }
}

impl fmt::Debug for TerminalWaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TerminalWaker")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn waker_is_clone_send_sync_and_fires() {
        fn assert_traits<T: Clone + Send + Sync>() {}
        assert_traits::<TerminalWaker>();

        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        let w = TerminalWaker::new(move || {
            h.fetch_add(1, Ordering::SeqCst);
        });
        let w2 = w.clone();
        std::thread::spawn(move || w2.wake()).join().unwrap();
        w.wake();
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }
}
