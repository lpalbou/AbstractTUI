//! COMPOSER wave: TextArea (backlog 0120) + the anchored passive-panel
//! completion dropdown (0500 slice), hardened through the REAL frame
//! loop — `Driver::turn` against `CaptureTerm`, wire bytes in (legacy,
//! kitty CSI-u, SGR mouse, bracketed paste), modeled VT screen out.
//!
//! Pins, by spec:
//! - submit vs newline chords on BOTH wires: plain Enter submits,
//!   legacy Alt+Enter (ESC CR) and kitty Shift+Enter (CSI 13;2u)
//!   insert; the buffer clears through the app's submit handler and
//!   history recall replays it (0120 §3/§4);
//! - grow-to-cap through the real layout loop (0120 §2);
//! - bracketed paste inserts newlines whole and never submits (§5);
//! - the completion dropdown opens ANCHORED at the caret (flipped
//!   above a bottom composer), navigates/accepts/dismisses via wire
//!   bytes, and closing repaints the vacated region from below;
//! - damage containment: with the panel open, a highlight move emits
//!   bytes bounded to the panel region — static chrome rows stay
//!   byte-identical (measured numbers printed);
//! - every emitted byte is modeled (`unknown_seq_count == 0`).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::anchored::{Completion, CompletionCandidate};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

const W: i32 = 44;
const H: i32 = 12;

fn config() -> RunConfig {
    RunConfig {
        // ADR-0003: `Capabilities` is `#[non_exhaustive]`; this file
        // compiles as a downstream crate, so construction goes
        // through `with`.
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
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// A transcript-shaped app: chrome line, content pane, bottom composer
/// with '/'+'@' completion, status line. Returns the composer state and
/// the submit log.
fn composer_app(app: &mut App) -> (TextAreaState, Rc<RefCell<Vec<String>>>) {
    let overlays = app.overlays();
    let submitted: Rc<RefCell<Vec<String>>> = Default::default();
    let s2 = submitted.clone();
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        let submit_state = state.clone();
        let composer = TextArea::new()
            .state(&state)
            .rows(1, 3)
            .placeholder("message")
            .on_submit(move |v| {
                s2.borrow_mut().push(v.to_string());
                submit_state.push_history(v);
                submit_state.clear();
            })
            .element(cx, &t)
            .autofocus()
            .build();
        let wrapped = Completion::new()
            .trigger('/', |q| {
                ["help", "theme", "clear", "quit"]
                    .iter()
                    .filter(|c| c.starts_with(q))
                    .map(|c| {
                        CompletionCandidate::new(format!("/{c}"), format!("/{c} ")).detail("cmd")
                    })
                    .collect()
            })
            .trigger('@', |q| {
                ["alice", "bob"]
                    .iter()
                    .filter(|c| c.starts_with(q))
                    .map(|c| CompletionCandidate::new(format!("@{c}"), format!("@{c} ")))
                    .collect()
            })
            .max_visible(4)
            .attach(cx, &overlays, &state, composer);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== transcript chrome =="))
            .child(
                Element::new()
                    .style(LayoutStyle::column().grow(1.0))
                    .child(text("pane row alpha"))
                    .child(text("pane row beta"))
                    .child(text("pane row gamma"))
                    .build(),
            )
            .child(wrapped)
            .child(text(" status: ready"))
            .build()
    })
    .expect("mount");
    let state = holder.borrow().clone().expect("state");
    (state, submitted)
}

fn screen_lines(term: &CaptureTerm) -> Vec<String> {
    term.screen()
        .to_text()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn submit_vs_newline_chords_on_both_wires_and_history_recall() {
    let mut app = App::new(Size::new(W, H));
    let (state, submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"hi");
    term.push_input(b"\x1b[13;2u"); // kitty Shift+Enter: newline
    term.push_input(b"there");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "hi\nthere");
    term.push_input(b"\x1b\r"); // legacy Alt+Enter: newline
    term.push_input(b"end");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "hi\nthere\nend");
    // Grow-to-cap: three content rows at rows(1, 3); the composer's
    // three frame-stroke rows are on screen along with all lines.
    let lines = screen_lines(&term);
    assert!(lines.iter().any(|l| l.contains("hi")));
    assert!(lines.iter().any(|l| l.contains("there")));
    assert!(lines.iter().any(|l| l.contains("end")));

    term.push_input(b"\r"); // plain Enter: submit
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*submitted.borrow(), vec!["hi\nthere\nend".to_string()]);
    assert_eq!(state.text(), "", "submit handler cleared the buffer");

    // History recall through the wire: empty buffer, Up recalls.
    term.push_input(b"\x1b[A");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "hi\nthere\nend", "Up recalled the entry");
    // Down at the end: forward past the newest restores the draft ("").
    term.push_input(b"\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "", "the (empty) draft returned");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn bracketed_paste_inserts_multiline_and_never_submits() {
    let mut app = App::new(Size::new(W, H));
    let (state, submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[200~first line\r\nsecond line\x1b[201~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "first line\nsecond line");
    assert!(submitted.borrow().is_empty(), "paste never submits");
    let lines = screen_lines(&term);
    assert!(lines.iter().any(|l| l.contains("first line")));
    assert!(lines.iter().any(|l| l.contains("second line")));
}

#[test]
fn completion_dropdown_full_round_trip_with_damage_containment() {
    let mut app = App::new(Size::new(W, H));
    let (state, submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let before_open = screen_lines(&term);
    assert!(
        before_open.iter().any(|l| l.contains("pane row beta")),
        "content pane visible before the dropdown"
    );

    // '/' opens the dropdown, anchored at the caret and flipped ABOVE
    // the bottom composer (no room below).
    term.push_input(b"/");
    settle(&mut driver, &mut app, &mut term);
    let open_bytes = term.take_bytes().len();
    let with_panel = screen_lines(&term);
    let help_row = with_panel
        .iter()
        .position(|l| l.contains("/help"))
        .expect("dropdown visible");
    let composer_row = with_panel
        .iter()
        .position(|l| l.contains('▐'))
        .expect("composer frame");
    assert!(help_row < composer_row, "panel sits above the composer");
    assert!(
        with_panel.iter().any(|l| l.contains("/quit")),
        "all four candidates offered: {with_panel:?}"
    );

    // Damage containment: a highlight move repaints the PANEL region
    // only — chrome/status/composer rows stay byte-identical, and the
    // emitted bytes stay far below a full-frame repaint.
    let panel_rows: Vec<usize> = with_panel
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains('/'))
        .map(|(i, _)| i)
        .collect();
    term.push_input(b"\x1b[B"); // Down: highlight row 1
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.rendered);
    settle(&mut driver, &mut app, &mut term);
    let nav_bytes = term.take_bytes();
    let after_nav = screen_lines(&term);
    for (i, (before, after)) in with_panel.iter().zip(&after_nav).enumerate() {
        if !panel_rows.contains(&i) {
            assert_eq!(before, after, "row {i} outside the panel changed");
        }
    }
    let nav_text = String::from_utf8_lossy(&nav_bytes);
    assert!(
        !nav_text.contains("chrome") && !nav_text.contains("status"),
        "static chrome must not re-emit: {nav_text:?}"
    );
    assert!(
        nav_bytes.len() < open_bytes,
        "highlight flip ({} bytes) cheaper than panel open ({} bytes)",
        nav_bytes.len(),
        open_bytes
    );
    println!(
        "measured: panel-open frame {} bytes; highlight-move frame {} bytes",
        open_bytes,
        nav_bytes.len()
    );

    // Enter accepts the highlighted candidate ("/theme": row 1).
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "/theme ");
    assert!(submitted.borrow().is_empty(), "accept is not a submit");
    let after_accept = screen_lines(&term);
    assert!(
        !after_accept.iter().any(|l| l.contains("/help")),
        "dropdown closed"
    );
    assert!(
        after_accept.iter().any(|l| l.contains("pane row beta")),
        "vacated region repainted from below"
    );
    assert!(
        after_accept.iter().any(|l| l.contains("/theme")),
        "accepted text in the composer"
    );

    // Esc dismisses a reopened dropdown (kitty CSI-u escape byte form),
    // and typing inside the dismissed token stays calm.
    term.push_input(b"x"); // "/theme x" -> token "x…" no trigger, closed
    term.push_input(b" /q");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen_lines(&term).iter().any(|l| l.contains("/quit")),
        "fresh trigger reopened"
    );
    term.push_input(b"\x1b[27u"); // kitty Escape
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("/quit")),
        "Escape dismissed"
    );
    term.push_input(b"u");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("/quit")),
        "same token stays muted"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

/// SGR mouse press+release at 0-based cell (x, y), as a terminal emits it.
fn sgr_click_bytes(x: i32, y: i32) -> (Vec<u8>, Vec<u8>) {
    (
        format!("\x1b[<0;{};{}M", x + 1, y + 1).into_bytes(),
        format!("\x1b[<0;{};{}m", x + 1, y + 1).into_bytes(),
    )
}

#[test]
fn mouse_click_accepts_a_candidate_through_the_wire() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"@");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    let bob_row = lines
        .iter()
        .position(|l| l.contains("@bob"))
        .expect("mention dropdown open") as i32;
    let bob_col = lines[bob_row as usize].find("@bob").unwrap() as i32;
    let (press, release) = sgr_click_bytes(bob_col, bob_row);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "@bob ", "click accepted the row");
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("@alice")),
        "dropdown closed after the click"
    );
}
