//! VERIFY cycle-6 component/callback attack. `Callback<T>` is the shared
//! event-wiring primitive (one component hands the same callback to a key
//! handler and a click handler). Its robustness contract:
//! - cloning SHARES the same closure (both clones drive one state);
//! - `noop` is truly inert;
//! - a callback OUTLIVES the scope that created it and stays callable as
//!   long as it only touches state it owns (no dangling / UB);
//! - touching state that WAS disposed fails LOUDLY and safely (a
//!   controlled panic, never memory unsafety) — the reactive contract is
//!   "use-after-dispose is a caught bug, not corruption".

use std::cell::Cell;
use std::rc::Rc;

use abstracttui::reactive::create_root;
use abstracttui::ui::Callback;

/// Clones share one underlying closure: three clones, one counter.
#[test]
fn callback_clones_share_one_closure() {
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    let cb: Callback<i32> = Callback::new(move |n| h.set(h.get() + n));
    let a = cb.clone();
    let b = cb.clone();
    a.call(1);
    b.call(10);
    cb.call(100);
    assert_eq!(
        hits.get(),
        111,
        "all clones must drive the same closure state"
    );
}

/// The default/noop callback is inert and safe to call repeatedly.
#[test]
fn noop_callback_is_inert() {
    let cb: Callback<&str> = Callback::noop();
    cb.call("ignored");
    cb.call("still ignored");
    let def: Callback<()> = Callback::default();
    def.call(());
}

/// A callback built INSIDE a reactive scope, capturing only its own
/// (non-reactive) state, must remain callable after the scope disposes —
/// the callback is not tied to the scope's lifetime and must not dangle.
#[test]
fn callback_outlives_its_creating_scope() {
    let sink = Rc::new(Cell::new(0));
    let mut escaped: Option<Callback<i32>> = None;
    let (root, ()) = create_root(|_cx| {
        let inner = sink.clone();
        escaped = Some(Callback::new(move |n| inner.set(inner.get() + n)));
    });
    // Dispose the scope the callback was born in.
    root.dispose();
    // The callback still works — it captured only the Rc counter.
    let cb = escaped.expect("callback escaped the scope");
    cb.call(7);
    cb.call(35);
    assert_eq!(sink.get(), 42, "callback must survive its scope's disposal");
}

/// A callback that touches a DISPOSED signal must fail as a CONTROLLED
/// panic (unwind), never undefined behavior. We prove "not UB" by
/// catching the unwind: the process stays alive and consistent, and a
/// fresh scope afterward works normally.
#[test]
fn callback_touching_disposed_signal_is_a_controlled_panic_not_ub() {
    use abstracttui::reactive::flush_effects;

    let mut sig_setter: Option<Callback<i32>> = None;
    let (root, ()) = create_root(|cx| {
        let value = cx.signal(0i32);
        sig_setter = Some(Callback::new(move |n| value.set(n)));
    });
    let cb = sig_setter.expect("setter escaped");
    // Dispose the scope: the signal is gone.
    root.dispose();
    flush_effects();

    // Calling the callback now reaches a disposed signal. The contract is
    // a LOUD, CONTROLLED failure — catch the unwind and assert we did not
    // corrupt anything (a brand-new scope still works).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb.call(5)));
    assert!(
        result.is_err(),
        "touching a disposed signal must fail loudly (documented panic), not silently proceed"
    );

    // The runtime is still usable — no poisoning, no UB.
    let (root2, out) = create_root(|cx| {
        let v = cx.signal(99i32);
        v.get_untracked()
    });
    assert_eq!(
        out, 99,
        "runtime remains consistent after a caught use-after-dispose"
    );
    root2.dispose();
}

/// Dropping every clone of a callback drops the captured state exactly
/// once (no leak, no double-drop).
#[test]
fn dropping_all_clones_drops_captured_state_once() {
    let drops = Rc::new(Cell::new(0));

    struct DropCounter(Rc<Cell<u32>>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let counter = DropCounter(drops.clone());
    let cb: Callback<()> = Callback::new(move |_| {
        // Capture the counter by holding a reference to it.
        let _ = &counter;
    });
    let clone1 = cb.clone();
    let clone2 = cb.clone();
    assert_eq!(drops.get(), 0, "no drop while clones live");
    drop(cb);
    drop(clone1);
    assert_eq!(drops.get(), 0, "still alive while one clone remains");
    drop(clone2);
    assert_eq!(
        drops.get(),
        1,
        "captured state drops exactly once when the last clone dies"
    );
}
