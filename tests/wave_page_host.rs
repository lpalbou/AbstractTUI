//! app-kits/0545 — PageHost acceptance through the REAL frame loop
//! (`Driver::turn` against `CaptureTerm`): clicks and chords as raw
//! wire bytes (legacy tilde + kitty CSI-u spellings), damage
//! containment on switch (byte-replay proof), zero idle with a parked
//! host, and a full-page Feed scrolling normally inside a page.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::text;
use abstracttui::widgets::{Feed, FeedItem, FeedState, PageHost};

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

/// Drive turns until idle (bounded).
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// SGR left click (press + release) at 1-BASED terminal coordinates.
fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

/// Three distinctive pages under an OUTSIDE header row (the host is
/// deliberately NOT the tree root here — the wrapper is the damage
/// containment control surface).
fn shell_app(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let host = PageHost::new()
            .page("alpha", "Alpha", |_| text("BODY ALPHA"))
            .page("beta", "Beta", |_| text("BODY BETA"))
            .page("gamma", "Gamma", |_| text("BODY GAMMA"))
            .number_jump(true)
            .element(cx, &t)
            .build();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("HEADER CHROME"))
            .child(host)
            .build()
    })
    .expect("mount");
    app
}

#[test]
fn pages_switch_by_click_chords_and_digits_through_the_wire() {
    let size = Size::new(44, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = shell_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY ALPHA"));

    // Click " Beta " (bar title row is terminal row 2; " Alpha " spans
    // cols 1..7, " Beta " starts at col 9).
    term.push_input(&sgr_click(10, 2));
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY BETA"), "{}", screen(&term));

    // Legacy wire chords: Ctrl+PgDn / Ctrl+PgUp.
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY GAMMA"), "{}", screen(&term));
    term.push_input(b"\x1b[5;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY BETA"), "{}", screen(&term));

    // Opt-in digit jump (focus sits on the bar after the click).
    term.push_input(b"3");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY GAMMA"), "{}", screen(&term));

    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn letter_chords_fire_on_both_wire_spellings_through_the_wire() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        // Host as the ROOT element: chords answer from frame one.
        PageHost::new()
            .page("one", "One", |_| text("BODY ONE"))
            .page("two", "Two", |_| text("BODY TWO"))
            .page("three", "Three", |_| text("BODY THREE"))
            .chords(
                &[KeyChord::plain(Key::Char('H'))],
                &[KeyChord::plain(Key::Char('L'))],
            )
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Legacy spelling: Shift baked into the byte.
    term.push_input(b"L");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY TWO"), "{}", screen(&term));

    // Kitty spelling: base codepoint + SHIFT modifier (CSI 108;2u).
    term.push_input(b"\x1b[108;2u");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY THREE"), "{}", screen(&term));

    // The unshifted letter means 'l', never 'L': no switch.
    term.push_input(b"l");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY THREE"), "{}", screen(&term));
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

/// Damage containment: the switch frame's bytes, replayed alone on a
/// fresh VT model, rebuild the bar and the page region — and touch
/// NOTHING outside the host (the header row stays blank in the
/// replay, because no byte ever addressed it).
#[test]
fn switch_frame_bytes_stay_inside_the_host_region() {
    let size = Size::new(44, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = shell_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    // Establish focus inside the host (the wrapper is not the host).
    term.push_input(&sgr_click(2, 2));
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    term.push_input(b"\x1b[6;5~"); // Ctrl+PgDn: Alpha -> Beta
    settle(&mut driver, &mut app, &mut term);
    let bytes = term.take_bytes();
    assert!(!bytes.is_empty(), "the switch must emit");

    let mut replay = VtScreen::new(size);
    replay.feed(&bytes);
    let replayed = replay.to_text();
    let lines: Vec<&str> = replayed.lines().collect();
    assert!(
        lines.first().map(|l| l.trim().is_empty()).unwrap_or(true),
        "the switch frame addressed the header row outside the host:\n{replayed}"
    );
    // The diff emits only CHANGED cells: "BODY ALPHA" -> "BODY BETA"
    // shares the "BODY " prefix, so the replay shows the delta "BETA".
    assert!(
        replayed.contains("BETA"),
        "the switch frame painted the new page's delta:\n{replayed}"
    );
    // The live screen still shows the untouched header.
    assert!(screen(&term).contains("HEADER CHROME"));
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

/// Zero idle: a parked host (pages with signals, badges bound) costs
/// nothing while nothing changes — idle turns, no renders, no bytes.
#[test]
fn parked_host_idles_at_zero() {
    let size = Size::new(44, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let unread = cx.signal(2u32);
        PageHost::new()
            .page("inbox", "Inbox", |_| text("BODY INBOX"))
            .page("done", "Done", |_| text("BODY DONE"))
            .badge("inbox", move || Some(unread.get().to_string()))
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    for _ in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("idle turn");
        assert!(turn.idle, "turn must report idle");
        assert!(!turn.rendered, "idle turn rendered");
    }
    assert!(term.bytes().is_empty(), "idle turns wrote bytes");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

/// A full-page Feed inside a page scrolls normally (wheel over the
/// content), and the container chord still switches pages over it.
#[test]
fn full_page_feed_scrolls_inside_a_page_and_chords_still_switch() {
    let size = Size::new(44, 12);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let builds: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let b = builds.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let b = b.clone();
        PageHost::new()
            .page("feed", "Feed", move |gcx| {
                *b.borrow_mut() += 1;
                let feed = FeedState::new(gcx);
                for i in 0..30 {
                    feed.push(
                        format!("h{i}"),
                        FeedItem::markdown(format!("**msg {i}** body")),
                    );
                }
                let follow = gcx.signal(false);
                Scroll::new(Feed::new(&feed).view(gcx))
                    .follow_tail(follow)
                    .view(gcx)
            })
            .page("other", "Other", |_| text("BODY OTHER"))
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("msg 0"), "{}", screen(&term));

    // Wheel-down over the page region: the feed scrolls in place.
    for _ in 0..4 {
        term.push_input(b"\x1b[<65;6;6M");
        settle(&mut driver, &mut app, &mut term);
    }
    let s = screen(&term);
    assert!(!s.contains("msg 0 "), "the top scrolled away:\n{s}");
    assert!(s.contains("msg"), "still on the feed page:\n{s}");
    assert_eq!(*builds.borrow(), 1, "scrolling never remounted the page");

    // The container chord still owns page switching over the feed.
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY OTHER"), "{}", screen(&term));
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
