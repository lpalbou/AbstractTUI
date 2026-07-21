//! Kitty keyboard protocol (CSI u) decoding, including progressive
//! enhancement forms: alternate keys, event types, associated text.
//!
//! OWNER: KERNEL. Grammar (sw.kovidgoyal.net/kitty/keyboard-protocol):
//!
//! ```text
//! CSI unicode-code[:shifted[:base-layout]] ; mods[:event] [; text-codepoints] u
//! ```
//!
//! The first parameter's first subparam is the key identity (lowercase /
//! layout-independent); `shifted`/`base-layout` alternates are currently
//! dropped (cycle-2 candidate: expose for shortcut matching across
//! layouts). Modifiers are `1 + bitmask` sharing our `Mods` bit layout.

use super::params::CsiParams;
use super::{Event, KeyCode, KeyEvent, ModifierKey};
use crate::input::legacy::mods_and_kind;

pub(crate) fn decode_csi_u(p: &CsiParams) -> Option<Event> {
    let code_point = p.get_or(0, 0);
    if code_point == 0 {
        return None; // "CSI u" with no code is not a key
    }
    let (mods, kind) = mods_and_kind(p, 1);
    let code = map_code(code_point)?;

    // Associated text: parameter section 3 carries codepoints as subparams.
    // Only allocate when the terminal actually reported text.
    let mut text: Option<String> = None;
    let mut j = 0;
    while let Some(cp) = p.sub(2, j) {
        if cp != 0 {
            if let Some(c) = char::from_u32(cp) {
                text.get_or_insert_with(String::new).push(c);
            }
        }
        j += 1;
    }

    let mut ev = KeyEvent::new(code, mods);
    ev.kind = kind;
    ev.text = text;
    // Kitty's dedicated keypad block (KP_0..KP_BEGIN): identity stays the
    // main-key equivalent, the flag carries the location.
    ev.keypad = (57399..=57427).contains(&code_point);
    Some(Event::Key(ev))
}

/// Kitty reuses ASCII values for the C0-representable keys and a private
/// range (57344+) for everything a legacy terminal cannot express.
fn map_code(cp: u32) -> Option<KeyCode> {
    Some(match cp {
        9 => KeyCode::Tab,
        13 => KeyCode::Enter,
        27 => KeyCode::Esc,
        127 => KeyCode::Backspace,
        57358 => KeyCode::CapsLock,
        57359 => KeyCode::ScrollLock,
        57360 => KeyCode::NumLock,
        57361 => KeyCode::PrintScreen,
        57362 => KeyCode::Pause,
        57363 => KeyCode::Menu,
        // F13..=F35.
        57376..=57398 => KeyCode::F((cp - 57376 + 13) as u8),
        // Keypad digits and operators: report the character identity —
        // apps overwhelmingly want "5", not "KP_5" — and set
        // `KeyEvent::keypad` (cycle 6) so distinct bindings stay possible.
        57399..=57408 => KeyCode::Char(char::from_u32(b'0' as u32 + (cp - 57399))?),
        57409 => KeyCode::Char('.'),
        57410 => KeyCode::Char('/'),
        57411 => KeyCode::Char('*'),
        57412 => KeyCode::Char('-'),
        57413 => KeyCode::Char('+'),
        57414 => KeyCode::Enter,
        57415 => KeyCode::Char('='),
        57416 => KeyCode::Char(','),
        // Keypad navigation (num lock off).
        57417 => KeyCode::Left,
        57418 => KeyCode::Right,
        57419 => KeyCode::Up,
        57420 => KeyCode::Down,
        57421 => KeyCode::PageUp,
        57422 => KeyCode::PageDown,
        57423 => KeyCode::Home,
        57424 => KeyCode::End,
        57425 => KeyCode::Insert,
        57426 => KeyCode::Delete,
        // 57427 KP_BEGIN (numpad 5, num lock off) and media keys
        // 57428..=57440: preserved raw as Functional.
        57441 => KeyCode::Modifier(ModifierKey::LeftShift),
        57442 => KeyCode::Modifier(ModifierKey::LeftCtrl),
        57443 => KeyCode::Modifier(ModifierKey::LeftAlt),
        57444 => KeyCode::Modifier(ModifierKey::LeftSuper),
        57445 => KeyCode::Modifier(ModifierKey::LeftHyper),
        57446 => KeyCode::Modifier(ModifierKey::LeftMeta),
        57447 => KeyCode::Modifier(ModifierKey::RightShift),
        57448 => KeyCode::Modifier(ModifierKey::RightCtrl),
        57449 => KeyCode::Modifier(ModifierKey::RightAlt),
        57450 => KeyCode::Modifier(ModifierKey::RightSuper),
        57451 => KeyCode::Modifier(ModifierKey::RightHyper),
        57452 => KeyCode::Modifier(ModifierKey::RightMeta),
        57453 => KeyCode::Modifier(ModifierKey::IsoLevel3Shift),
        57454 => KeyCode::Modifier(ModifierKey::IsoLevel5Shift),
        // Any other private-range code stays visible for app bindings.
        57344..=63743 => KeyCode::Functional(cp),
        _ => KeyCode::Char(char::from_u32(cp)?),
    })
}

#[cfg(test)]
mod tests {
    use super::super::{KeyEventKind, Mods, Parser};
    use super::*;

    fn key(bytes: &[u8]) -> KeyEvent {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(bytes, &mut out);
        assert_eq!(out.len(), 1, "one event expected: {out:?}");
        match out.remove(0) {
            Event::Key(k) => k,
            other => panic!("expected key, got {other:?}"),
        }
    }

    #[test]
    fn plain_and_modified() {
        let k = key(b"\x1b[97u"); // 'a' disambiguated
        assert_eq!(
            (k.code, k.mods, k.kind),
            (KeyCode::Char('a'), Mods::NONE, KeyEventKind::Press)
        );
        let k = key(b"\x1b[97;5u"); // Ctrl+a
        assert_eq!((k.code, k.mods), (KeyCode::Char('a'), Mods::CTRL));
        let k = key(b"\x1b[99;7u"); // Ctrl+Alt+c
        assert_eq!(k.mods, Mods::CTRL | Mods::ALT);
    }

    #[test]
    fn event_types() {
        let k = key(b"\x1b[97;1:1u");
        assert_eq!(k.kind, KeyEventKind::Press);
        let k = key(b"\x1b[97;1:2u");
        assert_eq!(k.kind, KeyEventKind::Repeat);
        let k = key(b"\x1b[97;1:3u");
        assert_eq!(
            (k.code, k.kind),
            (KeyCode::Char('a'), KeyEventKind::Release)
        );
        let k = key(b"\x1b[97;5:3u"); // Ctrl+a released
        assert_eq!((k.mods, k.kind), (Mods::CTRL, KeyEventKind::Release));
    }

    #[test]
    fn alternate_keys_use_primary_identity() {
        // shift+a reported with shifted alternate: identity stays 'a'.
        let k = key(b"\x1b[97:65;2u");
        assert_eq!((k.code, k.mods), (KeyCode::Char('a'), Mods::SHIFT));
        // base-layout alternate present too (cyrillic layout example).
        let k = key(b"\x1b[1089:1057:99;2u");
        assert_eq!(k.code, KeyCode::Char('с'));
    }

    #[test]
    fn associated_text() {
        let k = key(b"\x1b[97;2;65u"); // shift+a producing "A"
        assert_eq!(k.code, KeyCode::Char('a'));
        assert_eq!(k.text.as_deref(), Some("A"));
        let k = key(b"\x1b[97;1;72:105u"); // multi-codepoint text "Hi"
        assert_eq!(k.text.as_deref(), Some("Hi"));
        let k = key(b"\x1b[97u");
        assert_eq!(k.text, None); // no allocation on the plain path
    }

    #[test]
    fn c0_identities() {
        assert_eq!(key(b"\x1b[13u").code, KeyCode::Enter);
        assert_eq!(key(b"\x1b[9;5u").code, KeyCode::Tab); // Ctrl+Tab!
        assert_eq!(key(b"\x1b[27u").code, KeyCode::Esc); // disambiguated Esc
        let k = key(b"\x1b[127;3u"); // Alt+Backspace
        assert_eq!((k.code, k.mods), (KeyCode::Backspace, Mods::ALT));
    }

    #[test]
    fn functional_range() {
        assert_eq!(key(b"\x1b[57376u").code, KeyCode::F(13));
        assert_eq!(key(b"\x1b[57398u").code, KeyCode::F(35));
        assert_eq!(key(b"\x1b[57363u").code, KeyCode::Menu);
        assert_eq!(key(b"\x1b[57358u").code, KeyCode::CapsLock);
        assert_eq!(
            key(b"\x1b[57441;2u").code,
            KeyCode::Modifier(ModifierKey::LeftShift)
        );
        assert_eq!(key(b"\x1b[57404u").code, KeyCode::Char('5')); // KP_5
        assert_eq!(key(b"\x1b[57414u").code, KeyCode::Enter); // KP_ENTER
        assert_eq!(key(b"\x1b[57428u").code, KeyCode::Functional(57428)); // media
    }

    #[test]
    fn lock_modifiers_and_matching() {
        // caps lock latched while pressing ctrl+a: 64<<... mods = 1+4+64.
        let k = key(b"\x1b[97;69u");
        assert!(k.mods.contains(Mods::CTRL) && k.mods.contains(Mods::CAPS_LOCK));
        assert_eq!(k.mods.without_locks(), Mods::CTRL);
    }

    #[test]
    fn query_reply_routes_to_caps_not_keys() {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(b"\x1b[?31u", &mut out);
        assert!(matches!(
            out[0],
            Event::CapsReply(crate::term::caps::CapsReply::KittyKeyboard { flags: 31 })
        ));
    }
}
