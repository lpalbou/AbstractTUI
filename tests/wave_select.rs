//! SELECT wave: the choice-control family (backlog 0500) + the OWNED
//! anchored-popup mode, hardened through the REAL frame loop —
//! `Driver::turn` against `CaptureTerm`, wire bytes in (arrows, CSI-u
//! escape, SGR mouse), modeled VT screen out.
//!
//! Pins, by spec:
//! - full keyboard round trip: Tab focuses the trigger, Enter opens
//!   the popup, arrows move the HIGHLIGHT ONLY (the bound value and
//!   `on_change` untouched — 0250), Enter commits exactly once, the
//!   vacated region repaints from below;
//! - damage containment: with the popup open, a highlight move emits
//!   bytes bounded to the popup region — rows outside it stay
//!   byte-identical (measured numbers printed);
//! - the STACKED case (spec F1): modal → second modal above it → the
//!   select popup opens above BOTH, receives the keys, and closing it
//!   returns key ownership to the second modal;
//! - SGR click on the trigger opens; click on a row commits; an
//!   outside press dismisses WITHOUT acting on what is below;
//! - every emitted byte is modeled (`unknown_seq_count == 0`).
//!
//! The Combobox + MultiSelect wire pins live in the split sibling
//! `wave_select_faces.rs` (file budget).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rect, Size};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Phase, UiEvent};

const W: i32 = 44;
const H: i32 = 14;

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
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

fn screen_lines(term: &CaptureTerm) -> Vec<String> {
    term.screen()
        .to_text()
        .lines()
        .map(str::to_string)
        .collect()
}

/// SGR mouse press+release at 0-based cell (x, y).
fn sgr_click_bytes(x: i32, y: i32) -> (Vec<u8>, Vec<u8>) {
    (
        format!("\x1b[<0;{};{}M", x + 1, y + 1).into_bytes(),
        format!("\x1b[<0;{};{}m", x + 1, y + 1).into_bytes(),
    )
}

/// COLUMN of `needle` in a screen row — `str::find` returns a BYTE
/// offset, which drifts right of the true cell once the row carries
/// multibyte glyphs (the select frame strokes are 3 UTF-8 bytes each).
/// All glyphs in these screens are width-1, so chars == columns.
fn col_of(line: &str, needle: &str) -> i32 {
    let byte = line
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} in {line:?}"));
    line[..byte].chars().count() as i32
}

fn channel_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("stable").hint("lts"),
        SelectOption::new("beta"),
        SelectOption::new("nightly"),
        SelectOption::new("archive").disabled(true),
        SelectOption::new("custom"),
    ]
}

/// (bound value, on_change log, button-hit counter) of `select_app`.
type SelectAppState = (Signal<usize>, Rc<RefCell<Vec<usize>>>, Rc<RefCell<u32>>);

/// A settings-shaped app: chrome, a Select row with a button beside it
/// (outside-press victim), a live value line, status.
fn select_app(app: &mut App) -> SelectAppState {
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let hits: Rc<RefCell<u32>> = Default::default();
    let holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let (c2, h2, v2) = (changes.clone(), hits.clone(), holder.clone());
    app.mount(move |cx| {
        let value = cx.signal(0usize);
        *v2.borrow_mut() = Some(value);
        let c3 = c2.clone();
        let h3 = h2.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== settings chrome =="))
            .child(
                Element::new()
                    .style(LayoutStyle::row().h(1).gap(2))
                    .child(
                        Select::new(channel_options())
                            .value(value)
                            .layout(LayoutStyle::default().w(20).h(1).shrink(0.0))
                            .on_change(move |i| c3.borrow_mut().push(i))
                            .view(cx),
                    )
                    // Beside the select, OUTSIDE the popup's x-range:
                    // the outside-press victim.
                    .child(
                        abstracttui::widgets::Button::new("danger")
                            .on_click(move || *h3.borrow_mut() += 1)
                            .view(cx),
                    )
                    .build(),
            )
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("value = {}", value.get()))
            }))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(text(" status: ready"))
            .build()
    })
    .expect("mount");
    let value = holder.borrow().expect("value signal");
    (value, changes, hits)
}

#[test]
fn select_full_keyboard_round_trip_with_damage_containment() {
    let mut app = App::new(Size::new(W, H));
    let (value, changes, _hits) = select_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let before = screen_lines(&term);
    assert!(before[1].contains("stable"), "trigger shows the value");
    assert!(before[1].contains("▾"), "chevron affordance");
    assert!(before[2].contains("value = 0"));

    // Tab focused the trigger at driver start (first focusable); Enter
    // opens the popup below the trigger row.
    term.push_input(b"\t");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let open_bytes = term.take_bytes().len();
    let with_popup = screen_lines(&term);
    assert!(
        with_popup[2].contains("stable") && with_popup[2].contains("lts"),
        "popup row 0 under the trigger (hint renders): {with_popup:?}"
    );
    assert!(with_popup[3].contains("beta"));
    assert!(
        with_popup[6].contains("custom"),
        "all five options: {with_popup:?}"
    );
    let popup_rows: Vec<usize> = vec![2, 3, 4, 5, 6];

    // Damage containment: a highlight move repaints popup rows only.
    term.push_input(b"\x1b[B"); // Down
    settle(&mut driver, &mut app, &mut term);
    let nav_bytes = term.take_bytes();
    let after_nav = screen_lines(&term);
    for (i, (a, b)) in with_popup.iter().zip(&after_nav).enumerate() {
        if !popup_rows.contains(&i) {
            assert_eq!(a, b, "row {i} outside the popup changed");
        }
    }
    let nav_text = String::from_utf8_lossy(&nav_bytes);
    assert!(
        !nav_text.contains("chrome") && !nav_text.contains("status"),
        "static chrome must not re-emit: {nav_text:?}"
    );
    assert!(
        nav_bytes.len() < open_bytes,
        "highlight flip ({} bytes) cheaper than popup open ({} bytes)",
        nav_bytes.len(),
        open_bytes
    );
    println!(
        "measured: select popup-open frame {} bytes; highlight-move frame {} bytes",
        open_bytes,
        nav_bytes.len()
    );

    // 0250 through the wire: arrows never wrote the value.
    assert!(
        after_nav[2].contains("stable") || after_nav[2].contains("lts"),
        "popup covers the value line while open"
    );
    assert_eq!(value.get_untracked(), 0, "movement is not activation");
    assert!(changes.borrow().is_empty());

    // Enter commits the highlighted row (beta), closes, repaints below.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let after_commit = screen_lines(&term);
    assert_eq!(value.get_untracked(), 1, "beta committed");
    assert_eq!(changes.borrow().as_slice(), [1], "exactly one on_change");
    assert!(
        after_commit[1].contains("beta"),
        "trigger re-renders the value"
    );
    assert!(
        after_commit[2].contains("value = 1"),
        "vacated region repainted from below: {after_commit:?}"
    );
    assert!(!after_commit[3].contains("beta"), "popup gone");

    // Esc abandons: reopen, move, escape — value stays.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[B");
    term.push_input(b"\x1b[27u"); // kitty Escape
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(value.get_untracked(), 1, "Escape restored nothing new");
    assert_eq!(changes.borrow().as_slice(), [1]);
    assert!(
        screen_lines(&term)[2].contains("value = 1"),
        "popup closed on Escape"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn sgr_click_opens_picks_and_outside_press_never_acts_below() {
    let mut app = App::new(Size::new(W, H));
    let (value, _changes, hits) = select_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // The outside-press victim: the button beside the select, OUTSIDE
    // the popup's x-range (the popup is width-matched to the trigger).
    let first = screen_lines(&term);
    let button_row = first
        .iter()
        .position(|l| l.contains("danger"))
        .expect("button visible") as i32;
    let button_x = col_of(&first[button_row as usize], "danger");

    // Click the trigger: popup opens.
    let (press, release) = sgr_click_bytes(3, 1);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    let nightly_row = lines
        .iter()
        .position(|l| l.contains("nightly"))
        .expect("popup open") as i32;

    // OUTSIDE press over the button: dismisses, never acts below.
    let (press, release) = sgr_click_bytes(button_x + 2, button_row);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*hits.borrow(), 0, "outside press must not reach the button");
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("nightly")),
        "popup dismissed"
    );
    // The SAME click now (popup closed) fires the button — the swallow
    // was the popup's, not a dead zone.
    let (press, release) = sgr_click_bytes(button_x + 2, button_row);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*hits.borrow(), 1, "closed popup: clicks act normally");

    // Reopen and click a row: commit.
    let (press, release) = sgr_click_bytes(3, 1);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    let (press, release) = sgr_click_bytes(4, nightly_row);
    term.push_input(&press);
    term.push_input(&release);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(value.get_untracked(), 2, "click committed nightly");
    assert!(
        screen_lines(&term)[2].contains("value = 2"),
        "value line live"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn select_popup_inside_stacked_modals_receives_keys_and_returns_them() {
    // The spec's F1 acceptance case: modal -> second modal above it ->
    // the select popup layers above BOTH and owns the keys; closing it
    // hands input back to the second modal.
    let mut app = App::new(Size::new(W, H));
    let overlays = app.overlays();
    let modal2_keys: Rc<RefCell<Vec<char>>> = Default::default();
    let holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let (mk, vh) = (modal2_keys.clone(), holder.clone());
    app.mount(move |cx| {
        let value = cx.signal(0usize);
        *vh.borrow_mut() = Some(value);
        let overlays = overlays.clone();
        let mk = mk.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("root world"))
            .shortcut(KeyChord::plain(Key::Char('m')), move |_| {
                // Modal one (z 1000), which opens modal two (z 1100)
                // on 'n' — a real stack, unambiguous z order.
                let overlays2 = overlays.clone();
                let mk2 = mk.clone();
                let m1cx = cx.child();
                overlays.layer_tree(
                    1000,
                    Rect::new(2, 2, 40, 10),
                    true,
                    m1cx,
                    Element::new()
                        .style(LayoutStyle::column())
                        .on(Phase::Bubble, move |ctx, ev| {
                            if let UiEvent::Key(k) = ev {
                                if k.key == Key::Char('n') {
                                    let m2cx = m1cx.child();
                                    let mk3 = mk2.clone();
                                    overlays2.layer_tree(
                                        1100,
                                        Rect::new(4, 3, 34, 8),
                                        true,
                                        m2cx,
                                        Element::new()
                                            .style(LayoutStyle::column())
                                            .on(Phase::Bubble, move |_ctx, ev| {
                                                if let UiEvent::Key(k) = ev {
                                                    if let Key::Char(c) = k.key {
                                                        mk3.borrow_mut().push(c);
                                                    }
                                                }
                                            })
                                            .child(text("modal two"))
                                            .child(
                                                Select::new(channel_options())
                                                    .value(value)
                                                    .layout(
                                                        LayoutStyle::default()
                                                            .w(18)
                                                            .h(1)
                                                            .shrink(0.0),
                                                    )
                                                    .view(m2cx),
                                            )
                                            .build(),
                                    );
                                    ctx.stop_propagation();
                                }
                            }
                        })
                        .child(text("modal one"))
                        .build(),
                );
            })
            .build()
    })
    .expect("mount");
    let value = holder.borrow().expect("value");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"m"); // open modal one
    settle(&mut driver, &mut app, &mut term);
    assert!(screen_lines(&term).iter().any(|l| l.contains("modal one")));
    term.push_input(b"n"); // open modal two above it
    settle(&mut driver, &mut app, &mut term);
    assert!(screen_lines(&term).iter().any(|l| l.contains("modal two")));

    // Modal two's focus sits on the Select trigger; Enter opens the
    // popup ABOVE both modals.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("nightly")),
        "popup visible above the modal stack: {lines:?}"
    );

    // Keys go TO the popup: Down Down + Enter commits nightly; the
    // second modal never heard those keys.
    term.push_input(b"\x1b[B\x1b[B\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(value.get_untracked(), 2, "popup owned the keys");
    assert!(modal2_keys.borrow().is_empty(), "modal two heard nothing");
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("beta")),
        "popup closed"
    );

    // Key ownership returned to the SECOND modal.
    term.push_input(b"z");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        modal2_keys.borrow().as_slice(),
        ['z'],
        "second modal owns keys again"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
