//! Fine-grained reactivity: `Signal`, `Memo`, `Effect`, ownership scopes
//! and the frame scheduler. This is the engine's "React" — but SolidJS
//! style: dependencies tracked at read time, updates propagate to exactly
//! the affected computations, which mark exactly the affected UI regions
//! damaged. No virtual-DOM diffing, no full-frame immediate mode.
//!
//! Owner: REACT. Design rationale + literature survey:
//! `docs/design/reactive-ui.md`.
//!
//! ## Model in five sentences
//!
//! Nodes (signals, memos, effects, scopes) live in a hand-rolled
//! generational arena; user handles are `Copy` ids. Reads record edges
//! from the value to the *currently running* computation. A write marks
//! direct observers `Dirty` and transitive observers `Check` (two-phase
//! marking), queueing any effects it reaches. Effects flush in creation
//! order — outside `batch` immediately after the write, inside `batch`
//! once at the end — and each effect PULLS its sources up to date first,
//! so it observes a single consistent world (diamond-safe, glitch-free).
//! Memos recompute lazily on observation and stop propagation when the
//! new value compares equal.
//!
//! ## Quick start
//!
//! ```
//! use abstracttui::reactive::{batch, create_root};
//! use std::{cell::RefCell, rc::Rc};
//!
//! let log = Rc::new(RefCell::new(Vec::new()));
//! let (root, ()) = create_root(|cx| {
//!     let count = cx.signal(0);
//!     let doubled = cx.memo(move || count.get() * 2);
//!     let log2 = log.clone();
//!     cx.effect(move || log2.borrow_mut().push(doubled.get()));
//!     count.set(3);
//!     batch(|| {
//!         count.set(4);
//!         count.set(5); // coalesced: the effect sees only 10
//!     });
//! });
//! assert_eq!(*log.borrow(), vec![0, 6, 10]);
//! root.dispose();
//! ```

mod animate;
mod arena;
mod diag;
mod effect;
mod execute;
mod memo;
mod node;
mod runtime;
mod scheduler;
mod scope;
mod signal;

#[cfg(test)]
mod tests;

pub use animate::{
    after, animate, frame_tasks_pending, next_timer_deadline, run_due_timers, run_frame_tasks,
};
pub use diag::{diagnostics, enter_draw_phase, take_worker_failures, Diagnostics, DrawPhase};
pub use effect::Effect;
pub use memo::Memo;
pub use runtime::{batch, flush_effects, on_cleanup, stats, untrack, RuntimeStats};
pub use scheduler::{
    drain_posted, request_frame, set_frame_requester, set_wake_callback, spawn_worker,
    take_frame_request, wake_handle, wake_pending, FrameRequester, WakeHandle,
};
pub use scope::{create_root, RootScope, Scope};
pub use signal::Signal;

pub(crate) use arena::{GenArena, Key};
