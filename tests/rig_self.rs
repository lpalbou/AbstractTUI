//! Self-tests of the verification rig itself (REDTEAM): the VT model
//! must never panic on hostile bytes, must be chunking-invariant, and
//! the capture/snapshot/fuzz utilities must behave as documented. If the
//! rig can be broken, every verdict it renders is worthless — so the rig
//! gets attacked first.

use abstracttui::base::Size;
use abstracttui::testing::{
    hostile_corpus, random_splits, CaptureTerm, Rng, ScriptedRead, VtScreen,
};

// ------------------------------------------------------- fuzz: no panics

#[test]
fn vt_survives_hostile_corpus_one_shot() {
    for (i, chunk) in hostile_corpus(0xdead_beef, 600).iter().enumerate() {
        let mut s = VtScreen::new(Size::new(20, 6));
        s.feed(chunk);
        // Post-conditions that must hold no matter the input:
        let c = s.cursor();
        assert!(c.x >= 0 && c.x < 20 && c.y >= 0 && c.y < 6, "case {i}");
        let _ = s.to_text();
        let _ = s.to_styled_dump();
    }
}

#[test]
fn vt_survives_hostile_corpus_streamed_into_one_screen() {
    // All chunks into ONE screen: state carries across hostile inputs.
    let mut s = VtScreen::new(Size::new(20, 6));
    for chunk in hostile_corpus(0xfeed_f00d, 600) {
        s.feed(&chunk);
    }
    let _ = s.to_styled_dump();
}

#[test]
fn vt_survives_all_single_bytes_and_pairs() {
    for a in 0..=255u8 {
        let mut s = VtScreen::new(Size::new(4, 2));
        s.feed(&[a]);
    }
    // ESC + every byte, CSI + every byte: the state-machine edges.
    for a in 0..=255u8 {
        let mut s = VtScreen::new(Size::new(4, 2));
        s.feed(&[0x1b, a]);
        let mut s = VtScreen::new(Size::new(4, 2));
        s.feed(&[0x1b, b'[', a]);
    }
}

#[test]
fn vt_wide_pair_invariant_holds_under_fuzz() {
    // After ANY byte soup, no continuation may exist without a wide
    // leader to its immediate left, and no wide leader without its
    // continuation: the invariant the presenter will be judged by, so
    // the model itself must be incapable of violating it.
    let mut rng = Rng::new(31337);
    for _ in 0..300 {
        let mut s = VtScreen::new(Size::new(11, 4)); // odd width on purpose
        for _ in 0..rng.range(1, 6) {
            let chunk = match rng.below(4) {
                0 => abstracttui::testing::fuzzish::random_chunk(&mut rng, 48),
                1 => abstracttui::testing::fuzzish::sequence_shaped(&mut rng),
                _ => abstracttui::testing::fuzzish::truncated_utf8(&mut rng),
            };
            s.feed(&chunk);
        }
        for y in 0..4 {
            for x in 0..11 {
                let cell = s.cell(x, y).unwrap();
                if cell.is_continuation() {
                    assert!(
                        x > 0 && s.cell(x - 1, y).unwrap().is_wide_leader(),
                        "orphan continuation at {x},{y}:\n{}",
                        s.to_styled_dump()
                    );
                }
                if cell.is_wide_leader() {
                    assert!(
                        s.cell(x + 1, y)
                            .map(|c| c.is_continuation())
                            .unwrap_or(false),
                        "torn leader at {x},{y}:\n{}",
                        s.to_styled_dump()
                    );
                }
            }
        }
    }
}

// ----------------------------------------------- chunking invariance

#[test]
fn feeding_split_chunks_equals_one_shot() {
    // The property that makes incremental parsing trustworthy: any split
    // of the same bytes produces the same screen. Uses realistic frames
    // (styled text, wide chars, motion) and hostile soup.
    let mut rng = Rng::new(0x5eed);
    let mut frames: Vec<Vec<u8>> = vec![
        b"\x1b[2J\x1b[1;1H\x1b[1;38;2;10;200;30mhysteresis\x1b[0m \x1b[38;5;99mok".to_vec(),
        "\x1b[3;2H日本語テスト\x1b[4;1He\u{301}\u{302}combining"
            .as_bytes()
            .to_vec(),
        b"\x1b]8;;https://example.com\x1b\\click\x1b]8;;\x1b\\\x1b[?2026h\x1b[2;2Hx\x1b[?2026l"
            .to_vec(),
    ];
    for _ in 0..40 {
        let mut soup = Vec::new();
        for _ in 0..rng.range(1, 4) {
            soup.extend(abstracttui::testing::fuzzish::sequence_shaped(&mut rng));
            soup.extend(abstracttui::testing::fuzzish::truncated_utf8(&mut rng));
        }
        frames.push(soup);
    }
    for (i, frame) in frames.iter().enumerate() {
        let mut one = VtScreen::new(Size::new(16, 5));
        one.feed(frame);
        for max in [1usize, 2, 3, 7] {
            let mut split = VtScreen::new(Size::new(16, 5));
            for part in random_splits(&mut rng, frame, max) {
                split.feed(&part);
            }
            assert_eq!(
                one.to_styled_dump(),
                split.to_styled_dump(),
                "frame {i} split at max {max} diverged"
            );
        }
    }
}

// --------------------------------------------------------- capture term

#[test]
fn capture_term_round_trip_through_the_trait() {
    use abstracttui::term::{EnterOptions, Terminal};
    let mut t = CaptureTerm::new(Size::new(8, 2));
    t.enter(&EnterOptions::default()).unwrap();
    let enter_flushes = t.flush_count();
    t.write(b"\x1b[1;1H\x1b[31mred").unwrap();
    t.flush().unwrap();
    assert_eq!(t.screen().cell(0, 0).unwrap().ch(), 'r');
    assert!(t.screen().cell(0, 0).unwrap().paint.fg.is_some());
    assert_eq!(t.flush_count(), enter_flushes + 1);
    let bytes = t.take_bytes();
    assert!(bytes.ends_with(b"red"));
    t.leave().unwrap();
    assert!(!t.is_entered());
    // Leave restored every mode enter set (RT1-16 pairing, model-checked).
    assert!(!t.screen().modes().alt_screen());
    assert!(t.screen().modes().cursor_visible());
    assert!(!t.screen().modes().bracketed_paste());
    assert_eq!(t.screen().counters().kitty_push_depth, 0);
    assert_eq!(t.screen().unknown_seq_count(), 0);
}

#[test]
fn capture_term_scripted_input_order() {
    use abstracttui::term::{TermRead, Terminal};
    let mut t = CaptureTerm::new(Size::new(8, 2));
    t.push_input(b"a");
    t.push_idle();
    t.push_resize(Size::new(4, 4));
    assert!(matches!(t.read(None).unwrap(), TermRead::Input(b"a")));
    assert!(matches!(t.read(None).unwrap(), TermRead::Idle));
    assert!(matches!(t.read(None).unwrap(), TermRead::Resize(s) if s == Size::new(4, 4)));
    assert!(matches!(t.read(None).unwrap(), TermRead::Idle)); // exhausted
    assert_eq!(t.script_len(), 0);
    let _ = ScriptedRead::Idle; // variant stays public for scripting
}

#[test]
fn capture_term_tracks_unflushed_tail() {
    use abstracttui::term::Terminal;
    let mut t = CaptureTerm::new(Size::new(4, 1));
    t.write(b"ab").unwrap();
    t.flush().unwrap();
    t.write(b"cd").unwrap();
    assert_eq!(t.unflushed_bytes(), b"cd");
}

// ------------------------------------------------------------- snapshot

#[test]
fn snapshot_of_plain_grid() {
    let mut s = VtScreen::new(Size::new(8, 3));
    s.feed(b"\x1b[1;1Habc\x1b[2;2H\x1b[7mrev\x1b[0m\x1b[3;1Hend");
    abstracttui::testing::assert_snapshot("rig_plain_grid", &s.to_styled_dump());
}

// ------------------------------------------------------- rng statistics

#[test]
fn rng_byte_distribution_is_sane() {
    // Not a statistical test — a tripwire against a broken generator
    // (e.g. locked high bits) that would silently gut the fuzz corpus.
    let mut rng = Rng::new(1);
    let mut seen = [false; 256];
    for _ in 0..20_000 {
        seen[rng.byte() as usize] = true;
    }
    let covered = seen.iter().filter(|&&b| b).count();
    assert!(covered > 250, "byte coverage only {covered}/256");
}
