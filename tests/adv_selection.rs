//! Screen-text selection + copy acceptance (backlog 0270), driven
//! through the REAL pipeline: `app::Driver` frames against a
//! `CaptureTerm`, SGR mouse bytes in, VtScreen cells + OSC 52 capture
//! out. Also pins the tier-2 mouse-reporting suspend verb's byte pairs
//! and the damage containment of a drag.

use abstracttui::app::selection::{copy_to_clipboard, mouse_capture, selection};
use abstracttui::app::{current_theme, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element};

/// RFC 4648 base64 of `s` — the test's own oracle for OSC 52 payloads
/// (the engine's encoder is deliberately not imported: two independent
/// implementations must agree).
fn b64(s: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let d = s.as_bytes();
    let mut out = String::new();
    for c in d.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn caps() -> Capabilities {
    let mut c = Capabilities::default();
    c.truecolor = true;
    c.osc52_copy = true;
    c
}

/// Row `y` of the modeled screen (plain text, trailing blanks trimmed).
fn row(term: &CaptureTerm, y: usize) -> String {
    let dump = term.screen().to_text();
    dump.lines().nth(y).unwrap_or_default().to_string()
}

fn cfg() -> RunConfig {
    RunConfig {
        caps: Some(caps()),
        probe: false,
        ..RunConfig::default()
    }
}

/// Three known text rows in a plain column (no panes: the whole tree is
/// one pane, so multi-row drags flow across rows).
fn three_rows(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(|_cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(text("alpha beta"))
            .child(text("gamma delta"))
            .child(text("third line"))
            .build()
    })
    .unwrap();
    app
}

#[test]
fn drag_paints_highlight_and_copy_key_emits_osc52() {
    let size = Size::new(30, 6);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap(); // first paint

    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    let before = term.screen().cell(0, 0).unwrap().paint.bg;
    assert_ne!(before, Some(sel_bg), "precondition: no highlight yet");

    // Left-down at cell (0,0), drag to cell (4,0): selects "alpha".
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;5;1M");
    driver.turn(&mut app, &mut term).unwrap();

    // The highlight is REAL frame content: VtScreen sees selection_bg on
    // every selected cell, and the glyphs are unchanged.
    for x in 0..=4 {
        let cell = term.screen().cell(x, 0).unwrap();
        assert_eq!(
            cell.paint.bg,
            Some(sel_bg),
            "cell {x} must carry the selection ink"
        );
    }
    assert_eq!(term.screen().cell(5, 0).unwrap().paint.bg, before);
    assert!(row(&term, 0).starts_with("alpha beta"));

    // Copy key: exactly the selected text rides OSC 52 (clipboard `c`).
    term.push_input(b"c");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("alpha").as_str()))
    );
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn release_copies_multi_row_and_click_clears_highlight() {
    let size = Size::new(30, 6);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Drag from "gamma"'s g (0,1) down to (4,2), release: row-flow copy —
    // row 1 from the anchor to the pane's right edge (trailing blanks
    // trim), row 2 from the pane's left edge to the head inclusive.
    term.push_input(b"\x1b[<0;1;2M");
    term.push_input(b"\x1b[<32;5;3M");
    driver.turn(&mut app, &mut term).unwrap();
    term.push_input(b"\x1b[<0;5;3m"); // release copies
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission frame
    let expected = b64("gamma delta\nthird");
    assert_eq!(term.screen().clipboard(), Some(("c", expected.as_str())));

    // The region stays visible after release... then a click clears it
    // and the cells recompose from truth (original inks restored). Esc
    // clears identically — pinned at the unit level, where the bare-ESC
    // wire byte needs no disambiguation wait.
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    assert_eq!(term.screen().cell(2, 1).unwrap().paint.bg, Some(sel_bg));
    term.push_input(b"\x1b[<0;15;1M"); // click elsewhere
    driver.turn(&mut app, &mut term).unwrap();
    assert_ne!(
        term.screen().cell(2, 1).unwrap().paint.bg,
        Some(sel_bg),
        "a click must clear the highlight"
    );
    assert!(row(&term, 1).starts_with("gamma delta"));
}

#[test]
fn ctrl_c_copies_with_selection_and_quits_without() {
    let size = Size::new(20, 4);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Active selection: Ctrl+C is COPY (terminal muscle memory), not quit.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;5;1M");
    driver.turn(&mut app, &mut term).unwrap();
    term.push_input(b"\x03");
    let turn = driver.turn(&mut app, &mut term).unwrap();
    assert!(!turn.quit, "Ctrl+C with a visible selection copies");
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("alpha").as_str()))
    );

    // Cleared selection: Ctrl+C is the default quit again.
    term.push_input(b"\x1b[<0;9;3M"); // click clears
    driver.turn(&mut app, &mut term).unwrap();
    term.push_input(b"\x03");
    let turn = driver.turn(&mut app, &mut term).unwrap();
    assert!(turn.quit, "no selection: default quit restored");
}

#[test]
fn wheel_scrolling_keeps_working_while_selection_is_enabled() {
    let size = Size::new(20, 3);
    let mut app = App::new(size);
    app.mount(|cx| {
        let content = Element::new()
            .style(LayoutStyle::column())
            .child(text("line 0"))
            .child(text("line 1"))
            .child(text("line 2"))
            .child(text("line 3"))
            .child(text("line 4"))
            .child(text("line 5"))
            .build();
        Scroll::new(content).view(cx)
    })
    .unwrap();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    assert!(term.screen().to_text().contains("line 0"));

    // Wheel-down over the scroll: selection must NOT claim it.
    term.push_input(b"\x1b[<65;2;2M");
    driver.turn(&mut app, &mut term).unwrap();
    let after = term.screen().to_text();
    assert!(
        !after.contains("line 0") && after.contains("line 3"),
        "wheel must still scroll: {after}"
    );
}

#[test]
fn drag_damage_stays_bounded() {
    let size = Size::new(80, 24);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    let full_paint = term.take_bytes().len();

    // One-row drag: the emitted bytes must be a small fraction of a full
    // repaint (damage containment — only the selected span recomposes).
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;10;1M");
    driver.turn(&mut app, &mut term).unwrap();
    let drag_bytes = term.take_bytes().len();
    assert!(
        drag_bytes * 4 < full_paint,
        "drag repaint must stay bounded: {drag_bytes} vs full {full_paint}"
    );

    // Extending the drag by a few cells is smaller still.
    term.push_input(b"\x1b[<32;14;1M");
    driver.turn(&mut app, &mut term).unwrap();
    let extend_bytes = term.take_bytes().len();
    assert!(
        extend_bytes <= drag_bytes,
        "extension repaints at most the delta rows: {extend_bytes} vs {drag_bytes}"
    );
}

#[test]
fn suspend_verb_emits_exact_disarm_and_rearm_pairs() {
    let size = Size::new(20, 4);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    driver.turn(&mut app, &mut term).unwrap();
    let _ = term.take_bytes();

    // Handle path: request applies on the next turn.
    mouse_capture().suspend();
    driver.turn(&mut app, &mut term).unwrap();
    let bytes = term.take_bytes();
    let disarm = b"\x1b[?1006l\x1b[?1002l";
    assert!(
        bytes.windows(disarm.len()).any(|w| w == disarm),
        "suspend must disarm SGR encoding then tracking: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(!term.screen().modes().is_set(1002), "tracking mode off");
    assert!(!term.screen().modes().sgr_mouse(), "SGR encoding off");
    assert!(mouse_capture().is_suspended());

    mouse_capture().resume();
    driver.turn(&mut app, &mut term).unwrap();
    let bytes = term.take_bytes();
    let arm = b"\x1b[?1002h\x1b[?1006h";
    assert!(
        bytes.windows(arm.len()).any(|w| w == arm),
        "resume must re-arm tracking then SGR encoding: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(term.screen().modes().is_set(1002));
    assert!(term.screen().modes().sgr_mouse());
    assert!(!mouse_capture().is_suspended());

    // Immediate form for embedders (no turn needed).
    driver.set_mouse_reporting(&mut term, false).unwrap();
    let bytes = term.take_bytes();
    assert!(bytes.windows(disarm.len()).any(|w| w == disarm));

    // Leave still restores unconditionally (disarm rides leave_bytes).
    driver.finish(&mut term).unwrap();
    assert!(!term.screen().modes().is_set(1002));
}

#[test]
fn app_reachable_clipboard_copy_rides_custody() {
    let size = Size::new(20, 4);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    driver.turn(&mut app, &mut term).unwrap();

    // Any component code can queue a copy; the driver emits it through
    // presenter custody on the next frame (0150's clipboard leg).
    copy_to_clipboard("from a handler");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("from a handler").as_str()))
    );
    // Whitespace-only refuses (empty OSC 52 would CLEAR the clipboard).
    copy_to_clipboard("   ");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("from a handler").as_str()))
    );
}

#[test]
fn unadvertised_osc52_still_copies_but_notices_once() {
    let size = Size::new(20, 4);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut plain = caps();
    plain.osc52_copy = false;
    let cfg = RunConfig {
        caps: Some(plain),
        probe: false,
        ..RunConfig::default()
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).unwrap();
    driver.turn(&mut app, &mut term).unwrap();

    copy_to_clipboard("hello");
    copy_to_clipboard("again");
    driver.turn(&mut app, &mut term).unwrap();
    // The bytes still go out (fire-and-forget; unsupporting terminals
    // ignore the frame)...
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("again").as_str()))
    );
    // ...and the degradation is labeled exactly once.
    let notices: Vec<_> = app
        .startup_notices()
        .iter()
        .filter(|n| n.contains("OSC 52"))
        .collect();
    assert_eq!(notices.len(), 1, "one labeled notice: {notices:?}");
}

#[test]
fn selection_idles_at_zero_cost_when_inactive() {
    let size = Size::new(20, 4);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    let _ = term.take_bytes();

    // Select mode ON but nothing selected: turns are idle, zero bytes.
    for _ in 0..3 {
        let turn = driver.turn(&mut app, &mut term).unwrap();
        assert!(turn.idle, "select mode alone must not wake frames");
    }
    assert!(term.take_bytes().is_empty(), "idle means zero bytes");

    // A parked selection (drag finished, region visible) is idle too.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;4;1M");
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // release copy emission
    let _ = term.take_bytes();
    for _ in 0..3 {
        let turn = driver.turn(&mut app, &mut term).unwrap();
        assert!(turn.idle, "a parked selection costs nothing");
    }
    assert!(term.take_bytes().is_empty());
}

#[test]
fn pane_clamp_keeps_sibling_panes_out_of_the_copy() {
    let size = Size::new(40, 6);
    let mut app = App::new(size);
    app.mount(|_cx| {
        Element::new()
            .style(LayoutStyle::row())
            .child(
                Element::new()
                    .style(LayoutStyle::column().width(Dimension::Cells(20)).clip())
                    .child(text("left aaa"))
                    .child(text("left bbb"))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::column().grow(1.0).clip())
                    .child(text("right xxx"))
                    .child(text("right yyy"))
                    .build(),
            )
            .build()
    })
    .unwrap();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Anchor in the LEFT pane, drag the head deep into the RIGHT pane:
    // the clamp pins the selection to the anchor's pane — no cross-pane
    // leak, no right-pane glyphs in the copy.
    term.push_input(b"\x1b[<0;1;1M"); // down at (0,0)
    term.push_input(b"\x1b[<32;35;2M"); // drag to (34,1) — right pane
    driver.turn(&mut app, &mut term).unwrap();
    term.push_input(b"c");
    driver.turn(&mut app, &mut term).unwrap();
    let expected = b64("left aaa\nleft bbb");
    assert_eq!(term.screen().clipboard(), Some(("c", expected.as_str())));
}
