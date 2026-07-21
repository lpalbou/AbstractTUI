//! Runtime health surfaces: the RT1-2 draw-phase purity guard and the
//! RT1-15b worker-failure channel. Split from `runtime.rs` for the
//! file-size budget; the state lives in `Runtime`, these are its doors.

use super::runtime::{self, with_rt};

/// Marks the frame's DRAW phase for the RT1-2 purity guard. The ui layer
/// holds one of these while running draw closures; tracked reads during
/// that window are a debug panic / release-counted violation. Nesting-safe
/// (a depth counter), panic-safe (Drop decrements during unwind).
pub struct DrawPhase(());

impl Drop for DrawPhase {
    fn drop(&mut self) {
        runtime::exit_draw_phase();
    }
}

/// Enter the draw phase (see [`DrawPhase`]).
pub fn enter_draw_phase() -> DrawPhase {
    with_rt(|rt| rt.draw_depth += 1);
    DrawPhase(())
}

/// Record a labeled failure from a background worker (used by
/// `scheduler::spawn_worker`'s catch_unwind reporter; runs on the UI
/// thread as a posted job).
pub(crate) fn record_worker_failure(message: String) {
    with_rt(|rt| rt.worker_failures.push(message));
}

/// Drain pending worker failures; the app loop surfaces these as labeled
/// app errors instead of letting a dead worker read as silence (RT1-15b).
pub fn take_worker_failures() -> Vec<String> {
    with_rt(|rt| std::mem::take(&mut rt.worker_failures))
}

/// Runtime health report: draw-phase violations (release builds count
/// what debug builds panic on) and pending worker failures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    /// Tracked reads attempted during draw (RT1-2), release builds only.
    pub draw_read_violations: u64,
    /// First few violation descriptions, `#FALLBACK`-labeled.
    pub draw_read_samples: Vec<String>,
    /// Worker panics not yet taken via `take_worker_failures`.
    pub pending_worker_failures: usize,
}

pub fn diagnostics() -> Diagnostics {
    with_rt(|rt| Diagnostics {
        draw_read_violations: rt.draw_read_violations,
        draw_read_samples: rt.draw_read_samples.clone(),
        pending_worker_failures: rt.worker_failures.len(),
    })
}
