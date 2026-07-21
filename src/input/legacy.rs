//! Legacy escape-sequence key decoding: C0 controls, CSI letter finals,
//! CSI tilde keys, SS3 forms, and the xterm modifier-parameter convention.
//!
//! OWNER: KERNEL. These are the encodings every terminal emits by default;
//! the kitty protocol (kitty.rs) supersedes them when negotiated, but both
//! share this file's modifier decoding because kitty reuses the legacy
//! forms for arrows/Home/End/F1-F4 even in enhanced mode.

use super::params::CsiParams;
use super::{Event, KeyCode, KeyEvent, KeyEventKind, Mods};

/// Decode the xterm/kitty modifier parameter (`1 + bitmask`), plus kitty's
/// event-type subparameter when present (`mods:event`).
pub(crate) fn mods_and_kind(p: &CsiParams, idx: usize) -> (Mods, KeyEventKind) {
    let raw = p.get_or(idx, 1);
    let mods = Mods(raw.saturating_sub(1).min(255) as u8);
    let kind = match p.sub(idx, 1) {
        Some(2) => KeyEventKind::Repeat,
        Some(3) => KeyEventKind::Release,
        _ => KeyEventKind::Press,
    };
    (mods, kind)
}

/// C0 control bytes as keys. Raw mode disables ISIG/IXON, so Ctrl+C/S/Q
/// arrive here as ordinary bytes — policy (quit? copy?) belongs to the app.
pub(crate) fn control_key(b: u8) -> KeyEvent {
    match b {
        0x0d => KeyEvent::plain(KeyCode::Enter),
        0x09 => KeyEvent::plain(KeyCode::Tab),
        0x7f => KeyEvent::plain(KeyCode::Backspace),
        0x1b => KeyEvent::plain(KeyCode::Esc),
        // NUL is what Ctrl+Space produces.
        0x00 => KeyEvent::new(KeyCode::Char(' '), Mods::CTRL),
        // 0x01..=0x1a: Ctrl+letter, including the historic aliases the
        // terminal cannot distinguish (0x08 = Ctrl+H vs Backspace — modern
        // terminals send 0x7F for Backspace; 0x09/0x0D excluded above).
        0x01..=0x1a => KeyEvent::new(KeyCode::Char((b'a' + (b - 1)) as char), Mods::CTRL),
        0x1c => KeyEvent::new(KeyCode::Char('\\'), Mods::CTRL),
        0x1d => KeyEvent::new(KeyCode::Char(']'), Mods::CTRL),
        0x1e => KeyEvent::new(KeyCode::Char('^'), Mods::CTRL),
        0x1f => KeyEvent::new(KeyCode::Char('_'), Mods::CTRL),
        // Unreachable by construction (caller filters), but total anyway.
        _ => KeyEvent::plain(KeyCode::Unidentified),
    }
}

fn named_final(f: u8) -> Option<KeyCode> {
    Some(match f {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        // CSI E is keypad "begin" (numpad 5); map to a named F-key slot is
        // wrong — leave unidentified but consumed.
        b'E' => KeyCode::Unidentified,
        b'F' => KeyCode::End,
        b'H' => KeyCode::Home,
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'R' => KeyCode::F(3), // only when param0 == 1 (parser guards CPR)
        b'S' => KeyCode::F(4),
        _ => return None,
    })
}

/// `CSI [1;mods[:event]] X` letter-final keys (arrows, Home/End, F1-F4,
/// Shift+Tab). Kitty keeps these forms in enhanced mode, adding the event
/// subparameter, which is why `mods_and_kind` handles both worlds.
pub(crate) fn decode_named(f: u8, p: &CsiParams) -> Option<Event> {
    if f == b'Z' {
        // CSI Z = Shift+Tab ("backtab"); an explicit modifier param may add
        // more (e.g. CSI 1;5Z would be Ctrl+Shift+Tab).
        let (mods, kind) = mods_and_kind(p, 1);
        let mut ev = KeyEvent::new(KeyCode::Tab, mods | Mods::SHIFT);
        ev.kind = kind;
        return Some(Event::Key(ev));
    }
    let code = named_final(f)?;
    let (mods, kind) = mods_and_kind(p, 1);
    let mut ev = KeyEvent::new(code, mods);
    ev.kind = kind;
    Some(Event::Key(ev))
}

/// `CSI code [;mods[:event]] ~` keys. Includes kitty's relocated F3 (13~),
/// the xterm extended function rows, and xterm's modifyOtherKeys form
/// (`CSI 27 ; mods ; codepoint ~`) — the LEGACY path that makes editor
/// chords like Ctrl+Enter expressible without the kitty protocol.
pub(crate) fn decode_tilde(p: &CsiParams) -> Option<Event> {
    if p.get_or(0, 0) == 27 && p.len() >= 3 {
        // modifyOtherKeys=2: codepoint carried as the THIRD parameter.
        let cp = p.get_or(2, 0);
        let code = match cp {
            13 => KeyCode::Enter,
            9 => KeyCode::Tab,
            27 => KeyCode::Esc,
            127 | 8 => KeyCode::Backspace,
            _ => KeyCode::Char(char::from_u32(cp)?),
        };
        let (mods, kind) = mods_and_kind(p, 1);
        let mut ev = KeyEvent::new(code, mods);
        ev.kind = kind;
        return Some(Event::Key(ev));
    }
    let code = match p.get_or(0, 0) {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        11 => KeyCode::F(1),
        12 => KeyCode::F(2),
        13 => KeyCode::F(3),
        14 => KeyCode::F(4),
        15 => KeyCode::F(5),
        17 => KeyCode::F(6),
        18 => KeyCode::F(7),
        19 => KeyCode::F(8),
        20 => KeyCode::F(9),
        21 => KeyCode::F(10),
        23 => KeyCode::F(11),
        24 => KeyCode::F(12),
        25 => KeyCode::F(13),
        26 => KeyCode::F(14),
        28 => KeyCode::F(15),
        29 => KeyCode::F(16),
        31 => KeyCode::F(17),
        32 => KeyCode::F(18),
        33 => KeyCode::F(19),
        34 => KeyCode::F(20),
        _ => return None,
    };
    let (mods, kind) = mods_and_kind(p, 1);
    let mut ev = KeyEvent::new(code, mods);
    ev.kind = kind;
    Some(Event::Key(ev))
}

/// `ESC O X` (SS3) keys: application cursor mode arrows, F1-F4, and the
/// application-keypad block (DECKPAM). No modifier grammar here —
/// modified keys switch to the CSI forms. Keypad finals set
/// `KeyEvent::keypad`; identities stay the main-key equivalents so
/// bindings work unchanged.
pub(crate) fn decode_ss3(f: u8) -> Option<Event> {
    let (code, keypad) = match f {
        b'A' => (KeyCode::Up, false),
        b'B' => (KeyCode::Down, false),
        b'C' => (KeyCode::Right, false),
        b'D' => (KeyCode::Left, false),
        b'F' => (KeyCode::End, false),
        b'H' => (KeyCode::Home, false),
        b'P' => (KeyCode::F(1), false),
        b'Q' => (KeyCode::F(2), false),
        b'R' => (KeyCode::F(3), false),
        b'S' => (KeyCode::F(4), false),
        // DECKPAM application-keypad block (vt100 lineage).
        b'M' => (KeyCode::Enter, true),
        b'j' => (KeyCode::Char('*'), true),
        b'k' => (KeyCode::Char('+'), true),
        b'l' => (KeyCode::Char(','), true),
        b'm' => (KeyCode::Char('-'), true),
        b'n' => (KeyCode::Char('.'), true),
        b'o' => (KeyCode::Char('/'), true),
        b'p'..=b'y' => (KeyCode::Char((b'0' + (f - b'p')) as char), true),
        b'X' => (KeyCode::Char('='), true),
        _ => return None,
    };
    let mut ev = KeyEvent::plain(code);
    ev.keypad = keypad;
    Some(Event::Key(ev))
}

#[cfg(test)]
mod tests {
    use super::super::Parser;
    use super::*;

    fn events(bytes: &[u8]) -> Vec<Event> {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(bytes, &mut out);
        out
    }

    fn single_key(bytes: &[u8]) -> KeyEvent {
        let evs = events(bytes);
        assert_eq!(evs.len(), 1, "expected one event for {bytes:?}: {evs:?}");
        match &evs[0] {
            Event::Key(k) => k.clone(),
            other => panic!("expected key, got {other:?}"),
        }
    }

    #[test]
    fn plain_arrows_and_navigation() {
        assert_eq!(single_key(b"\x1b[A").code, KeyCode::Up);
        assert_eq!(single_key(b"\x1b[B").code, KeyCode::Down);
        assert_eq!(single_key(b"\x1b[C").code, KeyCode::Right);
        assert_eq!(single_key(b"\x1b[D").code, KeyCode::Left);
        assert_eq!(single_key(b"\x1b[H").code, KeyCode::Home);
        assert_eq!(single_key(b"\x1b[F").code, KeyCode::End);
        assert_eq!(single_key(b"\x1b[5~").code, KeyCode::PageUp);
        assert_eq!(single_key(b"\x1b[6~").code, KeyCode::PageDown);
        assert_eq!(single_key(b"\x1b[2~").code, KeyCode::Insert);
        assert_eq!(single_key(b"\x1b[3~").code, KeyCode::Delete);
        assert_eq!(single_key(b"\x1b[1~").code, KeyCode::Home);
        assert_eq!(single_key(b"\x1b[4~").code, KeyCode::End);
    }

    #[test]
    fn modified_arrows_and_tilde_keys() {
        let k = single_key(b"\x1b[1;5A"); // Ctrl+Up
        assert_eq!((k.code, k.mods), (KeyCode::Up, Mods::CTRL));
        let k = single_key(b"\x1b[1;2D"); // Shift+Left
        assert_eq!((k.code, k.mods), (KeyCode::Left, Mods::SHIFT));
        let k = single_key(b"\x1b[3;3~"); // Alt+Delete
        assert_eq!((k.code, k.mods), (KeyCode::Delete, Mods::ALT));
        let k = single_key(b"\x1b[1;8H"); // Ctrl+Alt+Shift+Home
        assert_eq!(k.mods, Mods::CTRL | Mods::ALT | Mods::SHIFT);
    }

    #[test]
    fn function_keys_all_encodings() {
        // SS3 (F1-F4 default), CSI-letter (modified), CSI-tilde (F5+).
        assert_eq!(single_key(b"\x1bOP").code, KeyCode::F(1));
        assert_eq!(single_key(b"\x1bOS").code, KeyCode::F(4));
        let k = single_key(b"\x1b[1;5P"); // Ctrl+F1
        assert_eq!((k.code, k.mods), (KeyCode::F(1), Mods::CTRL));
        assert_eq!(single_key(b"\x1b[15~").code, KeyCode::F(5));
        assert_eq!(single_key(b"\x1b[24~").code, KeyCode::F(12));
        assert_eq!(single_key(b"\x1b[13~").code, KeyCode::F(3)); // kitty F3
        let k = single_key(b"\x1b[24;2~"); // Shift+F12
        assert_eq!((k.code, k.mods), (KeyCode::F(12), Mods::SHIFT));
    }

    #[test]
    fn shift_tab_and_controls() {
        let k = single_key(b"\x1b[Z");
        assert_eq!((k.code, k.mods), (KeyCode::Tab, Mods::SHIFT));
        assert_eq!(single_key(b"\r").code, KeyCode::Enter);
        assert_eq!(single_key(b"\t").code, KeyCode::Tab);
        assert_eq!(single_key(b"\x7f").code, KeyCode::Backspace);
        let k = single_key(b"\x03"); // Ctrl+C as a byte, policy-free
        assert_eq!((k.code, k.mods), (KeyCode::Char('c'), Mods::CTRL));
        let k = single_key(b"\x00");
        assert_eq!((k.code, k.mods), (KeyCode::Char(' '), Mods::CTRL));
        let k = single_key(b"\x1f");
        assert_eq!((k.code, k.mods), (KeyCode::Char('_'), Mods::CTRL));
    }

    #[test]
    fn alt_prefixed_keys() {
        let k = single_key(b"\x1bx");
        assert_eq!((k.code, k.mods), (KeyCode::Char('x'), Mods::ALT));
        let k = single_key(b"\x1b\x7f"); // Alt+Backspace
        assert_eq!((k.code, k.mods), (KeyCode::Backspace, Mods::ALT));
        let k = single_key(b"\x1b\r"); // Alt+Enter
        assert_eq!((k.code, k.mods), (KeyCode::Enter, Mods::ALT));
        let k = single_key(b"\x1b\x03"); // Alt+Ctrl+C
        assert_eq!(
            (k.code, k.mods),
            (KeyCode::Char('c'), Mods::ALT | Mods::CTRL)
        );
        // Alt + non-ASCII: ESC then UTF-8 bytes.
        let k = single_key("\x1bé".as_bytes());
        assert_eq!((k.code, k.mods), (KeyCode::Char('é'), Mods::ALT));
    }

    #[test]
    fn cpr_vs_f3_disambiguation() {
        // CSI 12;40R: a cursor position report, not F3.
        let evs = events(b"\x1b[12;40R");
        assert!(
            matches!(
                evs[0],
                Event::CapsReply(crate::term::caps::CapsReply::CursorPos { row: 12, col: 40 })
            ),
            "{evs:?}"
        );
        // CSI 1;5R: F3 with Ctrl (param0 == 1 marks the key form).
        let k = single_key(b"\x1b[1;5R");
        assert_eq!((k.code, k.mods), (KeyCode::F(3), Mods::CTRL));
    }

    #[test]
    fn release_kind_via_subparam() {
        // kitty event-type subparam on a legacy form: CSI 1;1:3B = Up
        // released... (B=Down key; use Down to keep the table honest).
        let k = single_key(b"\x1b[1;1:3B");
        assert_eq!(k.code, KeyCode::Down);
        assert_eq!(k.kind, KeyEventKind::Release);
        assert_eq!(k.mods, Mods::NONE);
    }
}
