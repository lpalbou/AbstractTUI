//! Ownership scopes: every node belongs to a scope (or to a computation,
//! which is itself an owner); disposing a scope disposes its subtree —
//! children first, cleanups LIFO, arena slots freed with a generation
//! bump. This ties the LIFETIME of reactive state to the lifetime of the
//! UI region that created it, instead of Rust lexical scope — the
//! "reactive garbage collector" idea from leptos/SolidJS.
//!
//! Ownership here is EXPLICIT: `cx.signal(...)` attaches to `cx`, always.
//! Implicit-owner APIs make it too easy to accidentally hang state off a
//! long-lived root and leak it; with explicit scopes, where state lives
//! is visible at the call site. The one implicit piece is
//! [`super::runtime::on_cleanup`], which targets the *currently running*
//! computation so effect-teardown reads naturally inside effect bodies.

use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use super::effect::Effect;
use super::memo::Memo;
use super::node::{eq_any, NodeKind};
use super::runtime::{self, with_rt, RawId};
use super::signal::Signal;

/// `Copy` handle to an ownership scope. Component bodies receive one and
/// create their reactive state through it.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Scope {
    pub(crate) id: RawId,
}

impl std::fmt::Debug for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Scope({}, gen {})",
            self.id.key.index, self.id.key.generation
        )
    }
}

impl Scope {
    /// New signal owned by this scope.
    pub fn signal<T: 'static>(self, value: T) -> Signal<T> {
        let id = with_rt(|rt| {
            rt.check_thread(self.id);
            let key = rt.create_node(
                Some(self.id.key),
                NodeKind::Signal {
                    value: Rc::new(RefCell::new(Box::new(value) as Box<dyn Any>)),
                },
            );
            RawId { key, rt: rt.rt_id }
        });
        Signal {
            id,
            _marker: PhantomData,
        }
    }

    /// New lazy memo owned by this scope. `T: PartialEq` powers the
    /// equality cut-off that stops change propagation.
    pub fn memo<T, F>(self, f: F) -> Memo<T>
    where
        T: PartialEq + 'static,
        F: Fn() -> T + 'static,
    {
        let id = with_rt(|rt| {
            rt.check_thread(self.id);
            let compute: Rc<dyn Fn() -> Box<dyn Any>> =
                Rc::new(move || Box::new(f()) as Box<dyn Any>);
            let key = rt.create_node(
                Some(self.id.key),
                NodeKind::Memo {
                    value: Rc::new(RefCell::new(None)),
                    compute,
                    eq: eq_any::<T>,
                },
            );
            RawId { key, rt: rt.rt_id }
        });
        Memo {
            id,
            _marker: PhantomData,
        }
    }

    /// New effect owned by this scope; runs immediately, then re-runs
    /// (scheduled) whenever a tracked dependency changes.
    pub fn effect(self, f: impl FnMut() + 'static) -> Effect {
        Effect::new(self, None, f)
    }

    /// Like [`Scope::effect`] with a diagnostic label (RT1-15a): if this
    /// effect ever exceeds the per-flush run ceiling (a write feedback
    /// loop), the panic names the label instead of an anonymous node id.
    /// Recommended for every effect that WRITES signals.
    pub fn effect_labeled(self, label: &'static str, f: impl FnMut() + 'static) -> Effect {
        Effect::new(self, Some(label), f)
    }

    /// New child scope — the unit `ui::Dyn` disposes and recreates on
    /// every reactive re-render.
    pub fn child(self) -> Scope {
        let id = with_rt(|rt| {
            rt.check_thread(self.id);
            let key = rt.create_node(Some(self.id.key), NodeKind::Scope);
            RawId { key, rt: rt.rt_id }
        });
        Scope { id }
    }

    /// Register a cleanup on THIS scope (explicit target). For "cleanup
    /// before my effect re-runs", use the free function
    /// [`super::runtime::on_cleanup`] inside the effect body instead.
    pub fn on_cleanup(self, f: impl FnOnce() + 'static) {
        with_rt(|rt| {
            rt.check_thread(self.id);
            let Some(node) = rt.graph.get_mut(self.id.key) else {
                panic!("abstracttui reactive: on_cleanup on a disposed scope");
            };
            node.cleanups.push(Box::new(f));
        });
    }

    /// Run `f` with this scope as the current owner, so `on_cleanup` and
    /// framework internals attach here. Creation methods (`signal`, ...)
    /// do not need this — they take the scope explicitly.
    pub fn run<R>(self, f: impl FnOnce() -> R) -> R {
        let prev = with_rt(|rt| {
            rt.check_thread(self.id);
            if !rt.graph.contains(self.id.key) {
                panic!("abstracttui reactive: run on a disposed scope");
            }
            rt.current_owner.replace(self.id.key)
        });
        struct OwnerGuard(Option<super::arena::Key>);
        impl Drop for OwnerGuard {
            fn drop(&mut self) {
                runtime::restore_owner(self.0);
            }
        }
        let _guard = OwnerGuard(prev);
        f()
    }

    /// Dispose this scope and everything it owns. Children are disposed
    /// before parents (reverse creation order among siblings) and
    /// cleanups run LIFO — children may reference parent state, so the
    /// teardown order must be the reverse of construction.
    pub fn dispose(self) {
        runtime::dispose_node(self.id);
    }

    pub fn is_alive(self) -> bool {
        with_rt(|rt| rt.rt_id == self.id.rt && rt.graph.contains(self.id.key))
    }

    /// Provide a context value on THIS scope: every descendant scope can
    /// read it with [`Scope::use_context`] — shared state without prop
    /// drilling (React context parity). One value per TYPE per scope
    /// (re-providing replaces); a nested provide SHADOWS the ancestor's
    /// for its own subtree. `T: Clone` — provide `Signal<T>`, `Rc<T>`,
    /// or small values; readers get a clone.
    pub fn provide_context<T: Clone + 'static>(self, value: T) {
        use std::any::TypeId;
        with_rt(|rt| {
            rt.check_thread(self.id);
            if !rt.graph.contains(self.id.key) {
                panic!("abstracttui reactive: provide_context on a disposed scope");
            }
            let entry = rt.contexts.entry(self.id.key).or_default();
            let boxed: Rc<dyn Any> = Rc::new(value);
            match entry.iter_mut().find(|(t, _)| *t == TypeId::of::<T>()) {
                Some((_, slot)) => *slot = boxed,
                None => entry.push((TypeId::of::<T>(), boxed)),
            }
        });
    }

    /// Read the nearest provided context of type `T`: this scope first,
    /// then ancestors. `None` when nothing up the tree provided one —
    /// components decide whether that is a default or a loud error.
    pub fn use_context<T: Clone + 'static>(self) -> Option<T> {
        use std::any::TypeId;
        with_rt(|rt| {
            rt.check_thread(self.id);
            let mut cur = Some(self.id.key);
            while let Some(key) = cur {
                if let Some(entry) = rt.contexts.get(&key) {
                    if let Some((_, v)) = entry.iter().find(|(t, _)| *t == TypeId::of::<T>()) {
                        return v.downcast_ref::<T>().cloned();
                    }
                }
                cur = rt.graph.get(key).and_then(|n| n.parent);
            }
            None
        })
    }
}

/// An owning root for a whole reactive world (the app, a test). Dropping
/// it disposes the tree — but `dispose()` is available for explicit,
/// deterministic teardown.
pub struct RootScope {
    scope: Scope,
}

impl RootScope {
    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn dispose(self) {
        // Drop impl does the work; consuming `self` is the explicit form.
    }
}

impl Drop for RootScope {
    fn drop(&mut self) {
        if self.scope.is_alive() {
            runtime::dispose_node(self.scope.id);
        }
    }
}

/// Create a detached root scope and run `f` under it. Everything `f`
/// creates through `cx` lives until the returned [`RootScope`] is
/// disposed/dropped — leak tests pin `stats().live_nodes` around this.
pub fn create_root<R>(f: impl FnOnce(Scope) -> R) -> (RootScope, R) {
    let scope = {
        let id = with_rt(|rt| {
            let key = rt.create_node(None, NodeKind::Scope);
            RawId { key, rt: rt.rt_id }
        });
        Scope { id }
    };
    let result = scope.run(|| f(scope));
    (RootScope { scope }, result)
}
