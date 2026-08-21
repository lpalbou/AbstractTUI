//! Screen-text selection + copy acceptance (backlog 0270), driven
//! through the REAL pipeline: `app::Driver` frames against a
//! `CaptureTerm`, SGR mouse bytes in, VtScreen cells + OSC 52 capture
//! out. Also pins the tier-2 mouse-reporting suspend verb's byte pairs
//! and the damage containment of a drag.

use abstracttui::app::selection::{copy_to_clipboard, mouse_capture, selection};
use abstracttui::app::{current_theme, Driver, Modal, RunConfig};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::{Attrs, CaptureTerm};
use abstracttui::ui::{text, Element};
use abstracttui::widgets::{Button, Feed, FeedItem, FeedState, Scroll};

use std::cell::RefCell;
use std::rc::Rc;

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
        // Never spawn `pbcopy`/`wl-copy`/`xclip` from the suite: it would
        // overwrite the clipboard of whoever ran `cargo test`, and block
        // on a CI box with no X server.
        platform_clipboard: false,
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
fn release_copies_multi_row_and_clears_highlight() {
    let size = Size::new(30, 6);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Drag from "gamma"'s g (0,1) down to (4,2): highlight paints.
    term.push_input(b"\x1b[<0;1;2M");
    term.push_input(b"\x1b[<32;5;3M");
    driver.turn(&mut app, &mut term).unwrap();
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    assert_eq!(term.screen().cell(2, 1).unwrap().paint.bg, Some(sel_bg));

    // Release: row-flow copy — row 1 from the anchor to the pane's right
    // edge (trailing blanks trim), row 2 from the pane's left edge to
    // the head inclusive.
    term.push_input(b"\x1b[<0;5;3m"); // release copies
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission frame
    let expected = b64("gamma delta\nthird");
    assert_eq!(term.screen().clipboard(), Some(("c", expected.as_str())));

    // The copy ENDS the gesture (0290): the highlight clears itself and
    // the cells recompose from truth (original inks restored) — no
    // click/Esc needed, and no region lingers to swallow keys.
    assert_ne!(
        term.screen().cell(2, 1).unwrap().paint.bg,
        Some(sel_bg),
        "the release-copy must clear the highlight"
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

    // Mid-drag selection: Ctrl+C is COPY (terminal muscle memory), not
    // quit — and the copy ends the gesture (0290).
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
    assert!(
        !selection().is_active(),
        "the Ctrl+C copy cleared the region (0290)"
    );

    // Region gone: the NEXT Ctrl+C is the default quit again — no
    // click/Esc dance in between (the 0290 footgun, inverted).
    term.push_input(b"\x03");
    let turn = driver.turn(&mut app, &mut term).unwrap();
    assert!(turn.quit, "no selection: default quit restored");
}

/// Backlog 0290 regression: after a release-copy the app owns its keys
/// again IMMEDIATELY. The field failure: the retained region swallowed
/// every `c`/Enter (typing "cargo check" into a composer lost both
/// `c`s; Enter submitted nothing) until a click/Esc — with no effective
/// app-side workaround, because the selection layer consumes those keys
/// before tree dispatch.
#[test]
fn release_copy_frees_enter_and_c_for_the_app() {
    let size = Size::new(30, 6);
    let mut app = App::new(size);
    // Composer-shaped fixture: the root counts the Key/Wheel events
    // that actually REACH tree dispatch.
    let keys: std::rc::Rc<std::cell::RefCell<Vec<char>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let wheels: std::rc::Rc<std::cell::RefCell<u32>> = std::rc::Rc::new(std::cell::RefCell::new(0));
    let (k, w) = (keys.clone(), wheels.clone());
    app.mount(move |_cx| {
        Element::new()
            .on(abstracttui::ui::Phase::Bubble, move |_c, e| match e {
                abstracttui::ui::UiEvent::Key(key) => {
                    let ch = match key.key {
                        abstracttui::ui::Key::Char(c) => c,
                        abstracttui::ui::Key::Enter => '\n',
                        _ => '?',
                    };
                    k.borrow_mut().push(ch);
                }
                abstracttui::ui::UiEvent::Mouse(m) => {
                    if matches!(
                        m.kind,
                        abstracttui::ui::MouseKind::ScrollUp
                            | abstracttui::ui::MouseKind::ScrollDown
                    ) {
                        *w.borrow_mut() += 1;
                    }
                }
                _ => {}
            })
            .child(text("alpha beta"))
            .child(text("gamma delta"))
            .build()
    })
    .unwrap();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Drag "alpha", release: the copy fires and the gesture ends.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;5;1M");
    term.push_input(b"\x1b[<0;5;1m");
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("alpha").as_str()))
    );
    assert!(!selection().is_active(), "release-copy clears the region");

    // Typing "c" then Enter goes to the APP — the exact keystrokes the
    // retained region used to eat. The clipboard must NOT change (no
    // silent re-copy).
    term.push_input(b"c");
    term.push_input(b"\r");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        keys.borrow().as_slice(),
        &['c', '\n'],
        "post-copy keys must reach tree dispatch"
    );
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("alpha").as_str())),
        "no re-copy: the clipboard still holds the release copy"
    );

    // Wheel routing is untouched by any of this.
    term.push_input(b"\x1b[<65;2;2M");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*wheels.borrow(), 1, "wheel still reaches the tree");
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
        // No host clipboard: this pins the OSC-52-only route. Leaving it
        // on would make the assertion depend on whether the machine
        // running the suite happens to have `pbcopy`/`xclip` — and would
        // overwrite the clipboard of whoever ran `cargo test`.
        platform_clipboard: false,
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

// ---------------------------------------------------------------------
// 1300 — a live selection freezes follow-tail. Screen-space selection
// over a streaming transcript used to copy whatever the scroll had put
// on those cells by release time: the highlight said one thing, the
// clipboard got another.
// ---------------------------------------------------------------------

/// Turn until the measure → pin → render loop settles (the widgets-side
/// `settle` in driver terms).
fn settle_turns(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..5 {
        driver.turn(app, term).unwrap();
    }
}

#[test]
fn a_live_selection_freezes_the_streaming_tail_and_copies_what_it_shows() {
    let size = Size::new(20, 4);
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        let feed = FeedState::new(cx);
        let follow = cx.signal(true);
        *h.borrow_mut() = Some(feed.clone());
        Scroll::new(Feed::new(&feed).gap(0).view(cx))
            .follow_tail(follow)
            .view(cx)
    })
    .unwrap();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    let feed = holder.borrow_mut().take().expect("feed captured");

    // Row text WITHOUT the scrollbar column: the thumb legitimately
    // moves while frozen (content grew below the viewport and the bar
    // reports that honestly) — the text is what must hold still.
    let body = |term: &CaptureTerm, y: usize| {
        row(term, y)
            .chars()
            .take((size.w - 1) as usize)
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    for i in 0..8 {
        feed.push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    settle_turns(&mut driver, &mut app, &mut term);
    assert_eq!(body(&term, 3), "line 7", "pinned to the tail");
    assert_eq!(body(&term, 1), "line 5");

    // Drag across "line 5" (row 1): cells (0,1)..(5,1).
    term.push_input(b"\x1b[<0;1;2M");
    term.push_input(b"\x1b[<32;6;2M");
    driver.turn(&mut app, &mut term).unwrap();
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    assert_eq!(
        term.screen().cell(2, 1).unwrap().paint.bg,
        Some(sel_bg),
        "the drag painted a region"
    );

    // The stream keeps arriving mid-drag. Before 1300 the tail pin
    // scrolled these rows up under the highlight.
    for i in 8..12 {
        feed.push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    settle_turns(&mut driver, &mut app, &mut term);
    assert_eq!(
        body(&term, 1),
        "line 5",
        "the frozen tail must hold the highlighted row"
    );
    assert_eq!(body(&term, 3), "line 7", "no row moved during the freeze");
    assert_eq!(
        term.screen().cell(2, 1).unwrap().paint.bg,
        Some(sel_bg),
        "the highlight still covers the same text"
    );

    // Release: the clipboard gets exactly the highlighted row...
    term.push_input(b"\x1b[<0;6;2m");
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission frame
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("line 5").as_str()))
    );

    // ...and clearing the region thaws the tail: back to live, at the
    // tail as it stands now (the frozen appends are not replayed).
    settle_turns(&mut driver, &mut app, &mut term);
    assert_eq!(
        body(&term, 3),
        "line 11",
        "thaw re-pins to the live tail: {:?}",
        (0..4).map(|y| body(&term, y)).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------
// 0285 — click-through: the selection layer owns the gesture only once
// it DRAGS. Plain clicks reach the widgets (the field P0: with select
// mode on app-wide, every Button was dead by mouse — the layer consumed
// every left Down and Up ahead of overlay/tree routing).
// ---------------------------------------------------------------------

/// Click-through fixture: two buttons ("A" at cells 0..3, "B" at 3..6,
/// row 0) over a text row (row 1). Returns the app + per-button click
/// counters.
fn two_buttons(size: Size) -> (App, Rc<RefCell<u32>>, Rc<RefCell<u32>>) {
    let a: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let b: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let (a2, b2) = (a.clone(), b.clone());
    let mut app = App::new(size);
    app.mount(move |cx| {
        let (a3, b3) = (a2.clone(), b2.clone());
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    .style(LayoutStyle::row().height(Dimension::Cells(1)))
                    .child(
                        Button::new("A")
                            .on_click(move || *a3.borrow_mut() += 1)
                            .view(cx),
                    )
                    .child(
                        Button::new("B")
                            .on_click(move || *b3.borrow_mut() += 1)
                            .view(cx),
                    )
                    .build(),
            )
            .child(text("gamma delta"))
            .build()
    })
    .unwrap();
    (app, a, b)
}

/// The consumer's named regression (first-app P0): select mode ON, a
/// Button under the cursor, a plain SGR click — the Button must fire.
/// Before click-through the selection layer consumed both halves of the
/// click and no Button anywhere could ever fire by mouse.
#[test]
fn plain_click_fires_buttons_while_select_mode_is_on() {
    let size = Size::new(30, 4);
    let (mut app, a, b) = two_buttons(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Down + Up on button A's cell (1,0): a drag-less click.
    term.push_input(b"\x1b[<0;2;1M");
    term.push_input(b"\x1b[<0;2;1m");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*a.borrow(), 1, "the click reached the button");
    assert_eq!(*b.borrow(), 0);
    assert!(!selection().is_active(), "a plain click paints nothing");
    assert_eq!(
        term.screen().clipboard(),
        None,
        "a plain click copies nothing"
    );
}

/// Drag slop: a click whose press wiggles WITHIN the anchor cell (a
/// same-cell Drag between Down and Up — terminals report sub-cell
/// motion) is still a click, not a one-cell selection.
#[test]
fn same_cell_wiggle_still_clicks_the_button() {
    let size = Size::new(30, 4);
    let (mut app, a, _b) = two_buttons(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    term.push_input(b"\x1b[<0;2;1M"); // down at (1,0)
    term.push_input(b"\x1b[<32;2;1M"); // drag on the SAME cell
    term.push_input(b"\x1b[<0;2;1m"); // release
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*a.borrow(), 1, "a wiggly click still clicks");
    assert!(!selection().is_active(), "same-cell drags never paint");
    assert_eq!(term.screen().clipboard(), None);
}

/// A drag that STARTS on a button selects text and must neither click
/// the button nor wedge it: at the first cross-cell drag the layer
/// claims the gesture and the driver resolves the passed-through press
/// — the button un-presses WITHOUT firing (release-inside-rect decides)
/// and the pointer capture drops, so the NEXT click routes fresh.
#[test]
fn drag_select_over_a_button_neither_clicks_nor_wedges_it() {
    let size = Size::new(30, 4);
    let (mut app, a, b) = two_buttons(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);

    // Down inside A (0,0), drag to (2,0): the layer claims the gesture.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;3;1M");
    driver.turn(&mut app, &mut term).unwrap();
    // The selection highlight paints over the button...
    let cell = term.screen().cell(1, 0).unwrap();
    assert_eq!(cell.paint.bg, Some(sel_bg), "highlight paints over A");
    // ...but the press was RESOLVED at the claim: no stuck pressed
    // state. Pressed wears the selection pair + BOLD; the highlight
    // keeps the cell's attrs — a stuck press would read bold here.
    assert!(
        !cell.paint.attrs.contains(Attrs::BOLD),
        "the claim un-pressed the button (no stuck pressed state)"
    );
    assert_eq!(*a.borrow(), 0, "claiming the gesture never clicks");
    assert_eq!(
        app.tree().pointer_capture(),
        None,
        "the claim released the pointer capture"
    );

    // Release: the drag copies (selection over widgets keeps working)
    // and still never clicks.
    term.push_input(b"\x1b[<0;3;1m");
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64(" A").as_str())),
        "the drag-release copied the button's screen text"
    );
    assert_eq!(*a.borrow(), 0, "the drag never clicked A");

    // The gesture left nothing behind: a normal click on B fires B —
    // a stuck capture would have routed this click back to A.
    term.push_input(b"\x1b[<0;5;1M");
    term.push_input(b"\x1b[<0;5;1m");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*b.borrow(), 1, "the next click routes fresh");
    assert_eq!(*a.borrow(), 0);
}

/// The dismissal rule: a click while a selection is VISIBLE clears it
/// and is CONSUMED — both halves (the user was dismissing the
/// highlight, not aiming at the widget beneath). Esc/click-clear
/// parity, stated in docs/api.md. Post-0290 a region exists only
/// mid-drag, so the click arrives as a degenerate second Down — the
/// rule holds for whatever stream produces it.
#[test]
fn click_dismissing_a_visible_selection_never_fires_the_widget_beneath() {
    let size = Size::new(30, 4);
    let (mut app, _a, b) = two_buttons(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);

    // Paint a region on the text row (no release: region stays visible).
    term.push_input(b"\x1b[<0;1;2M");
    term.push_input(b"\x1b[<32;6;2M");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(term.screen().cell(0, 1).unwrap().paint.bg, Some(sel_bg));

    // A click lands on button B while the region is visible: the click
    // DISMISSES the selection and never reaches the button.
    term.push_input(b"\x1b[<0;5;1M");
    term.push_input(b"\x1b[<0;5;1m");
    driver.turn(&mut app, &mut term).unwrap();
    assert!(!selection().is_active(), "the click cleared the region");
    assert_ne!(
        term.screen().cell(0, 1).unwrap().paint.bg,
        Some(sel_bg),
        "the highlight cells recomposed from truth"
    );
    assert_eq!(*b.borrow(), 0, "the dismissal click never fires widgets");

    // With nothing visible, the very next click is the widgets' again.
    term.push_input(b"\x1b[<0;5;1M");
    term.push_input(b"\x1b[<0;5;1m");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*b.borrow(), 1, "the follow-up click fires normally");
}

/// The consumer's exact shape: an approval MODAL with engine Buttons,
/// select mode on app-wide. The click must route into the modal tree
/// and fire; a drag over the modal must claim + release the modal
/// tree's capture (the overlay half of the cancel path).
#[test]
fn modal_buttons_are_clickable_while_select_mode_is_on() {
    let size = Size::new(30, 9);
    let ok: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let scope_holder: Rc<RefCell<Option<abstracttui::reactive::Scope>>> =
        Rc::new(RefCell::new(None));
    let sh = scope_holder.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        *sh.borrow_mut() = Some(cx);
        Element::new().child(text("underneath")).build()
    })
    .unwrap();
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    let cx = scope_holder.borrow().expect("scope");
    let ok2 = ok.clone();
    let tokens = current_theme().tokens;
    // 10x3 panel centered in 30x9 -> bounds (10,3), 1-cell padding ->
    // content at (11,4); Button "OK" occupies cells (11..15, 4).
    let modal = Modal::open(&overlays, cx, size, Size::new(10, 3), move |mcx| {
        Button::new("OK")
            .on_click(move || *ok2.borrow_mut() += 1)
            .element(mcx, &tokens)
            .build()
    });
    driver.turn(&mut app, &mut term).unwrap();

    // Plain click on OK: fires through the modal tree.
    term.push_input(b"\x1b[<0;13;5M");
    term.push_input(b"\x1b[<0;13;5m");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(*ok.borrow(), 1, "modal button clicks in select mode");

    // Drag over the modal: claim releases the MODAL tree's capture.
    term.push_input(b"\x1b[<0;13;5M");
    term.push_input(b"\x1b[<32;15;5M");
    driver.turn(&mut app, &mut term).unwrap();
    let modal_tree = modal.layer().tree().expect("modal tree");
    assert_eq!(
        modal_tree.pointer_capture(),
        None,
        "the claim released the modal tree's capture"
    );
    assert_eq!(*ok.borrow(), 1, "the drag never clicked");
    term.push_input(b"\x1b[<0;15;5m"); // release copies "OK"
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("OK").as_str())),
        "drag-copy from modal content still works"
    );
    assert_eq!(*ok.borrow(), 1);
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

/// A copy CONFIRMS itself (first-app/1320). The clipboard lives outside
/// the app, so the user cannot see whether anything landed or whether it
/// landed whole — the character count is the receipt. Field report
/// (abstractcode-tui, 2026-08-19): the engine used to announce "OSC 52
/// unavailable" on a copy that had SUCCEEDED through the host clipboard,
/// which reads as a failure and says nothing about the copy.
#[test]
fn a_successful_copy_reports_its_size() {
    let size = Size::new(30, 6);
    let mut app = three_rows(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    // Drag "alpha" (5 chars, one row) and copy it.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;5;1M");
    term.push_input(b"c");
    driver.turn(&mut app, &mut term).unwrap();
    assert_eq!(
        term.screen().clipboard(),
        Some(("c", b64("alpha").as_str()))
    );
    let notices = app.startup_notices();
    assert_eq!(
        notices.last().map(String::as_str),
        Some("copied 5 characters to the clipboard"),
        "the receipt names the size: {notices:?}"
    );
    assert!(
        !notices.iter().any(|n| n.contains("unavailable")),
        "a copy that WORKED must not announce a degradation: {notices:?}"
    );

    // A multi-row copy also reports its rows — a squashed multi-line
    // paste is the failure this count makes visible.
    term.push_input(b"\x1b[<0;1;2M");
    term.push_input(b"\x1b[<32;5;3M");
    term.push_input(b"c");
    driver.turn(&mut app, &mut term).unwrap();
    let notices = app.startup_notices();
    assert_eq!(
        notices.last().map(String::as_str),
        Some("copied 17 characters (2 lines) to the clipboard"),
        "{notices:?}"
    );
}

// ---------------------------------------------------------------------------
// Drag zones: select mode stands down where a widget already owns drags
// ---------------------------------------------------------------------------

/// A `Scroll` over `rows` plain text lines. Plain on purpose: a column
/// publishes its full extent in ONE frame, where a `Feed` needs two
/// (see `tests/scroll_remount_offset.rs`) — this suite is about the
/// gesture, not the measure.
fn scrolled_column(size: Size, rows: i32) -> (App, Rc<RefCell<Option<Signal<i32>>>>) {
    let mut app = App::new(size);
    let wire: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let w = wire.clone();
    app.mount(move |cx| {
        let oy = cx.signal(0i32);
        *w.borrow_mut() = Some(oy);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..rows {
            col = col.child(text(format!("item-{i:02}")));
        }
        Scroll::new(col.build()).offset_y(oy).view(cx)
    })
    .unwrap();
    (app, wire)
}

/// Any cell wearing the selection ink anywhere on screen.
fn any_highlight(term: &CaptureTerm, size: Size) -> bool {
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    (0..size.h).any(|y| {
        (0..size.w).any(|x| {
            term.screen()
                .cell(x, y)
                .is_some_and(|c| c.paint.bg == Some(sel_bg))
        })
    })
}

/// THE BUG: with select mode on, a press on the scrollbar thumb used to
/// become a text selection — the layer claimed the gesture at the first
/// cross-cell drag and the driver cancelled the strip's pointer capture
/// with it, so the thumb never moved and the user got a highlight over
/// the content they were trying to scroll.
///
/// A press that lands on a drag-owning surface belongs to that surface.
/// The layer must never arm an anchor there.
#[test]
fn thumb_drag_scrolls_with_select_mode_on() {
    let size = Size::new(30, 10);
    let (mut app, wire) = scrolled_column(size, 30);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    // Settle: the content extent publishes through a deferred `after(0)`
    // (the `size_probe` seam), and the strip is inert until it lands.
    for _ in 0..8 {
        if driver.turn(&mut app, &mut term).unwrap().idle {
            break;
        }
    }
    let oy = wire.borrow().expect("offset wire");
    assert_eq!(oy.get_untracked(), 0, "precondition: parked at the top");

    // The strip is the last column; at offset 0 the thumb sits on the
    // top rows. Grab it (col 30 = x 29, row 1 = y 0), then drag down.
    term.push_input(b"\x1b[<0;30;1M");
    term.push_input(b"\x1b[<32;30;6M");
    driver.turn(&mut app, &mut term).unwrap();

    assert!(
        oy.get_untracked() > 0,
        "the thumb drag must scroll the content (offset stayed {})",
        oy.get_untracked()
    );
    assert!(
        !selection().is_active(),
        "a thumb drag is not a text selection"
    );
    assert!(
        !any_highlight(&term, size),
        "no cell may wear the selection ink for a thumb drag"
    );

    // Release copies nothing: there was never a region.
    term.push_input(b"\x1b[<0;30;6m");
    driver.turn(&mut app, &mut term).unwrap();
    driver.turn(&mut app, &mut term).unwrap(); // custody emission turn
    assert_eq!(
        term.screen().clipboard(),
        None,
        "a thumb drag must not reach the clipboard"
    );
}

/// The stand-down is anchored at the DOWN, not at the pointer: a drag
/// that starts in the content and wanders over the strip keeps
/// selecting. Only where the gesture BEGINS decides who owns it.
#[test]
fn a_selection_that_crosses_the_strip_keeps_selecting() {
    let size = Size::new(30, 10);
    let (mut app, wire) = scrolled_column(size, 30);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();
    let oy = wire.borrow().expect("offset wire");

    // Down in the text at (0,0), drag right ONTO the strip column.
    term.push_input(b"\x1b[<0;1;1M");
    term.push_input(b"\x1b[<32;30;1M");
    driver.turn(&mut app, &mut term).unwrap();

    assert!(selection().is_active(), "the anchor was in the content");
    assert_eq!(
        oy.get_untracked(),
        0,
        "somebody else's gesture never steers the offset"
    );
}

/// An auto-hidden bar with fitting content is INVISIBLE, and an
/// invisible target must not eat a gesture either way: it does not
/// steer the offset (`scroll.rs` already refuses), and it must not
/// suppress a selection that starts on it.
#[test]
fn an_invisible_bar_does_not_eat_a_selection() {
    let size = Size::new(30, 10);
    // Four rows in a ten-row viewport: nothing overflows, no thumb.
    let (mut app, _wire) = scrolled_column(size, 4);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).unwrap();
    selection().set_enabled(true);
    driver.turn(&mut app, &mut term).unwrap();

    term.push_input(b"\x1b[<0;30;1M");
    term.push_input(b"\x1b[<32;30;3M");
    driver.turn(&mut app, &mut term).unwrap();
    assert!(
        selection().is_active(),
        "no bar to own the drag: the selection layer keeps it"
    );
}
