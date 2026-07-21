//! Computation execution: preparing a re-run (clear previous children,
//! cleanups, edges), running user closures with tracking context, memo
//! equality cut-off, and the pull-phase (`update_if_necessary`).
//!
//! Split from `runtime.rs` to keep both under the file-size budget; the
//! borrow discipline is shared: user code NEVER runs under the runtime
//! borrow — everything it needs is `Rc`-cloned out first.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::arena::Key;
use super::node::{remove_source_edges, EqFn, NodeKind, NodeState};
use super::runtime::{restore_context, with_rt, DisposeBundle, Runtime, MSG_CYCLE};

pub(crate) struct Prep {
    pub prev_owner: Option<Key>,
    pub prev_observer: Option<Key>,
    pub cleanups: Vec<Box<dyn FnOnce()>>,
    pub dropped: Vec<super::node::Node>,
    pub work: Work,
}

pub(crate) enum Work {
    Memo {
        compute: Rc<dyn Fn() -> Box<dyn Any>>,
        value: Rc<RefCell<Option<Box<dyn Any>>>>,
        eq: EqFn,
    },
    Effect {
        run: Rc<RefCell<dyn FnMut()>>,
    },
}

impl Runtime {
    /// Prepare a computation re-run: dispose everything the previous run
    /// created (children reverse-creation, then own cleanups LIFO), drop
    /// old dependency edges, stamp a fresh run epoch and install the node
    /// as current owner + observer.
    pub(crate) fn begin_run(&mut self, id: Key) -> Option<Prep> {
        let work = {
            let node = self.graph.get(id)?;
            if node.running {
                panic!("{MSG_CYCLE}");
            }
            match &node.kind {
                NodeKind::Memo { value, compute, eq } => Work::Memo {
                    compute: compute.clone(),
                    value: value.clone(),
                    eq: *eq,
                },
                NodeKind::Effect { run } => Work::Effect { run: run.clone() },
                _ => return None,
            }
        };
        let mut bundle = DisposeBundle::default();
        let (owned, own_cleanups) = {
            let node = self.graph.get_mut(id).expect("checked above");
            (
                std::mem::take(&mut node.owned),
                std::mem::take(&mut node.cleanups),
            )
        };
        for c in owned.into_iter().rev() {
            self.collect_dispose(c, &mut bundle);
        }
        bundle.cleanups.extend(own_cleanups.into_iter().rev());
        remove_source_edges(&mut self.graph, id);
        self.epoch_counter += 1;
        {
            let node = self.graph.get_mut(id).expect("checked above");
            node.run_epoch = self.epoch_counter;
            node.running = true;
        }
        let prev_owner = self.current_owner.replace(id);
        let prev_observer = self.current_observer.replace(id);
        Some(Prep {
            prev_owner,
            prev_observer,
            cleanups: bundle.cleanups,
            dropped: bundle.dropped,
            work,
        })
    }
}

/// Restores owner/observer context and clears the `running` flag even if
/// user code panicked — a poisoned tracking context after a caught panic
/// would corrupt every later computation.
struct CtxGuard {
    node: Key,
    prev_owner: Option<Key>,
    prev_observer: Option<Key>,
}

impl Drop for CtxGuard {
    fn drop(&mut self) {
        restore_context(self.node, self.prev_owner, self.prev_observer);
    }
}

/// Execute one computation (memo recompute or effect run) with full
/// cleanup / tracking / equality-cutoff semantics.
pub(crate) fn run_computation(id: Key) {
    let Some(prep) = with_rt(|rt| rt.begin_run(id)) else {
        return;
    };
    let guard = CtxGuard {
        node: id,
        prev_owner: prep.prev_owner,
        prev_observer: prep.prev_observer,
    };
    // Cleanups (and drops of the previous run's nodes) run OUTSIDE the
    // runtime borrow and BEFORE the new run — Solid's onCleanup contract.
    for c in prep.cleanups {
        c();
    }
    drop(prep.dropped);
    match prep.work {
        Work::Memo { compute, value, eq } => {
            let new = compute(); // user code — tracked via the installed context
            let changed = {
                let mut cell = value.borrow_mut();
                let changed = match cell.as_ref() {
                    Some(old) => !eq(old.as_ref(), new.as_ref()),
                    None => true,
                };
                if changed {
                    *cell = Some(new);
                }
                changed
            };
            drop(guard); // restore context before touching the graph again
            with_rt(|rt| {
                if let Some(node) = rt.graph.get_mut(id) {
                    node.state = NodeState::Clean;
                }
                if changed {
                    rt.mark_direct_observers_dirty(id);
                }
            });
        }
        Work::Effect { run } => {
            (run.borrow_mut())(); // user code
            drop(guard);
            with_rt(|rt| {
                if let Some(node) = rt.graph.get_mut(id) {
                    node.state = NodeState::Clean;
                }
            });
        }
    }
}

/// Up-phase (pull): bring `root` current if it is a computation. `Check`
/// nodes interrogate their sources in read order; the first source that
/// recomputes to a different value flips them `Dirty` (observed on the
/// revisit — equivalent to reactively's early-break). `Dirty` nodes
/// recompute. Clean resolution un-colors the node.
///
/// The traversal stack is explicit so the FRAMEWORK never recurses; note
/// that user computations still nest on the native stack (a memo reading
/// a memo runs inside the outer compute), so dependency DEPTH is bounded
/// by native stack size — documented engine limit.
pub(crate) fn update_if_necessary(root: Key) {
    enum Act {
        Skip,
        Run,
        Step { next: Option<Key> },
    }
    let mut stack: Vec<(Key, usize)> = vec![(root, 0)];
    while let Some((id, idx)) = stack.pop() {
        let act = with_rt(|rt| {
            let (state, src_at) = {
                let Some(node) = rt.graph.get(id) else {
                    return Act::Skip;
                };
                if !node.kind.is_computation() {
                    return Act::Skip;
                }
                (node.state, node.sources.get(idx).copied())
            };
            match state {
                NodeState::Clean => Act::Skip,
                NodeState::Dirty => Act::Run,
                NodeState::Check => match src_at {
                    Some(src) => {
                        let needs = rt
                            .graph
                            .get(src)
                            .map(|s| s.kind.is_computation() && s.state != NodeState::Clean)
                            .unwrap_or(false);
                        Act::Step {
                            next: needs.then_some(src),
                        }
                    }
                    None => {
                        // Every source resolved clean: nothing actually
                        // changed upstream (equality cut-off absorbed it).
                        if let Some(n) = rt.graph.get_mut(id) {
                            n.state = NodeState::Clean;
                        }
                        Act::Skip
                    }
                },
            }
        });
        match act {
            Act::Skip => {}
            Act::Run => run_computation(id),
            Act::Step { next } => {
                stack.push((id, idx + 1));
                if let Some(src) = next {
                    stack.push((src, 0));
                }
            }
        }
    }
}
