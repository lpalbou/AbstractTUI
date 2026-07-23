//! Double-click acceptance (app-kits 0535), driven through the REAL
//! pipeline: `app::Driver` frames against a `CaptureTerm`, raw SGR
//! mouse bytes in, the driver's INJECTED clock (`Driver::set_clock`)
//! owning time — the same seam animations and timers test through, now
//! also feeding click-chain synthesis (the driver publishes its turn
//! clock as the ambient event time).
//!
//! Pins:
//! - Table: single click SELECTS (and its highlight is real frame
//!   content); the double-click's second press ACTIVATES exactly once;
//!   a slow second click (past the 400ms window) never activates —
//!   selection on click 1 is never suppressed and both presses deliver
//!   normally (the no-double-fire/no-suppression contract);
//! - Enter activates the selected Table row (the keyboard twin);
//! - List: double-click fires `on_activate` by SUBSUMPTION (click 2
//!   lands on the row click 1 selected) — exactly once;
//! - zero-idle: once the clicks settle, turns are idle and emit zero
//!   bytes (synthesis adds no frame pressure).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::{current_theme, App, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::widgets::{ColWidth, Column};

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
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// One SGR left click at 1-BASED terminal coordinates.
fn sgr_click(term: &mut CaptureTerm, col: i32, row: i32) {
    term.push_input(format!("\x1b[<0;{col};{row}M").as_bytes());
    term.push_input(format!("\x1b[<0;{col};{row}m").as_bytes());
}

#[test]
fn table_double_click_activates_once_slow_second_click_never() {
    let size = Size::new(24, 6);
    let mut app = App::new(size);
    let activated: Rc<RefCell<Vec<usize>>> = Default::default();
    let a2 = activated.clone();
    let sel_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let s2 = sel_holder.clone();
    app.mount(move |cx| {
        let sel = cx.signal(0usize);
        *s2.borrow_mut() = Some(sel);
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(
                Table::new(vec![
                    Column::new("name", ColWidth::Flex(1.0)),
                    Column::new("size", ColWidth::Cells(6)),
                ])
                .rows(
                    (0..4)
                        .map(|i| vec![format!("route-{i}"), format!("{i} kB")])
                        .collect(),
                )
                .selection(sel)
                .on_activate(move |i| a2.borrow_mut().push(i))
                .view(cx),
            )
            .build()
    })
    .expect("mount");
    let sel = sel_holder.borrow().expect("selection probe");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    // Injected clock: the test owns time (the seam animations/timers
    // already use — click chains ride the SAME clock).
    let now = Rc::new(Cell::new(Instant::now()));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    settle(&mut driver, &mut app, &mut term);

    // Click 1 on body row 2 (screen row 3, SGR row 4): SELECTS, never
    // activates — and the moved highlight is REAL frame content.
    sgr_click(&mut term, 2, 4);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(sel.get_untracked(), 2, "click 1 moved the selection");
    assert!(
        activated.borrow().is_empty(),
        "a single click must not activate: {:?}",
        activated.borrow()
    );
    let sel_bg = current_theme().tokens.get(TokenId::SelectionBg);
    assert_eq!(
        term.screen().cell(0, 3).unwrap().paint.bg,
        Some(sel_bg),
        "selected row wears the selection ground after click 1"
    );

    // Click 2, same cell, 120ms later: the double-click's second press
    // — on_activate fires EXACTLY once, selection stays.
    now.set(now.get() + Duration::from_millis(120));
    sgr_click(&mut term, 2, 4);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*activated.borrow(), vec![2], "double-click activates once");
    assert_eq!(sel.get_untracked(), 2);

    // A SLOW second click on the selected row (2s later, far past the
    // 400ms window): selects only — the Table convention (a table is a
    // browsing surface; only a timed double-click or Enter opens).
    now.set(now.get() + Duration::from_secs(2));
    sgr_click(&mut term, 2, 4);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        *activated.borrow(),
        vec![2],
        "slow re-click on the selected row must not activate"
    );

    // The keyboard twin: Enter activates the selected row (the click
    // already focused the table).
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*activated.borrow(), vec![2, 2], "Enter activates");

    // Zero-idle: with everything settled, a turn is idle and the wire
    // stays silent — synthesis adds no frame pressure.
    let _ = term.take_bytes();
    let turn = driver.turn(&mut app, &mut term).expect("turn");
    assert!(turn.idle, "settled app must be idle");
    assert!(
        term.take_bytes().is_empty(),
        "idle turns emit zero bytes with click synthesis live"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

#[test]
fn list_double_click_fires_its_activation_by_subsumption() {
    // List needs no chain gate: click 1 selects, click 2 is a click on
    // the ALREADY-selected row (the 0250 picker gesture) — the pin here
    // is that with chain synthesis live under the driver, the second
    // press still fires on_activate EXACTLY once.
    let size = Size::new(20, 6);
    let mut app = App::new(size);
    let activated: Rc<RefCell<Vec<usize>>> = Default::default();
    let a2 = activated.clone();
    app.mount(move |cx| {
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(
                List::of(["alpha", "beta", "gamma", "delta"])
                    .on_activate(move |i| a2.borrow_mut().push(i))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");

    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let now = Rc::new(Cell::new(Instant::now()));
    let clock = now.clone();
    driver.set_clock(move || clock.get());
    settle(&mut driver, &mut app, &mut term);

    // Double-click row 2 ("gamma", screen row 2, SGR row 3).
    sgr_click(&mut term, 2, 3);
    settle(&mut driver, &mut app, &mut term);
    assert!(activated.borrow().is_empty(), "click 1 selects only");
    now.set(now.get() + Duration::from_millis(120));
    sgr_click(&mut term, 2, 3);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        *activated.borrow(),
        vec![2],
        "double-click on a List row fires on_activate exactly once"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}
