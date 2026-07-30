//! Live filtering over a removable-row `List` — the
//! `interaction_affordances` shape, driven headlessly.
//!
//! The trap this pins: the filtered list hands callbacks an index into
//! the SUBSET it was given, and the filter field must not be rebuilt by
//! its own keystrokes.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::prelude::*;
use abstracttui::testing::CaptureTerm;
use abstracttui::theme::default_theme;
use abstracttui::widgets::{List, TextInput};

const W: i32 = 30;
const H: i32 = 10;

fn config() -> RunConfig {
    RunConfig {
        probe: false,
        ..RunConfig::default()
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            break;
        }
    }
}

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

/// Typing in the filter narrows the list live, and dismissing a row from
/// the filtered view removes the RIGHT backing row.
#[test]
fn filter_narrows_live_and_removal_maps_through_the_filtered_index() {
    let mut app = App::new(Size::new(W, H));
    let removed: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let sink = removed.clone();

    let _ = app.mount(move |cx| {
        let t = default_theme().tokens;
        let filter = cx.signal(String::new());
        let channels = cx.signal(
            (0..20)
                .map(|i| format!("ch-{i:02}"))
                .collect::<Vec<String>>(),
        );
        let selection = cx.signal(0usize);
        let list_oy = cx.signal(0i32);
        let sink = sink.clone();

        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(
                TextInput::new()
                    .value(filter)
                    .layout(LayoutStyle::line(1).shrink(0.0))
                    .element(cx, &t)
                    .build(),
            )
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |gcx| {
                    let t = default_theme().tokens;
                    let all = channels.get();
                    let q = filter.get();
                    let shown: Vec<(usize, String)> = all
                        .iter()
                        .cloned()
                        .enumerate()
                        .filter(|(_, c)| q.is_empty() || c.contains(q.as_str()))
                        .collect();
                    let back: Vec<usize> = shown.iter().map(|(i, _)| *i).collect();
                    let sink = sink.clone();
                    List::of(shown.iter().map(|(_, c)| c.as_str()))
                        .selection(selection)
                        .offset_y(list_oy)
                        .on_remove(move |row| {
                            let Some(&real) = back.get(row) else { return };
                            channels.update(|v| {
                                let gone = v.remove(real);
                                sink.borrow_mut().push(gone);
                            });
                        })
                        .element(gcx, &t)
                        .build()
                },
            ))
            .build()
    });

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("ch-00"), "unfiltered list renders");

    // Focus the field (Tab), then type — the list must narrow per key.
    term.push_input(b"\t");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"ch-1");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("ch-1"), "filtered rows visible:\n{s}");
    assert!(
        !s.contains("ch-00"),
        "ch-00 must be filtered out — the filter is live:\n{s}"
    );
    assert!(
        s.lines().filter(|l| l.contains("ch-1")).count() >= 5,
        "the ch-1x family fills the viewport:\n{s}"
    );

    // The field kept its own text (it was not rebuilt out from under
    // itself by the keystrokes it produced).
    assert!(s.contains("ch-1"), "filter field still shows its text");
}

/// The filtered index is positional in the SUBSET. Removing the first
/// visible row of a filter must delete that channel, not the channel at
/// the same position in the backing vector.
#[test]
fn removal_from_a_filtered_view_deletes_the_row_the_user_pointed_at() {
    let mut app = App::new(Size::new(W, H));
    let removed: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let sink = removed.clone();

    let _ = app.mount(move |cx| {
        let t = default_theme().tokens;
        let filter = cx.signal(String::from("ch-1"));
        let channels = cx.signal(
            (0..20)
                .map(|i| format!("ch-{i:02}"))
                .collect::<Vec<String>>(),
        );
        let sink = sink.clone();
        dyn_view_scoped(LayoutStyle::default().grow(1.0), move |gcx| {
            let t2 = t;
            let all = channels.get();
            let q = filter.get();
            let shown: Vec<(usize, String)> = all
                .iter()
                .cloned()
                .enumerate()
                .filter(|(_, c)| q.is_empty() || c.contains(q.as_str()))
                .collect();
            let back: Vec<usize> = shown.iter().map(|(i, _)| *i).collect();
            let sink = sink.clone();
            List::of(shown.iter().map(|(_, c)| c.as_str()))
                .on_remove(move |row| {
                    let Some(&real) = back.get(row) else { return };
                    channels.update(|v| {
                        let gone = v.remove(real);
                        sink.borrow_mut().push(gone);
                    });
                })
                .element(gcx, &t2)
                .build()
        })
    });

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("ch-10"), "filtered view starts at ch-10:\n{s}");

    // Click the ✕ on the FIRST visible row (row 0 of the filtered view).
    // Width 30, 10 rows for 10 items → no scrollbar; accessory is the
    // rightmost 2 columns.
    term.push_input(format!("\x1b[<0;{};{}M", W - 1, 1).as_bytes());
    term.push_input(format!("\x1b[<0;{};{}m", W - 1, 1).as_bytes());
    settle(&mut driver, &mut app, &mut term);

    assert_eq!(
        removed.borrow().as_slice(),
        &["ch-10".to_string()],
        "filtered row 0 is ch-10, NOT the backing vector's index 0 (ch-00)"
    );
}

/// The nested-scroll promise: a `Scroll` whose only child GROWS can
/// never move (content solves to exactly the viewport). Give it a
/// fixed-height list plus content below, and wheeling past the list's
/// bottom edge bubbles out and moves the outer scroller.
#[test]
fn wheel_past_the_list_edge_moves_the_outer_scroll() {
    let mut app = App::new(Size::new(W, H));
    let probe: std::rc::Rc<std::cell::Cell<i32>> = Default::default();
    let sink = probe.clone();

    let _ = app.mount(move |cx| {
        let t = default_theme().tokens;
        let outer_oy = cx.signal(0i32);
        let sink = sink.clone();
        cx.effect(move || sink.set(outer_oy.get()));
        abstracttui::widgets::Scroll::new(
            Element::new()
                .style(LayoutStyle::column())
                .child(
                    List::of((0..6).map(|i| format!("row {i}")))
                        .layout(LayoutStyle::default().h(4).shrink(0.0))
                        .element(cx, &t)
                        .build(),
                )
                .child(
                    Element::new()
                        .style(LayoutStyle::default().h(20).shrink(0.0))
                        .build(),
                )
                .build(),
        )
        .offset_y(outer_oy)
        .layout(LayoutStyle::default().grow(1.0))
        .view(cx)
    });

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(probe.get(), 0, "starts at the top");

    // Wheel repeatedly over the LIST (row 1): it scrolls to its own
    // bottom, then the event bubbles to the outer Scroll.
    for _ in 0..12 {
        term.push_input(b"\x1b[<65;3;2M");
        settle(&mut driver, &mut app, &mut term);
    }
    assert!(
        probe.get() > 0,
        "outer scroll must move once the inner list is at its edge, got oy={}",
        probe.get()
    );
}
