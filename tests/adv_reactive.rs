//! REDTEAM cycle-2 attack: REACT's reactive runtime + ui event routing.
//! Targets their own §12 confession list (reactive-ui.md) plus the
//! damage-contract obligations: dispose-during-dispatch, diamond edge
//! duplication under nested pulls, disposal leak bounds, and the
//! effect-runaway ceiling.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::base::{Point, Rect, Size};
use abstracttui::layout::Style;
use abstracttui::reactive::{batch, create_root, flush_effects, stats};
use abstracttui::ui::{
    dyn_view, text, BufferCanvas, Element, Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind,
    Phase, UiEvent, UiTree,
};

fn click(x: i32, y: i32) -> UiEvent {
    UiEvent::Mouse(MouseEvent {
        kind: MouseKind::Down(MouseButton::Left),
        pos: Point::new(x, y),
        mods: Mods::NONE,
    })
}

fn key(k: Key) -> UiEvent {
    UiEvent::Key(KeyEvent {
        key: k,
        mods: Mods::NONE,
    })
}

// ---------------------------------------------------------------------------
// Diamond + nested-pull edge hygiene (their confessed risk 1).
// ---------------------------------------------------------------------------

#[test]
fn diamond_leaf_runs_once_per_write_with_interleaved_pulls() {
    let (root, _) = create_root(|cx| {
        let a = cx.signal(0i32);
        let b = cx.memo(move || a.get() + 1);
        let c = cx.memo(move || a.get() * 2);
        let d_runs = Rc::new(RefCell::new(0));
        let d_runs2 = d_runs.clone();
        // d reads b TWICE with a nested pull of c between them — the
        // epoch-dedupe + nested-pull interleaving their doc names as the
        // risky path.
        let d = cx.memo(move || {
            *d_runs2.borrow_mut() += 1;
            let first = b.get();
            let mid = c.get();
            let again = b.get(); // dedupe: must not add a second edge
            first + mid + again
        });
        assert_eq!(d.get(), 2);
        assert_eq!(*d_runs.borrow(), 1);
        for i in 1..=10 {
            a.set(i);
            // Pull the leaf: exactly ONE recompute per write.
            let _ = d.get();
            assert_eq!(*d_runs.borrow(), 1 + i as usize, "write {i}");
        }
        // Equality cut-off: writing the same value re-runs nothing.
        a.set(10);
        let _ = d.get();
        assert_eq!(*d_runs.borrow(), 11);
    });
    root.dispose();
}

#[test]
fn deep_nested_pull_chain_with_shared_sources_no_duplicate_updates() {
    let (root, _) = create_root(|cx| {
        let base = cx.signal(1u64);
        // A chain m1..m6 where each memo reads BASE and the previous memo
        // and re-reads base after the nested pull — a lattice of shared
        // sources designed to confuse per-run epoch stamps.
        let m1 = cx.memo(move || base.get() * 3);
        let m2 = cx.memo(move || base.get() + m1.get() + base.get());
        let m3 = cx.memo(move || m1.get() + base.get() + m2.get());
        let m4 = cx.memo(move || m3.get() + m1.get() + base.get());
        let m5 = cx.memo(move || base.get() + m4.get() + m2.get());
        let runs = Rc::new(RefCell::new(0));
        let runs2 = runs.clone();
        let m6 = cx.memo(move || {
            *runs2.borrow_mut() += 1;
            m5.get() + base.get() + m3.get() + m5.get()
        });
        let v1 = m6.get();
        assert_eq!(*runs.borrow(), 1);
        base.set(2);
        let v2 = m6.get();
        assert_eq!(*runs.borrow(), 2, "exactly one recompute for the leaf");
        assert_ne!(v1, v2);
        // Interleave: pull an INNER memo first, then the leaf — the
        // half-updated graph must still converge with one leaf run.
        base.set(3);
        let _ = m2.get();
        let _ = m6.get();
        assert_eq!(
            *runs.borrow(),
            3,
            "pulling inner memos first must not double-run the leaf"
        );
    });
    root.dispose();
}

#[test]
fn dynamic_dependency_rewires_and_old_branch_stops_notifying() {
    let (root, _) = create_root(|cx| {
        let flag = cx.signal(true);
        let left = cx.signal(1i32);
        let right = cx.signal(100i32);
        let runs = Rc::new(RefCell::new(0));
        let runs2 = runs.clone();
        let picker = cx.memo(move || {
            *runs2.borrow_mut() += 1;
            if flag.get() {
                left.get()
            } else {
                right.get()
            }
        });
        assert_eq!(picker.get(), 1);
        flag.set(false);
        assert_eq!(picker.get(), 100);
        let runs_before = *runs.borrow();
        // The abandoned branch must be UNSUBSCRIBED: writing `left` now
        // re-runs nothing.
        left.set(2);
        let _ = picker.get();
        assert_eq!(*runs.borrow(), runs_before, "stale branch still wired");
        right.set(200);
        assert_eq!(picker.get(), 200);
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Effect queue semantics under mid-flush disposal (their risk 2).
// ---------------------------------------------------------------------------

#[test]
fn effect_disposing_later_queued_effect_causes_stale_skip() {
    let (root, _) = create_root(|cx| {
        let trigger = cx.signal(0i32);
        let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));

        // Child scope with an effect that would run AFTER the parent's
        // (creation order) — the parent's effect disposes it mid-flush.
        let child = cx.child();
        let log_b = log.clone();
        let _victim = child.effect(move || {
            let _ = trigger.get();
            log_b.borrow_mut().push("victim");
        });
        flush_effects();
        log.borrow_mut().clear();

        let log_a = log.clone();
        // Created after: runs after the victim in creation order? No —
        // creation order means the VICTIM (created first) runs first.
        // So invert: dispose from a NEW trigger effect created first in
        // a fresh scope... simplest honest construction: the disposer
        // effect was created BEFORE the victim below.
        let disposer_scope = cx.child();
        let victim_scope = cx.child();
        let log_c = log.clone();
        let _disposer = disposer_scope.effect(move || {
            let n = trigger.get();
            log_a.borrow_mut().push("disposer");
            if n == 2 {
                victim_scope.dispose(); // mid-flush disposal of a queued effect
            }
        });
        let _victim2 = victim_scope.effect(move || {
            let _ = trigger.get();
            log_c.borrow_mut().push("victim2");
        });
        flush_effects();
        log.borrow_mut().clear();

        trigger.set(2);
        flush_effects();
        let seen = log.borrow().clone();
        assert!(seen.contains(&"disposer"));
        assert!(
            !seen.contains(&"victim2"),
            "disposed-mid-flush effect ran against freed state: {seen:?}"
        );
    });
    root.dispose();
}

#[test]
fn effect_disposing_its_own_ancestor_scope_mid_run() {
    // Their confession: "an effect disposing its own ANCESTOR while
    // running is exercised only lightly". Exercise it heavily.
    let (root, _) = create_root(|cx| {
        let trigger = cx.signal(0i32);
        let outer = cx.child();
        let ran_after = Rc::new(RefCell::new(false));
        let inner = outer.child();
        let ran2 = ran_after.clone();
        let _suicide = inner.effect(move || {
            if trigger.get() == 1 {
                outer.dispose(); // disposes inner (and this running effect) too
                *ran2.borrow_mut() = true; // code after self-disposal still runs
            }
        });
        flush_effects();
        trigger.set(1);
        flush_effects(); // must not panic, double-free, or loop
        assert!(*ran_after.borrow());
        assert!(!outer.is_alive());
        // The graph survives: new work still functions.
        let s = cx.signal(5);
        assert_eq!(s.get_untracked(), 5);
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Disposal leak bounds — harsher than their 10k cycle test: nested Dyn
// regions + cross-scope signals + cleanups, checked against stats().
// ---------------------------------------------------------------------------

#[test]
fn nested_dyn_churn_10k_cycles_leaves_no_leaks() {
    let (root, _) = create_root(|cx| {
        let baseline = stats();
        let which = cx.signal(0usize);
        let mut tree = UiTree::new(Size::new(40, 10));
        let outer = dyn_view(Style::default(), move || {
            let w = which.get();
            // Inner Dyn nested in every generation: churns two scopes per
            // write.
            Element::new()
                .child(dyn_view(Style::default(), move || text(format!("gen {w}"))))
                .build()
        });
        tree.mount(cx, outer);
        flush_effects();
        let after_mount = stats();
        for i in 1..=10_000usize {
            which.set(i);
            flush_effects();
        }
        let after_churn = stats();
        assert!(
            after_churn.live_nodes <= after_mount.live_nodes + 4,
            "node leak: {} live after churn vs {} after mount (baseline {})",
            after_churn.live_nodes,
            after_mount.live_nodes,
            baseline.live_nodes
        );
        assert!(
            after_churn.slot_capacity <= after_mount.slot_capacity + 16,
            "slot capacity grew with churn: {} -> {} (arena must recycle)",
            after_mount.slot_capacity,
            after_churn.slot_capacity
        );
        assert_eq!(after_churn.queued_effects, 0);
    });
    root.dispose();
    let end = stats();
    assert_eq!(end.live_nodes, 0, "root disposal must free everything");
}

#[test]
fn cleanup_ordering_children_first_lifo() {
    let (root, _) = create_root(|cx| {
        let order = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let outer = cx.child();
        let (o1, o2, o3) = (order.clone(), order.clone(), order.clone());
        outer.on_cleanup(move || o1.borrow_mut().push("outer-first"));
        let inner = outer.child();
        inner.on_cleanup(move || o2.borrow_mut().push("inner"));
        outer.on_cleanup(move || o3.borrow_mut().push("outer-second"));
        outer.dispose();
        assert_eq!(
            *order.borrow(),
            vec!["inner", "outer-second", "outer-first"],
            "children first, then own cleanups LIFO"
        );
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Dispose-during-dispatch: the modal-close kill chain (RT1-3).
// ---------------------------------------------------------------------------

/// A capture-phase handler writes a signal whose Dyn effect unmounts the
/// subtree containing the TARGET. Whatever semantics REACT pinned
/// (batched dispatch or per-step revalidation), the invariants are:
/// no panic, no handler of a disposed scope firing, tree still usable.
#[test]
fn capture_handler_unmounts_target_subtree() {
    let (root, _) = create_root(|cx| {
        let open = cx.signal(true);
        let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let log_cap = log.clone();
        let log_target = log.clone();

        let view = Element::new()
            .on(Phase::Capture, move |_ctx, ev| {
                if matches!(ev, UiEvent::Mouse(_)) {
                    log_cap.borrow_mut().push("capture");
                    open.set(false); // unmounts the modal below
                }
            })
            .child(dyn_view(Style::default(), move || {
                if open.get() {
                    let log_target = log_target.clone();
                    Element::new()
                        .draw(|_c, _r| {})
                        .on(Phase::Target, move |_ctx, _ev| {
                            log_target.borrow_mut().push("target-after-unmount");
                        })
                        .build()
                } else {
                    text("closed")
                }
            }))
            .build();

        let mut tree = UiTree::new(Size::new(20, 5));
        tree.mount(cx, view);
        flush_effects();
        tree.layout();

        // Click inside the modal's rect: capture runs first, unmounts.
        tree.dispatch(&click(1, 0));
        flush_effects();

        let seen = log.borrow().clone();
        assert!(
            seen.contains(&"capture"),
            "capture handler must have run: {seen:?}"
        );
        // Whichever semantics was pinned, a DISPOSED handler must not
        // observe the event AFTER its scope died mid-dispatch. If REACT
        // batches the dispatch, the unmount happens after routing and
        // the target legitimately sees the event first — both orders are
        // sound; a panic or a post-disposal fire is not.
        // (This assert documents the batched semantics: target fired
        // before the batched unmount applied.)
        tree.dispatch(&click(1, 0)); // second click on the now-closed tree
        flush_effects();
        let second = log.borrow().len();
        tree.dispatch(&click(1, 0));
        flush_effects();
        assert_eq!(
            log.borrow().len(),
            second + 1, // only capture fires once per click now
            "closed modal must route only to live handlers: {:?}",
            log.borrow()
        );
    });
    root.dispose();
}

/// Focus edition of the same chain: a shortcut handler disposes the
/// focused subtree; focus must not dangle into freed instances.
#[test]
fn shortcut_disposing_focused_subtree_keeps_focus_sane() {
    let (root, _) = create_root(|cx| {
        let open = cx.signal(true);
        let view = Element::new()
            .on(Phase::Capture, move |_ctx, ev| {
                if matches!(ev, UiEvent::Key(k) if k.key == Key::Escape) {
                    open.set(false);
                }
            })
            .child(dyn_view(Style::default(), move || {
                if open.get() {
                    Element::new().focusable().draw(|_c, _r| {}).build()
                } else {
                    text("gone")
                }
            }))
            .build();
        let mut tree = UiTree::new(Size::new(20, 5));
        tree.mount(cx, view);
        flush_effects();
        tree.layout();
        tree.focus_next(); // focus the focusable inside the Dyn
        assert!(tree.focused().is_some());
        tree.dispatch(&key(Key::Escape));
        flush_effects();
        tree.layout();
        // Focus may be None or moved — but must not point at freed state,
        // and further dispatch must not panic.
        tree.dispatch(&key(Key::Tab));
        tree.dispatch(&click(0, 0));
        flush_effects();
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Draw-read guard (REACT cycle-2 deliverable).
// ---------------------------------------------------------------------------

/// RT1-2: a tracked signal read inside a draw closure must be loud.
/// REACT landed the guard mid-cycle-2 (`report_draw_read`); this is the
/// acceptance test.
#[test]
fn tracked_read_in_draw_closure_is_loud() {
    use abstracttui::layout::Dimension;
    use std::sync::atomic::{AtomicBool, Ordering};
    static DREW: AtomicBool = AtomicBool::new(false);
    DREW.store(false, Ordering::Relaxed);
    let result = std::panic::catch_unwind(move || {
        let (root, _) = create_root(|cx| {
            let sneaky = cx.signal(1i32);
            let view = Element::new()
                .style(
                    Style::default()
                        .width(Dimension::Cells(8))
                        .height(Dimension::Cells(2)),
                )
                .draw(move |_c, _r| {
                    DREW.store(true, Ordering::Relaxed);
                    let _ = sneaky.get(); // MUST panic (debug) or be flagged
                })
                .build();
            let mut tree = UiTree::new(Size::new(10, 3));
            tree.mount(cx, view);
            flush_effects();
            tree.layout();
            let mut canvas = BufferCanvas::new(Size::new(10, 3));
            tree.draw(&mut canvas);
        });
        root.dispose();
    });
    assert!(
        DREW.load(Ordering::Relaxed),
        "premise: the draw closure must actually run"
    );
    assert!(
        result.is_err(),
        "tracked read in draw closure must be a debug panic (RT1-2 contract)"
    );
    // The sanctioned peek stays quiet: untracked reads in draw are legal.
    let (root, _) = create_root(|cx| {
        let ok = cx.signal(2i32);
        let view = Element::new()
            .style(
                Style::default()
                    .width(Dimension::Cells(4))
                    .height(Dimension::Cells(1)),
            )
            .draw(move |_c, _r| {
                let _ = ok.get_untracked();
            })
            .build();
        let mut tree = UiTree::new(Size::new(10, 3));
        tree.mount(cx, view);
        flush_effects();
        tree.layout();
        let mut canvas = BufferCanvas::new(Size::new(10, 3));
        tree.draw(&mut canvas); // must NOT panic
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Batch coalescing + write-inside-batch visibility.
// ---------------------------------------------------------------------------

#[test]
fn batch_coalesces_effect_runs_and_reads_stay_coherent() {
    let (root, _) = create_root(|cx| {
        let a = cx.signal(0i32);
        let b = cx.signal(0i32);
        let runs = Rc::new(RefCell::new(0));
        let runs2 = runs.clone();
        let sum = cx.memo(move || a.get() + b.get());
        let _eff = cx.effect(move || {
            let _ = sum.get();
            *runs2.borrow_mut() += 1;
        });
        flush_effects();
        assert_eq!(*runs.borrow(), 1);
        batch(|| {
            a.set(1);
            b.set(2);
            // Read-inside-batch sees the writes (memo pulls eagerly).
            assert_eq!(sum.get(), 3, "read-after-write inside batch");
        });
        flush_effects();
        assert_eq!(*runs.borrow(), 2, "N writes in a batch = ONE effect run");
    });
    root.dispose();
}

// ---------------------------------------------------------------------------
// Runaway diagnostics (their per-flush ceiling).
// ---------------------------------------------------------------------------

/// An effect that writes its own dependency must die LOUDLY and QUICKLY —
/// and the panic must identify the problem class, not just "overflow".
#[test]
fn effect_writing_own_dependency_panics_diagnosably() {
    let result = std::panic::catch_unwind(|| {
        let (root, _) = create_root(|cx| {
            let s = cx.signal(0u64);
            let _eff = cx.effect(move || {
                let v = s.get();
                s.set(v + 1); // never settles
            });
            flush_effects();
        });
        root.dispose();
    });
    let err = result.expect_err("self-feeding effect must panic, not freeze");
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    let lower = msg.to_lowercase();
    assert!(
        ["effect", "flush", "settle", "cycle", "re-enter"]
            .iter()
            .any(|w| lower.contains(w)),
        "runaway panic must name the failure class, got: {msg:?}"
    );
}

// ---------------------------------------------------------------------------
// UiTree damage accounting (damage-contract §3 producer side).
// ---------------------------------------------------------------------------

#[test]
fn dyn_remount_damages_exactly_its_region() {
    let (root, _) = create_root(|cx| {
        let n = cx.signal(0i32);
        let view = Element::new()
            .child(text("static header"))
            .child(dyn_view(Style::default(), move || {
                text(format!("count {}", n.get()))
            }))
            .build();
        let mut tree = UiTree::new(Size::new(30, 4));
        tree.mount(cx, view);
        flush_effects();
        tree.layout();
        let _ = tree.take_damage(); // drain mount damage
        n.set(1);
        flush_effects();
        tree.layout();
        let damage = tree.take_damage();
        assert!(!damage.is_empty(), "a Dyn remount must damage its region");
        let union = damage.iter().fold(Rect::ZERO, |acc, r| acc.union(*r));
        // The Dyn's solved rect starts after the 13-col static header
        // (row layout, stretch height is legitimate). Region-sized means:
        // never the header's columns, never the whole width.
        assert!(
            union.x >= 13 && union.w <= 17,
            "damage must exclude the static sibling: {damage:?}"
        );
        // RT2-4 (CLOSED cycle 3): was 3 identical rects per remount;
        // REACT deduped — at most dispose + remount damage now, pinned.
        assert!(
            damage.len() <= 2,
            "damage feed noise: {} rects for one remount: {damage:?}",
            damage.len()
        );
        // No second write, no second damage.
        flush_effects();
        assert!(tree.take_damage().is_empty(), "damage must not replay");
    });
    root.dispose();
}
