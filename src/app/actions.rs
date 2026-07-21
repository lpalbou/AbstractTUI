//! Named actions with optional key chords — the app-global keymap and
//! the registry a command palette lists.
//!
//! Position in the key-resolution order (documented in reactive-ui.md
//! §12a and §16): focused-widget handlers → ancestor handlers → widget
//! shortcuts → OVERLAY routing → root tree → ACTIONS. An action fires
//! only when nothing in the UI consumed the chord, so a text input
//! typing 's' never triggers a bare-'s' binding while focused.
//!
//! Deliberately small: no contexts/"when" clauses, no chord sequences
//! (Emacs prefixes), no user keymap files — those are app policy this
//! registry can host later. Names are unique ids ("file.save"); a
//! palette renders `list()` (name + chord) and calls `run(name)`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ui::KeyChord;

/// Palette-facing view of one action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInfo {
    pub name: String,
    pub chord: Option<KeyChord>,
}

struct Entry {
    name: String,
    chord: Option<KeyChord>,
    run: Box<dyn FnMut()>,
}

#[derive(Default)]
struct Registry {
    entries: Vec<Entry>,
    by_chord: HashMap<KeyChord, usize>,
}

/// Cloneable handle to the app's action registry (`App::actions()`).
#[derive(Clone, Default)]
pub struct Actions {
    inner: Rc<RefCell<Registry>>,
}

impl Actions {
    pub fn new() -> Actions {
        Actions::default()
    }

    /// Register `name` with an optional chord. Returns false (and
    /// registers nothing) when the name is taken or the chord is bound —
    /// collisions are a programming error the caller should hear about,
    /// not a silent last-writer-wins.
    pub fn register(
        &self,
        name: impl Into<String>,
        chord: Option<KeyChord>,
        run: impl FnMut() + 'static,
    ) -> bool {
        let name = name.into();
        let mut reg = self.inner.borrow_mut();
        if reg.entries.iter().any(|e| e.name == name) {
            return false;
        }
        if let Some(c) = chord {
            if reg.by_chord.contains_key(&c) {
                return false;
            }
            let idx = reg.entries.len();
            reg.by_chord.insert(c, idx);
        }
        reg.entries.push(Entry {
            name,
            chord,
            run: Box::new(run),
        });
        true
    }

    /// Remove an action by name (palette entries and its chord binding
    /// disappear together). Returns whether it existed.
    pub fn unregister(&self, name: &str) -> bool {
        let mut reg = self.inner.borrow_mut();
        let Some(i) = reg.entries.iter().position(|e| e.name == name) else {
            return false;
        };
        reg.entries.remove(i);
        // Indices shifted: rebuild the chord map (registry sizes are
        // tens, not thousands; clarity beats cleverness here).
        reg.by_chord.clear();
        let chords: Vec<(KeyChord, usize)> = reg
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.chord.map(|c| (c, i)))
            .collect();
        reg.by_chord.extend(chords);
        true
    }

    /// Re-bind (or unbind with `None`) an existing action's chord.
    /// Returns false when the action is unknown or the chord is taken.
    pub fn rebind(&self, name: &str, chord: Option<KeyChord>) -> bool {
        let mut reg = self.inner.borrow_mut();
        let Some(i) = reg.entries.iter().position(|e| e.name == name) else {
            return false;
        };
        if let Some(c) = chord {
            if reg.by_chord.get(&c).is_some_and(|&j| j != i) {
                return false;
            }
        }
        if let Some(old) = reg.entries[i].chord {
            reg.by_chord.remove(&old);
        }
        reg.entries[i].chord = chord;
        if let Some(c) = chord {
            reg.by_chord.insert(c, i);
        }
        true
    }

    /// Run by name (palette selection). Returns whether it existed.
    pub fn run(&self, name: &str) -> bool {
        // Take the callback OUT while it runs: an action opening a
        // palette that lists actions must not hit a RefCell collision.
        let taken = {
            let mut reg = self.inner.borrow_mut();
            reg.entries.iter().position(|e| e.name == name).map(|i| {
                (
                    i,
                    std::mem::replace(&mut reg.entries[i].run, Box::new(|| {})),
                )
            })
        };
        let Some((i, mut run)) = taken else {
            return false;
        };
        run();
        let mut reg = self.inner.borrow_mut();
        // The action may have unregistered itself (or others) while
        // running; only put the callback back where it still belongs.
        if let Some(e) = reg.entries.get_mut(i) {
            e.run = run;
        }
        true
    }

    /// Fire the action bound to `chord`, if any. The driver calls this
    /// with keys nothing in the UI consumed.
    pub fn dispatch_chord(&self, chord: KeyChord) -> bool {
        let name = {
            let reg = self.inner.borrow();
            reg.by_chord
                .get(&chord)
                .map(|&i| reg.entries[i].name.clone())
        };
        match name {
            Some(n) => self.run(&n),
            None => false,
        }
    }

    /// Palette feed: every action, registration order, with its chord.
    pub fn list(&self) -> Vec<ActionInfo> {
        self.inner
            .borrow()
            .entries
            .iter()
            .map(|e| ActionInfo {
                name: e.name.clone(),
                chord: e.chord,
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.borrow().entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Key, Mods};
    use std::cell::Cell;

    fn chord(key: Key, mods: Mods) -> KeyChord {
        KeyChord { key, mods }
    }

    #[test]
    fn register_dispatch_and_collision_rules() {
        let actions = Actions::new();
        let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let f = fired.clone();
        let save = chord(Key::Char('s'), Mods::CTRL);
        assert!(actions.register("file.save", Some(save), move || f.set(f.get() + 1)));
        assert!(
            !actions.register("file.save", None, || {}),
            "duplicate name refused"
        );
        assert!(
            !actions.register("other", Some(save), || {}),
            "duplicate chord refused"
        );
        assert!(actions.dispatch_chord(save));
        assert_eq!(fired.get(), 1);
        assert!(!actions.dispatch_chord(chord(Key::Char('x'), Mods::NONE)));
        assert!(actions.run("file.save"));
        assert_eq!(fired.get(), 2);
        assert_eq!(
            actions.list(),
            vec![ActionInfo {
                name: "file.save".into(),
                chord: Some(save)
            }]
        );
    }

    #[test]
    fn rebind_and_unregister_keep_the_chord_map_honest() {
        let actions = Actions::new();
        let a = chord(Key::Char('a'), Mods::CTRL);
        let b = chord(Key::Char('b'), Mods::CTRL);
        assert!(actions.register("one", Some(a), || {}));
        assert!(actions.register("two", None, || {}));
        assert!(actions.rebind("one", Some(b)));
        assert!(!actions.dispatch_chord(a), "old chord unbound");
        assert!(actions.dispatch_chord(b));
        assert!(actions.rebind("two", Some(a)), "freed chord reusable");
        assert!(!actions.rebind("two", Some(b)), "occupied chord refused");
        assert!(actions.unregister("one"));
        assert!(!actions.dispatch_chord(b), "unregistered chord gone");
        assert!(
            actions.dispatch_chord(a),
            "survivor still routed after index shift"
        );
    }

    #[test]
    fn action_may_reenter_the_registry_while_running() {
        let actions = Actions::new();
        let inner = actions.clone();
        let observed: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let o = observed.clone();
        assert!(actions.register("palette.open", None, move || {
            // A palette lists actions from INSIDE an action.
            o.set(inner.list().len());
        }));
        assert!(actions.run("palette.open"));
        assert_eq!(observed.get(), 1);
    }
}
