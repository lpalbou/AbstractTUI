//! REDTEAM cycle-2 attack: KERNEL's input parser + event reader + probe.
//!
//! Invariants under attack (src/input/parser.rs header + term-input.md):
//! never panic, split-feed == one-shot feed, bounded buffers, zero
//! garbage-to-text leakage, ESC deadlines, probe never hangs and never
//! leaks late replies as text.

use std::time::{Duration, Instant};

use abstracttui::base::Size;
use abstracttui::input::{Event, EventReader, KeyCode, KeyEvent, Mods, Parser};
use abstracttui::term::{Capabilities, EnterOptions, KittyFlags, Terminal};
use abstracttui::testing::fuzzish::{self, Rng};
use abstracttui::testing::{hostile_corpus, CaptureTerm};

fn parse_all(chunks: &[Vec<u8>]) -> Vec<Event> {
    let mut p = Parser::new();
    let mut out = Vec::new();
    for c in chunks {
        p.feed(c, &mut out);
    }
    p.finish(&mut out);
    out
}

// ---------------------------------------------------------------- fuzzing

/// Cycle-7: keypad events survive the parser and carry the `keypad` flag
/// while keeping their MAIN-key identity (so bindings work unchanged),
/// and they split-invariantly like every other sequence. SS3 DECKPAM
/// forms (`ESC O M/j..y/X`) are the wire the application-keypad emits.
#[test]
fn ss3_keypad_events_carry_flag_and_identity() {
    // (bytes, expected code, expected keypad).
    let cases: &[(&[u8], KeyCode, bool)] = &[
        (b"\x1bOM", KeyCode::Enter, true),     // keypad Enter
        (b"\x1bOp", KeyCode::Char('0'), true), // keypad 0
        (b"\x1bOy", KeyCode::Char('9'), true), // keypad 9
        (b"\x1bOk", KeyCode::Char('+'), true), // keypad +
        (b"\x1bOA", KeyCode::Up, false),       // SS3 arrow (NOT keypad)
        (b"\x1bOP", KeyCode::F(1), false),     // SS3 F1 (NOT keypad)
    ];
    for (bytes, code, keypad) in cases {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(bytes, &mut out);
        p.finish(&mut out);
        let key = out.iter().find_map(|e| match e {
            Event::Key(k) => Some(k),
            _ => None,
        });
        let key = key.unwrap_or_else(|| panic!("no key event for {bytes:?}"));
        assert_eq!(key.code, *code, "wrong identity for {bytes:?}");
        assert_eq!(key.keypad, *keypad, "wrong keypad flag for {bytes:?}");
    }

    // Split-invariance of a keypad sequence: any chunking is identical.
    let seq = b"\x1bOp\x1bOM\x1bOk";
    let whole = {
        let mut p = Parser::new();
        let mut o = Vec::new();
        p.feed(seq, &mut o);
        p.finish(&mut o);
        o
    };
    for split in 1..seq.len() {
        let mut p = Parser::new();
        let mut o = Vec::new();
        p.feed(&seq[..split], &mut o);
        p.feed(&seq[split..], &mut o);
        p.finish(&mut o);
        assert_eq!(o, whole, "keypad split at {split} diverged");
    }
}

/// Cycle-7: the KeyEvent constructors KERNEL added (so construction sites
/// survive field additions) produce the expected events, and `keypad`
/// defaults false on all of them.
#[test]
fn keyevent_constructors_default_keypad_false() {
    assert_eq!(KeyEvent::char('a').code, KeyCode::Char('a'));
    assert!(!KeyEvent::char('a').keypad);
    assert_eq!(KeyEvent::plain(KeyCode::Enter).code, KeyCode::Enter);
    assert!(!KeyEvent::plain(KeyCode::Enter).keypad);
    assert!(!KeyEvent::new(KeyCode::Char('z'), Mods::CTRL).keypad);
    assert_eq!(
        KeyEvent::new(KeyCode::Char('z'), Mods::CTRL).mods,
        Mods::CTRL
    );
}

#[test]
fn parser_survives_hostile_corpus_and_finish() {
    // 1200 hostile chunks, each into a fresh parser AND all into one
    // parser (state carries across chunks) — no panic is the assertion.
    let corpus = hostile_corpus(0x6b_39c2, 1200);
    let mut streaming = Parser::new();
    let mut sink = Vec::new();
    for chunk in &corpus {
        let mut fresh = Parser::new();
        fresh.feed(chunk, &mut sink);
        fresh.finish(&mut sink);
        streaming.feed(chunk, &mut sink);
        sink.clear();
    }
    streaming.finish(&mut sink);
}

/// THE property: any chunking of the same bytes yields the same events.
/// finish() flushes pending state so a trailing partial cannot differ.
#[test]
fn split_feed_equals_one_shot_feed() {
    let mut rng = Rng::new(0x51ee7);
    // Realistic + hostile streams.
    let mut streams: Vec<Vec<u8>> = vec![
        b"hello\x1b[A\x1b[1;5C\x1bOP\x1b[Z".to_vec(),
        b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<64;3;3M".to_vec(),
        b"\x1b[200~pasted \x1b[201 text\x1b[201~tail".to_vec(),
        b"\x1b[?1;2;4c\x1b[?2026;2$y\x1bP>|kitty 0.38\x1b\\".to_vec(),
        "héllo 日本 🎉\u{1b}[Iworld\u{1b}[O".as_bytes().to_vec(),
        b"\x1b[97;5u\x1b[13;2u\x1b[57441;1u".to_vec(),
    ];
    for _ in 0..60 {
        let mut soup = Vec::new();
        for _ in 0..rng.range(1, 5) {
            soup.extend(fuzzish::sequence_shaped(&mut rng));
            soup.extend(fuzzish::truncated_utf8(&mut rng));
            soup.extend(fuzzish::random_chunk(&mut rng, 24));
        }
        streams.push(soup);
    }
    for (i, stream) in streams.iter().enumerate() {
        let one_shot = parse_all(std::slice::from_ref(stream));
        for max in [1usize, 2, 3, 5, 9] {
            let parts = fuzzish::random_splits(&mut rng, stream, max);
            let split = parse_all(&parts);
            assert_eq!(
                one_shot, split,
                "stream {i}: split(max {max}) diverged from one-shot"
            );
        }
    }
}

#[test]
fn all_single_bytes_and_esc_pairs_are_safe() {
    let mut sink = Vec::new();
    for b in 0..=255u8 {
        let mut p = Parser::new();
        p.feed(&[b], &mut sink);
        p.finish(&mut sink);
        sink.clear();
    }
    for b in 0..=255u8 {
        let mut p = Parser::new();
        p.feed(&[0x1b, b], &mut sink);
        p.feed(&[0x1b, b'[', b], &mut sink);
        p.finish(&mut sink);
        sink.clear();
    }
}

// ------------------------------------------------------ bounded buffers

#[test]
fn unterminated_csi_is_capped_and_resyncs() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    // 5000 parameter bytes then a real final byte: way past the 256-byte
    // cap. The parser aborts to Unknown at the cap and — correct resync —
    // keeps swallowing sequence bytes until the sequence's true final,
    // so the '1's never leak as text.
    let mut stream = vec![0x1b, b'['];
    stream.extend(std::iter::repeat_n(b'1', 5000));
    stream.push(b'H');
    p.feed(&stream, &mut out);
    assert!(
        out.iter().any(|e| matches!(e, Event::Unknown(_))),
        "capped CSI must surface as Unknown, got {out:?}"
    );
    for e in &out {
        match e {
            Event::Unknown(bytes) => {
                assert!(
                    bytes.len() <= 64,
                    "Unknown payload must be capped at 64 bytes"
                )
            }
            Event::Key(k) => panic!("capped-sequence bytes leaked as key {k:?}"),
            _ => {}
        }
    }
    // Frame sync: ordinary input parses normally after the final byte.
    out.clear();
    p.feed(b"x\x1b[A", &mut out);
    assert!(
        out.iter()
            .any(|e| matches!(e, Event::Key(k) if k.code == KeyCode::Char('x'))),
        "text after a terminated capped sequence must parse, got {out:?}"
    );
    assert!(out
        .iter()
        .any(|e| matches!(e, Event::Key(k) if k.code == KeyCode::Up)));
}

#[test]
fn giant_paste_flushes_in_bounded_chunks() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[200~", &mut out);
    // 200 KiB of paste content, fed in slabs.
    let slab = vec![b'p'; 8192];
    for _ in 0..25 {
        p.feed(&slab, &mut out);
    }
    p.feed(b"\x1b[201~", &mut out);
    let pastes: Vec<&String> = out
        .iter()
        .filter_map(|e| match e {
            Event::Paste(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(
        pastes.len() >= 2,
        "200 KiB must flush as multiple Paste events"
    );
    let total: usize = pastes.iter().map(|s| s.len()).sum();
    assert_eq!(total, 25 * 8192, "no pasted byte may be lost or invented");
    let max = pastes.iter().map(|s| s.len()).max().unwrap();
    assert!(max <= 100 * 1024, "single Paste event too large: {max}");
}

#[test]
fn esc_inside_paste_and_torn_terminator_survive() {
    // ESC[201 that never completes + real ESC inside content, split at
    // every byte — the classic terminator-scan traps.
    let stream = b"\x1b[200~abc\x1b[201Xdef\x1bghi\x1b[201~z";
    let one = parse_all(std::slice::from_ref(&stream.to_vec()));
    let content: String = one
        .iter()
        .filter_map(|e| match e {
            Event::Paste(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(content, "abc\x1b[201Xdef\x1bghi");
    // Byte-at-a-time must agree.
    let bytes: Vec<Vec<u8>> = stream.iter().map(|b| vec![*b]).collect();
    assert_eq!(one, parse_all(&bytes));
}

// ------------------------------------------------- garbage-to-text leakage

#[test]
fn swallowed_sequences_never_leak_text() {
    // Every one of these must produce ZERO Key(Char) events. (The X10
    // mouse case consumes exactly ESC[M + 3 payload bytes — " ab" here.)
    let hostile: &[&[u8]] = &[
        b"\x1b[M ab",              // X10 mouse: 3 payload bytes swallowed
        b"\x1b]0;fake title\x07",  // OSC consumed
        b"\x1bP+q544e\x1b\\",      // DCS consumed
        b"\x1b_Gi=1;OK\x1b\\",     // APC consumed
        b"\x1b[38;2;300;300;300m", // SGR is terminal-bound, not input
        b"\x1b[999999999999999H",
    ];
    for stream in hostile {
        let events = parse_all(std::slice::from_ref(&stream.to_vec()));
        for e in &events {
            assert!(
                !matches!(
                    e,
                    Event::Key(KeyEvent {
                        code: KeyCode::Char(_),
                        ..
                    })
                ),
                "sequence {:?} leaked text: {events:?}",
                String::from_utf8_lossy(stream)
            );
        }
    }
    // And the byte AFTER a swallowed X10 payload is honestly user input.
    let events = parse_all(&[b"\x1b[M abc".to_vec()]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Key(k) if k.code == KeyCode::Char('c'))),
        "the byte after the 3-byte X10 payload is real input: {events:?}"
    );
}

#[test]
fn kitty_and_legacy_keys_decode_exactly() {
    let events = parse_all(&[b"\x1b[97;5u".to_vec()]);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Key(k) => {
            assert_eq!(k.code, KeyCode::Char('a'));
            assert!(k.mods.contains(Mods::CTRL));
        }
        other => panic!("expected Ctrl+A, got {other:?}"),
    }
    // Shift+Tab, F5, Alt+Backspace.
    let events = parse_all(&[b"\x1b[Z\x1b[15~\x1b\x7f".to_vec()]);
    assert!(
        matches!(&events[0], Event::Key(k) if k.code == KeyCode::Tab && k.mods.contains(Mods::SHIFT))
    );
    assert!(matches!(&events[1], Event::Key(k) if k.code == KeyCode::F(5)));
    assert!(
        matches!(&events[2], Event::Key(k) if k.code == KeyCode::Backspace && k.mods.contains(Mods::ALT))
    );
}

/// KERNEL's confessed weak spot (a): `CSI 1;5R` — cursor-position report
/// grammar colliding with legacy Ctrl+F3. Their documented resolution:
/// param0 == 1 decodes as F3 (we never send DSR 6 in this cycle).
/// This test PINS the documented choice so any future DSR use must
/// revisit it consciously (finding RT2-K1 tracks the hazard).
#[test]
fn csi_1_5_r_ambiguity_pinned_as_f3() {
    let events = parse_all(&[b"\x1b[1;5R".to_vec()]);
    assert_eq!(events.len(), 1, "exactly one event from the ambiguous form");
    match &events[0] {
        Event::Key(k) => {
            assert_eq!(k.code, KeyCode::F(3), "documented resolution: F3 side");
            assert!(k.mods.contains(Mods::CTRL));
        }
        Event::CapsReply(r) => {
            panic!("resolution CHANGED to CPR side: {r:?} — update finding RT2-K1")
        }
        other => panic!("unexpected decode {other:?}"),
    }
    // Unambiguous CPR (param0 != 1) must be a caps reply, not a key.
    let events = parse_all(&[b"\x1b[24;80R".to_vec()]);
    assert!(
        matches!(&events[0], Event::CapsReply(_)),
        "CSI 24;80R must decode as a cursor-position report, got {events:?}"
    );
}

// ------------------------------------------------------- reader deadlines

/// Bare ESC resolves as the Esc key once the (virtual: zero) deadline
/// passes — no sleeping: esc_timeout = 0 makes the pending deadline
/// already-elapsed on the next poll.
#[test]
fn reader_bare_esc_resolves_after_deadline() {
    let mut term = CaptureTerm::new(Size::new(10, 2));
    term.push_input(b"\x1b");
    let mut reader = EventReader::new();
    reader.esc_timeout = Duration::ZERO;
    let deadline = Instant::now(); // already elapsed: poll never blocks
    let ev = reader.poll_event(&mut term, Some(deadline)).unwrap();
    assert_eq!(
        ev,
        Some(Event::Key(KeyEvent::plain(KeyCode::Esc))),
        "bare ESC + expired deadline must deliver the Esc key"
    );
}

/// The counter-case: ESC followed by the rest of a sequence in a LATER
/// chunk must still decode as one key when the deadline has not fired.
#[test]
fn reader_split_sequence_survives_when_deadline_generous() {
    let mut term = CaptureTerm::new(Size::new(10, 2));
    term.push_input(b"\x1b");
    term.push_input(b"[A"); // arrives "later" (next scripted read)
    let mut reader = EventReader::new(); // default 30ms/500ms: generous
    let ev = reader.poll_event(&mut term, Some(Instant::now())).unwrap();
    assert_eq!(
        ev,
        Some(Event::Key(KeyEvent::plain(KeyCode::Up))),
        "split ESC-[A with time to spare must decode as Up, not Esc+[+A"
    );
}

/// Torn-sequence deadline: ESC[ with nothing following resolves as
/// Unknown once seq_timeout (virtual zero) expires.
#[test]
fn reader_torn_sequence_flushes_as_unknown() {
    let mut term = CaptureTerm::new(Size::new(10, 2));
    term.push_input(b"\x1b[12;3");
    let mut reader = EventReader::new();
    reader.seq_timeout = Duration::ZERO;
    let ev = reader.poll_event(&mut term, Some(Instant::now())).unwrap();
    assert!(
        matches!(ev, Some(Event::Unknown(_))),
        "torn sequence past its deadline must flush as Unknown, got {ev:?}"
    );
}

#[test]
fn reader_deadline_expiry_returns_none() {
    let mut term = CaptureTerm::new(Size::new(10, 2));
    let mut reader = EventReader::new();
    let ev = reader.poll_event(&mut term, Some(Instant::now())).unwrap();
    assert_eq!(
        ev, None,
        "empty script + expired deadline = None, never hangs"
    );
}

#[test]
fn reader_resize_passthrough_orders_with_input() {
    let mut term = CaptureTerm::new(Size::new(10, 2));
    term.push_input(b"a");
    term.push_resize(Size::new(20, 5));
    term.push_input(b"b");
    let mut reader = EventReader::new();
    let d = Some(Instant::now());
    assert!(
        matches!(reader.poll_event(&mut term, d).unwrap(), Some(Event::Key(k)) if k.code == KeyCode::Char('a'))
    );
    assert!(
        matches!(reader.poll_event(&mut term, d).unwrap(), Some(Event::Resize(s)) if s == Size::new(20, 5))
    );
    assert!(
        matches!(reader.poll_event(&mut term, d).unwrap(), Some(Event::Key(k)) if k.code == KeyCode::Char('b'))
    );
}

// ------------------------------------------------------------- the probe

/// Mute terminal: the probe writes its queries, gets nothing, and ends at
/// the deadline with caps unchanged — never hangs (virtual deadline: zero
/// timeout + non-blocking CaptureTerm).
#[test]
fn probe_on_mute_terminal_ends_at_deadline() {
    let mut term = CaptureTerm::new(Size::new(80, 24));
    let mut reader = EventReader::new();
    let mut caps = Capabilities::default();
    let passthrough =
        abstracttui::input::probe_active(&mut term, &mut reader, &mut caps, Duration::ZERO)
            .unwrap();
    assert!(passthrough.is_empty());
    assert!(!caps.kitty_graphics && !caps.sixel);
    // The query batch itself was written (the terminal is not dumb).
    assert!(
        !term.bytes().is_empty(),
        "probe must have written its queries"
    );
}

/// Dumb terminal: not one query byte (their RT1-6b fix — verified here
/// from the outside).
#[test]
fn probe_on_dumb_terminal_writes_nothing() {
    let mut term = CaptureTerm::new(Size::new(80, 24));
    let mut reader = EventReader::new();
    let mut caps = Capabilities::with(|c| c.dumb = true);
    let passthrough =
        abstracttui::input::probe_active(&mut term, &mut reader, &mut caps, Duration::ZERO)
            .unwrap();
    assert!(passthrough.is_empty());
    assert!(
        term.bytes().is_empty(),
        "a dumb terminal must never be interrogated with escapes; wrote {:?}",
        String::from_utf8_lossy(term.bytes())
    );
}

/// Answering terminal: replies fold into caps; user keystrokes arriving
/// MID-PROBE come back in order instead of vanishing.
#[test]
fn probe_folds_replies_and_returns_user_input() {
    let mut term = CaptureTerm::new(Size::new(80, 24));
    // kitty keyboard reply, user mashing a key, sync-output DECRPM,
    // then the DA1 sentinel (with sixel attribute 4).
    term.push_input(b"\x1b[?1u");
    term.push_input(b"x");
    term.push_input(b"\x1b[?2026;2$y");
    term.push_input(b"\x1b[?62;4;22c");
    let mut reader = EventReader::new();
    let mut caps = Capabilities::default();
    let passthrough =
        abstracttui::input::probe_active(&mut term, &mut reader, &mut caps, Duration::ZERO)
            .unwrap();
    assert!(caps.kitty_keyboard && caps.sync_output_2026 && caps.sixel);
    assert_eq!(passthrough.len(), 1, "the keystroke must survive the probe");
    assert!(matches!(&passthrough[0], Event::Key(k) if k.code == KeyCode::Char('x')));
}

/// LATE replies (multiplexer passthrough answering after the sentinel):
/// they arrive in the ordinary event stream as CapsReply — never as text,
/// never as Unknown garbage (RT1-6c).
#[test]
fn late_probe_reply_is_a_caps_event_not_text() {
    let mut term = CaptureTerm::new(Size::new(80, 24));
    term.push_input(b"\x1b[?62c"); // DA1 sentinel: probe completes
                                   // ... probe done; late kitty-graphics reply + XTVERSION land later.
    term.push_input(b"\x1b_Gi=4242;OK\x1b\\");
    term.push_input(b"\x1bP>|kitty 0.40\x1b\\");
    term.push_input(b"k");
    let mut reader = EventReader::new();
    let mut caps = Capabilities::default();
    let _ = abstracttui::input::probe_active(&mut term, &mut reader, &mut caps, Duration::ZERO)
        .unwrap();
    // Drain what arrived after the probe closed.
    let d = Some(Instant::now());
    let mut after = Vec::new();
    while let Some(ev) = reader.poll_event(&mut term, d).unwrap() {
        after.push(ev);
    }
    assert!(
        after.iter().any(|e| matches!(e, Event::CapsReply(_))),
        "late replies must surface as CapsReply events: {after:?}"
    );
    assert!(
        after.iter().all(
            |e| !matches!(e, Event::Key(KeyEvent { code: KeyCode::Char(c), .. }) if *c != 'k')
        ),
        "late reply bytes leaked into text: {after:?}"
    );
    assert!(
        after
            .iter()
            .any(|e| matches!(e, Event::Key(k) if k.code == KeyCode::Char('k'))),
        "the real keystroke after the late replies must still arrive"
    );
}

// ------------------------------------------------- enter/leave discipline

/// Every DECSET from enter has a matching DECRST from leave, kitty push
/// has pop — asserted through the VT model's mode/counter tracking, for
/// every EnterOptions shape.
#[test]
fn enter_leave_balance_across_option_shapes() {
    use abstracttui::term::MouseMode;
    let shapes = [
        EnterOptions::default(),
        EnterOptions {
            kitty_keyboard: KittyFlags::standard(),
            ..EnterOptions::default()
        },
        EnterOptions {
            mouse: MouseMode::AnyMotion,
            ..EnterOptions::default()
        },
        EnterOptions {
            mouse: MouseMode::Off,
            alternate_screen: false,
            ..EnterOptions::default()
        },
        EnterOptions {
            alternate_screen: false,
            hide_cursor: false,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: KittyFlags(0),
        },
    ];
    for (i, opts) in shapes.iter().enumerate() {
        let mut term = CaptureTerm::new(Size::new(10, 4));
        term.enter(opts).unwrap();
        term.leave().unwrap();
        let screen = term.screen();
        let modes = screen.modes().all_set();
        // Boot posture is exactly what must remain: cursor visible (25)
        // and autowrap (7). Anything else set = unbalanced enter/leave.
        assert!(
            modes.iter().all(|m| *m == 25 || *m == 7),
            "shape {i}: modes still set after leave: {modes:?}"
        );
        assert!(
            screen.modes().cursor_visible(),
            "shape {i}: cursor must be restored"
        );
        assert_eq!(
            screen.counters().kitty_push_depth,
            0,
            "shape {i}: kitty pop missing"
        );
        assert_eq!(
            screen.unknown_seq_count(),
            0,
            "shape {i}: unmodeled bytes in enter/leave"
        );
    }
}
