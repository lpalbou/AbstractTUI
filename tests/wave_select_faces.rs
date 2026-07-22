//! SELECT wave, face half: Combobox + MultiSelect through the REAL
//! frame loop (`Driver::turn` against `CaptureTerm`) — split sibling
//! of wave_select.rs for the file budget. Same wire-in/modeled-VT-out
//! posture; helper duplication across wave files is the house style
//! (each integration test file is its own crate).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

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

#[test]
fn combobox_editor_sits_on_the_trigger_row_types_and_commits() {
    let mut app = App::new(Size::new(W, H));
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let (c2, v2) = (changes.clone(), holder.clone());
    app.mount(move |cx| {
        let value = cx.signal(usize::MAX);
        *v2.borrow_mut() = Some(value);
        let c3 = c2.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== model picker =="))
            .child(
                Element::new()
                    .style(LayoutStyle::row().h(1))
                    .child(
                        Combobox::new(vec![
                            SelectOption::new("gpt-tiny"),
                            SelectOption::new("gpt-large"),
                            SelectOption::new("qwen-mini"),
                            SelectOption::new("qwen-max"),
                        ])
                        .value(value)
                        .placeholder("model…")
                        .layout(LayoutStyle::default().w(22).h(1).shrink(0.0))
                        .on_change(move |i| c3.borrow_mut().push(i))
                        .view(cx),
                    )
                    .build(),
            )
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("picked = {}", value.get()))
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
    let value = holder.borrow().expect("value");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen_lines(&term)[1].contains("model…"), "placeholder");

    // Tab focuses, Enter opens: the editor mounts ON the trigger row
    // (anchor row included) — the frame strokes stay at row 1.
    term.push_input(b"\t\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[1].contains('▐'),
        "editor frame on the trigger row: {lines:?}"
    );
    assert!(lines[2].contains("gpt-tiny"), "options below: {lines:?}");
    assert!(
        lines.iter().any(|l| l.contains("4 of 4")),
        "status line: {lines:?}"
    );

    // Typing lands in the popup editor (zero visual jump) + refilters.
    term.push_input(b"qwen");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[1].contains("qwen"),
        "query renders on the trigger row"
    );
    assert!(
        !lines.iter().any(|l| l.contains("gpt-tiny")),
        "filtered out"
    );
    assert!(lines.iter().any(|l| l.contains("2 of 4")));

    // Enter commits the first match (qwen-mini).
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(value.get_untracked(), 2);
    assert_eq!(changes.borrow().as_slice(), [2]);
    let lines = screen_lines(&term);
    assert!(lines[1].contains("qwen-mini"), "trigger shows the commit");
    assert!(lines[2].contains("picked = 2"), "vacated region repainted");

    // Non-match commits nothing: reopen, type junk, Enter, Escape.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"zzz\r");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen_lines(&term).iter().any(|l| l.contains("no matches")),
        "honest empty state"
    );
    assert_eq!(value.get_untracked(), 2, "no-match Enter commits nothing");
    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(changes.borrow().as_slice(), [2], "one commit total");

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

#[test]
fn multiselect_space_toggles_and_enter_commits_through_the_wire() {
    let mut app = App::new(Size::new(W, H));
    let changes: Rc<RefCell<Vec<Vec<String>>>> = Default::default();
    let holder: Rc<RefCell<Option<Signal<Vec<String>>>>> = Default::default();
    let (c2, v2) = (changes.clone(), holder.clone());
    app.mount(move |cx| {
        let values = cx.signal(Vec::<String>::new());
        *v2.borrow_mut() = Some(values);
        let c3 = c2.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== permissions =="))
            .child(
                Element::new()
                    .style(LayoutStyle::row().h(1))
                    .child(
                        MultiSelect::new(vec![
                            SelectOption::new("read"),
                            SelectOption::new("write"),
                            SelectOption::new("exec"),
                        ])
                        .values(values)
                        .placeholder("permissions…")
                        .layout(LayoutStyle::default().w(24).h(1).shrink(0.0))
                        .on_change(move |set| c3.borrow_mut().push(set))
                        .view(cx),
                    )
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(text(" status: ready"))
            .build()
    })
    .expect("mount");
    let values = holder.borrow().expect("values");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\t\r"); // focus + open
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen_lines(&term)[2].contains("[ ] read"),
        "{:?}",
        screen_lines(&term)
    );
    term.push_input(b" "); // toggle read
    settle(&mut driver, &mut app, &mut term);
    assert!(screen_lines(&term)[2].contains("[x] read"));
    assert!(values.get_untracked().is_empty(), "toggle is not a commit");
    term.push_input(b"\x1b[B\x1b[B "); // down down, toggle exec
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\r"); // commit
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        values.get_untracked(),
        vec!["read".to_string(), "exec".to_string()]
    );
    assert_eq!(changes.borrow().len(), 1, "one on_change per commit");
    assert!(
        screen_lines(&term)[1].contains("read, exec"),
        "collapsed row joins the labels: {:?}",
        screen_lines(&term)[1]
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
