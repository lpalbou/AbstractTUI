//! Disclosure wave (first-app 0260 + field-agora 0850): the fold/
//! unfold card and the Feed item-press enabler, through the REAL
//! `Driver` — wire bytes in via `CaptureTerm`, modeled VT screen out
//! (the wave_choice_0271.rs harness posture; helper duplication across
//! integration files is the house style).
//!
//! Covers the commissioned integration surface: click-on-title
//! toggles, Enter toggles while focused, the capped body's visible
//! scrollbar + wheel scrolling, zero idle bytes with cards parked
//! (folded AND unfolded), toggle damage contained to the card's band,
//! and SGR item presses reporting `(key, row_within_item)`.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

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

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..128 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 128 turns");
}

fn boot(app: &mut App, term: &mut CaptureTerm) -> Driver {
    let mut driver = Driver::new(app, term, config()).expect("driver");
    settle(&mut driver, app, term);
    driver
}

fn screen_lines(term: &CaptureTerm) -> Vec<String> {
    term.screen()
        .to_text()
        .lines()
        .map(str::to_string)
        .collect()
}

/// SGR left click (press + release) at 1-BASED terminal coordinates.
fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

/// SGR wheel-down at 1-based coordinates.
fn sgr_wheel_down(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<65;{col};{row}M").into_bytes()
}

/// 1-based rows addressed by absolute CUP (`ESC [ r ; c H` / `ESC [ H`)
/// in an emitted byte stream — the damage-containment probe (the
/// presenter re-anchors each damaged row absolutely; the frame trailer
/// parks bottom-LEFT, so the park row is the screen's last row).
fn cup_rows(bytes: &[u8]) -> Vec<i32> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b';') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'H' {
                let params = &bytes[i + 2..j];
                let row: i32 = params
                    .split(|&b| b == b';')
                    .next()
                    .filter(|p| !p.is_empty())
                    .map(|p| String::from_utf8_lossy(p).parse().unwrap_or(1))
                    .unwrap_or(1);
                rows.push(row);
            }
            i = j;
        }
        i += 1;
    }
    rows
}

const W: i32 = 32;
const H: i32 = 12;

fn twelve_lines() -> String {
    (0..12)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ===========================================================================
// Toggle surfaces through the wire: click on the title row, Enter while
// focused (the click focused the header — the tree's click-to-focus rule).
// ===========================================================================

#[test]
fn click_on_the_title_unfolds_and_enter_folds_back() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("alpha card", "alpha body text").view(cx))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let lines = screen_lines(&term);
    assert!(
        lines[0].contains('▸') && lines[0].contains("alpha card"),
        "folded header renders: {lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("alpha body")),
        "folded: no body on screen"
    );

    // Click the title row (screen row 0 = SGR row 1).
    term.push_input(&sgr_click(4, 1));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(lines[0].contains('▾'), "glyph flips open: {lines:#?}");
    assert!(
        lines.iter().any(|l| l.contains("alpha body")),
        "unfolded: body renders: {lines:#?}"
    );

    // The click focused the header: Enter folds it back.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(lines[0].contains('▸'), "{lines:#?}");
    assert!(
        !lines.iter().any(|l| l.contains("alpha body")),
        "Enter while focused re-folds: {lines:#?}"
    );
}

// ===========================================================================
// The capped body: visible scrollbar on overflow, wheel scrolls the body.
// ===========================================================================

#[test]
fn capped_body_shows_a_scrollbar_and_the_wheel_scrolls_it() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("log", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(4)
                    .view(cx),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let lines = screen_lines(&term);
    assert!(lines[1].contains("line 0"), "{lines:#?}");
    assert!(lines[4].contains("line 3"), "4 capped rows: {lines:#?}");
    assert!(
        lines[5].contains("BELOW"),
        "card ends at the cap: {lines:#?}"
    );
    // The thumb lives in the body's right column (host right pad = 1).
    let bar_col = W - 2;
    let bar: String = (1..5)
        .filter_map(|y| term.screen().cell(bar_col, y).map(|c| c.ch()))
        .collect();
    assert!(
        bar.contains('┃'),
        "scrollbar thumb visible on overflow: {bar:?}\n{lines:#?}"
    );

    // Wheel-down over the body (row 3 on screen = SGR row 3+1): +3 rows.
    term.push_input(&sgr_wheel_down(5, 3));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[1].contains("line 3"),
        "wheel scrolled the body: {lines:#?}"
    );
    assert!(lines[5].contains("BELOW"), "card extent held: {lines:#?}");
}

// ===========================================================================
// Idle honesty: cards parked (one folded, one unfolded + capped scroll)
// cost zero bytes on idle turns.
// ===========================================================================

#[test]
fn parked_cards_cost_zero_idle_bytes() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("folded", "hidden").view(cx))
            .child(
                Disclosure::text("open", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(3)
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let _ = term.take_bytes();

    for i in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("idle turn");
        assert!(turn.idle, "turn {i} must be idle");
        assert!(!turn.rendered, "turn {i} rendered");
    }
    assert!(
        term.bytes().is_empty(),
        "idle turns wrote bytes: {:?}",
        String::from_utf8_lossy(term.bytes())
    );
}

// ===========================================================================
// Damage containment: toggling a card repaints its own band, never the
// rows above it.
// ===========================================================================

#[test]
fn toggle_damage_stays_inside_the_cards_band() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("top status row"))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("second static row"))
                    .build(),
            )
            .child(Disclosure::text("deep card", "one\ntwo").view(cx))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let before = screen_lines(&term);
    let _ = term.take_bytes();

    // Toggle the card (header at screen row 2 = SGR row 3).
    term.push_input(&sgr_click(4, 3));
    settle(&mut driver, &mut app, &mut term);
    let bytes = term.take_bytes();
    let rows = cup_rows(&bytes);
    assert!(
        !rows.is_empty(),
        "the toggle must repaint something: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        rows.iter().all(|&r| r >= 3),
        "damage leaked above the card (CUP rows {rows:?}): {:?}",
        String::from_utf8_lossy(&bytes)
    );
    let after = screen_lines(&term);
    assert_eq!(before[0], after[0], "static row 0 untouched");
    assert_eq!(before[1], after[1], "static row 1 untouched");
    assert!(
        after.iter().any(|l| l.contains("one")),
        "the card did unfold: {after:#?}"
    );
}

// ===========================================================================
// Modal composition: a Disclosure inside a Modal toggles and its
// measured card height stays honest inside the panel.
// ===========================================================================

#[test]
fn disclosure_composes_inside_a_modal() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let ss = scope_slot.clone();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .focusable()
            .child(text("host app row"))
            .build()
    })
    .expect("mount");
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let cx = scope_slot.borrow().expect("scope");
    // 24x8 panel centered in 32x12 -> bounds (4,2), 1-cell padding ->
    // content at (5,3): the card header renders on screen row 3.
    let _modal = Modal::open(&overlays, cx, size, Size::new(24, 8), |mcx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("in modal", "modal body line").view(mcx))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("FOOTER"))
                    .build(),
            )
            .build()
    });
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[3].contains('▸') && lines[3].contains("in modal"),
        "card header inside the panel: {lines:#?}"
    );
    assert!(
        lines[4].contains("FOOTER"),
        "folded card measures one row — the footer sits right below: {lines:#?}"
    );

    // Click the title (row 3 -> SGR row 4): the body opens IN the panel
    // and pushes the footer down by exactly the body height.
    term.push_input(&sgr_click(8, 4));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("modal body line")),
        "body renders inside the modal: {lines:#?}"
    );
    assert!(
        lines[5].contains("FOOTER"),
        "unfolded card measures header + 1 body row: {lines:#?}"
    );
    assert!(
        lines[0].contains("host app row"),
        "the host row above the panel is untouched: {lines:#?}"
    );
}

// ===========================================================================
// Feed item press through the wire: SGR click reports (key,
// row_within_item); gap rows are silent.
// ===========================================================================

#[test]
fn feed_sgr_click_reports_key_and_row_within_item() {
    let size = Size::new(W, H);
    let log: Rc<RefCell<Vec<(String, i32)>>> = Rc::default();
    let sink = log.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        let feed = FeedState::new(cx);
        feed.push("a", FeedItem::text("alpha"));
        feed.push("b", FeedItem::text("b one\nb two"));
        let sink = sink.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Feed::new(&feed)
                    .on_item_press(move |key, row| sink.borrow_mut().push((key.into(), row)))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    // Content rows: 0 = a[0], 1 = gap, 2 = b[0], 3 = b[1] (SGR +1).
    term.push_input(&sgr_click(3, 3)); // b, row 0
    settle(&mut driver, &mut app, &mut term);
    term.push_input(&sgr_click(3, 2)); // gap: silent
    settle(&mut driver, &mut app, &mut term);
    term.push_input(&sgr_click(3, 4)); // b, row 1
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        &[("b".into(), 0), ("b".into(), 1)],
        "wire presses map to (key, row_within_item); the gap is silent"
    );
}
