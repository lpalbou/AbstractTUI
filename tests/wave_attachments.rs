//! ATTACHMENTS wave (backlog first-app/0273): the three file-attachment
//! surfaces through the REAL frame loop — `Driver::turn` against
//! `CaptureTerm`, wire bytes in (bracketed paste, keys), modeled VT
//! screen out.
//!
//! Pins, by spec:
//! - `TextArea::on_paste` + `input::paste::classify` wired the way a
//!   client composer wires them: a bracketed-paste FILE DROP becomes an
//!   attachment chip and inserts NOTHING; prose pastes insert exactly
//!   as before; the classifier is reachable from a foreign crate;
//! - `FilePicker` inside a `Modal` (the intended host): autofocus lands
//!   on the filter through the modal mount, type-to-filter + Enter pick
//!   through wire bytes, `on_pick` may close the modal synchronously,
//!   Esc stays the HOST's (the picker never consumes it);
//! - zero idle: composer + open picker parked cost zero bytes;
//! - every emitted byte is modeled (`unknown_seq_count == 0`).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::input::paste::classify;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;
use abstracttui::widgets::{FileEntry, FilePicker, FileSource};

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

/// The seam from a FOREIGN crate: a hermetic source over a fixed tree
/// (this compiling is the ADR-0003 §4-style proof the trait + entry
/// shapes are usable downstream).
struct FakeSource;

impl FileSource for FakeSource {
    fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        match path {
            "/fake" => Ok(vec![
                FileEntry::dir("nested"),
                FileEntry::file("alpha.txt", Some(64)),
                FileEntry::file("beta.png", Some(2048)),
            ]),
            p if p.ends_with("nested") => Ok(vec![FileEntry::file("deep.rs", Some(9))]),
            other => Err(format!("no such dir: {other}")),
        }
    }
}

// =====================================================================
// Surface 1 + 2: on_paste intercept + the drop classifier, through
// bracketed-paste wire bytes.
// =====================================================================

#[test]
fn bracketed_paste_drop_becomes_a_chip_and_prose_still_inserts() {
    const W: i32 = 44;
    const H: i32 = 10;
    let mut app = App::new(Size::new(W, H));
    let chips: Rc<RefCell<Vec<String>>> = Default::default();
    let chips2 = chips.clone();
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let chip_count = cx.signal(0usize);
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        let chips_in = chips2.clone();
        let composer = TextArea::new()
            .state(&state)
            .placeholder("message")
            .rows(1, 3)
            // The client recipe (commons#5408): classify consumes
            // file-drop pastes into chips, everything else inserts.
            .on_paste(move |pasted| match classify(pasted) {
                Some(paths) => {
                    chips_in.borrow_mut().extend(paths);
                    chip_count.update(|n| *n = chips_in.borrow().len());
                    PasteAction::Consume
                }
                None => PasteAction::Insert,
            })
            .element(cx, &t)
            .autofocus()
            .build();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== attach composer =="))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                text(format!("chips: {}", chip_count.get()))
            }))
            .child(composer)
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let state = holder.borrow().clone().expect("state");

    // A real drop: Terminal.app spelling (escaped space), bracketed.
    term.push_input(b"\x1b[200~/tmp/pic\\ 1.png \x1b[201~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(chips.borrow().as_slice(), ["/tmp/pic 1.png"]);
    assert_eq!(state.text(), "", "drop inserted nothing");
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("chips: 1")),
        "chip row rendered: {lines:?}"
    );

    // A multi-file drop (iTerm2 quoted spelling) extends the chips.
    term.push_input(b"\x1b[200~'/a/one file.txt' '/b/two.txt'\x1b[201~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        chips.borrow().as_slice(),
        ["/tmp/pic 1.png", "/a/one file.txt", "/b/two.txt"]
    );
    assert_eq!(state.text(), "", "drops never touch the buffer");

    // Prose pastes INSERT exactly as before — no chip, text lands.
    term.push_input(b"\x1b[200~see /usr/bin for details\x1b[201~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(chips.borrow().len(), 3, "prose is never a chip");
    assert_eq!(state.text(), "see /usr/bin for details");
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("see /usr/bin")),
        "prose visible in the composer: {lines:?}"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

// =====================================================================
// Surface 3: FilePicker inside a Modal, keys through the wire.
// =====================================================================

/// (app, mount scope slot, recorded multi-select batches).
type PickerApp = (
    App,
    Rc<RefCell<Option<Scope>>>,
    Rc<RefCell<Vec<Vec<String>>>>,
);

fn picker_app(size: Size) -> PickerApp {
    let mut app = App::new(size);
    let scope_holder: Rc<RefCell<Option<Scope>>> = Default::default();
    let s2 = scope_holder.clone();
    app.mount(move |cx| {
        *s2.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== transcript underneath =="))
            .child(text("pane row alpha"))
            .build()
    })
    .expect("mount");
    let picked: Rc<RefCell<Vec<Vec<String>>>> = Default::default();
    (app, scope_holder, picked)
}

#[test]
fn file_picker_in_a_modal_picks_through_wire_keys_and_closes() {
    let size = Size::new(44, 14);
    let (mut app, scope_holder, picked) = picker_app(size);
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    let cx = scope_holder.borrow().expect("scope");
    let modal_slot: Rc<RefCell<Option<Modal>>> = Default::default();
    let slot_in = modal_slot.clone();
    let picked_in = picked.clone();
    let modal = Modal::open(&overlays, cx, size, Size::new(34, 10), move |mcx| {
        let t = current_theme().tokens;
        FilePicker::new(FakeSource)
            .start_in("/fake")
            .on_pick(move |paths| {
                picked_in.borrow_mut().push(paths);
                // Close-the-modal from inside dispatch: the picker's
                // disposal-safety contract.
                if let Some(m) = slot_in.borrow_mut().take() {
                    m.close();
                }
            })
            .element(mcx, &t)
            .build()
    });
    *modal_slot.borrow_mut() = Some(modal);
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("/fake")),
        "breadcrumb on screen: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("▸ nested")),
        "dir row on screen: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("2.0K")),
        "size column on screen: {lines:?}"
    );

    // Autofocus landed on the filter through the modal mount:
    // type-to-filter narrows live through the wire.
    term.push_input(b"beta");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        !lines.iter().any(|l| l.contains("alpha.txt")),
        "filter narrowed: {lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("beta.png")));

    // Enter picks the file, on_pick closes the modal synchronously.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        picked.borrow().as_slice(),
        [vec![format!(
            "{}",
            std::path::Path::new("/fake").join("beta.png").display()
        )]]
    );
    let lines = screen_lines(&term);
    assert!(
        !lines.iter().any(|l| l.contains("beta.png")),
        "picker gone after close: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("pane row alpha")),
        "vacated region repainted from below: {lines:?}"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn picker_keyboard_navigation_through_the_wire_and_esc_stays_the_hosts() {
    let size = Size::new(44, 14);
    let (mut app, scope_holder, picked) = picker_app(size);
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    let cx = scope_holder.borrow().expect("scope");
    let modal_slot: Rc<RefCell<Option<Modal>>> = Default::default();
    let slot_esc = modal_slot.clone();
    let picked_in = picked.clone();
    let modal = Modal::open(&overlays, cx, size, Size::new(34, 10), move |mcx| {
        let t = current_theme().tokens;
        Element::new()
            .style(LayoutStyle::fill())
            // Esc is the HOST's dismissal (the picker never consumes
            // it) — the closer pattern from the shell wave.
            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                if let Some(m) = slot_esc.borrow_mut().take() {
                    m.close();
                }
            })
            .child(
                FilePicker::new(FakeSource)
                    .start_in("/fake")
                    .multi_select(true)
                    .on_pick(move |paths| picked_in.borrow_mut().push(paths))
                    .element(mcx, &t)
                    .build(),
            )
            .build()
    });
    *modal_slot.borrow_mut() = Some(modal);
    settle(&mut driver, &mut app, &mut term);

    // Enter on the dir row descends; Backspace (empty filter) returns.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("deep.rs")),
        "descended into nested: {lines:?}"
    );
    term.push_input(b"\x7f"); // Backspace (legacy wire)
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen_lines(&term).iter().any(|l| l.contains("alpha.txt")),
        "back at /fake"
    );

    // Down to a file, Space marks it (badge renders), Down + Space
    // marks the second; Enter commits BOTH in mark order.
    term.push_input(b"\x1b[B"); // alpha.txt
    term.push_input(b" ");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen_lines(&term).iter().any(|l| l.contains("1 marked")),
        "badge after first mark"
    );
    term.push_input(b"\x1b[B"); // beta.png
    term.push_input(b" ");
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let expect = |name: &str| {
        std::path::Path::new("/fake")
            .join(name)
            .display()
            .to_string()
    };
    assert_eq!(
        picked.borrow().as_slice(),
        [vec![expect("alpha.txt"), expect("beta.png")]]
    );

    // Esc bubbles OUT of the picker to the host's shortcut: closed.
    term.push_input(b"\x1b[27u"); // kitty Escape (unambiguous wire)
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen_lines(&term).iter().any(|l| l.contains("alpha.txt")),
        "host closed the modal on Esc"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

// =====================================================================
// Zero idle: the whole attachments UI parked costs zero bytes.
// =====================================================================

#[test]
fn parked_attachments_ui_costs_zero_idle_bytes() {
    let size = Size::new(44, 14);
    let (mut app, scope_holder, _picked) = picker_app(size);
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Open the picker modal and let everything settle…
    let cx = scope_holder.borrow().expect("scope");
    let _modal = Modal::open(&overlays, cx, size, Size::new(34, 10), move |mcx| {
        let t = current_theme().tokens;
        FilePicker::new(FakeSource)
            .start_in("/fake")
            .multi_select(true)
            .element(mcx, &t)
            .build()
    });
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // …then a parked picker (signals bound, no animation, no timers)
    // is BYTE-SILENT: the zero-idle law.
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
