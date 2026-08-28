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
//!   byte-identical (measured numbers printed) — AND the frame is
//!   checked to carry the repaint, since a frame that painted nothing
//!   is bounded by every region;
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
        ..RunConfig::default()
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
    // Ctrl+J, the UNIVERSAL newline chord (0295): 0x0a IS Ctrl+J on the
    // legacy wire — works even where Shift+Enter cannot be reported.
    term.push_input(b"\n");
    term.push_input(b"tail");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "hi\nthere\nend\ntail");
    // Grow-to-cap: content rows at rows(1, 3); the composer's
    // frame-stroke rows are on screen along with the visible lines.
    let lines = screen_lines(&term);
    assert!(lines.iter().any(|l| l.contains("there")));
    assert!(lines.iter().any(|l| l.contains("end")));
    assert!(lines.iter().any(|l| l.contains("tail")));

    term.push_input(b"\r"); // plain Enter: submit
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        *submitted.borrow(),
        vec!["hi\nthere\nend\ntail".to_string()]
    );
    assert_eq!(state.text(), "", "submit handler cleared the buffer");

    // History recall through the wire: empty buffer, Up recalls.
    term.push_input(b"\x1b[A");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        state.text(),
        "hi\nthere\nend\ntail",
        "Up recalled the entry"
    );
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
    // The panel region is named by the CANDIDATE LABELS, not by "the row
    // holds a '/'". That looser rule scored the COMPOSER row as panel —
    // the draft is "/" — so the row this assertion most needs to protect
    // (adjacent to the panel, holding the caret) was the one row exempt
    // from it, while the comment above promised it stayed identical.
    // Measured: loose [6, 7, 8, 9, 10] against strict [6, 7, 8, 9], with
    // the composer on row 10.
    let panel_rows: Vec<usize> = with_panel
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            ["/help", "/theme", "/clear", "/quit"]
                .iter()
                .any(|c| l.contains(c))
        })
        .map(|(i, _)| i)
        .collect();
    assert!(
        !panel_rows.contains(&composer_row),
        "the composer row must be INSIDE the checked set, never exempt \
         from it: {panel_rows:?} with the composer on {composer_row}"
    );
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
    // The row loop is only informative if it CAN report a difference on
    // the composer row: it does, on the panel-open transition, over the
    // same rows through the same comparison. Its silence during the
    // highlight move is then a fact about the frame, not about the check.
    assert_ne!(
        before_open[composer_row], with_panel[composer_row],
        "control: opening the panel does change the composer row"
    );
    let nav_text = String::from_utf8_lossy(&nav_bytes);
    assert!(
        !nav_text.contains("chrome") && !nav_text.contains("status"),
        "static chrome must not re-emit: {nav_text:?}"
    );
    // Containment has two sides, and only one of them was checked. A
    // frame that repainted NOTHING satisfies every assertion above — no
    // row changed, no chrome re-emitted, fewer bytes than the open — so
    // deleting the repaint left this test green. The frame must also
    // carry the work: it addresses exactly the two rows whose highlight
    // moved (measured: CUP to rows 7 and 8, 1-based).
    assert!(
        nav_text.contains("/help") && nav_text.contains("/theme"),
        "the highlight move must repaint the two rows it changed: {nav_text:?}"
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

/// first-app/0291 through the real frame loop: the field failure was
/// an `.autofocus()`ed composer whose placeholder NEVER painted one
/// pixel (focused from boot; the classic rule paints it only when
/// unfocused). With the opt-in, the VT screen must show the caret
/// block (cursor-token bg) at the composer's first text cell and the
/// hint beside it in `text_faint`; one typed character removes it.
#[test]
fn autofocused_composer_paints_placeholder_beside_caret_on_screen() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let composer = TextArea::new()
            .placeholder("describe a task")
            .placeholder_while_focused(true)
            .rows(1, 3)
            .element(cx, &t)
            .autofocus()
            .build();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== chrome =="))
            .child(composer)
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    let tokens = current_theme().tokens;
    let row = 1; // chrome line above; composer's single row below it
    let hint_row = screen_lines(&term)[row as usize].clone();
    assert!(
        hint_row.contains("describe a task"),
        "autofocused composer must paint its hint: {hint_row:?}"
    );
    // Caret block visible at the first text cell (x=1, past the
    // stroke), cursor-token bg; the hint starts one cell past it.
    let caret = term.screen().cell(1, row).expect("caret cell");
    assert_eq!(
        caret.paint.bg,
        Some(tokens.cursor),
        "caret bg is the cursor token"
    );
    let hint = term.screen().cell(2, row).expect("hint cell");
    assert_eq!(hint.ch(), 'd');
    assert_eq!(
        hint.paint.fg,
        Some(tokens.text_faint),
        "hint ink is text_faint"
    );

    // One typed character hides the hint; the glyph takes the cell.
    term.push_input(b"h");
    settle(&mut driver, &mut app, &mut term);
    let typed_row = screen_lines(&term)[row as usize].clone();
    assert!(
        !typed_row.contains("describe a task"),
        "typing hides the focused placeholder: {typed_row:?}"
    );
    assert_eq!(term.screen().cell(1, row).expect("cell").ch(), 'h');
    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

// ---------------------------------------------------------------------
// Word-wise editing over the WIRE (first-app/1310). The classifier's
// unit tests pin the table; these pin that real terminal BYTES reach it
// through the parser and move a real caret in a real composer. Every
// row names the terminal that sends it.
// ---------------------------------------------------------------------

/// Byte offset of `needle` in the fixture (assertions read as intent,
/// not as magic numbers).
fn at(needle: &str) -> usize {
    "alpha beta gamma".find(needle).expect("fixture substring")
}

#[test]
fn word_motion_arrives_on_every_terminal_spelling() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // macOS Terminal.app / iTerm2 (Natural Text Editing): Option+Left
    // is `ESC b`, the readline binding they borrow. This is the
    // spelling that used to do NOTHING at all.
    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1bb");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.caret_byte(), at("gamma"), "ESC b: back one word");

    // The Linux/Windows convention: Ctrl+Left is `CSI 1;5D`.
    term.push_input(b"\x1b[1;5D");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.caret_byte(), at("beta"), "CSI 1;5D: back one word");

    // kitty/WezTerm/ghostty/foot/xterm: Alt+Right is `CSI 1;3C`.
    term.push_input(b"\x1b[1;3C");
    settle(&mut driver, &mut app, &mut term);
    let after_beta = at("beta") + "beta".len();
    assert_eq!(state.caret_byte(), after_beta, "CSI 1;3C: forward one word");

    // macOS Option+Right: `ESC f`.
    term.push_input(b"\x1bf");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        state.caret_byte(),
        "alpha beta gamma".len(),
        "ESC f: forward one word"
    );
    assert_eq!(state.text(), "alpha beta gamma", "motion never edits");
}

#[test]
fn delete_word_arrives_on_every_terminal_spelling() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // macOS Option+Backspace: `ESC DEL`.
    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b\x7f");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "alpha beta ", "ESC DEL: rub out one word");

    // readline's Ctrl+W (`unix-word-rubout`, byte 0x17).
    term.push_input(b"\x17");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "alpha ", "Ctrl+W: rub out one word");

    // Alt+D (`ESC d`) deletes FORWARD from the caret.
    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1bb"); // to "gamma"
    term.push_input(b"\x1bb"); // to "beta"
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.caret_byte(), at("beta"));
    term.push_input(b"\x1bd");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "alpha  gamma", "ESC d: delete word forward");

    // Ctrl+Delete (`CSI 3;5~`), the Linux/Windows spelling of the same.
    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1bb");
    term.push_input(b"\x1bb");
    term.push_input(b"\x1b[3;5~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        state.text(),
        "alpha  gamma",
        "CSI 3;5~: delete word forward"
    );
    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

/// Alt+Left on the modern wires, and the kitty CSI-u dialect of the
/// readline letters. The repo pins both dialects everywhere else; the
/// word table is no exception.
#[test]
fn word_chords_work_on_the_kitty_wire_too() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[1;3D"); // legacy Alt+Left
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.caret_byte(), at("gamma"), "CSI 1;3D");

    term.push_input(b"\x1b[98;3u"); // kitty Alt+b
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.caret_byte(), at("beta"), "kitty Alt+b");

    term.push_input(b"\x1b[127;3u"); // kitty Alt+Backspace
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "beta gamma", "kitty Alt+Backspace rubs out");
}

/// Shift rides along: the same gesture EXTENDS a selection by word, and
/// typing replaces what it covered.
#[test]
fn shift_word_chords_extend_the_selection() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[1;4D"); // Shift+Alt+Left: select "gamma"
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"Z");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "alpha beta Z", "word selection was replaced");

    // Ctrl+Shift+Left is the same gesture on the Linux/Windows wire.
    state.set_text("alpha beta gamma");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[1;6D");
    term.push_input(b"Q");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(state.text(), "alpha beta Q");
    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

/// CODEX PARITY (first-app/1310). One row per binding in Codex's
/// default `EditorKeymap` (`codex-rs/tui/src/keymap.rs`), driven as
/// real wire bytes: if a row here stops matching Codex, muscle memory
/// breaks for anyone moving between the two.
#[test]
fn navigation_chords_match_codex_defaults() {
    let mut app = App::new(Size::new(W, H));
    let (state, _submitted) = composer_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // editor.move_word_left = alt('b'), Alt+Left, Ctrl+Left
    for (label, bytes) in [
        ("alt+b", &b"\x1bb"[..]),
        ("alt+left", &b"\x1b[1;3D"[..]),
        ("ctrl+left", &b"\x1b[1;5D"[..]),
    ] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(state.caret_byte(), at("gamma"), "move_word_left {label}");
    }

    // editor.move_word_right = alt('f'), Alt+Right, Ctrl+Right
    for (label, bytes) in [
        ("alt+f", &b"\x1bf"[..]),
        ("alt+right", &b"\x1b[1;3C"[..]),
        ("ctrl+right", &b"\x1b[1;5C"[..]),
    ] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(b"\x1bb"); // to "gamma"
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(
            state.caret_byte(),
            "alpha beta gamma".len(),
            "move_word_right {label}"
        );
    }

    // editor.move_line_start = Home, Ctrl+A / move_line_end = End, Ctrl+E
    for (label, bytes) in [("home", &b"\x1b[H"[..]), ("ctrl+a", &b"\x01"[..])] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(state.caret_byte(), 0, "move_line_start {label}");
    }
    for (label, bytes) in [("end", &b"\x1b[F"[..]), ("ctrl+e", &b"\x05"[..])] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(b"\x1bb");
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(
            state.caret_byte(),
            "alpha beta gamma".len(),
            "move_line_end {label}"
        );
    }

    // editor.delete_backward_word = Alt+Backspace, Ctrl+Backspace, Ctrl+W
    for (label, bytes) in [
        ("alt+backspace", &b"\x1b\x7f"[..]),
        ("ctrl+backspace", &b"\x1b[127;5u"[..]),
        ("ctrl+w", &b"\x17"[..]),
    ] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(state.text(), "alpha beta ", "delete_backward_word {label}");
    }

    // editor.delete_forward_word = Alt+Delete, Ctrl+Delete, Alt+D
    for (label, bytes) in [
        ("alt+delete", &b"\x1b[3;3~"[..]),
        ("ctrl+delete", &b"\x1b[3;5~"[..]),
        ("alt+d", &b"\x1bd"[..]),
    ] {
        state.set_text("alpha beta gamma");
        settle(&mut driver, &mut app, &mut term);
        term.push_input(b"\x1bb");
        term.push_input(b"\x1bb"); // caret at "beta"
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(state.text(), "alpha  gamma", "delete_forward_word {label}");
    }
    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

/// A composer hosted INSIDE an overlay layer at a non-zero screen
/// origin, mounted focused on a draft whose caret already sits in a
/// trigger token — so its panel opens with NO event ever routed through
/// the completion wrapper.
///
/// That is the whole point of the case. `caret_cell` is LAYER-LOCAL by
/// contract, so the dropdown's screen anchor is the caret translated by
/// the layer's origin; the wrapper learned that origin from its
/// capture-phase handler and from nowhere else, so a composer nobody
/// has typed into yet anchored as though its layer were at (0, 0).
/// Opening the layer from a ROOT shortcut is what keeps the composer
/// eventless: the 'd' key is consumed by the root tree.
fn layered_composer_app(app: &mut App, bounds: Rect) {
    let overlays = app.overlays();
    app.mount(move |cx| {
        let overlays = overlays.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("root world"))
            .shortcut(KeyChord::plain(Key::Char('d')), move |_| {
                let lcx = cx.child();
                let t = use_theme(lcx).get().tokens;
                let state = TextAreaState::new(lcx);
                // The draft is already a live trigger token: the
                // controller opens on the layer's first solve.
                state.set_text("/he");
                let composer = TextArea::new()
                    .state(&state)
                    .rows(1, 1)
                    .element(lcx, &t)
                    .autofocus()
                    .build();
                let wrapped = Completion::new()
                    .trigger('/', |q| {
                        ["help", "theme", "clear", "quit"]
                            .iter()
                            .filter(|c| c.starts_with(q))
                            .map(|c| CompletionCandidate::new(format!("/{c}"), format!("/{c} ")))
                            .collect()
                    })
                    .max_visible(4)
                    .attach(lcx, &overlays, &state, composer);
                overlays.layer_tree(
                    1000,
                    bounds,
                    true,
                    lcx,
                    Element::new()
                        .style(LayoutStyle::column())
                        .child(text("drawer chrome"))
                        .child(
                            Element::new()
                                .style(LayoutStyle::column().grow(1.0))
                                .build(),
                        )
                        .child(wrapped)
                        .build(),
                );
            })
            .build()
    })
    .expect("mount");
}

#[test]
fn a_composer_inside_a_layer_anchors_its_panel_in_screen_space() {
    // Layer origin (12, 2): both axes non-zero, and both misses are
    // observable in this pose (checked, not assumed — the first version
    // of this test asserted a vertical bound that the DEFECT satisfied
    // too, which is a check that cannot fail).
    let bounds = Rect::new(12, 2, 28, 7);
    let mut app = App::new(Size::new(W, H));
    layered_composer_app(&mut app, bounds);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"d"); // root shortcut opens the layer
    settle(&mut driver, &mut app, &mut term);

    let lines = screen_lines(&term);
    let (row, line) = lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.contains("/help"))
        .map(|(i, l)| (i as i32, l.clone()))
        .unwrap_or_else(|| panic!("dropdown never opened: {lines:?}"));
    let col = line.find("/help").expect("found above") as i32;

    let composer_row = lines
        .iter()
        .position(|l| l.contains('▐'))
        .expect("composer frame on screen") as i32;

    // BOTH axes are checked because both were wrong, and each one alone
    // is satisfiable by a half-fix. Measured: layer-local placement puts
    // the panel at (row 7, col 5); screen placement at (row 9, col 17),
    // with the composer's own frame on row 8.
    assert_eq!(
        row,
        composer_row + 1,
        "the panel belongs on the row under the composer ({composer_row}), \
         not where the untranslated caret row put it: {lines:?}"
    );
    assert!(
        col >= bounds.x,
        "panel opened at column {col}, left of its layer's own edge {} — \
         the caret was anchored in layer-local space: {lines:?}",
        bounds.x
    );
    println!("measured: panel at row {row}, column {col}; layer at {bounds:?}");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}
