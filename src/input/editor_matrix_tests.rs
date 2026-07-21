//! The editor key matrix: one table pinning every decode an editor/textarea
//! relies on — all modifier combos on navigation keys, editor chords,
//! F1-F24, keypad distinction, and both wire dialects (legacy incl. xterm
//! modifyOtherKeys, and kitty CSI-u).
//!
//! OWNER: KERNEL. If a row here changes meaning, an editor breaks: treat
//! edits as contract changes, not test maintenance.

use super::{Event, KeyCode, KeyEvent, KeyEventKind, Mods, Parser};

fn decode_one(bytes: &[u8]) -> KeyEvent {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(bytes, &mut out);
    assert_eq!(out.len(), 1, "exactly one event for {bytes:?}: {out:?}");
    match out.remove(0) {
        Event::Key(k) => k,
        other => panic!("expected key for {bytes:?}, got {other:?}"),
    }
}

struct Row {
    name: &'static str,
    bytes: &'static [u8],
    code: KeyCode,
    mods: Mods,
    kind: KeyEventKind,
    keypad: bool,
}

const P: KeyEventKind = KeyEventKind::Press;
const S: Mods = Mods::SHIFT;
const A: Mods = Mods::ALT;
const C: Mods = Mods::CTRL;

fn row(name: &'static str, bytes: &'static [u8], code: KeyCode, mods: Mods) -> Row {
    Row {
        name,
        bytes,
        code,
        mods,
        kind: P,
        keypad: false,
    }
}

#[test]
fn editor_key_matrix() {
    use KeyCode::*;
    let rows = vec![
        // ---- arrows: every modifier combo, legacy CSI 1;mods X ----
        row("up", b"\x1b[A", Up, Mods::NONE),
        row("shift+up (select)", b"\x1b[1;2A", Up, S),
        row("alt+up", b"\x1b[1;3A", Up, A),
        row("alt+shift+up", b"\x1b[1;4A", Up, Mods(S.0 | A.0)),
        row("ctrl+up", b"\x1b[1;5A", Up, C),
        row(
            "ctrl+shift+up (word-select up)",
            b"\x1b[1;6A",
            Up,
            Mods(C.0 | S.0),
        ),
        row("ctrl+alt+up", b"\x1b[1;7A", Up, Mods(C.0 | A.0)),
        row("ctrl+alt+shift+up", b"\x1b[1;8A", Up, Mods(C.0 | A.0 | S.0)),
        // The editor chords, spelled out on every arrow.
        row("ctrl+left (word jump)", b"\x1b[1;5D", Left, C),
        row("ctrl+right (word jump)", b"\x1b[1;5C", Right, C),
        row("shift+left (selection)", b"\x1b[1;2D", Left, S),
        row("shift+right (selection)", b"\x1b[1;2C", Right, S),
        row(
            "ctrl+shift+left (word selection)",
            b"\x1b[1;6D",
            Left,
            Mods(C.0 | S.0),
        ),
        row(
            "ctrl+shift+right (word selection)",
            b"\x1b[1;6C",
            Right,
            Mods(C.0 | S.0),
        ),
        row("shift+down", b"\x1b[1;2B", Down, S),
        // ---- home/end/pgup/pgdn/insert/delete, legacy forms + mods ----
        row("home (letter)", b"\x1b[H", Home, Mods::NONE),
        row("end (letter)", b"\x1b[F", End, Mods::NONE),
        row("home (tilde)", b"\x1b[1~", Home, Mods::NONE),
        row("end (tilde)", b"\x1b[4~", End, Mods::NONE),
        row("shift+home (select to BOL)", b"\x1b[1;2H", Home, S),
        row("shift+end (select to EOL)", b"\x1b[1;2F", End, S),
        row("ctrl+home (doc start)", b"\x1b[1;5H", Home, C),
        row("ctrl+end (doc end)", b"\x1b[1;5F", End, C),
        row("pgup", b"\x1b[5~", PageUp, Mods::NONE),
        row("pgdn", b"\x1b[6~", PageDown, Mods::NONE),
        row("shift+pgup", b"\x1b[5;2~", PageUp, S),
        row("ctrl+pgdn (tab switch)", b"\x1b[6;5~", PageDown, C),
        row("insert", b"\x1b[2~", Insert, Mods::NONE),
        row("delete", b"\x1b[3~", Delete, Mods::NONE),
        row("shift+delete (cut)", b"\x1b[3;2~", Delete, S),
        row("ctrl+delete (word delete)", b"\x1b[3;5~", Delete, C),
        // ---- enter/tab/backspace: modified forms need a protocol ----
        row("enter", b"\r", Enter, Mods::NONE),
        row("tab", b"\t", Tab, Mods::NONE),
        row("backspace", b"\x7f", Backspace, Mods::NONE),
        row("shift+tab (backtab)", b"\x1b[Z", Tab, S),
        // xterm modifyOtherKeys=2 (LEGACY terminals with the resource on):
        row("ctrl+enter (mok)", b"\x1b[27;5;13~", Enter, C),
        row("shift+enter (mok)", b"\x1b[27;2;13~", Enter, S),
        row(
            "ctrl+shift+tab (mok)",
            b"\x1b[27;6;9~",
            Tab,
            Mods(C.0 | S.0),
        ),
        row("ctrl+backspace (mok)", b"\x1b[27;5;127~", Backspace, C),
        row(
            "ctrl+backspace bs-code (mok)",
            b"\x1b[27;5;8~",
            Backspace,
            C,
        ),
        // kitty CSI-u forms of the same chords:
        row("ctrl+enter (kitty)", b"\x1b[13;5u", Enter, C),
        row("shift+enter (kitty)", b"\x1b[13;2u", Enter, S),
        row("ctrl+tab (kitty)", b"\x1b[9;5u", Tab, C),
        row("ctrl+shift+tab (kitty)", b"\x1b[9;6u", Tab, Mods(C.0 | S.0)),
        row("ctrl+backspace (kitty)", b"\x1b[127;5u", Backspace, C),
        row("alt+backspace (kitty)", b"\x1b[127;3u", Backspace, A),
        // ---- alt chords (ESC prefix, legacy) ----
        row("alt+enter (esc prefix)", b"\x1b\r", Enter, A),
        row("alt+backspace (esc prefix)", b"\x1b\x7f", Backspace, A),
        // ---- ctrl+shift on characters (kitty only; legacy collapses) ----
        row(
            "ctrl+shift+z (redo)",
            b"\x1b[122;6u",
            Char('z'),
            Mods(C.0 | S.0),
        ),
        row(
            "ctrl+shift+k (kitty)",
            b"\x1b[107;6u",
            Char('k'),
            Mods(C.0 | S.0),
        ),
        // ---- function keys: F1-F12 legacy ----
        row("f1 (ss3)", b"\x1bOP", F(1), Mods::NONE),
        row("f2 (ss3)", b"\x1bOQ", F(2), Mods::NONE),
        row("f3 (ss3)", b"\x1bOR", F(3), Mods::NONE),
        row("f4 (ss3)", b"\x1bOS", F(4), Mods::NONE),
        row("f5", b"\x1b[15~", F(5), Mods::NONE),
        row("f6", b"\x1b[17~", F(6), Mods::NONE),
        row("f7", b"\x1b[18~", F(7), Mods::NONE),
        row("f8", b"\x1b[19~", F(8), Mods::NONE),
        row("f9", b"\x1b[20~", F(9), Mods::NONE),
        row("f10", b"\x1b[21~", F(10), Mods::NONE),
        row("f11", b"\x1b[23~", F(11), Mods::NONE),
        row("f12", b"\x1b[24~", F(12), Mods::NONE),
        row("shift+f5", b"\x1b[15;2~", F(5), S),
        row("ctrl+f1 (csi letter)", b"\x1b[1;5P", F(1), C),
        // ---- F13-F24: legacy tilde where defined, kitty always ----
        row("f13 (tilde)", b"\x1b[25~", F(13), Mods::NONE),
        row("f14 (tilde)", b"\x1b[26~", F(14), Mods::NONE),
        row("f15 (tilde)", b"\x1b[28~", F(15), Mods::NONE),
        row("f16 (tilde)", b"\x1b[29~", F(16), Mods::NONE),
        row("f17 (tilde)", b"\x1b[31~", F(17), Mods::NONE),
        row("f18 (tilde)", b"\x1b[32~", F(18), Mods::NONE),
        row("f19 (tilde)", b"\x1b[33~", F(19), Mods::NONE),
        row("f20 (tilde)", b"\x1b[34~", F(20), Mods::NONE),
        row("f13 (kitty)", b"\x1b[57376u", F(13), Mods::NONE),
        row("f21 (kitty)", b"\x1b[57384u", F(21), Mods::NONE),
        row("f22 (kitty)", b"\x1b[57385u", F(22), Mods::NONE),
        row("f23 (kitty)", b"\x1b[57386u", F(23), Mods::NONE),
        row("f24 (kitty)", b"\x1b[57387u", F(24), Mods::NONE),
        row("ctrl+f24 (kitty)", b"\x1b[57387;5u", F(24), C),
    ];

    for r in &rows {
        let k = decode_one(r.bytes);
        assert_eq!(k.code, r.code, "[{}] code for {:?}", r.name, r.bytes);
        assert_eq!(k.mods, r.mods, "[{}] mods for {:?}", r.name, r.bytes);
        assert_eq!(k.kind, r.kind, "[{}] kind", r.name);
        assert_eq!(k.keypad, r.keypad, "[{}] keypad flag", r.name);
    }

    // Split-invariance: the whole matrix byte-at-a-time must decode
    // identically (editors receive exactly these sequences over slow ssh).
    for r in &rows {
        let mut p = Parser::new();
        let mut out = Vec::new();
        for &b in r.bytes {
            p.feed(&[b], &mut out);
        }
        // A lone ESC prefix row needs the flush the reader would apply.
        p.flush_pending(&mut out);
        let keys: Vec<&KeyEvent> = out
            .iter()
            .filter_map(|e| match e {
                Event::Key(k) => Some(k),
                _ => None,
            })
            .collect();
        assert_eq!(keys.len(), 1, "[{}] split decode: {out:?}", r.name);
        assert_eq!(keys[0].code, r.code, "[{}] split code", r.name);
        assert_eq!(keys[0].mods, r.mods, "[{}] split mods", r.name);
    }
}

#[test]
fn keypad_distinction_when_the_protocol_can_tell() {
    // kitty keypad block: identity = main-key equivalent, keypad = true.
    let k = decode_one(b"\x1b[57399u"); // KP_0
    assert_eq!((k.code, k.keypad), (KeyCode::Char('0'), true));
    let k = decode_one(b"\x1b[57414u"); // KP_ENTER
    assert_eq!((k.code, k.keypad), (KeyCode::Enter, true));
    let k = decode_one(b"\x1b[57421;5u"); // ctrl+KP_PAGE_UP
    assert_eq!(
        (k.code, k.mods, k.keypad),
        (KeyCode::PageUp, Mods::CTRL, true)
    );
    let k = decode_one(b"\x1b[57413u"); // KP_ADD
    assert_eq!((k.code, k.keypad), (KeyCode::Char('+'), true));
    // Main-key twins stay unflagged.
    let k = decode_one(b"\x1b[13u");
    assert_eq!((k.code, k.keypad), (KeyCode::Enter, false));
    let k = decode_one(b"\x1b[5~");
    assert_eq!((k.code, k.keypad), (KeyCode::PageUp, false));
    // SS3 application-keypad (DECKPAM) legacy block.
    let k = decode_one(b"\x1bOp");
    assert_eq!((k.code, k.keypad), (KeyCode::Char('0'), true));
    let k = decode_one(b"\x1bOy");
    assert_eq!((k.code, k.keypad), (KeyCode::Char('9'), true));
    let k = decode_one(b"\x1bOM");
    assert_eq!((k.code, k.keypad), (KeyCode::Enter, true));
    let k = decode_one(b"\x1bOm");
    assert_eq!((k.code, k.keypad), (KeyCode::Char('-'), true));
    let k = decode_one(b"\x1bOX");
    assert_eq!((k.code, k.keypad), (KeyCode::Char('='), true));
    // Chord matching deliberately ignores the flag: a KP_ENTER satisfies
    // an Enter binding unless the app reads `keypad` itself.
    let k = decode_one(b"\x1b[57414u");
    assert!(k.chord_matches(KeyCode::Enter, Mods::NONE));
}

/// The combos LEGACY terminals genuinely cannot express — pinned so the
/// degradation is a documented fact, not a surprise. Kitty (or xterm's
/// modifyOtherKeys) is required for these; the APP degrades features,
/// the parser never guesses.
#[test]
fn legacy_undecidable_combos_documented() {
    // Ctrl+Enter == Enter on the legacy wire (both 0x0D).
    let k = decode_one(b"\r");
    assert_eq!((k.code, k.mods), (KeyCode::Enter, Mods::NONE));
    // Ctrl+Backspace arrives as 0x08, which is byte-identical to Ctrl+H;
    // we decode Ctrl+H (modern terminals send 0x7F for plain Backspace).
    let k = decode_one(b"\x08");
    assert_eq!((k.code, k.mods), (KeyCode::Char('h'), Mods::CTRL));
    // Shift+Enter == Enter; Tab==Ctrl+I (0x09).
    let k = decode_one(b"\t");
    assert_eq!((k.code, k.mods), (KeyCode::Tab, Mods::NONE));
    // Ctrl+Shift+letter collapses to Ctrl+letter (C0 bytes carry no case).
    let k = decode_one(b"\x1a"); // ctrl+z and ctrl+shift+z identically
    assert_eq!((k.code, k.mods), (KeyCode::Char('z'), Mods::CTRL));
}
