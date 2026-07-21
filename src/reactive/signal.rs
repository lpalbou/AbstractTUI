//! `Signal<T>`: the root reactive value. Reads subscribe the running
//! computation; writes mark the dependent subgraph and schedule effects.

use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use super::node::NodeKind;
use super::runtime::{self, maybe_flush, with_rt, RawId, MSG_DISPOSED};

/// A `Copy` handle to a reactive value stored in the runtime arena.
///
/// The four verbs, one law each: `get`/`with` are TRACKED reads (the
/// running computation subscribes); `get_untracked`/`with_untracked`
/// read without subscribing; `set` replaces and notifies; `update`
/// mutates in place and notifies. `set_if_changed` adds the equality
/// cut-off; `try_get_untracked` answers `None` after disposal instead
/// of panicking.
///
/// ```
/// use abstracttui::reactive::create_root;
///
/// let (root, ()) = create_root(|cx| {
///     let count = cx.signal(1);
///     let doubled = cx.memo(move || count.get() * 2); // tracked read
///     assert_eq!(doubled.get(), 2);
///     count.update(|c| *c += 1);
///     assert_eq!(doubled.get(), 4); // memo recomputed
///     count.set(2); // same value...
///     assert_eq!(doubled.get(), 4); // ...set() still notifies; memo EQ cuts off
/// });
/// root.dispose();
/// ```
///
/// `PhantomData<fn() -> T>` (not `PhantomData<T>`) keeps the handle
/// `Copy + Send` regardless of `T`: the handle is just an index — the
/// value itself never leaves its thread, and using a handle on the wrong
/// thread panics on the runtime-id stamp instead of aliasing.
pub struct Signal<T> {
    pub(crate) id: RawId,
    pub(crate) _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Signal<T> {}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Signal({}, gen {})",
            self.id.key.index, self.id.key.generation
        )
    }
}

impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Signal<T> {}

/// Fetch the value cell, optionally recording a dependency edge. The cell
/// is `Rc`ed out so the arena borrow is released before user code touches
/// the value (reads inside `with` may re-enter the runtime). Untyped by
/// design: the caller's typed accessor downcasts.
fn cell_of(id: RawId, track: bool) -> Rc<RefCell<Box<dyn Any>>> {
    with_rt(|rt| {
        rt.check_thread(id);
        if track {
            rt.track_read(id.key);
        }
        match &rt
            .graph
            .get(id.key)
            .unwrap_or_else(|| panic!("{MSG_DISPOSED}"))
            .kind
        {
            NodeKind::Signal { value } => value.clone(),
            _ => unreachable!("typed handle points at a non-signal node"),
        }
    })
}

impl<T: 'static> Signal<T> {
    /// Tracked read by reference. Panics if the signal was disposed.
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        let cell = cell_of(self.id, true);
        let guard = cell.borrow();
        f(guard.downcast_ref::<T>().expect("signal value type"))
    }

    /// Tracked read by clone — the everyday accessor.
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Read without subscribing (peek). Useful inside effects that must
    /// not re-run when this particular value changes.
    pub fn with_untracked<R>(self, f: impl FnOnce(&T) -> R) -> R {
        let cell = cell_of(self.id, false);
        let guard = cell.borrow();
        f(guard.downcast_ref::<T>().expect("signal value type"))
    }

    pub fn get_untracked(self) -> T
    where
        T: Clone,
    {
        self.with_untracked(T::clone)
    }

    /// Untracked read that survives disposal: `None` instead of the
    /// disposed-signal panic. THE read for closures that outlive their
    /// data by design — `access_value` snapshots, diagnostics — where
    /// "gone" is an answer, not a bug. Everywhere else prefer the
    /// panicking reads: a disposed read in normal dataflow IS a bug.
    pub fn try_get_untracked(self) -> Option<T>
    where
        T: Clone,
    {
        if !self.is_alive() {
            return None;
        }
        Some(self.with_untracked(T::clone))
    }

    /// Replace the value and notify dependents (no equality check — use
    /// [`Signal::set_if_changed`] for cut-off semantics).
    pub fn set(self, value: T) {
        let cell = cell_of(self.id, false);
        let old = {
            let mut guard = cell.borrow_mut();
            std::mem::replace(&mut *guard, Box::new(value) as Box<dyn Any>)
        };
        // Old value drops here, after the cell borrow is released: its
        // `Drop` may read this very signal.
        drop(old);
        with_rt(|rt| rt.mark_written(self.id.key));
        maybe_flush();
    }

    /// Mutate in place and notify. The closure must not read this same
    /// signal reactively (the cell is exclusively borrowed during `f`).
    pub fn update(self, f: impl FnOnce(&mut T)) {
        let cell = cell_of(self.id, false);
        {
            let mut guard = cell.borrow_mut();
            f(guard.downcast_mut::<T>().expect("signal value type"));
        }
        with_rt(|rt| rt.mark_written(self.id.key));
        maybe_flush();
    }

    /// Equality cut-off write: if the new value compares equal, nothing is
    /// marked, nothing re-runs. Returns whether a change propagated.
    pub fn set_if_changed(self, value: T) -> bool
    where
        T: PartialEq,
    {
        let cell = cell_of(self.id, false);
        let old = {
            let mut guard = cell.borrow_mut();
            let current = guard.downcast_mut::<T>().expect("signal value type");
            if *current == value {
                return false;
            }
            std::mem::replace(&mut *guard, Box::new(value) as Box<dyn Any>)
        };
        drop(old);
        with_rt(|rt| rt.mark_written(self.id.key));
        maybe_flush();
        true
    }

    /// False once the owning scope (or an explicit dispose) freed it.
    pub fn is_alive(self) -> bool {
        with_rt(|rt| rt.rt_id == self.id.rt && rt.graph.contains(self.id.key))
    }

    /// Manual early disposal (normally the owning scope handles this).
    pub fn dispose(self) {
        runtime::dispose_node(self.id);
    }
}
