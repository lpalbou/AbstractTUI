//! The thread-local reactive runtime: node creation, dependency tracking,
//! two-phase invalidation, lazy recomputation, ownership disposal and the
//! effect flush loop.
//!
//! ## Why single-threaded + thread-local
//!
//! The graph lives in ONE `thread_local` and is never shared across
//! threads. Terminal output is inherently serialized (one byte stream),
//! so a `Send + Sync` graph would buy nothing except a lock acquisition
//! on *every signal read* — the hottest operation in the system. Instead,
//! timers/IO threads hand work to the UI thread through
//! [`super::scheduler::WakeHandle`] (posted closures + wakeup), which is
//! both cheaper and impossible to deadlock. Handles carry the id of the
//! runtime that minted them; using one on the wrong thread is a loud
//! panic, never silent aliasing.
//!
//! ## Borrow discipline (the one rule that keeps this sound)
//!
//! `with_rt` hands out `&mut Runtime` under a `RefCell` borrow. User code
//! (computations, cleanups, `Drop` impls of stored values) re-enters the
//! runtime, so it must NEVER run under that borrow. Every operation is
//! therefore structured as: borrow -> mutate graph -> collect closures /
//! `Rc`s -> release -> run user code -> repeat. Node payloads are `Rc`ed
//! precisely so they can be cloned out before the borrow is released.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::base::FrameRequester;

use super::arena::Key;
use super::execute::update_if_necessary;
use super::node::{
    add_edge, remove_observer_edges, remove_source_edges, Graph, Node, NodeKind, NodeState,
};
use super::scheduler::RemoteShared;

/// Hard ceiling on TOTAL effect executions in one flush — the backstop
/// against many effects ping-ponging collectively.
const MAX_FLUSH_RUNS: usize = 100_000;

/// Ceiling on ONE effect's executions within a single flush (RT1-15a).
/// This fires long before the global ceiling (1k runs instead of 100k),
/// and the panic names the effect's creation label — a runaway loop
/// should cost milliseconds and produce a culprit, not seconds and a
/// mystery.
const MAX_RUNS_PER_EFFECT_PER_FLUSH: u32 = 1_000;

/// How many draw-phase violation descriptions are retained for the
/// diagnostics getter in release builds (the count is unbounded).
const DRAW_VIOLATION_SAMPLE_CAP: usize = 8;

// Panic messages NAME THE FIX (cycle-8 audit): a user hitting one at
// 2am should know what to change without opening this file.
pub(crate) const MSG_DISPOSED: &str =
    "abstracttui reactive: handle used after its node was disposed. FIX: keep the owning \
     scope alive as long as the handle (state a Dyn rebuilds belongs OUTSIDE its closure — \
     see dyn_view vs dyn_view_scoped), or use Signal::try_get_untracked where 'gone' is a \
     valid answer";
pub(crate) const MSG_WRONG_THREAD: &str =
    "abstracttui reactive: handle used on a thread that did not create it. FIX: the reactive \
     graph is single-threaded by design — send data to the UI thread with spawn_worker/post \
     (reactive::remote) and write signals from the posted closure, never from the worker";
pub(crate) const MSG_CYCLE: &str =
    "abstracttui reactive: dependency cycle — a computation re-entered itself while running. \
     FIX: a memo/effect (transitively) reads its own output; break the loop by reading the \
     input with get_untracked or splitting the state into two signals";
pub(crate) const MSG_DRAW_READ: &str =
    "tracked signal read inside a DRAW closure — the region will never repaint when this \
     value changes (RT1-2). FIX: move the read into a dyn_view (re-renders on change) or \
     capture the value before the closure; use get_untracked for a deliberate stale peek";

/// A `Key` stamped with the id of the runtime (thread) that owns it.
/// Handles are `Copy + Send` so cross-thread *transport* (e.g. inside a
/// posted closure) is allowed; cross-thread *use* fails the stamp check.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RawId {
    pub key: Key,
    pub rt: u32,
}

pub(crate) struct Runtime {
    pub graph: Graph,
    pub rt_id: u32,
    /// Node that owns whatever is created right now (scope or computation).
    pub current_owner: Option<Key>,
    /// Computation whose dependencies are being tracked right now.
    /// `None` inside `untrack` and outside any computation.
    pub current_observer: Option<Key>,
    /// Bumped at every computation run; used to dedupe repeated reads of
    /// the same source within one run in O(1).
    pub epoch_counter: u64,
    pub order_counter: u64,
    pub batch_depth: u32,
    pub flushing: bool,
    /// Effects awaiting execution (dirty or check — resolved at flush).
    pub queue: Vec<Key>,
    pub remote: Arc<RemoteShared>,
    pub frame_requested: bool,
    pub frame_requester: Option<Rc<dyn FrameRequester>>,
    /// Nonzero while the frame's DRAW phase runs (nesting-safe). Tracked
    /// reads while set are the RT1-2 stale-pixel bug: no computation
    /// owns the region, so nothing ever re-renders it.
    pub draw_depth: u32,
    /// Total draw-phase tracked-read violations (release builds count
    /// instead of panicking; debug builds never get here).
    pub draw_read_violations: u64,
    /// First few violation descriptions, for the diagnostics getter.
    pub draw_read_samples: Vec<String>,
    /// Bumped once per `flush_effects` entry; per-effect run counters
    /// reset lazily by comparing their stamp against this.
    pub flush_epoch: u64,
    /// Labeled panics reported by workers spawned via
    /// `scheduler::spawn_worker` (RT1-15b), delivered as posted jobs.
    pub worker_failures: Vec<String>,
    /// Per-frame callbacks (animations). Run once per frame in phase U;
    /// a task returning false is dropped. Empty list = zero idle cost.
    pub frame_tasks: Vec<Box<dyn FnMut(std::time::Instant) -> bool>>,
    /// One-shot timers (toast dismissal, debounce). Unlike frame tasks
    /// these do NOT keep frames coming: the loop sleeps until the
    /// earliest deadline (`next_timer_deadline`) — zero wakeups before.
    pub timers: Vec<(std::time::Instant, Box<dyn FnOnce()>)>,
    /// Scope-provided context values (`provide_context`/`use_context`):
    /// sparse side map (only providers pay), removed on dispose.
    pub contexts: std::collections::HashMap<Key, ContextEntries>,
}

static NEXT_RT_ID: AtomicU32 = AtomicU32::new(1);

impl Runtime {
    fn new() -> Self {
        Runtime {
            graph: Graph::new(),
            rt_id: NEXT_RT_ID.fetch_add(1, Ordering::Relaxed),
            current_owner: None,
            current_observer: None,
            epoch_counter: 0,
            order_counter: 0,
            batch_depth: 0,
            flushing: false,
            queue: Vec::new(),
            remote: Arc::new(RemoteShared::new()),
            frame_requested: false,
            frame_requester: None,
            draw_depth: 0,
            draw_read_violations: 0,
            draw_read_samples: Vec::new(),
            flush_epoch: 0,
            worker_failures: Vec::new(),
            frame_tasks: Vec::new(),
            timers: Vec::new(),
            contexts: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn check_thread(&self, id: RawId) {
        if id.rt != self.rt_id {
            panic!("{MSG_WRONG_THREAD}");
        }
    }

    /// Create a node, optionally attaching it to an owner scope.
    /// Memos are born `Dirty` (lazy: nothing is computed until observed).
    pub(crate) fn create_node(&mut self, owner: Option<Key>, kind: NodeKind) -> Key {
        self.order_counter += 1;
        let mut node = Node::new(kind, self.order_counter);
        if matches!(node.kind, NodeKind::Memo { .. }) {
            node.state = NodeState::Dirty;
        }
        node.parent = owner;
        let key = self.graph.insert(node);
        if let Some(o) = owner {
            if !self.graph.contains(o) {
                // Creating under a dead scope would leak the node forever
                // (nobody left to dispose it) — refuse loudly.
                self.graph.remove(key);
                panic!(
                    "abstracttui reactive: node created under a disposed scope. FIX: the \
                     scope you captured died (a Dyn generation, a closed modal); create \
                     state on a scope that outlives the use — the mount scope for durable \
                     state, the generation scope (dyn_view_scoped) for per-render state"
                );
            }
            let needs_sweep = {
                let onode = self.graph.get_mut(o).expect("checked above");
                onode.owned.push(key);
                // Explicitly-disposed children leave stale keys behind; sweep
                // them amortized (every power-of-two growth past 32) so a
                // long-lived scope with heavy child churn stays bounded
                // without paying O(siblings) on every dispose.
                onode.owned.len() >= 32 && onode.owned.len().is_power_of_two()
            };
            if needs_sweep {
                let owned = std::mem::take(&mut self.graph.get_mut(o).expect("owner").owned);
                let filtered: Vec<Key> = owned
                    .into_iter()
                    .filter(|k| self.graph.contains(*k))
                    .collect();
                self.graph.get_mut(o).expect("owner").owned = filtered;
            }
        }
        key
    }

    /// A tracked read arrived while phase D was running: debug builds
    /// panic naming the offending node (loud during development, when the
    /// widget author is looking); release builds count + keep a bounded
    /// sample so `diagnostics()` can surface the label without killing a
    /// shipped app over a stale region.
    fn report_draw_read(&mut self, source: Key) {
        let who = self
            .graph
            .get(source)
            .map(|n| n.describe(source))
            .unwrap_or_else(|| "disposed node".to_string());
        if cfg!(debug_assertions) {
            panic!("abstracttui reactive: {MSG_DRAW_READ}; offending read: {who}");
        }
        self.draw_read_violations += 1;
        if self.draw_read_samples.len() < DRAW_VIOLATION_SAMPLE_CAP {
            self.draw_read_samples
                .push(format!("#FALLBACK {MSG_DRAW_READ}; offending read: {who}"));
        }
    }

    /// Record `current_observer reads source`.
    ///
    /// Dedupe subtlety (REDTEAM: this is diamond-country): a single global
    /// epoch is NOT enough. If memo B is pulled in the middle of memo A's
    /// run, B's run overwrites `seen_epoch` on shared sources; back in A, a
    /// pure epoch check would either miss the dedupe or (worse, if the
    /// check were `seen == current_global`) silently skip adding A's edge.
    /// So: epoch hit => certainly already added this run, done. Epoch miss
    /// => fall back to scanning this run's source list before linking.
    pub(crate) fn track_read(&mut self, source: Key) {
        // RT1-2: a tracked read from a DRAW closure is the stale-pixel bug
        // — draw runs outside any computation, so the read subscribes
        // nothing and the region never repaints when the value changes.
        // `current_observer.is_none()` identifies exactly that case: a
        // memo legitimately recomputed during draw (via an untracked pull)
        // reads its own sources under `observer = the memo`, which is
        // graph maintenance, not a widget read. Untracked reads
        // (`get_untracked`) never reach this function and remain the
        // sanctioned way to peek at data captured for painting.
        if self.draw_depth > 0 && self.current_observer.is_none() {
            self.report_draw_read(source);
            return; // nothing to subscribe anyway (no observer)
        }
        let Some(observer) = self.current_observer else {
            return;
        };
        let obs_epoch = match self.graph.get(observer) {
            Some(n) => n.run_epoch,
            None => return, // observer disposed mid-run (pathological); drop the edge
        };
        {
            let Some(src) = self.graph.get_mut(source) else {
                panic!("{MSG_DISPOSED}");
            };
            if src.seen_epoch == obs_epoch {
                return;
            }
            src.seen_epoch = obs_epoch;
        }
        let duplicate = self
            .graph
            .get(observer)
            .map(|o| o.sources.contains(&source))
            .unwrap_or(true);
        if !duplicate {
            add_edge(&mut self.graph, source, observer);
        }
    }

    /// Down-phase of the two-phase marking: direct observers of a written
    /// value become `Dirty`, transitive observers become `Check`. Only a
    /// node's FIRST transition away from `Clean` descends — its downstream
    /// was already marked then, which is what makes repeated writes cheap
    /// and update storms linear in the affected subgraph.
    ///
    /// Iterative on purpose: fanout chains can be deep and the native
    /// stack is not ours to burn during a storm.
    pub(crate) fn mark_written(&mut self, source: Key) {
        let mut stack: Vec<(Key, NodeState)> = match self.graph.get(source) {
            Some(n) => n.observers.iter().map(|&o| (o, NodeState::Dirty)).collect(),
            None => return,
        };
        while let Some((id, level)) = stack.pop() {
            let Some(node) = self.graph.get_mut(id) else {
                continue;
            };
            if node.state >= level {
                continue;
            }
            let was_clean = node.state == NodeState::Clean;
            node.state = level;
            if node.kind.is_effect() && !node.queued {
                node.queued = true;
                self.queue.push(id);
            }
            if was_clean {
                for &o in node.observers.clone().iter() {
                    stack.push((o, NodeState::Check));
                }
            }
        }
    }

    /// After a memo recomputed to a DIFFERENT value: its direct observers
    /// are definitely stale. No descend — the original down-phase already
    /// marked everything transitively `Check`; this only upgrades the
    /// certainty of the first hop (the equality cut-off gate).
    pub(crate) fn mark_direct_observers_dirty(&mut self, source: Key) {
        let observers: Vec<Key> = match self.graph.get(source) {
            Some(n) => n.observers.clone(),
            None => return,
        };
        for id in observers {
            if let Some(node) = self.graph.get_mut(id) {
                if node.state < NodeState::Dirty {
                    node.state = NodeState::Dirty;
                }
                // Defensive: an effect that attached mid-flush may not be
                // queued yet.
                if node.kind.is_effect() && !node.queued {
                    node.queued = true;
                    self.queue.push(id);
                }
            }
        }
    }

    /// Detach + free `root` and its whole ownership subtree. Cleanups are
    /// COLLECTED here (under the borrow) and RUN by the caller (outside
    /// it); freed `Node`s ride along so their user payloads drop outside
    /// the borrow too (a stored value's `Drop` may re-enter the runtime).
    ///
    /// Order invariant: children before parents (reverse creation order
    /// among siblings), cleanups LIFO within a node. Children may hold
    /// references to parent-provided state, so they must die first — same
    /// order leptos's `Owner::cleanup` and Rust drop order use.
    pub(crate) fn collect_dispose(&mut self, root: Key, out: &mut DisposeBundle) {
        enum Phase {
            Enter(Key),
            Finish(Key),
        }
        let mut stack = vec![Phase::Enter(root)];
        while let Some(phase) = stack.pop() {
            match phase {
                Phase::Enter(id) => {
                    let Some(node) = self.graph.get_mut(id) else {
                        continue;
                    };
                    let owned = std::mem::take(&mut node.owned);
                    stack.push(Phase::Finish(id));
                    // Push in creation order so the LIFO stack visits the
                    // most recently created child first.
                    for c in owned {
                        stack.push(Phase::Enter(c));
                    }
                }
                Phase::Finish(id) => {
                    if let Some(node) = self.graph.get_mut(id) {
                        let mut cleanups = std::mem::take(&mut node.cleanups);
                        cleanups.reverse(); // LIFO
                        out.cleanups.extend(cleanups);
                    }
                    remove_source_edges(&mut self.graph, id);
                    remove_observer_edges(&mut self.graph, id);
                    // Context values provided on this scope die with it
                    // (dropped OUTSIDE the borrow, with the node).
                    if let Some(ctx) = self.contexts.remove(&id) {
                        out.dropped_contexts.push(ctx);
                    }
                    if let Some(node) = self.graph.remove(id) {
                        out.dropped.push(node);
                    }
                }
            }
        }
    }
}

/// One scope's provided context values: (type, boxed value) pairs.
pub(crate) type ContextEntries = Vec<(std::any::TypeId, Rc<dyn std::any::Any>)>;

#[derive(Default)]
pub(crate) struct DisposeBundle {
    pub cleanups: Vec<Box<dyn FnOnce()>>,
    pub dropped: Vec<Node>,
    /// Context values from disposed scopes; dropped after the borrow
    /// releases (a value's Drop may re-enter the runtime).
    pub dropped_contexts: Vec<ContextEntries>,
}

thread_local! {
    static RT: RefCell<Runtime> = RefCell::new(Runtime::new());
}

/// The ONLY way to touch the runtime. Never run user code inside `f`.
pub(crate) fn with_rt<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RT.with(|cell| f(&mut cell.borrow_mut()))
}

/// Panic-safe owner restore for guards living outside this module.
pub(crate) fn restore_owner(prev: Option<Key>) {
    let _ = RT.try_with(|cell| {
        if let Ok(mut rt) = cell.try_borrow_mut() {
            rt.current_owner = prev;
        }
    });
}

/// Panic-safe full context restore after a computation run (owner,
/// observer, `running` flag). Used by `execute::CtxGuard`; a poisoned
/// tracking context after a caught panic would corrupt every later
/// computation, so this must succeed even during unwinding.
pub(crate) fn restore_context(node: Key, prev_owner: Option<Key>, prev_observer: Option<Key>) {
    let _ = RT.try_with(|cell| {
        if let Ok(mut rt) = cell.try_borrow_mut() {
            rt.current_owner = prev_owner;
            rt.current_observer = prev_observer;
            if let Some(n) = rt.graph.get_mut(node) {
                n.running = false;
            }
        }
    });
}

/// Drain the effect queue in creation order until it stays empty.
/// Creation order runs outer effects before the inner effects they own —
/// an outer re-render disposes stale inner effects, which then skip here
/// via the generation check instead of running against dead state.
pub fn flush_effects() {
    let already = with_rt(|rt| {
        if rt.flushing {
            true
        } else {
            rt.flushing = true;
            false
        }
    });
    if already {
        return;
    }
    struct FlushGuard;
    impl Drop for FlushGuard {
        fn drop(&mut self) {
            let _ = RT.try_with(|cell| {
                if let Ok(mut rt) = cell.try_borrow_mut() {
                    rt.flushing = false;
                }
            });
        }
    }
    let _guard = FlushGuard;
    with_rt(|rt| rt.flush_epoch += 1);
    let mut runs: usize = 0;
    loop {
        let mut batch: Vec<(u64, Key)> = with_rt(|rt| {
            let queue = std::mem::take(&mut rt.queue);
            queue
                .into_iter()
                .filter_map(|k| rt.graph.get(k).map(|n| (n.order, k)))
                .collect()
        });
        if batch.is_empty() {
            break;
        }
        batch.sort_unstable_by_key(|(order, _)| *order);
        for (_, id) in batch {
            // RT1-15a: per-effect run accounting. A ping-pong pair (A's
            // effect writes B's dependency and vice versa) trips this at
            // ~1k runs with a NAMED culprit, milliseconds into the storm —
            // long before the global backstop would fire after seconds of
            // frozen UI with no attribution.
            let culprit = with_rt(|rt| {
                let epoch = rt.flush_epoch;
                let node = rt.graph.get_mut(id)?;
                node.queued = false;
                if node.flush_stamp != epoch {
                    node.flush_stamp = epoch;
                    node.flush_runs = 0;
                }
                node.flush_runs += 1;
                (node.flush_runs > MAX_RUNS_PER_EFFECT_PER_FLUSH).then(|| node.describe(id))
            });
            if let Some(who) = culprit {
                panic!(
                    "abstracttui reactive: {who} ran more than \
                     {MAX_RUNS_PER_EFFECT_PER_FLUSH} times in one flush — it (transitively) \
                     rewrites its own dependencies. FIX: read the rewritten signal with \
                     get_untracked inside the effect, or split read/write state; name the \
                     culprit with effect_labeled to trace it"
                );
            }
            runs += 1;
            if runs > MAX_FLUSH_RUNS {
                panic!(
                    "abstracttui reactive: flush did not settle after {MAX_FLUSH_RUNS} effect \
                     runs — some effect chain keeps re-dirtying itself. FIX: find the writer \
                     (label effects with effect_labeled — the per-effect ceiling usually \
                     names it first) and cut its tracked read of what it writes"
                );
            }
            update_if_necessary(id);
        }
    }
}

/// Flush unless writes are being coalesced (batch) or a flush is already
/// draining (its loop will pick up new work).
pub(crate) fn maybe_flush() {
    let should = with_rt(|rt| rt.batch_depth == 0 && !rt.flushing && !rt.queue.is_empty());
    if should {
        flush_effects();
    }
}

/// Coalesce writes: effects observe only the final state, once, when the
/// outermost batch ends. Nesting is allowed.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    with_rt(|rt| rt.batch_depth += 1);
    struct BatchGuard;
    impl Drop for BatchGuard {
        fn drop(&mut self) {
            let _ = RT.try_with(|cell| {
                if let Ok(mut rt) = cell.try_borrow_mut() {
                    rt.batch_depth = rt.batch_depth.saturating_sub(1);
                }
            });
        }
    }
    let result = {
        let _guard = BatchGuard;
        f()
    };
    maybe_flush();
    result
}

/// Run `f` with dependency tracking suspended: reads inside do not
/// subscribe the current computation.
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    let prev = with_rt(|rt| rt.current_observer.take());
    struct UntrackGuard(Option<Key>);
    impl Drop for UntrackGuard {
        fn drop(&mut self) {
            let prev = self.0;
            let _ = RT.try_with(|cell| {
                if let Ok(mut rt) = cell.try_borrow_mut() {
                    rt.current_observer = prev;
                }
            });
        }
    }
    let _guard = UntrackGuard(prev);
    f()
}

/// Register a cleanup on the CURRENTLY RUNNING owner (computation or
/// scope body). Inside an effect this runs before the effect's next
/// re-run and at disposal — the Solid `onCleanup` contract.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    with_rt(|rt| {
        let Some(owner) = rt.current_owner else {
            panic!(
                "abstracttui reactive: on_cleanup called outside any scope or computation. \
                 FIX: call it inside an effect body (cleanup-before-rerun) or use \
                 Scope::on_cleanup(cx, ..) to target a scope explicitly"
            );
        };
        rt.graph
            .get_mut(owner)
            .expect("current owner is always live")
            .cleanups
            .push(Box::new(f));
    });
}

/// Detach + free a node tree; runs cleanups after releasing the borrow.
pub(crate) fn dispose_node(id: RawId) {
    let bundle = with_rt(|rt| {
        rt.check_thread(id);
        let mut bundle = DisposeBundle::default();
        rt.collect_dispose(id.key, &mut bundle);
        bundle
    });
    for c in bundle.cleanups {
        c();
    }
    drop(bundle.dropped);
}

/// Observable counters for leak tests and diagnostics.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RuntimeStats {
    /// Live nodes of any kind (signals, memos, effects, scopes).
    pub live_nodes: usize,
    /// Slots ever allocated — bounded by peak concurrency, not churn.
    pub slot_capacity: usize,
    /// Effects currently queued for the next flush.
    pub queued_effects: usize,
}

pub fn stats() -> RuntimeStats {
    with_rt(|rt| RuntimeStats {
        live_nodes: rt.graph.live(),
        slot_capacity: rt.graph.capacity_slots(),
        queued_effects: rt.queue.len(),
    })
}

/// Panic-safe draw-depth decrement for `diag::DrawPhase`'s Drop.
pub(crate) fn exit_draw_phase() {
    let _ = RT.try_with(|cell| {
        if let Ok(mut rt) = cell.try_borrow_mut() {
            rt.draw_depth = rt.draw_depth.saturating_sub(1);
        }
    });
}
