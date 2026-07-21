//! Hand-rolled generational arena (slotmap-style) — the storage substrate
//! for reactive nodes, and reused by the layout and ui trees.
//!
//! WHY generational indices instead of `Rc` graphs: reactive graphs are
//! cyclic by nature (source -> observer -> source bookkeeping), and `Rc`
//! cycles leak. An arena gives `Copy` handles (`Key`) that can be stored
//! in closures and moved across component boundaries freely; a stale key
//! (slot freed and reused) is detected by the generation counter instead
//! of dereferencing freed state. This mirrors what leptos and sycamore do
//! with `slotmap`, hand-rolled here per the dependency policy.

/// A generational key: `index` addresses the slot, `generation` must match
/// the slot's current generation or the key is stale (its node was freed).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

struct Slot<T> {
    /// Odd trick avoided on purpose: we keep an explicit `Option` rather
    /// than a union so the arena stays 100% safe code.
    value: Option<T>,
    /// Incremented every time the slot is vacated, so old keys go stale.
    generation: u32,
}

/// Fixed-address storage with O(1) insert/remove and stale-key detection.
pub struct GenArena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Default for GenArena<T> {
    fn default() -> Self {
        GenArena {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }
}

impl<T> GenArena<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, value: T) -> Key {
        self.live += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.value.is_none());
            slot.value = Some(value);
            // Generation was already bumped at removal time; reusing it here
            // means keys minted before the removal can never match.
            return Key {
                index,
                generation: slot.generation,
            };
        }
        let index = self.slots.len() as u32;
        // Generations start at 1 so `Key::DEAD` (generation 0) never matches.
        self.slots.push(Slot {
            value: Some(value),
            generation: 1,
        });
        Key {
            index,
            generation: 1,
        }
    }

    pub fn get(&self, key: Key) -> Option<&T> {
        let slot = self.slots.get(key.index as usize)?;
        if slot.generation != key.generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        let slot = self.slots.get_mut(key.index as usize)?;
        if slot.generation != key.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Frees the slot and bumps its generation. Returns the value so the
    /// caller can defer dropping it (reactive nodes hold user closures and
    /// values whose `Drop` may re-enter the runtime — they must be dropped
    /// outside any runtime borrow).
    pub fn remove(&mut self, key: Key) -> Option<T> {
        let slot = self.slots.get_mut(key.index as usize)?;
        if slot.generation != key.generation || slot.value.is_none() {
            return None;
        }
        let value = slot.value.take();
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(key.index);
        self.live -= 1;
        value
    }

    pub fn contains(&self, key: Key) -> bool {
        self.get(key).is_some()
    }

    /// Number of live values. Leak tests pin this: create/dispose cycles
    /// must return it to its baseline.
    pub fn live(&self) -> usize {
        self.live
    }

    /// Total slots ever allocated (live + free). Bounded by the maximum
    /// number of *concurrently* live nodes, never by total churn — the
    /// property that makes 10k create/dispose cycles allocation-stable.
    pub fn capacity_slots(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_keys_do_not_resolve() {
        let mut a = GenArena::new();
        let k1 = a.insert("one");
        assert_eq!(a.get(k1), Some(&"one"));
        assert_eq!(a.remove(k1), Some("one"));
        assert_eq!(a.get(k1), None);
        // Slot is reused, but the old key stays dead.
        let k2 = a.insert("two");
        assert_eq!(k1.index, k2.index);
        assert_ne!(k1.generation, k2.generation);
        assert_eq!(a.get(k1), None);
        assert_eq!(a.get(k2), Some(&"two"));
    }

    #[test]
    fn churn_keeps_slot_count_bounded() {
        let mut a = GenArena::new();
        for i in 0..10_000 {
            let k = a.insert(i);
            assert_eq!(a.remove(k), Some(i));
        }
        assert_eq!(a.live(), 0);
        assert!(a.capacity_slots() <= 1, "churn must reuse freed slots");
    }

    #[test]
    fn dead_key_never_matches() {
        // Generation 0 is never assigned to a live slot (they start at 1),
        // so a zeroed key can serve as an always-dead sentinel.
        let mut a: GenArena<i32> = GenArena::new();
        a.insert(7);
        assert_eq!(
            a.get(Key {
                index: u32::MAX,
                generation: 0
            }),
            None
        );
        assert_eq!(
            a.get(Key {
                index: 0,
                generation: 0
            }),
            None
        );
    }
}
