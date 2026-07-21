//! `Memo<T>`: lazy, cached derived value with equality cut-off.
//!
//! Laziness: a memo is born `Dirty` with no cached value and no edges; it
//! computes the first time somebody OBSERVES it, and recomputes only when
//! (a) marked stale by the two-phase walk AND (b) observed again. A memo
//! nobody reads costs nothing forever.
//!
//! Cut-off: recomputing to an equal value does not dirty downstream —
//! observers marked `Check` resolve back to `Clean` without running.
//! This is why `T: PartialEq` is required at creation.

use std::marker::PhantomData;

use super::execute::update_if_necessary;
use super::node::NodeKind;
use super::runtime::{self, with_rt, RawId, MSG_DISPOSED};

/// `Copy` handle to a cached computation. Same marker rationale as
/// [`super::signal::Signal`].
pub struct Memo<T> {
    pub(crate) id: RawId,
    pub(crate) _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Memo<T> {}
impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Memo({}, gen {})",
            self.id.key.index, self.id.key.generation
        )
    }
}

impl<T: 'static> Memo<T> {
    /// Tracked read; brings the cache up to date first (pull phase).
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        self.read(true, f)
    }

    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Untracked read. Still recomputes if stale — "untracked" is about
    /// not subscribing, never about serving stale data.
    pub fn with_untracked<R>(self, f: impl FnOnce(&T) -> R) -> R {
        self.read(false, f)
    }

    pub fn get_untracked(self) -> T
    where
        T: Clone,
    {
        self.with_untracked(T::clone)
    }

    fn read<R>(self, track: bool, f: impl FnOnce(&T) -> R) -> R {
        with_rt(|rt| rt.check_thread(self.id));
        // Pull: resolve Check/Dirty into a fresh cached value. Runs user
        // computations, so it happens before we take any cell borrow.
        update_if_necessary(self.id.key);
        let cell = with_rt(|rt| {
            if track {
                rt.track_read(self.id.key);
            }
            match &rt
                .graph
                .get(self.id.key)
                .unwrap_or_else(|| panic!("{MSG_DISPOSED}"))
                .kind
            {
                NodeKind::Memo { value, .. } => value.clone(),
                _ => unreachable!("typed handle points at a non-memo node"),
            }
        });
        let guard = cell.borrow();
        let value = guard
            .as_ref()
            .expect("memo has a value after update_if_necessary")
            .downcast_ref::<T>()
            .expect("memo value type");
        f(value)
    }

    pub fn is_alive(self) -> bool {
        with_rt(|rt| rt.rt_id == self.id.rt && rt.graph.contains(self.id.key))
    }

    pub fn dispose(self) {
        runtime::dispose_node(self.id);
    }
}
