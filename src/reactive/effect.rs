//! `Effect`: eager side effect bound to the signals it reads.
//!
//! Lifecycle: runs once synchronously at creation (establishing its
//! dependency edges), then re-runs after any dependency change — but
//! SCHEDULED through the effect queue, never inline mid-write. Inline
//! execution would let an effect observe a half-applied batch (the
//! glitch this whole design exists to prevent).
//!
//! Cleanups registered with [`super::runtime::on_cleanup`] during a run
//! execute before the next run and at disposal — the standard contract
//! for tearing down subscriptions, timers, or mounted UI subtrees.

use std::cell::RefCell;
use std::rc::Rc;

use super::execute::run_computation;
use super::node::NodeKind;
use super::runtime::{self, with_rt, RawId};
use super::scope::Scope;

/// `Copy` handle to a running effect; primarily used to dispose it early
/// (e.g. a `Dyn` view unmounting).
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Effect {
    pub(crate) id: RawId,
}

impl std::fmt::Debug for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Effect({}, gen {})",
            self.id.key.index, self.id.key.generation
        )
    }
}

impl Effect {
    /// Create under `scope` and run immediately (tracked first run).
    /// `label` (RT1-15a) names this effect in runaway-loop panics and
    /// diagnostics; pass one for any effect that writes signals.
    pub(crate) fn new(
        scope: Scope,
        label: Option<&'static str>,
        f: impl FnMut() + 'static,
    ) -> Effect {
        let id = with_rt(|rt| {
            rt.check_thread(scope.id);
            let key = rt.create_node(
                Some(scope.id.key),
                NodeKind::Effect {
                    run: Rc::new(RefCell::new(f)),
                },
            );
            if let Some(node) = rt.graph.get_mut(key) {
                node.label = label;
            }
            RawId { key, rt: rt.rt_id }
        });
        // First run happens outside the runtime borrow (it is user code).
        // It is synchronous even inside a batch: an effect must establish
        // its dependencies at creation or the first write would miss it.
        run_computation(id.key);
        Effect { id }
    }

    pub fn is_alive(self) -> bool {
        with_rt(|rt| rt.rt_id == self.id.rt && rt.graph.contains(self.id.key))
    }

    /// Stop the effect forever: runs its pending cleanups, disposes
    /// everything it owns, unlinks its dependency edges, frees its slot.
    /// Safe to call twice (the second call is a no-op on a stale key).
    pub fn dispose(self) {
        runtime::dispose_node(self.id);
    }
}
