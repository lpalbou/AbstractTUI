//! Parser framing tests: UTF-8 resumption, paste robustness, caps-reply
//! routing, garbage resilience. Key-family decode tests live next to their
//! decoders (legacy.rs / kitty.rs / mouse.rs).
//!
//! OWNER: KERNEL.

use super::parser::{Parser, UNKNOWN_CAP};
use super::{Event, KeyCode, KeyEvent, Mods};
use crate::term::caps::CapsReply;

fn feed_all(bytes: &[u8]) -> Vec<Event> {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(bytes, &mut out);
    out
}

/// Feed one byte at a time: every sequence must survive maximal splitting.
fn feed_split(bytes: &[u8]) -> Vec<Event> {
    let mut p = Parser::new();
    let mut out = Vec::new();
    for &b in bytes {
        p.feed(&[b], &mut out);
    }
    out
}

fn chars(evs: &[Event]) -> String {
    evs.iter()
        .filter_map(|e| match e {
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                ..
            }) => Some(*c),
            _ => None,
        })
        .collect()
}

// ---- UTF-8 ----

#[test]
fn utf8_multibyte_and_split_boundaries() {
    let text = "aé漢🎉z";
    // Whole chunk and byte-at-a-time must agree exactly.
    assert_eq!(chars(&feed_all(text.as_bytes())), text);
    assert_eq!(chars(&feed_split(text.as_bytes())), text);
    // Split inside the 4-byte emoji at every position.
    let bytes = "🎉".as_bytes();
    for cut in 1..bytes.len() {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(&bytes[..cut], &mut out);
        assert!(out.is_empty(), "no event before the char completes");
        p.feed(&bytes[cut..], &mut out);
        assert_eq!(chars(&out), "🎉", "cut at {cut}");
    }
}

#[test]
fn utf8_invalid_becomes_replacement_never_panics() {
    // Stray continuation byte.
    assert_eq!(chars(&feed_all(b"\x80")), "\u{fffd}");
    // Overlong-encoding lead bytes (0xC0/0xC1 are never valid).
    assert_eq!(chars(&feed_all(b"\xc0\xafA")), "\u{fffd}\u{fffd}A");
    // Lead byte then a non-continuation: replacement, then the byte
    // reprocesses on its own (here: 'A').
    assert_eq!(chars(&feed_all(b"\xe2\x82A")), "\u{fffd}A");
    // 0xF5..=0xFF are outside the Unicode range.
    assert_eq!(chars(&feed_all(b"\xffB")), "\u{fffd}B");
    // Surrogate half encoded as UTF-8 (CESU-8 style): rejected.
    assert_eq!(chars(&feed_all(b"\xed\xa0\x80")), "\u{fffd}");
}

#[test]
fn utf8_partial_resolves_at_finish() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\xe2\x82", &mut out); // first 2 bytes of '€'
    assert!(out.is_empty());
    p.finish(&mut out);
    assert_eq!(chars(&out), "\u{fffd}");
}

// ---- bracketed paste ----

fn paste_content(evs: &[Event]) -> String {
    evs.iter()
        .filter_map(|e| match e {
            Event::Paste(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn paste_basic_and_split_terminator() {
    let stream = b"\x1b[200~hello world\x1b[201~";
    assert_eq!(paste_content(&feed_all(stream)), "hello world");
    // The classic trap: the terminator split byte-by-byte across chunks.
    assert_eq!(paste_content(&feed_split(stream)), "hello world");
}

#[test]
fn paste_with_embedded_escapes() {
    // A paste containing ESC, a fake CSI, and a *partial* end marker.
    let stream = b"\x1b[200~a\x1b[Bb\x1b[201xc\x1b[201~";
    let evs = feed_all(stream);
    assert_eq!(paste_content(&evs), "a\x1b[Bb\x1b[201xc");
    // Nothing inside the paste leaked as key events.
    assert!(evs.iter().all(|e| matches!(e, Event::Paste(_))), "{evs:?}");
}

#[test]
fn paste_terminator_prefix_repeated() {
    // "ESC[201" (almost-terminator) followed by ESC starting the real one:
    // the mismatch replay must rescan from the new ESC, not drop it.
    let stream = b"\x1b[200~x\x1b[201\x1b[201~";
    assert_eq!(paste_content(&feed_all(stream)), "x\x1b[201");
    assert_eq!(paste_content(&feed_split(stream)), "x\x1b[201");
}

#[test]
fn unterminated_paste_flushes_at_finish() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[200~data with no end", &mut out);
    assert!(paste_content(&out).is_empty());
    p.finish(&mut out);
    assert_eq!(paste_content(&out), "data with no end");
}

#[test]
fn huge_paste_flushes_in_bounded_chunks() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[200~", &mut out);
    let block = vec![b'x'; 300 * 1024];
    p.feed(&block, &mut out);
    p.feed(b"\x1b[201~", &mut out);
    let total: usize = out
        .iter()
        .map(|e| match e {
            Event::Paste(s) => s.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(total, 300 * 1024, "no content lost");
    assert!(out.len() >= 4, "flushed in multiple bounded events");
}

/// Editor-grade paste: 5 MB of real multi-line content must stream
/// through in bounded chunks (never one 5 MB buffer, never truncation),
/// reassembling byte-exactly — newlines, CRs, tabs and multibyte text
/// included. The parser preserves BYTES; CR-vs-LF normalization of pasted
/// line endings is app policy (terminals conventionally convert LF to CR
/// in pastes — the editor decides, the parser must not).
#[test]
fn five_megabyte_multiline_paste_streams_bounded_and_exact() {
    use super::parser::PASTE_FLUSH;
    // ~40-byte line with every editor-relevant byte class, repeated past
    // 5 MB; content deliberately includes an almost-terminator.
    let line = "fn main() {\t// touché 🎉\r\n\x1b[201x}\n";
    let mut content = String::new();
    while content.len() < 5 * 1024 * 1024 {
        content.push_str(line);
    }
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[200~", &mut out);
    // Feed in awkward 7000-byte chunks so flush boundaries and UTF-8
    // splits land everywhere over ~750 chunk seams.
    let bytes = content.as_bytes();
    let mut i = 0;
    let mut reassembled = String::with_capacity(content.len());
    let mut max_event = 0usize;
    let mut events = 0usize;
    while i < bytes.len() {
        let end = (i + 7000).min(bytes.len());
        p.feed(&bytes[i..end], &mut out);
        i = end;
        for e in out.drain(..) {
            match e {
                Event::Paste(s) => {
                    max_event = max_event.max(s.len());
                    events += 1;
                    reassembled.push_str(&s);
                }
                other => panic!("non-paste event mid-paste: {other:?}"),
            }
        }
    }
    p.feed(b"\x1b[201~", &mut out);
    for e in out.drain(..) {
        if let Event::Paste(s) = e {
            max_event = max_event.max(s.len());
            events += 1;
            reassembled.push_str(&s);
        }
    }
    assert_eq!(reassembled, content, "byte-exact reassembly");
    // Bounded memory: no event (and thus no internal buffer) materially
    // exceeds the flush cap (+ a few bytes of half-matched terminator).
    assert!(
        max_event <= PASTE_FLUSH + 8,
        "event of {max_event} bytes exceeds the {PASTE_FLUSH} cap"
    );
    assert!(
        events >= content.len() / PASTE_FLUSH,
        "streamed, not hoarded"
    );
}

#[test]
fn paste_flush_boundary_preserves_half_matched_terminator() {
    // Fill the buffer to just under the flush threshold, then feed
    // ESC ESC: the second ESC re-arms the terminator match while the first
    // is replayed into content, crossing the flush threshold — the flush
    // must NOT reset the half-match, or the real terminator that follows
    // would be missed and leak as text.
    use super::parser::PASTE_FLUSH;
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[200~", &mut out);
    p.feed(&vec![b'x'; PASTE_FLUSH - 1], &mut out);
    p.feed(b"\x1b\x1b", &mut out);
    p.feed(b"[201~", &mut out);
    assert!(chars(&out).is_empty(), "terminator leaked as text: {out:?}");
    let content = paste_content(&out);
    assert_eq!(content.len(), PASTE_FLUSH - 1 + 1, "one ESC is content");
    assert!(content.ends_with('\x1b'));
}

#[test]
fn stray_paste_end_is_swallowed() {
    let evs = feed_all(b"\x1b[201~x");
    assert!(matches!(evs[0], Event::Unknown(_)), "{evs:?}");
    assert_eq!(chars(&evs), "x");
}

// ---- caps replies + string frames ----

#[test]
fn caps_replies_route_correctly() {
    let evs = feed_all(b"\x1b[?1u\x1b[?2026;1$y\x1b[?62;4;22c\x1b[12;40R");
    assert_eq!(
        evs,
        vec![
            Event::CapsReply(CapsReply::KittyKeyboard { flags: 1 }),
            Event::CapsReply(CapsReply::DecMode {
                mode: 2026,
                status: 1
            }),
            Event::CapsReply(CapsReply::PrimaryDa {
                params: vec![62, 4, 22]
            }),
            Event::CapsReply(CapsReply::CursorPos { row: 12, col: 40 }),
        ]
    );
}

#[test]
fn xtsmgraphics_and_winops_replies() {
    // XTSMGRAPHICS color-register report + XTWINOPS cell-size report
    // (CSI 16 t reply is CSI 6 ; height ; width t).
    let evs = feed_all(b"\x1b[?1;0;256S\x1b[6;18;9t\x1b[8;24;80t");
    assert_eq!(
        evs,
        vec![
            Event::CapsReply(CapsReply::XtSmGraphics {
                item: 1,
                status: 0,
                value: 256
            }),
            Event::CapsReply(CapsReply::WindowOp { op: 6, a: 18, b: 9 }),
            Event::CapsReply(CapsReply::WindowOp {
                op: 8,
                a: 24,
                b: 80
            }),
        ]
    );
    // Byte-at-a-time framing survives too.
    assert_eq!(feed_split(b"\x1b[?1;0;256S"), feed_all(b"\x1b[?1;0;256S"));
    // A bare CSI t (no op) is not a report: swallowed as Unknown.
    let evs = feed_all(b"\x1b[t");
    assert!(matches!(evs[0], Event::Unknown(_)), "{evs:?}");
}

#[test]
fn late_probe_replies_stay_caps_events() {
    // RT1-6c: a multiplexer answering long after the DA1 sentinel (the
    // probe is over, the app is running) must still decode as CapsReply —
    // never as text or Unknown key garbage. Every probe-batch reply frame,
    // fed AFTER a complete probe exchange, split at every byte.
    let full_probe_exchange =
        b"\x1b[?1u\x1b[?2026;2$y\x1b[?1;0;64S\x1b[6;20;10t\x1b_Gi=4242;OK\x1b\\\x1b[?62;4c";
    let late_replies: &[&[u8]] = &[
        b"\x1b[?0u",
        b"\x1b[?2026;1$y",
        b"\x1b[?1;0;16S",
        b"\x1b[6;18;9t",
        b"\x1b_Gi=4242;OK\x1b\\",
        b"\x1bP>|tmux 3.4\x1b\\",
        b"\x1b[?6c",
    ];
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(full_probe_exchange, &mut out);
    out.clear();
    for frame in late_replies {
        for &b in *frame {
            p.feed(&[b], &mut out); // worst-case splitting
        }
    }
    assert!(!out.is_empty());
    for ev in &out {
        assert!(
            matches!(ev, Event::CapsReply(_)),
            "late reply leaked as non-caps event: {ev:?}"
        );
    }
    // And the parser is still clean for real input afterwards.
    out.clear();
    p.feed(b"ok", &mut out);
    assert_eq!(chars(&out), "ok");
}

#[test]
fn dcs_xtversion_and_apc_kitty_frames() {
    let evs = feed_all(b"\x1bP>|WezTerm 20240203\x1b\\\x1b_Gi=4242;OK\x1b\\");
    assert_eq!(
        evs,
        vec![
            Event::CapsReply(CapsReply::XtVersion {
                text: "WezTerm 20240203".into()
            }),
            Event::CapsReply(CapsReply::KittyGraphics {
                raw: b"Gi=4242;OK".to_vec()
            }),
        ]
    );
    // Same, split byte-by-byte (ST across a chunk boundary).
    let evs = feed_split(b"\x1bP>|kitty 0.38.1\x1b\\");
    assert_eq!(
        evs,
        vec![Event::CapsReply(CapsReply::XtVersion {
            text: "kitty 0.38.1".into()
        })]
    );
}

#[test]
fn osc_terminated_by_bel_and_st() {
    let evs = feed_all(b"\x1b]11;rgb:1e1e/2e2e/3e3e\x07");
    assert!(matches!(evs[0], Event::CapsReply(CapsReply::Osc { .. })));
    let evs = feed_all(b"\x1b]11;rgb:1e1e/2e2e/3e3e\x1b\\");
    assert!(matches!(evs[0], Event::CapsReply(CapsReply::Osc { .. })));
}

#[test]
fn focus_events() {
    assert_eq!(
        feed_all(b"\x1b[I\x1b[O"),
        vec![Event::FocusGained, Event::FocusLost]
    );
}

// ---- unknown / hostile input ----

#[test]
fn foreign_csi_swallowed_not_leaked() {
    // Made-up sequences: swallowed as Unknown, surrounding text intact.
    // (Window-op reports are no longer Unknown — they route to caps.)
    let evs = feed_all(b"a\x1b[99zb\x1b[>5;2Xc");
    assert_eq!(chars(&evs), "abc");
    assert_eq!(
        evs.iter()
            .filter(|e| matches!(e, Event::Unknown(_)))
            .count(),
        2
    );
}

#[test]
fn x10_mouse_payload_consumed() {
    // CSI M + 3 payload bytes (would be "! ! !" as text if leaked).
    let evs = feed_all(b"\x1b[M!!!after");
    assert_eq!(chars(&evs), "after");
    assert!(matches!(evs[0], Event::Unknown(_)));
}

#[test]
fn esc_esc_yields_esc_key_then_sequence() {
    let evs = feed_all(b"\x1b\x1b[A");
    assert_eq!(
        evs,
        vec![
            Event::Key(KeyEvent::plain(KeyCode::Esc)),
            Event::Key(KeyEvent::plain(KeyCode::Up)),
        ]
    );
}

#[test]
fn esc_inside_csi_aborts_and_recovers() {
    let evs = feed_all(b"\x1b[1;5\x1b[B");
    // Torn CSI flushed as Unknown, then the fresh sequence decodes.
    assert!(matches!(evs[0], Event::Unknown(_)));
    assert_eq!(evs[1], Event::Key(KeyEvent::plain(KeyCode::Down)));
}

#[test]
fn oversized_csi_is_bounded_and_discarded() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b[", &mut out);
    let params = vec![b'1'; 5000];
    p.feed(&params, &mut out);
    p.feed(b"m", &mut out); // final byte ends the discard
    p.feed(b"ok", &mut out);
    let unknown_bytes: usize = out
        .iter()
        .map(|e| match e {
            Event::Unknown(v) => v.len(),
            _ => 0,
        })
        .sum();
    assert!(unknown_bytes <= UNKNOWN_CAP, "unknowns are capped");
    // The oversized tail did NOT leak as text.
    assert_eq!(chars(&out), "ok");
}

#[test]
fn every_single_byte_alone_never_panics() {
    for b in 0..=255u8 {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(&[b], &mut out);
        p.finish(&mut out);
    }
}

#[test]
fn random_garbage_chunks_never_panic() {
    // Deterministic xorshift so failures reproduce; no rand dependency.
    let mut state = 0x243f_6a88_85a3_08d3u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _round in 0..200 {
        let len = (next() % 512) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (next() & 0xff) as u8).collect();
        let mut p = Parser::new();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let chunk = 1 + (next() % 17) as usize;
            let end = (i + chunk).min(bytes.len());
            p.feed(&bytes[i..end], &mut out);
            i = end;
        }
        p.finish(&mut out);
    }
}

#[test]
fn escape_soup_recovers_to_clean_text() {
    // Interleave every frame type, torn and complete, then plain text: the
    // machine must end in Ground and decode the tail perfectly.
    let soup: &[u8] =
        b"\x1b[\x1b]osc-torn\x1b\\\x1bP dcs\x1b\\\x1b_apc\x1b\\\x1bOZ\x1b[<0;1;1Mdone";
    let evs = feed_all(soup);
    assert!(chars(&evs).ends_with("done"), "{evs:?}");
}

#[test]
fn alt_char_after_flush_pending() {
    // ESC then nothing: flush resolves to Esc key; ESC-[ then nothing:
    // flush resolves to Alt+[.
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b", &mut out);
    p.flush_pending(&mut out);
    assert_eq!(out, vec![Event::Key(KeyEvent::plain(KeyCode::Esc))]);
    out.clear();
    p.feed(b"\x1b[", &mut out);
    p.flush_pending(&mut out);
    assert_eq!(
        out,
        vec![Event::Key(KeyEvent::new(KeyCode::Char('['), Mods::ALT))]
    );
}

/// Paste-boundary fuzz: random content (embedded ESCs, newlines, tabs,
/// partial terminators, multibyte text) through random chunk seams must
/// reassemble byte-exactly — bracketed paste is the ONLY paste path
/// (OSC 52 read is forbidden by design), so editors depend entirely on
/// this reassembly being lossless.
#[test]
fn paste_boundary_fuzz_reassembles_exactly() {
    let mut state = 0xdead_beef_cafe_f00du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // Building blocks chosen to attack the terminator matcher and the
    // UTF-8 chunk-seam logic specifically.
    let blocks: &[&str] = &[
        "plain text ",
        "\n",
        "\r\n",
        "\t",
        "\u{1b}",        // lone ESC as content
        "\u{1b}[",       // CSI-looking content
        "\u{1b}[201",    // almost-terminator
        "\u{1b}[200~",   // nested paste-START marker as content
        "émoji 🎉 漢",   // multibyte at seams
        "\u{1b}]52;c;x", // OSC-52-looking content stays content
    ];
    for round in 0..60 {
        let mut content = String::new();
        let n = 3 + (next() % 40) as usize;
        for _ in 0..n {
            content.push_str(blocks[(next() % blocks.len() as u64) as usize]);
        }
        let mut wire = Vec::new();
        wire.extend_from_slice(b"\x1b[200~");
        wire.extend_from_slice(content.as_bytes());
        wire.extend_from_slice(b"\x1b[201~");
        // Random chunking, 1..=13 bytes.
        let mut p = Parser::new();
        let mut out = Vec::new();
        let mut i = 0;
        while i < wire.len() {
            let end = (i + 1 + (next() % 13) as usize).min(wire.len());
            p.feed(&wire[i..end], &mut out);
            i = end;
        }
        let mut reassembled = String::new();
        for e in &out {
            match e {
                Event::Paste(s) => reassembled.push_str(s),
                other => panic!("round {round}: non-paste event {other:?}"),
            }
        }
        assert_eq!(
            reassembled, content,
            "round {round}: paste reassembly diverged"
        );
        // Parser is clean afterwards: a following keystroke decodes.
        out.clear();
        p.feed(b"q", &mut out);
        assert_eq!(chars(&out), "q", "round {round}: dirty state after paste");
    }
}

/// Manual throughput report (not a pass/fail gate — REDTEAM owns perf
/// budgets): `cargo test --lib -- --ignored parser_throughput_report
/// --nocapture`. Kept in-tree so cycle-over-cycle numbers come from the
/// same corpus.
#[test]
#[ignore = "manual throughput report, run with --nocapture"]
fn parser_throughput_report() {
    // Mixed corpus: text, CJK, arrows with mods, kitty CSI-u, SGR mouse.
    let mut unit = Vec::new();
    unit.extend_from_slice("The quick brown fox 漢字テスト🎉 ".as_bytes());
    unit.extend_from_slice(b"\x1b[A\x1b[1;5C\x1b[97;5u\x1b[<32;40;12M\x1b[3~");
    let mut stream = Vec::with_capacity(8 << 20);
    while stream.len() < (8 << 20) {
        stream.extend_from_slice(&unit);
    }
    let mut p = Parser::new();
    let mut out = Vec::new();
    let t0 = std::time::Instant::now();
    for chunk in stream.chunks(4096) {
        p.feed(chunk, &mut out);
        out.clear(); // drop events like a draining loop would
    }
    let dt = t0.elapsed();
    let mb = stream.len() as f64 / (1024.0 * 1024.0);
    println!(
        "parser throughput: {:.0} MB/s ({:.1} MB in {:?})",
        mb / dt.as_secs_f64(),
        mb,
        dt
    );
}

#[test]
fn unknown_event_bytes_are_capped() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    // A 200-byte foreign CSI (within SEQ_CAP) with an unknown final.
    let mut seq = b"\x1b[".to_vec();
    seq.extend(vec![b'9'; 200]);
    seq.push(b'z');
    p.feed(&seq, &mut out);
    match &out[0] {
        Event::Unknown(v) => assert!(v.len() <= UNKNOWN_CAP),
        other => panic!("expected unknown, got {other:?}"),
    }
}
