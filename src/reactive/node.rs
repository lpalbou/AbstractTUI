//! Reactive graph nodes and the paired-slot edge bookkeeping.
//!
//! Every reactive primitive (signal, memo, effect, scope) is one `Node` in
//! one arena. Edges are stored twice — `sources` on the reader and
//! `observers` on the value — with *paired slot indices* so a single edge
//! can be unlinked in O(1) from either side (`swap_remove` + back-pointer
//! fixup). This is SolidJS's `sources/sourceSlots` scheme; without it,
//! re-running a computation that watches a hot signal would pay a linear
//! scan of that signal's observer list, and pathological update storms
//! become quadratic.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::arena::{GenArena, Key};

/// Two-phase marking state (graph coloring, reactively-style).
///
/// - `Clean`: value is current.
/// - `Check`: *might* be stale — some transitive source changed; must ask
///   its sources before deciding to recompute (the lazy "green" state).
/// - `Dirty`: definitely stale — a direct source changed value; must
///   recompute when observed.
///
/// The ordering derives `<` comparisons in the marking walk: a node is only
/// upgraded, never downgraded, during the down-phase.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NodeState {
    Clean = 0,
    Check = 1,
    Dirty = 2,
}

/// Type-erased equality used for memo cut-off. A plain fn pointer (no
/// allocation): monomorphized once per `T` at memo creation.
pub(crate) type EqFn = fn(&dyn Any, &dyn Any) -> bool;

pub(crate) fn eq_any<T: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
    match (a.downcast_ref::<T>(), b.downcast_ref::<T>()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Node payloads. Values live behind `Rc<RefCell<..>>` so the runtime can
/// hand a clone to user code and *release the arena borrow first* — user
/// closures re-enter the runtime (reads, writes, creations) and holding
/// the arena `RefCell` across them would panic.
pub(crate) enum NodeKind {
    /// Pure ownership node: owns children/cleanups, no value, no edges.
    Scope,
    Signal {
        value: Rc<RefCell<Box<dyn Any>>>,
    },
    Memo {
        /// `None` until first observed compute (memos are lazy from birth).
        value: Rc<RefCell<Option<Box<dyn Any>>>>,
        compute: Rc<dyn Fn() -> Box<dyn Any>>,
        eq: EqFn,
    },
    Effect {
        run: Rc<RefCell<dyn FnMut()>>,
    },
}

impl NodeKind {
    pub(crate) fn is_effect(&self) -> bool {
        matches!(self, NodeKind::Effect { .. })
    }

    pub(crate) fn is_computation(&self) -> bool {
        matches!(self, NodeKind::Memo { .. } | NodeKind::Effect { .. })
    }
}

pub(crate) struct Node {
    pub kind: NodeKind,
    pub state: NodeState,

    // --- dependency edges (computations read; signals/memos are read) ---
    /// What this node read during its last run (computations only).
    pub sources: Vec<Key>,
    /// `source_slots[k]` = index of *this* node inside `sources[k].observers`.
    pub source_slots: Vec<u32>,
    /// Who read this node (signals and memos only).
    pub observers: Vec<Key>,
    /// `observer_slots[j]` = index of *this* node inside `observers[j].sources`.
    pub observer_slots: Vec<u32>,

    // --- ownership tree ---
    /// Owner (the scope/computation this node was created under). The
    /// context system walks this upward (`use_context`); it mirrors the
    /// `owned` edge exactly.
    pub parent: Option<Key>,
    /// Children in creation order: disposed in reverse before this node.
    pub owned: Vec<Key>,
    /// Cleanup callbacks, run LIFO on dispose / before re-run.
    pub cleanups: Vec<Box<dyn FnOnce()>>,

    // --- bookkeeping ---
    /// Monotonic creation order; the effect queue flushes in this order so
    /// outer (earlier-created) effects run before inner ones — an outer
    /// re-render may dispose inner effects, which then skip as stale.
    pub order: u64,
    /// True while sitting in the scheduler queue (dedupes enqueue).
    pub queued: bool,
    /// True while its computation is on the call stack (cycle detection).
    pub running: bool,
    /// As an observer: the epoch of its current run (see `track_read`).
    pub run_epoch: u64,
    /// As a source: last epoch in which it recorded an edge (read dedupe).
    pub seen_epoch: u64,
    /// Diagnostic name effects can carry so runaway-loop panics point at
    /// a creation site instead of an anonymous node (RT1-15a).
    pub label: Option<&'static str>,
    /// Runs performed within the flush stamped below; a single effect
    /// exceeding the per-flush ceiling aborts loudly, naming `label`.
    pub flush_runs: u32,
    /// Which flush `flush_runs` counts for (compared to the runtime's
    /// flush epoch; a stale stamp resets the counter).
    pub flush_stamp: u64,
}

impl Node {
    pub(crate) fn new(kind: NodeKind, order: u64) -> Self {
        Node {
            kind,
            // Memos are born Dirty (never computed); everything else Clean.
            state: NodeState::Clean,
            sources: Vec::new(),
            source_slots: Vec::new(),
            observers: Vec::new(),
            observer_slots: Vec::new(),
            parent: None,
            owned: Vec::new(),
            cleanups: Vec::new(),
            order,
            queued: false,
            running: false,
            run_epoch: 0,
            seen_epoch: 0,
            label: None,
            flush_runs: 0,
            flush_stamp: 0,
        }
    }

    /// Human-readable identity for panic/diagnostic messages.
    pub(crate) fn describe(&self, key: super::arena::Key) -> String {
        let kind = match self.kind {
            NodeKind::Scope => "scope",
            NodeKind::Signal { .. } => "signal",
            NodeKind::Memo { .. } => "memo",
            NodeKind::Effect { .. } => "effect",
        };
        match self.label {
            Some(l) => format!("{kind} '{l}' (node #{})", key.index),
            None => format!("{kind} node #{}", key.index),
        }
    }
}

pub(crate) type Graph = GenArena<Node>;

/// Record "observer reads source". Invariant established:
/// `source.observers[j] == observer` with `source.observer_slots[j] == k`
/// and `observer.sources[k] == source` with `observer.source_slots[k] == j`.
pub(crate) fn add_edge(graph: &mut Graph, source: Key, observer: Key) {
    let k = {
        let obs = graph
            .get_mut(observer)
            .expect("add_edge: observer vanished");
        obs.sources.push(source);
        obs.source_slots.push(u32::MAX); // patched below once j is known
        obs.sources.len() - 1
    };
    let j = {
        let src = graph.get_mut(source).expect("add_edge: source vanished");
        src.observers.push(observer);
        src.observer_slots.push(k as u32);
        src.observers.len() - 1
    };
    graph
        .get_mut(observer)
        .expect("add_edge: observer vanished")
        .source_slots[k] = j as u32;
}

/// Unlink every source edge of `observer` (used before a re-run and at
/// disposal). O(1) per edge thanks to the paired slots: after
/// `swap_remove` on the source side, the element that moved into position
/// `j` gets its back-pointer (its own `source_slots` entry) repaired.
pub(crate) fn remove_source_edges(graph: &mut Graph, observer: Key) {
    let pairs: Vec<(Key, u32)> = {
        let Some(obs) = graph.get_mut(observer) else {
            return;
        };
        obs.sources
            .drain(..)
            .zip(obs.source_slots.drain(..))
            .collect()
    };
    for (source, slot) in pairs {
        let j = slot as usize;
        let moved = {
            let Some(src) = graph.get_mut(source) else {
                continue;
            }; // source already disposed
            if j >= src.observers.len() || src.observers[j] != observer {
                continue; // edge already unlinked from the other side
            }
            src.observers.swap_remove(j);
            src.observer_slots.swap_remove(j);
            if j < src.observers.len() {
                Some((src.observers[j], src.observer_slots[j] as usize))
            } else {
                None
            }
        };
        if let Some((moved_obs, k2)) = moved {
            if let Some(o2) = graph.get_mut(moved_obs) {
                if k2 < o2.source_slots.len() {
                    o2.source_slots[k2] = j as u32;
                }
            }
        }
    }
}

/// Unlink every observer edge of `source` (used when disposing a signal or
/// memo that still has readers). Symmetric to `remove_source_edges`.
pub(crate) fn remove_observer_edges(graph: &mut Graph, source: Key) {
    let pairs: Vec<(Key, u32)> = {
        let Some(src) = graph.get_mut(source) else {
            return;
        };
        src.observers
            .drain(..)
            .zip(src.observer_slots.drain(..))
            .collect()
    };
    for (observer, slot) in pairs {
        let k = slot as usize;
        let moved = {
            let Some(obs) = graph.get_mut(observer) else {
                continue;
            };
            if k >= obs.sources.len() || obs.sources[k] != source {
                continue;
            }
            obs.sources.swap_remove(k);
            obs.source_slots.swap_remove(k);
            if k < obs.sources.len() {
                Some((obs.sources[k], obs.source_slots[k] as usize))
            } else {
                None
            }
        };
        if let Some((moved_src, j2)) = moved {
            if let Some(s2) = graph.get_mut(moved_src) {
                if j2 < s2.observer_slots.len() {
                    s2.observer_slots[j2] = k as u32;
                }
            }
        }
    }
}

/// Test-only invariant sweep: every edge must be paired correctly from
/// both sides. Kept compiled under cfg(test) so behavior suites can call
/// it after churn-heavy scenarios.
#[cfg(test)]
pub(crate) fn check_edge_invariants(graph: &Graph, keys: &[Key]) {
    for &key in keys {
        let Some(node) = graph.get(key) else { continue };
        assert_eq!(node.sources.len(), node.source_slots.len());
        assert_eq!(node.observers.len(), node.observer_slots.len());
        for (k, (&src, &slot)) in node.sources.iter().zip(&node.source_slots).enumerate() {
            let s = graph.get(src).expect("dangling source");
            let j = slot as usize;
            assert_eq!(s.observers[j], key, "observer back-pointer broken");
            assert_eq!(s.observer_slots[j] as usize, k, "slot pairing broken");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_node(graph: &mut Graph, order: u64) -> Key {
        graph.insert(Node::new(NodeKind::Scope, order))
    }

    #[test]
    fn edge_add_remove_repairs_slots() {
        let mut g = Graph::new();
        let s1 = scope_node(&mut g, 0);
        let s2 = scope_node(&mut g, 1);
        let o1 = scope_node(&mut g, 2);
        let o2 = scope_node(&mut g, 3);
        // o1 reads s1, s2; o2 reads s1.
        add_edge(&mut g, s1, o1);
        add_edge(&mut g, s2, o1);
        add_edge(&mut g, s1, o2);
        check_edge_invariants(&g, &[s1, s2, o1, o2]);
        // Removing o1's edges forces the swap_remove fixup on s1 (o2 moves
        // from slot 1 to slot 0).
        remove_source_edges(&mut g, o1);
        check_edge_invariants(&g, &[s1, s2, o1, o2]);
        assert!(g.get(o1).unwrap().sources.is_empty());
        assert_eq!(g.get(s1).unwrap().observers, vec![o2]);
        assert!(g.get(s2).unwrap().observers.is_empty());
        // And source-side removal repairs the observer side.
        remove_observer_edges(&mut g, s1);
        check_edge_invariants(&g, &[s1, s2, o1, o2]);
        assert!(g.get(o2).unwrap().sources.is_empty());
    }
}
