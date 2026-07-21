//! EventReader + probe driver tests: scripted-terminal flows for
//! deadlines, wakes, batching, pixel conversion, probe gating and the
//! tmux grace. Split from reader.rs to keep the driver readable.
//!
//! OWNER: KERNEL.

use super::*;
use crate::base::Size;
use crate::input::{KeyCode, KeyEvent};
use crate::term::EnterOptions;

/// A scripted terminal: each `read` pops the next outcome; an empty
/// script reads as `Idle` (deadline expiry). Bytes written to it are
/// recorded so probe-gating tests can prove "not a single query byte".
struct ScriptTerm {
    script: VecDeque<ScriptItem>,
    /// Holds the current chunk so `TermRead::Input` can borrow it, the
    /// same internal-buffer shape the real backends use.
    current: Vec<u8>,
    sent: Vec<u8>,
}
enum ScriptItem {
    Bytes(Vec<u8>),
    Resize(Size),
    Wake,
}
impl ScriptTerm {
    fn new(items: Vec<ScriptItem>) -> Self {
        ScriptTerm {
            script: items.into(),
            current: Vec::new(),
            sent: Vec::new(),
        }
    }

    fn script_is_empty(&self) -> bool {
        self.script.is_empty()
    }
}
impl Terminal for ScriptTerm {
    fn enter(&mut self, _o: &EnterOptions) -> Result<()> {
        Ok(())
    }
    fn leave(&mut self) -> Result<()> {
        Ok(())
    }
    fn size(&mut self) -> Result<Size> {
        Ok(Size::new(80, 24))
    }
    fn read(&mut self, _deadline: Option<Instant>) -> Result<TermRead<'_>> {
        match self.script.pop_front() {
            Some(ScriptItem::Bytes(b)) => {
                self.current = b;
                Ok(TermRead::Input(&self.current))
            }
            Some(ScriptItem::Resize(s)) => Ok(TermRead::Resize(s)),
            Some(ScriptItem::Wake) => Ok(TermRead::Wake),
            None => Ok(TermRead::Idle),
        }
    }
    fn write(&mut self, b: &[u8]) -> Result<()> {
        self.sent.extend_from_slice(b);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn events_flow_and_resize_passes_through() {
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"a".to_vec()),
        ScriptItem::Resize(Size::new(100, 40)),
        ScriptItem::Bytes(b"\x1b[A".to_vec()),
    ]);
    let mut r = EventReader::new();
    let deadline = Some(Instant::now() + Duration::from_millis(200));
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::Key(KeyEvent::plain(KeyCode::Char('a'))))
    );
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::Resize(Size::new(100, 40)))
    );
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::Key(KeyEvent::plain(KeyCode::Up)))
    );
}

#[test]
fn bare_esc_resolves_after_timeout() {
    let mut term = ScriptTerm::new(vec![ScriptItem::Bytes(b"\x1b".to_vec())]);
    let mut r = EventReader::new();
    r.esc_timeout = Duration::from_millis(5);
    let got = r
        .poll_event(&mut term, Some(Instant::now() + Duration::from_millis(500)))
        .unwrap();
    assert_eq!(got, Some(Event::Key(KeyEvent::plain(KeyCode::Esc))));
}

#[test]
fn split_sequence_survives_the_esc_timeout() {
    // ESC arrives alone, then "[A" in a later chunk: must decode as Up,
    // not flush as a premature Esc key (the seq deadline is long).
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"\x1b".to_vec()),
        ScriptItem::Bytes(b"[A".to_vec()),
    ]);
    let mut r = EventReader::new();
    r.esc_timeout = Duration::from_secs(60); // never fires in this test
    let got = r
        .poll_event(&mut term, Some(Instant::now() + Duration::from_millis(500)))
        .unwrap();
    assert_eq!(got, Some(Event::Key(KeyEvent::plain(KeyCode::Up))));
}

#[test]
fn deadline_expiry_returns_none() {
    let mut term = ScriptTerm::new(vec![]);
    let mut r = EventReader::new();
    let got = r
        .poll_event(&mut term, Some(Instant::now() + Duration::from_millis(10)))
        .unwrap();
    assert_eq!(got, None);
}

#[test]
fn probe_folds_replies_and_passes_user_input_through() {
    use crate::term::caps::CapsReply;
    // Terminal answers: kitty kbd reply, a user keystroke racing the
    // probe, DECRPM, then DA1.
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"\x1b[?1u".to_vec()),
        ScriptItem::Bytes(b"q".to_vec()),
        ScriptItem::Bytes(b"\x1b[?2026;2$y".to_vec()),
        ScriptItem::Bytes(b"\x1b[?62;4c".to_vec()),
    ]);
    let mut r = EventReader::new();
    let mut caps = Capabilities::default();
    let leftover = probe_active(&mut term, &mut r, &mut caps, Duration::from_millis(200)).unwrap();
    assert!(caps.kitty_keyboard);
    assert!(caps.sync_output_2026);
    assert!(caps.sixel);
    assert_eq!(
        leftover,
        vec![Event::Key(KeyEvent::plain(KeyCode::Char('q')))]
    );
    // CapsReply variants never leak into the passthrough.
    assert!(!leftover
        .iter()
        .any(|e| matches!(e, Event::CapsReply(CapsReply::PrimaryDa { .. }))));
}

#[test]
fn probe_survives_a_mute_terminal() {
    let mut term = ScriptTerm::new(vec![]);
    let mut r = EventReader::new();
    let mut caps = Capabilities::default();
    let t0 = Instant::now();
    let leftover = probe_active(&mut term, &mut r, &mut caps, Duration::from_millis(30)).unwrap();
    assert!(leftover.is_empty());
    assert_eq!(caps, Capabilities::default());
    assert!(t0.elapsed() < Duration::from_secs(5), "must not hang");
}

#[test]
fn probe_skipped_entirely_for_dumb_terminals() {
    // RT1-6b: not one query byte may reach a dumb terminal.
    let mut term = ScriptTerm::new(vec![]);
    let mut r = EventReader::new();
    let mut caps = Capabilities {
        dumb: true,
        ..Capabilities::default()
    };
    let leftover = probe_active(&mut term, &mut r, &mut caps, Duration::from_millis(30)).unwrap();
    assert!(leftover.is_empty());
    assert!(
        term.sent.is_empty(),
        "query bytes written at a dumb terminal"
    );
}

#[test]
fn wake_returns_none_and_preserves_order() {
    let mut term = ScriptTerm::new(vec![ScriptItem::Wake, ScriptItem::Bytes(b"a".to_vec())]);
    let mut r = EventReader::new();
    let deadline = Some(Instant::now() + Duration::from_secs(5));
    // Wake surfaces as None (service the loop)...
    assert_eq!(r.poll_event(&mut term, deadline).unwrap(), None);
    // ...and nothing queued was lost.
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::Key(KeyEvent::plain(KeyCode::Char('a'))))
    );
}

#[test]
fn probe_is_not_ended_early_by_a_wake() {
    // A waker fires mid-probe (scheduler startup); the probe must keep
    // waiting for its sentinel instead of treating the wake as a
    // deadline expiry.
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"\x1b[?1u".to_vec()),
        ScriptItem::Wake,
        ScriptItem::Bytes(b"\x1b[?62;4c".to_vec()),
    ]);
    let mut r = EventReader::new();
    let mut caps = Capabilities::default();
    probe_active(&mut term, &mut r, &mut caps, Duration::from_millis(500)).unwrap();
    assert!(caps.kitty_keyboard, "reply before the wake folded");
    assert!(caps.sixel, "sentinel AFTER the wake still folded");
}

#[test]
fn late_replies_after_probe_surface_as_caps_events_only() {
    // RT1-6c at the reader level: replies landing after probe
    // completion flow through poll_event as CapsReply — an app that
    // ignores them drops them; nothing masquerades as keystrokes.
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"\x1b[?62;4c".to_vec()), // sentinel: probe done
        ScriptItem::Bytes(b"\x1b[?2026;1$y\x1b[6;18;9tq".to_vec()), // 2s-late replies + a keystroke
    ]);
    let mut r = EventReader::new();
    let mut caps = Capabilities::default();
    probe_active(&mut term, &mut r, &mut caps, Duration::from_millis(500)).unwrap();
    let deadline = Some(Instant::now() + Duration::from_secs(5));
    use crate::term::caps::CapsReply;
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::CapsReply(CapsReply::DecMode {
            mode: 2026,
            status: 1
        }))
    );
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::CapsReply(CapsReply::WindowOp { op: 6, a: 18, b: 9 }))
    );
    // The user's keystroke right behind them is intact.
    assert_eq!(
        r.poll_event(&mut term, deadline).unwrap(),
        Some(Event::Key(KeyEvent::plain(KeyCode::Char('q'))))
    );
}

#[test]
fn pixel_mouse_converts_to_cells_and_keeps_raw() {
    use crate::input::{MouseEvent, MouseKind};
    // Drag report at pixel (95,41) 1-based -> raw (94,40); 10x20 cells
    // -> cell (9,2).
    let mut term = ScriptTerm::new(vec![ScriptItem::Bytes(b"\x1b[<32;95;41M".to_vec())]);
    let mut r = EventReader::new();
    r.enable_pixel_mouse(crate::base::PixelSize::new(10, 20));
    let deadline = Some(Instant::now() + Duration::from_secs(2));
    match r.poll_event(&mut term, deadline).unwrap() {
        Some(Event::Mouse(MouseEvent {
            pos, pixel, kind, ..
        })) => {
            assert_eq!(kind, MouseKind::Drag);
            assert_eq!(pos, crate::base::Point::new(9, 2));
            assert_eq!(pixel, Some(crate::base::Point::new(94, 40)));
        }
        other => panic!("expected mouse, got {other:?}"),
    }
    // Disabled: coordinates pass through as cells, no pixel field.
    let mut term = ScriptTerm::new(vec![ScriptItem::Bytes(b"\x1b[<32;95;41M".to_vec())]);
    r.disable_pixel_mouse();
    match r.poll_event(&mut term, deadline).unwrap() {
        Some(Event::Mouse(MouseEvent { pos, pixel, .. })) => {
            assert_eq!(pos, crate::base::Point::new(94, 40));
            assert_eq!(pixel, None);
        }
        other => panic!("expected mouse, got {other:?}"),
    }
    // A degenerate cell size is refused: conversion never divides by
    // zero and never silently passes pixels through as cells.
    r.enable_pixel_mouse(crate::base::PixelSize::new(0, 0));
    let mut term = ScriptTerm::new(vec![ScriptItem::Bytes(b"\x1b[<32;95;41M".to_vec())]);
    match r.poll_event(&mut term, deadline).unwrap() {
        Some(Event::Mouse(MouseEvent { pixel, .. })) => assert_eq!(pixel, None),
        other => panic!("expected mouse, got {other:?}"),
    }
}

#[test]
fn poll_many_drains_a_burst_in_one_call() {
    let mut term = ScriptTerm::new(vec![
        ScriptItem::Bytes(b"abc".to_vec()),
        ScriptItem::Bytes(b"\x1b[Ade".to_vec()),
    ]);
    let mut r = EventReader::new();
    let mut out = Vec::new();
    let n = r
        .poll_many(
            &mut term,
            &mut out,
            Some(Instant::now() + Duration::from_secs(2)),
        )
        .unwrap();
    // One call: both chunks decoded — 3 chars, Up, 2 chars, in order.
    assert_eq!(n, 6, "{out:?}");
    assert_eq!(out.len(), 6);
    assert_eq!(out[0], Event::Key(KeyEvent::plain(KeyCode::Char('a'))));
    assert_eq!(out[3], Event::Key(KeyEvent::plain(KeyCode::Up)));
    assert_eq!(out[5], Event::Key(KeyEvent::plain(KeyCode::Char('e'))));
    // Nothing left: the next call reports 0 (deadline expiry).
    let n = r
        .poll_many(
            &mut term,
            &mut out,
            Some(Instant::now() + Duration::from_millis(10)),
        )
        .unwrap();
    assert_eq!(n, 0);
    assert_eq!(out.len(), 6, "out is append-only");
}

#[test]
fn poll_many_returns_zero_on_wake() {
    let mut term = ScriptTerm::new(vec![ScriptItem::Wake, ScriptItem::Bytes(b"x".to_vec())]);
    let mut r = EventReader::new();
    let mut out = Vec::new();
    let deadline = Some(Instant::now() + Duration::from_secs(2));
    // Wake ends the wait with nothing decoded: service the loop.
    assert_eq!(r.poll_many(&mut term, &mut out, deadline).unwrap(), 0);
    // The queued input is intact for the next batch.
    assert_eq!(r.poll_many(&mut term, &mut out, deadline).unwrap(), 1);
    assert_eq!(out[0], Event::Key(KeyEvent::plain(KeyCode::Char('x'))));
}

#[test]
fn focus_resize_interleave_with_fast_keys_drops_nothing() {
    // Property: a fast key stream interleaved with resizes, focus flips
    // and wakes surfaces EVERY event in script order (wakes vanish into
    // `Ok(0)` boundaries by contract; everything else must come through).
    // Deterministic xorshift so failures reproduce.
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    #[derive(Debug, PartialEq)]
    enum Expect {
        Key(char),
        FocusIn,
        FocusOut,
        Resize(Size),
    }
    let mut script = Vec::new();
    let mut expected = Vec::new();
    for _round in 0..120 {
        match next() % 5 {
            0 | 1 => {
                // A burst of printable keys, sometimes with focus reports
                // EMBEDDED mid-chunk (terminals do interleave them).
                let mut chunk = Vec::new();
                let n = 1 + (next() % 24) as usize;
                for _ in 0..n {
                    if next() % 11 == 0 {
                        let focus_in = next() % 2 == 0;
                        chunk.extend_from_slice(if focus_in { b"\x1b[I" } else { b"\x1b[O" });
                        expected.push(if focus_in {
                            Expect::FocusIn
                        } else {
                            Expect::FocusOut
                        });
                    } else {
                        let c = (b'a' + (next() % 26) as u8) as char;
                        chunk.push(c as u8);
                        expected.push(Expect::Key(c));
                    }
                }
                script.push(ScriptItem::Bytes(chunk));
            }
            2 => {
                let sz = Size::new(40 + (next() % 200) as i32, 10 + (next() % 90) as i32);
                script.push(ScriptItem::Resize(sz));
                expected.push(Expect::Resize(sz));
            }
            3 => script.push(ScriptItem::Wake),
            _ => {
                // Focus flip alone in its own chunk.
                script.push(ScriptItem::Bytes(b"\x1b[I".to_vec()));
                expected.push(Expect::FocusIn);
            }
        }
    }
    let mut term = ScriptTerm::new(script);
    let mut r = EventReader::new();
    let mut got = Vec::new();
    let mut out = Vec::new();
    // Drain until the script is exhausted; poll_many returning 0 is a
    // wake/deadline boundary, never the end (the script still has items
    // until read() returns Idle repeatedly — bound the loop generously).
    for _ in 0..2000 {
        out.clear();
        let n = r
            .poll_many(
                &mut term,
                &mut out,
                Some(Instant::now() + Duration::from_millis(5)),
            )
            .unwrap();
        for e in out.drain(..) {
            match e {
                Event::Key(k) => {
                    if let crate::input::KeyCode::Char(c) = k.code {
                        got.push(Expect::Key(c));
                    } else {
                        panic!("unexpected key {k:?}");
                    }
                }
                Event::FocusGained => got.push(Expect::FocusIn),
                Event::FocusLost => got.push(Expect::FocusOut),
                Event::Resize(sz) => got.push(Expect::Resize(sz)),
                other => panic!("unexpected event {other:?}"),
            }
        }
        if n == 0 && term.script_is_empty() {
            break;
        }
    }
    assert_eq!(got.len(), expected.len(), "event count: nothing dropped");
    assert_eq!(got, expected, "script order preserved end to end");
}

#[test]
fn refresh_cell_pixel_size_uses_platform_then_keeps_wire_answer() {
    use crate::base::PixelSize;
    use crate::term::probe::refresh_cell_pixel_size;

    // ScriptTerm has no platform measurement (trait default None):
    // an existing wire-probed value must be KEPT, not erased.
    let mut term = ScriptTerm::new(vec![]);
    let mut caps = Capabilities {
        cell_pixel_size: Some(PixelSize::new(9, 18)),
        ..Capabilities::default()
    };
    assert_eq!(
        refresh_cell_pixel_size(&mut term, &mut caps),
        Some(PixelSize::new(9, 18))
    );

    // A terminal that CAN measure overrides.
    struct PxTerm(ScriptTerm);
    impl Terminal for PxTerm {
        fn enter(&mut self, o: &EnterOptions) -> Result<()> {
            self.0.enter(o)
        }
        fn leave(&mut self) -> Result<()> {
            self.0.leave()
        }
        fn size(&mut self) -> Result<Size> {
            self.0.size()
        }
        fn read(&mut self, d: Option<Instant>) -> Result<TermRead<'_>> {
            self.0.read(d)
        }
        fn write(&mut self, b: &[u8]) -> Result<()> {
            self.0.write(b)
        }
        fn flush(&mut self) -> Result<()> {
            self.0.flush()
        }
        fn cell_pixel_size(&mut self) -> Option<PixelSize> {
            Some(PixelSize::new(10, 20))
        }
    }
    let mut term = PxTerm(ScriptTerm::new(vec![]));
    assert_eq!(
        refresh_cell_pixel_size(&mut term, &mut caps),
        Some(PixelSize::new(10, 20))
    );
    assert_eq!(caps.cell_pixel_size, Some(PixelSize::new(10, 20)));
}
