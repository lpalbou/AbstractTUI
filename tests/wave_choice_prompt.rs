//! CHOICE wave (app-kits/0515): the decision gate through the REAL
//! frame loop — `Driver::turn` against `CaptureTerm`, wire bytes in,
//! modeled VT screen out. Helper duplication across wave files is the
//! house style (each integration test file is its own crate).
//!
//! REVIEWER's adversarial acceptance lives in
//! tests/wave_choice_review.rs; this file is the BUILDER lane.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

const W: i32 = 52;
const H: i32 = 18;

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

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

/// Mount an app whose `o` key opens a gate built by `build`, with the
/// last outcome rendered on a status line. Returns the outcome log.
fn gate_app(
    size: Size,
    build: impl Fn(ChoicePrompt) -> ChoicePrompt + 'static,
) -> (App, Rc<RefCell<Vec<ChoiceOutcome>>>) {
    let mut app = App::new(size);
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    app.mount(move |cx| {
        let status = cx.signal(String::from("idle"));
        let build = Rc::new(build);
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('o')), {
                let sink = sink.clone();
                move |_| {
                    let sink = sink.clone();
                    build(ChoicePrompt::new("Proceed how?"))
                        .on_resolve(move |o| {
                            status.set(match &o {
                                ChoiceOutcome::Answered(a) => {
                                    let mut s = a.selected.join("+");
                                    if let Some(other) = &a.other {
                                        s.push_str(&format!("+other:{other}"));
                                    }
                                    format!("answered:{s}")
                                }
                                ChoiceOutcome::Cancelled => String::from("cancelled"),
                            });
                            sink.borrow_mut().push(o);
                        })
                        .open(cx);
                }
            })
            .child(text("== decision console =="))
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("last: {}", status.get()))
            }))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(text(" footer: steady"))
            .build()
    })
    .expect("mount");
    (app, outcomes)
}

fn basic(p: ChoicePrompt) -> ChoicePrompt {
    p.option("a", "Alpha")
        .option("b", "Beta")
        .option("c", "Gamma")
}

#[test]
fn gate_single_full_keyboard_round_trip_and_vacated_repaint() {
    let (mut app, outcomes) = gate_app(Size::new(W, H), basic);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("last: idle"));

    term.push_input(b"o"); // open the gate
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Proceed how?"), "prompt over everything: {s}");
    assert!(s.contains("● Alpha"), "candidate on row one: {s}");
    assert!(s.contains("Esc cancels"), "hint row: {s}");

    // Arrows move the candidate; movement never resolves (0250).
    term.push_input(b"\x1b[B\x1b[B\x1b[A");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("● Beta"), "{}", screen(&term));
    assert!(outcomes.borrow().is_empty(), "moves never resolve");

    // Enter commits; the modal region repaints from below.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(!s.contains("Proceed how?"), "modal vacated: {s}");
    assert!(
        s.contains("last: answered:b"),
        "outcome reached the app: {s}"
    );
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })]
    );

    // Exactly-once: stray Enter after resolution changes nothing.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(outcomes.borrow().len(), 1);

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_multiple_space_toggles_and_confirm_commits_the_set() {
    let (mut app, outcomes) = gate_app(Size::new(W, H), |p| basic(p).allow_multiple(true));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("☐ Alpha"), "checkbox glyphs: {s}");
    assert!(s.contains("Confirm"), "Confirm button: {s}");

    // Toggle Gamma then Alpha (out of order); commit canonicalizes.
    term.push_input(b"\x1b[B\x1b[B \x1b[A\x1b[A ");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("☑ Alpha") && s.contains("☑ Gamma"), "{s}");
    assert!(outcomes.borrow().is_empty(), "toggles never resolve");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["a".into(), "c".into()],
            other: None,
        })],
        "canonical option order through the wire"
    );
    assert!(screen(&term).contains("last: answered:a+c"));

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_other_reveals_editor_digits_type_and_enter_commits() {
    let (mut app, outcomes) = gate_app(Size::new(W, H), |p| basic(p).allow_other("Other…"));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains("type your answer"),
        "editor hidden until engaged"
    );

    // End lands on the Other row; the editor reveals and focuses.
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● Other…"), "Other is the candidate: {s}");
    assert!(s.contains("type your answer"), "editor revealed: {s}");

    // A hollow Other refuses commit, visibly; digits then TYPE (never
    // jump-select) and Enter commits the trimmed text.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert!(outcomes.borrow().is_empty(), "hollow Other refused");
    assert!(
        screen(&term).contains("needs text"),
        "refusal is visible: {}",
        screen(&term)
    );
    term.push_input(b"custom 42\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: Some("custom 42".into()),
        })],
        "digits typed into the editor"
    );
    assert!(screen(&term).contains("last: answered:+other:custom 42"));

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_escape_cancels_and_the_gate_reopens_clean() {
    let (mut app, outcomes) = gate_app(Size::new(W, H), basic);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Proceed how?"));
    term.push_input(b"\x1b[27u"); // Escape: an explicit outcome
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(screen(&term).contains("last: cancelled"));
    assert!(!screen(&term).contains("Proceed how?"), "modal gone");

    // Re-open: a fresh gate with fresh state.
    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("● Alpha"), "fresh candidate");
    term.push_input(b"3\r"); // number jump + commit
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [
            ChoiceOutcome::Cancelled,
            ChoiceOutcome::Answered(ChoiceAnswer {
                selected: vec!["c".into()],
                other: None,
            })
        ]
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_outside_click_never_dismisses_or_acts_below() {
    // A live button sits under the modal; the outside press must
    // neither dismiss the gate nor fire the button below.
    let mut app = App::new(Size::new(W, H));
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let clicks: Rc<RefCell<u32>> = Default::default();
    let (sink, clicked) = (outcomes.clone(), clicks.clone());
    app.mount(move |cx| {
        let sink = sink.clone();
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('o')), move |_| {
                let sink = sink.clone();
                ChoicePrompt::new("Proceed how?")
                    .option("a", "Alpha")
                    .option("b", "Beta")
                    .on_resolve(move |o| sink.borrow_mut().push(o))
                    .open(cx);
            })
            .child(
                Button::new("below")
                    .on_click({
                        let clicked = clicked.clone();
                        move || *clicked.borrow_mut() += 1
                    })
                    .view(cx),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Sanity: the button IS clickable before the gate opens.
    term.push_input(b"\x1b[<0;3;1M\x1b[<0;3;1m");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*clicks.borrow(), 1, "button live before the gate");

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Proceed how?"));

    // Same press with the gate open: swallowed, gate stays, no click.
    term.push_input(b"\x1b[<0;3;1M\x1b[<0;3;1m");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Proceed how?"), "gate not dismissed");
    assert_eq!(*clicks.borrow(), 1, "press never acted below");
    assert!(outcomes.borrow().is_empty(), "no resolution");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_resolve_may_dispose_the_opener_scope_under_the_driver() {
    // The 0297 law through the real loop: on_resolve disposes the
    // scope the prompt was opened from, synchronously.
    let mut app = App::new(Size::new(W, H));
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    app.mount(move |cx| {
        let sink = sink.clone();
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('o')), move |_| {
                let opener = cx.child();
                let sink = sink.clone();
                ChoicePrompt::new("Tear me down?")
                    .option("y", "Yes")
                    .on_resolve(move |o| {
                        sink.borrow_mut().push(o);
                        opener.dispose();
                    })
                    .open(opener);
            })
            .child(text("host"))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Tear me down?"));
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(outcomes.borrow().len(), 1, "resolved despite disposal");
    assert!(!screen(&term).contains("Tear me down?"), "modal gone");
    assert!(screen(&term).contains("host"), "app intact under it");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_twenty_options_window_around_the_highlight() {
    let (mut app, outcomes) = gate_app(Size::new(W, H), |mut p| {
        for i in 1..=20 {
            p = p.option(format!("o{i}"), format!("Option {i:02}"));
        }
        p.max_visible(6)
    });
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Option 01"), "window head: {s}");
    assert!(!s.contains("Option 09"), "tail windowed out: {s}");
    assert!(s.contains("1/20"), "position note: {s}");

    for _ in 0..19 {
        term.push_input(b"\x1b[B");
    }
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● Option 20"), "highlight reached the tail: {s}");
    assert!(!s.contains("Option 01"), "head windowed out: {s}");
    assert!(s.contains("20/20"), "position follows: {s}");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["o20".into()],
            other: None,
        })]
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_letters_danger_and_must_choose_through_the_wire() {
    // Wave-5 cycle 2: per-option shortcut letters (F2), the danger
    // tint option present (F7 — color pinned in unit tests), and
    // must-choose mode (F3): Esc refuses visibly, letters commit.
    let (mut app, outcomes) = gate_app(Size::new(60, H), |p| {
        p.option_key("approve", "Approve", 'a')
            .option_key("all", "Approve all", 'A')
            .option_key("deny", "Deny", 'd')
            .danger("deny")
            .dismissable(false)
    });
    let mut term = CaptureTerm::new(Size::new(60, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(
        s.contains("(a)") && s.contains("(A)"),
        "letters render: {s}"
    );
    assert!(s.contains("a/A/d pick"), "hint names the letters: {s}");
    assert!(!s.contains("Cancel"), "must-choose: no Cancel button: {s}");

    // Esc refuses VISIBLY; the gate stays; nothing resolves.
    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("an answer is required"), "visible refusal: {s}");
    assert!(outcomes.borrow().is_empty(), "no resolution on refused Esc");

    // The uppercase letter commits its own option, exactly once.
    term.push_input(b"A");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["all".into()],
            other: None,
        })],
        "letter commit through the wire"
    );
    assert!(!screen(&term).contains("Proceed how?"), "gate closed");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn gate_sequence_chains_questions_through_the_wire() {
    let mut app = App::new(Size::new(W, H));
    let outcomes: Rc<RefCell<Vec<ChoiceSequenceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    app.mount(move |cx| {
        let sink = sink.clone();
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('o')), move |_| {
                let sink = sink.clone();
                let mut q1 = ChoiceQuestion::new("Step one?");
                q1.options.push(ChoiceOption::new("a", "Alpha"));
                q1.options.push(ChoiceOption::new("b", "Beta"));
                let mut q2 = ChoiceQuestion::new("Step two?");
                q2.options.push(ChoiceOption::new("x", "Xen"));
                q2.allow_multiple = true;
                ChoiceSequence::new(vec![q1, q2])
                    .on_resolve(move |o| sink.borrow_mut().push(o))
                    .open(cx);
            })
            .child(text("wizard host"))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"o");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Step one?"));
    term.push_input(b"\x1b[B\r"); // pick Beta
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("Step two?"),
        "next gate opened from the previous resolution: {}",
        screen(&term)
    );
    term.push_input(b" \r"); // toggle Xen, confirm
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceSequenceOutcome::Completed(vec![
            ChoiceAnswer {
                selected: vec!["b".into()],
                other: None,
            },
            ChoiceAnswer {
                selected: vec!["x".into()],
                other: None,
            },
        ])]
    );
    assert!(!screen(&term).contains("Step"), "all gates closed");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
