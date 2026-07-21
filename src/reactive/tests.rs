//! Behavioral suite for the reactive runtime: the REDTEAM-facing
//! contracts. Every test here documents a guarantee the engine makes.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

/// Shared run-log for observing effect executions.
fn log() -> (Rc<RefCell<Vec<i64>>>, impl Fn(i64) + Clone + 'static) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let l2 = log.clone();
    (log, move |v| l2.borrow_mut().push(v))
}

#[test]
fn diamond_runs_leaf_once_per_write() {
    // a -> b, a -> c, (b, c) -> d: the classic glitch case. d must run
    // exactly once per write to a and must never observe b and c derived
    // from different generations of a.
    let (runs, push) = log();
    let (root, ()) = create_root(|cx| {
        let a = cx.signal(1i64);
        let b = cx.memo(move || a.get() * 10);
        let c = cx.memo(move || a.get() + 1);
        cx.effect(move || {
            let b_v = b.get();
            let c_v = c.get();
            // Consistency: both derive from the same `a`.
            assert_eq!(
                b_v / 10,
                c_v - 1,
                "glitch: b and c from different generations"
            );
            push(b_v + c_v);
        });
        assert_eq!(*runs.borrow(), vec![10 + 2]);
        a.set(2);
        assert_eq!(
            *runs.borrow(),
            vec![12, 20 + 3],
            "exactly one re-run per write"
        );
        a.set(3);
        assert_eq!(runs.borrow().len(), 3);
    });
    root.dispose();
}

#[test]
fn deep_diamond_converges_once() {
    // Two memo layers between the signal and the effect, converging twice.
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let a = cx.signal(0i64);
        let l = cx.memo(move || a.get() + 1);
        let r = cx.memo(move || a.get() + 2);
        let m = cx.memo(move || l.get() * r.get());
        let r2 = cx.memo(move || r.get() * 100);
        cx.effect(move || push(m.get() + r2.get()));
        assert_eq!(runs.borrow().len(), 1);
        a.set(5);
        assert_eq!(runs.borrow().len(), 2, "one re-run despite four paths");
        assert_eq!(*runs.borrow().last().unwrap(), (6 * 7) + 700);
    });
}

#[test]
fn memo_equality_cutoff_stops_propagation() {
    let (runs, push) = log();
    let memo_runs = Rc::new(RefCell::new(0));
    let (_root, ()) = create_root(|cx| {
        let n = cx.signal(1i64);
        let mr = memo_runs.clone();
        let parity = cx.memo(move || {
            *mr.borrow_mut() += 1;
            n.get() % 2
        });
        cx.effect(move || push(parity.get()));
        assert_eq!(runs.borrow().len(), 1);
        assert_eq!(*memo_runs.borrow(), 1);
        n.set(3); // parity unchanged: memo recomputes, effect must NOT run
        assert_eq!(*memo_runs.borrow(), 2, "memo recomputes (dirty + observed)");
        assert_eq!(
            runs.borrow().len(),
            1,
            "cut-off: equal value must not propagate"
        );
        n.set(4); // parity flips: effect runs
        assert_eq!(runs.borrow().len(), 2);
        assert_eq!(*runs.borrow(), vec![1, 0]);
    });
}

#[test]
fn set_if_changed_cuts_at_the_signal() {
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let n = cx.signal(7i64);
        cx.effect(move || push(n.get()));
        assert!(!n.set_if_changed(7), "equal write reports no change");
        assert_eq!(runs.borrow().len(), 1);
        assert!(n.set_if_changed(8));
        assert_eq!(*runs.borrow(), vec![7, 8]);
    });
}

#[test]
fn memo_is_lazy_until_observed() {
    let computes = Rc::new(RefCell::new(0));
    let (_root, ()) = create_root(|cx| {
        let a = cx.signal(1i64);
        let c2 = computes.clone();
        let m = cx.memo(move || {
            *c2.borrow_mut() += 1;
            a.get() * 2
        });
        assert_eq!(*computes.borrow(), 0, "no observation, no compute");
        a.set(2);
        a.set(3);
        assert_eq!(*computes.borrow(), 0, "writes alone never force a compute");
        assert_eq!(m.get_untracked(), 6);
        assert_eq!(*computes.borrow(), 1);
        assert_eq!(m.get_untracked(), 6);
        assert_eq!(*computes.borrow(), 1, "clean memo serves the cache");
    });
}

#[test]
fn batch_coalesces_writes_into_one_flush() {
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let x = cx.signal(1i64);
        let y = cx.signal(10i64);
        cx.effect(move || push(x.get() + y.get()));
        assert_eq!(*runs.borrow(), vec![11]);
        batch(|| {
            x.set(2);
            y.set(20);
            assert_eq!(runs.borrow().len(), 1, "no effect runs inside the batch");
        });
        assert_eq!(*runs.borrow(), vec![11, 22], "one run, final values only");
        // Nested batches flush once, at the outermost close.
        batch(|| {
            x.set(3);
            batch(|| y.set(30));
            assert_eq!(runs.borrow().len(), 2);
        });
        assert_eq!(*runs.borrow(), vec![11, 22, 33]);
    });
}

#[test]
fn effects_flush_in_creation_order() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let (_root, ()) = create_root(|cx| {
        let s = cx.signal(0i64);
        for tag in 0..4i64 {
            let o = order.clone();
            cx.effect(move || {
                s.get();
                o.borrow_mut().push(tag);
            });
        }
        order.borrow_mut().clear();
        s.set(1);
        assert_eq!(
            *order.borrow(),
            vec![0, 1, 2, 3],
            "creation order, deterministic"
        );
    });
}

#[test]
fn disposal_runs_cleanups_children_first_lifo() {
    let trace: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let (root, ()) = create_root(|cx| {
        let t = trace.clone();
        cx.on_cleanup(move || t.borrow_mut().push("root-a"));
        let t = trace.clone();
        cx.on_cleanup(move || t.borrow_mut().push("root-b"));
        let child = cx.child();
        let t = trace.clone();
        child.on_cleanup(move || t.borrow_mut().push("child"));
        let grand = child.child();
        let t = trace.clone();
        grand.on_cleanup(move || t.borrow_mut().push("grandchild"));
        let sibling = cx.child();
        let t = trace.clone();
        sibling.on_cleanup(move || t.borrow_mut().push("sibling"));
    });
    root.dispose();
    // Children before parents; siblings in reverse creation order;
    // within one node LIFO (root-b before root-a).
    assert_eq!(
        *trace.borrow(),
        vec!["sibling", "grandchild", "child", "root-b", "root-a"]
    );
}

#[test]
fn effect_cleanup_runs_before_each_rerun_and_at_dispose() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let (root, ()) = create_root(|cx| {
        let s = cx.signal(0i64);
        let t = trace.clone();
        cx.effect(move || {
            let v = s.get();
            t.borrow_mut().push(format!("run {v}"));
            let t2 = t.clone();
            on_cleanup(move || t2.borrow_mut().push(format!("clean {v}")));
        });
        s.set(1);
        s.set(2);
    });
    root.dispose();
    assert_eq!(
        *trace.borrow(),
        vec!["run 0", "clean 0", "run 1", "clean 1", "run 2", "clean 2"],
        "cleanup interleaves before each re-run and fires at disposal"
    );
}

#[test]
fn disposal_frees_slots_and_leaves_no_nodes() {
    let baseline = stats().live_nodes;
    let (root, ()) = create_root(|cx| {
        let a = cx.signal(1i64);
        let m = cx.memo(move || a.get() + 1);
        cx.effect(move || {
            m.get();
        });
        let child = cx.child();
        child.signal("hello".to_string());
    });
    assert_eq!(
        stats().live_nodes,
        baseline + 6,
        "root + signal + memo + effect + child + child-signal"
    );
    root.dispose();
    assert_eq!(stats().live_nodes, baseline, "dispose must free every slot");
}

#[test]
fn stale_handles_after_dispose_are_detected() {
    let (root, sig) = create_root(|cx| cx.signal(5i64));
    assert!(sig.is_alive());
    root.dispose();
    assert!(!sig.is_alive());
    let poked = std::panic::catch_unwind(|| sig.get());
    assert!(
        poked.is_err(),
        "reading a disposed signal must panic loudly"
    );
}

#[test]
fn no_leak_after_10k_create_dispose_cycles() {
    let baseline = stats();
    for i in 0..10_000i64 {
        let (root, ()) = create_root(|cx| {
            let s = cx.signal(i);
            let m = cx.memo(move || s.get() * 2);
            cx.effect(move || {
                m.get();
            });
            s.set(i + 1);
        });
        root.dispose();
    }
    let after = stats();
    assert_eq!(
        after.live_nodes, baseline.live_nodes,
        "nodes leaked across cycles"
    );
    assert_eq!(after.queued_effects, 0, "queue leaked stale entries");
    // Slot storage is bounded by peak concurrent liveness (4 nodes here),
    // never by total churn.
    assert!(
        after.slot_capacity <= baseline.slot_capacity + 8,
        "arena grew with churn: {} -> {}",
        baseline.slot_capacity,
        after.slot_capacity
    );
}

#[test]
fn dyn_style_rerun_disposes_previous_children() {
    // The ui::Dyn pattern: an effect that rebuilds a child scope per run.
    // Prior children must be gone (nodes freed) after each re-run.
    let baseline = stats().live_nodes;
    let (root, ()) = create_root(|cx| {
        let version = cx.signal(0i64);
        let holder: Rc<RefCell<Option<Scope>>> = Rc::new(RefCell::new(None));
        let outer = cx;
        cx.effect(move || {
            let v = version.get();
            if let Some(old) = holder.borrow_mut().take() {
                old.dispose();
            }
            let child = outer.child();
            // Simulated subtree: a couple of nodes per render generation.
            let s = child.signal(v);
            child.memo(move || s.get() + 1);
            *holder.borrow_mut() = Some(child);
        });
        let after_first = stats().live_nodes;
        for i in 1..=100 {
            version.set(i);
        }
        assert_eq!(
            stats().live_nodes,
            after_first,
            "re-renders must not accumulate nodes"
        );
    });
    root.dispose();
    assert_eq!(stats().live_nodes, baseline);
}

#[test]
fn untracked_reads_do_not_subscribe() {
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let hot = cx.signal(0i64);
        let cold = cx.signal(100i64);
        cx.effect(move || push(hot.get() + cold.get_untracked()));
        cold.set(200); // no subscription -> no re-run
        assert_eq!(runs.borrow().len(), 1);
        hot.set(1); // re-run sees the fresh untracked value
        assert_eq!(*runs.borrow(), vec![100, 201]);
    });
}

#[test]
fn dynamic_dependencies_retrack_each_run() {
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let use_left = cx.signal(true);
        let left = cx.signal(1i64);
        let right = cx.signal(100i64);
        cx.effect(move || {
            push(if use_left.get() {
                left.get()
            } else {
                right.get()
            })
        });
        right.set(200); // not currently a dependency
        assert_eq!(runs.borrow().len(), 1, "unused branch must not trigger");
        use_left.set(false); // switch branches
        assert_eq!(*runs.borrow(), vec![1, 200]);
        left.set(2); // now LEFT is the unused branch
        assert_eq!(runs.borrow().len(), 2, "stale edge survived a retrack");
        right.set(300);
        assert_eq!(*runs.borrow(), vec![1, 200, 300]);
    });
}

#[test]
fn writes_inside_effects_settle_within_one_flush() {
    // An effect cascading into another signal (no cycle): both effects
    // settle in the same flush, exactly once each per external write.
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let a = cx.signal(1i64);
        let b = cx.signal(0i64);
        cx.effect(move || b.set(a.get() * 10));
        cx.effect(move || push(b.get()));
        assert_eq!(*runs.borrow(), vec![10]);
        a.set(2);
        assert_eq!(*runs.borrow(), vec![10, 20]);
    });
}

#[test]
fn signal_used_from_wrong_thread_panics_not_aliases() {
    let (_root, sig) = create_root(|cx| cx.signal(1i64));
    let result = std::thread::spawn(move || std::panic::catch_unwind(|| sig.get()))
        .join()
        .expect("thread join");
    assert!(
        result.is_err(),
        "cross-thread use must panic, never alias another runtime"
    );
}

#[test]
fn posted_closures_drive_signals_from_other_threads() {
    let (runs, push) = log();
    let (_root, ()) = create_root(|cx| {
        let s = cx.signal(0i64);
        cx.effect(move || push(s.get()));
        let handle = wake_handle();
        std::thread::spawn(move || {
            handle.post(move || s.set(42)); // executes on the UI thread
        })
        .join()
        .expect("thread join");
        assert_eq!(
            runs.borrow().len(),
            1,
            "nothing runs until the UI thread drains"
        );
        drain_posted();
        assert_eq!(*runs.borrow(), vec![0, 42]);
    });
}

#[test]
fn reactive_cycle_panics_with_diagnosis() {
    let caught = std::panic::catch_unwind(|| {
        let (_root, ()) = create_root(|cx| {
            let s = cx.signal(0i64);
            // Effect that writes its own dependency: must be caught, not
            // spin forever.
            cx.effect(move || s.set(s.get() + 1));
        });
    });
    assert!(caught.is_err(), "self-feeding effect must be detected");
}

#[test]
fn draw_phase_tracked_read_panics_in_debug() {
    // RT1-2: a tracked read while phase D runs is the stale-pixel bug.
    let (_root, sig) = create_root(|cx| cx.signal(1i64));
    let caught = std::panic::catch_unwind(|| {
        let _phase = enter_draw_phase();
        sig.get() // tracked read in draw: debug panic
    });
    assert!(caught.is_err(), "tracked read during draw must be loud");
    // The guard's Drop ran during unwind: we are OUT of the draw phase,
    // and normal reads work again.
    assert_eq!(sig.get(), 1);
}

#[test]
fn draw_phase_untracked_reads_and_nested_computations_are_fine() {
    let (_root, (sig, memo)) = create_root(|cx| {
        let s = cx.signal(2i64);
        let m = cx.memo(move || s.get() * 10);
        (s, m)
    });
    {
        let _phase = enter_draw_phase();
        // Sanctioned: untracked peeks at captured data.
        assert_eq!(sig.get_untracked(), 2);
        // Sanctioned: a memo recomputing during draw reads its sources
        // under its OWN observer context — graph maintenance, not a
        // widget read. (The memo is pulled untracked here.)
        assert_eq!(memo.get_untracked(), 20);
    }
    assert_eq!(sig.get(), 2, "draw phase ends with the guard");
}

#[test]
fn runaway_effect_pair_panics_naming_the_label() {
    // RT1-15a: two effects rewriting each other's dependencies never
    // settle; the per-effect counter must abort quickly AND name a label
    // (the self-feeding single-effect case is caught even earlier by the
    // running-flag cycle check — see reactive_cycle_panics_with_diagnosis).
    // The storm must start from an EXTERNAL write: a cascade that loops
    // back into an effect still on the call stack (e.g. during its
    // creation run) trips the running-flag re-entry check instead. Armed
    // via a gate signal so creation settles cleanly first.
    let caught = std::panic::catch_unwind(|| {
        let (_root, ()) = create_root(|cx| {
            let armed = cx.signal(false);
            let a = cx.signal(0i64);
            let b = cx.signal(0i64);
            cx.effect_labeled("ping", move || {
                let v = a.get();
                if armed.get() {
                    b.set(v + 1);
                }
            });
            cx.effect_labeled("pong", move || {
                let v = b.get();
                if armed.get() {
                    a.set(v + 1);
                }
            });
            armed.set(true); // both effects re-run and start rewriting each other
        });
    });
    let err = caught.expect_err("ping-pong effects must be detected");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_else(|| {
        err.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .unwrap_or_default()
    });
    assert!(
        msg.contains("'ping'") || msg.contains("'pong'"),
        "panic must name a culprit label, got: {msg}"
    );
}

#[test]
fn spawned_worker_panic_surfaces_as_labeled_failure() {
    // RT1-15b: a dead worker must not be silence.
    let _ = take_worker_failures(); // isolate from other tests on this thread
    let handle = spawn_worker("thumbnail-decoder", || panic!("decode failed: bad header"));
    handle
        .join()
        .expect("worker thread itself must not propagate");
    // The failure arrives as a posted job; drain it on the UI thread.
    drain_posted();
    let failures = take_worker_failures();
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].contains("thumbnail-decoder"),
        "label present: {failures:?}"
    );
    assert!(
        failures[0].contains("decode failed"),
        "message present: {failures:?}"
    );
    assert_eq!(
        diagnostics().pending_worker_failures,
        0,
        "take drained them"
    );
}

#[test]
fn edge_churn_preserves_pairing_invariants() {
    use super::node::check_edge_invariants;
    use super::runtime::with_rt;
    let (_root, keys) = create_root(|cx| {
        let signals: Vec<_> = (0..8).map(|i| cx.signal(i as i64)).collect();
        let toggle = cx.signal(0usize);
        let sig_reads = signals.clone();
        cx.effect(move || {
            // Read a rotating, overlapping subset: heavy edge churn.
            let t = toggle.get();
            for s in sig_reads.iter().skip(t % 4).take(4) {
                s.get();
            }
        });
        for i in 1..40 {
            toggle.set(i);
        }
        let mut keys: Vec<_> = signals.iter().map(|s| s.id.key).collect();
        keys.push(toggle.id.key);
        keys
    });
    with_rt(|rt| check_edge_invariants(&rt.graph, &keys));
}

// ---------------------------------------------------------------------------
// Context (provide/use) — cycle 7.
// ---------------------------------------------------------------------------

#[test]
fn context_flows_down_shadows_and_dies_with_its_scope() {
    #[derive(Clone, PartialEq, Debug)]
    struct AppConfig(&'static str);

    let (root, ()) = create_root(|cx| {
        assert_eq!(cx.use_context::<AppConfig>(), None, "nothing provided yet");
        cx.provide_context(AppConfig("root"));
        assert_eq!(
            cx.use_context::<AppConfig>(),
            Some(AppConfig("root")),
            "own scope"
        );

        let child = cx.child();
        assert_eq!(
            child.use_context::<AppConfig>(),
            Some(AppConfig("root")),
            "descendants inherit"
        );

        // Shadowing: a nested provide wins for ITS subtree only.
        let branch = cx.child();
        branch.provide_context(AppConfig("branch"));
        let leaf = branch.child();
        assert_eq!(leaf.use_context::<AppConfig>(), Some(AppConfig("branch")));
        assert_eq!(child.use_context::<AppConfig>(), Some(AppConfig("root")));

        // Distinct types coexist on one scope.
        cx.provide_context(42i64);
        assert_eq!(leaf.use_context::<i64>(), Some(42));

        // A disposed provider's context is GONE (the side map entry is
        // removed with the scope, not leaked).
        branch.dispose();
        let after = cx.child();
        assert_eq!(after.use_context::<AppConfig>(), Some(AppConfig("root")));
    });
    root.dispose();
}

#[test]
fn context_signal_is_the_shared_store_pattern() {
    // The documented store convention: provide a Signal, every consumer
    // reads/writes the SAME state without prop drilling.
    let (root, ()) = create_root(|cx| {
        let count = cx.signal(0i32);
        cx.provide_context(count);
        let consumer = cx.child();
        let got: Signal<i32> = consumer.use_context().expect("provided");
        got.set(5);
        assert_eq!(count.get_untracked(), 5, "one shared signal, two doors");
    });
    root.dispose();
}

#[test]
fn try_get_untracked_is_inert_after_disposal() {
    let (root, ()) = create_root(|cx| {
        let child = cx.child();
        let s = child.signal(7i32);
        assert_eq!(s.try_get_untracked(), Some(7));
        child.dispose();
        assert_eq!(
            s.try_get_untracked(),
            None,
            "disposed reads answer None, never panic"
        );
        assert!(!s.is_alive());
    });
    root.dispose();
}
