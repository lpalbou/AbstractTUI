//! Word-wise editing chords: ONE spelling table for [`TextInput`] and
//! [`TextArea`] (first-app/1310).
//!
//! The two widgets already knew what a word jump MEANS; what they did
//! not know is how many ways a terminal spells one. The same physical
//! gesture — "move by word" — reaches an app as three unrelated events
//! depending on the emulator and its settings, and a widget that
//! recognizes only one of them looks broken on the other two:
//!
//! The table is CODEX's default editor keymap, adopted verbatim
//! (`codex-rs/tui/src/keymap.rs`, `EditorKeymap` defaults) so a user
//! moving between the two feels no difference:
//!
//! | Codex binding | Gesture | Reaches us as |
//! |---|---|---|
//! | `move_word_left` | Alt+b, Alt+←, Ctrl+← | `Char('b')`+ALT, `Left`+ALT, `Left`+CTRL |
//! | `move_word_right` | Alt+f, Alt+→, Ctrl+→ | `Char('f')`+ALT, `Right`+ALT, `Right`+CTRL |
//! | `delete_backward_word` | Alt+Backspace, Ctrl+Backspace, Ctrl+W | `Backspace`+ALT/CTRL, `Char('w')`+CTRL |
//! | `delete_forward_word` | Alt+Delete, Ctrl+Delete, Alt+d | `Delete`+ALT/CTRL, `Char('d')`+ALT |
//!
//! Three spellings per gesture is not redundancy: macOS has no
//! Option-arrow escape of its own, so its terminals borrow readline's
//! `ESC b`/`ESC f` (iTerm2 ships exactly that in its "Natural Text
//! Editing" preset), while `CSI 1;3D` is the kitty/WezTerm/ghostty
//! form and `CSI 1;5D` the Linux/Windows one. Codex binds all three for
//! that reason, and so do we.
//!
//! Line start/end (`move_line_start` / `move_line_end`: Home/Ctrl+A,
//! End/Ctrl+E) live in the widgets' own key match, next to the
//! Home/End arms they share behavior with.
//!
//! SHIFT is deliberately not part of any pattern: it rides along to
//! extend the selection (`CSI 1;4D` = Shift+Alt+Left), and both widgets
//! already grow their selection from the modifier. Matching on the
//! word bit alone keeps "extend by word" working for free.
//!
//! ## What this costs an app
//!
//! A focused editor now CONSUMES `Alt+b`/`Alt+f`/`Alt+d` and `Ctrl+W`,
//! which previously fell through to global actions. That is the price
//! of the feature — those are editing chords wherever text has focus,
//! exactly as in readline, and they reach the keymap normally whenever
//! no editor holds focus.
//!
//! OWNER: REACT (input widgets).

use crate::ui::{Key, Mods};

/// A word-wise editing gesture, resolved from every spelling a terminal
/// might use for it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum WordIntent {
    /// Caret to the previous word boundary (Shift extends).
    Left,
    /// Caret to the next word boundary (Shift extends).
    Right,
    /// Delete from the previous word boundary to the caret.
    DeleteBack,
    /// Delete from the caret to the next word boundary.
    DeleteForward,
}

/// Classify one key event as a word-wise gesture, or `None` when it is
/// ordinary input. See the module table for the spellings and why each
/// one exists.
pub(crate) fn word_intent(key: Key, mods: Mods) -> Option<WordIntent> {
    // `ui::Mods` carries no lock bits (the input→ui conversion drops
    // caps/num), so the raw value is already chord-clean.
    let alt = mods.contains(Mods::ALT);
    let ctrl = mods.contains(Mods::CTRL);
    if !alt && !ctrl {
        return None;
    }
    // AltGr arrives as Ctrl+Alt on Windows/Linux. It is an input
    // modifier, not an editing chord — including on the arrow/delete
    // forms — so an editor must leave the complete combination for an
    // app/terminal to interpret. Shift is deliberately transparent
    // below, where it means "extend the selection".
    if alt && ctrl {
        return None;
    }
    match key {
        Key::Left => Some(WordIntent::Left),
        Key::Right => Some(WordIntent::Right),
        Key::Backspace => Some(WordIntent::DeleteBack),
        Key::Delete => Some(WordIntent::DeleteForward),
        // The readline letters, ALT-only: `Ctrl+B`/`Ctrl+F` are
        // readline's CHARACTER steps (and Ctrl+D is delete-forward /
        // EOF), so claiming them for word motion would be wrong on the
        // very convention this table is quoting.
        Key::Char('b') if alt && !ctrl => Some(WordIntent::Left),
        Key::Char('f') if alt && !ctrl => Some(WordIntent::Right),
        Key::Char('d') if alt && !ctrl => Some(WordIntent::DeleteForward),
        // Ctrl+W: readline's `unix-word-rubout`, CTRL-only.
        Key::Char('w') if ctrl && !alt => Some(WordIntent::DeleteBack),
        _ => None,
    }
}

#[cfg(test)]
#[path = "edit_keys_tests.rs"]
mod tests;
