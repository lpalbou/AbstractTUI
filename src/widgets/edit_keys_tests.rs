//! The spelling table, pinned. Each case names the terminal that sends
//! it — a row that changes should be a deliberate decision about a real
//! emulator, not an accident.

use super::*;

/// `CSI 1;3D` / `CSI 1;3C` — kitty, WezTerm, ghostty, foot, xterm.
#[test]
fn alt_arrows_are_word_motion() {
    assert_eq!(
        word_intent(Key::Left, Mods::ALT),
        Some(WordIntent::Left),
        "Alt+Left"
    );
    assert_eq!(word_intent(Key::Right, Mods::ALT), Some(WordIntent::Right));
}

/// `CSI 1;5D` / `CSI 1;5C` — the Linux/Windows convention.
#[test]
fn ctrl_arrows_are_word_motion() {
    assert_eq!(word_intent(Key::Left, Mods::CTRL), Some(WordIntent::Left));
    assert_eq!(word_intent(Key::Right, Mods::CTRL), Some(WordIntent::Right));
}

/// `ESC b` / `ESC f` — macOS Terminal.app and iTerm2's Natural Text
/// Editing preset, the readline binding they both borrow.
#[test]
fn readline_letters_are_word_motion() {
    assert_eq!(
        word_intent(Key::Char('b'), Mods::ALT),
        Some(WordIntent::Left)
    );
    assert_eq!(
        word_intent(Key::Char('f'), Mods::ALT),
        Some(WordIntent::Right)
    );
}

/// Shift rides along so "extend selection by word" is the same gesture
/// (`CSI 1;4D` = Shift+Alt+Left).
#[test]
fn shift_extends_rather_than_disqualifying() {
    assert_eq!(
        word_intent(Key::Left, Mods::ALT | Mods::SHIFT),
        Some(WordIntent::Left)
    );
    assert_eq!(
        word_intent(Key::Right, Mods::CTRL | Mods::SHIFT),
        Some(WordIntent::Right)
    );
}

/// Option+Backspace (`ESC DEL`), Ctrl+W (readline `unix-word-rubout`),
/// Alt+D (`ESC d`), Ctrl+Delete (`CSI 3;5~`).
#[test]
fn delete_word_family() {
    assert_eq!(
        word_intent(Key::Backspace, Mods::ALT),
        Some(WordIntent::DeleteBack)
    );
    assert_eq!(
        word_intent(Key::Char('w'), Mods::CTRL),
        Some(WordIntent::DeleteBack)
    );
    assert_eq!(
        word_intent(Key::Char('d'), Mods::ALT),
        Some(WordIntent::DeleteForward)
    );
    assert_eq!(
        word_intent(Key::Delete, Mods::CTRL),
        Some(WordIntent::DeleteForward)
    );
}

/// Unmodified keys are ordinary input — the classifier must never claim
/// a plain arrow, a plain letter, or a bare Backspace.
#[test]
fn bare_keys_are_never_word_gestures() {
    for key in [
        Key::Left,
        Key::Right,
        Key::Backspace,
        Key::Delete,
        Key::Char('b'),
        Key::Char('f'),
        Key::Char('d'),
        Key::Char('w'),
    ] {
        assert_eq!(word_intent(key, Mods::NONE), None, "{key:?} unmodified");
        assert_eq!(
            word_intent(key, Mods::SHIFT),
            None,
            "{key:?} with Shift alone"
        );
    }
}

/// The letters are claimed on ONE modifier only. `Ctrl+B`/`Ctrl+F` are
/// readline's character steps and `Ctrl+D` is delete-forward/EOF —
/// claiming them here would contradict the convention being quoted, and
/// would eat chords apps legitimately bind (abstractcode-tui binds
/// Ctrl+D today).
#[test]
fn letters_do_not_answer_to_the_other_modifier() {
    assert_eq!(word_intent(Key::Char('b'), Mods::CTRL), None);
    assert_eq!(word_intent(Key::Char('f'), Mods::CTRL), None);
    assert_eq!(word_intent(Key::Char('d'), Mods::CTRL), None);
    assert_eq!(word_intent(Key::Char('w'), Mods::ALT), None);
    // ...and neither answers to BOTH held together. AltGr is reported
    // as Ctrl+Alt on Windows/Linux, so this guard applies to every row
    // in the table, not only the printable-letter spellings.
    assert_eq!(word_intent(Key::Char('b'), Mods::ALT | Mods::CTRL), None);
    assert_eq!(word_intent(Key::Char('w'), Mods::ALT | Mods::CTRL), None);
    assert_eq!(word_intent(Key::Left, Mods::ALT | Mods::CTRL), None);
    assert_eq!(word_intent(Key::Backspace, Mods::ALT | Mods::CTRL), None);
}

/// Letters outside the table stay the app's (Alt+X, Ctrl+L, …).
#[test]
fn unrelated_chords_fall_through() {
    for ch in ['a', 'c', 'e', 'l', 'q', 't', 'x', 'z'] {
        assert_eq!(word_intent(Key::Char(ch), Mods::ALT), None, "Alt+{ch}");
        assert_eq!(word_intent(Key::Char(ch), Mods::CTRL), None, "Ctrl+{ch}");
    }
}
