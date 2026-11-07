//! A press that opens a modal must not leave its tree holding the pointer.
//!
//! Field report (AbstractGateway console, round 15): a segment / switch /
//! select acted on mouse DOWN and opened a dialog in the same event. The
//! root tree captured the pointer on that press (automatic press capture);
//! the RELEASE was routed to the new modal layer, so the root tree never
//! saw it and kept the capture. 0.7.1 recovers at the NEXT PRESS in that
//! tree, but every pointer event before it — the wheel, motion, a drag —
//! still went to the old widget. The driver now drops a capture left in
//! any tree that did not receive the release, at the release itself.
//!
//! Each case checks a WHEEL first (the event the next-press recovery
//! cannot fix), then a press.

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element, MouseButton, MouseKind, Phase, UiEvent, View};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Counts of what one row received: (left presses, wheel-downs).
#[derive(Clone, Default)]
struct Seen {
    presses: Rc<Cell<u32>>,
    wheels: Rc<Cell<u32>>,
}

impl Seen {
    fn record(&self, ev: &UiEvent) -> bool {
        if let UiEvent::Mouse(m) = ev {
            match m.kind {
                MouseKind::Down(MouseButton::Left) => {
                    self.presses.set(self.presses.get() + 1);
                    return true;
                }
                MouseKind::ScrollDown => self.wheels.set(self.wheels.get() + 1),
                _ => {}
            }
        }
        false
    }
}

/// A one-row target labelled `label`; `on_press` runs on a left press.
fn row(label: &str, seen: Seen, on_press: impl Fn() + 'static) -> View {
    Element::new()
        .style(LayoutStyle::line(1))
        .on(Phase::Bubble, move |_ctx, ev| {
            if seen.record(ev) {
                on_press();
            }
        })
        .child(text(label.to_string()))
        .build()
}

/// OPENER (row 0), a spacer, OTHER (row 2).
fn rows(opener: View, other: View) -> View {
    Element::new()
        .style(LayoutStyle::column())
        .child(opener)
        .child(Element::new().style(LayoutStyle::line(1)).build())
        .child(other)
        .build()
}

fn config() -> RunConfig {
    RunConfig {
        probe: false,
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.unicode_ok = true;
        })),
        ..RunConfig::default()
    }
}

fn turn(d: &mut Driver, a: &mut App, t: &mut CaptureTerm) {
    for _ in 0..2 {
        d.turn(a, t).expect("turn");
    }
}

#[test]
fn a_press_that_opens_a_modal_leaves_no_stale_capture_behind() {
    let size = Size::new(40, 10);
    let mut app = App::new(size);
    let overlays = app.overlays();
    let opener = Seen::default();
    let other = Seen::default();
    let modal: Rc<RefCell<Option<Modal>>> = Rc::new(RefCell::new(None));
    let (op, ot, ms) = (opener.clone(), other.clone(), modal.clone());
    app.mount(move |cx| {
        let ov = overlays.clone();
        let open = move || {
            let m = Modal::open(&ov, cx, Size::new(40, 10), Size::new(20, 3), |_| {
                text("dialog")
            });
            *ms.borrow_mut() = Some(m);
        };
        rows(row("OPENER", op, open), row("OTHER", ot, || {}))
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    // Press + release on OPENER (screen row 0): the press opens the
    // modal, the release lands in the modal's layer.
    term.push_input(b"\x1b[<0;2;1M\x1b[<0;2;1m");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(opener.presses.get(), 1);
    assert!(modal.borrow().is_some(), "the press opened the modal");
    // Close the modal (its own button, a key, the host).
    if let Some(m) = modal.borrow_mut().take() {
        m.close();
    }
    turn(&mut driver, &mut app, &mut term);
    // A wheel over OTHER (screen row 2) is OTHER's, not the old
    // press target's.
    term.push_input(b"\x1b[<65;2;3M");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(other.wheels.get(), 1, "the wheel reached OTHER");
    assert_eq!(opener.wheels.get(), 0, "the old capture did not take it");
    // And so is a press.
    term.push_input(b"\x1b[<0;2;3M\x1b[<0;2;3m");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(other.presses.get(), 1, "the press reached OTHER");
    assert_eq!(opener.presses.get(), 1, "the old capture did not steal it");
}

/// The overlay half: a press INSIDE a modal that opens a second, higher
/// layer (an engine Select's popup in a form) — the release goes to that
/// layer, and the modal's tree must not keep the press capture.
#[test]
fn a_press_in_a_modal_that_opens_another_layer_leaves_no_stale_capture() {
    use abstracttui::app::LayerHandle;
    use abstracttui::base::Rect;
    let size = Size::new(40, 10);
    let mut app = App::new(size);
    let overlays = app.overlays();
    let opener = Seen::default();
    let other = Seen::default();
    let popup: Rc<RefCell<Option<LayerHandle>>> = Rc::new(RefCell::new(None));
    let (op, ot, pp) = (opener.clone(), other.clone(), popup.clone());
    let ov = overlays.clone();
    app.mount(move |cx| {
        let ov2 = ov.clone();
        // The form: a modal holding OPENER (row 0) and OTHER (row 2).
        let _form = Modal::open(&ov, cx, Size::new(40, 10), Size::new(30, 6), move |mcx| {
            let open = move || {
                // The popup: a higher modal layer over the form.
                let z = ov2.top_z() + 1;
                let h =
                    ov2.layer_tree(z, Rect::new(0, 0, 40, 10), true, mcx.child(), text("popup"));
                *pp.borrow_mut() = Some(h);
            };
            rows(row("OPENER", op, open), row("OTHER", ot, || {}))
        });
        text("page")
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    // The form sits at (5,2) with a 1-cell padding: content from (6,3).
    // OPENER on screen row 3 (1-based 4), OTHER on row 5 (1-based 6).
    term.push_input(b"\x1b[<0;8;4M\x1b[<0;8;4m");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(
        opener.presses.get(),
        1,
        "the press reached OPENER in the form"
    );
    assert!(popup.borrow().is_some(), "the press opened the popup");
    // The popup closes (a pick); the next pointer events in the form
    // belong to what is under the pointer.
    if let Some(h) = popup.borrow_mut().take() {
        h.remove();
    }
    turn(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[<65;8;6M");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(other.wheels.get(), 1, "the wheel reached OTHER in the form");
    assert_eq!(
        opener.wheels.get(),
        0,
        "the form's stale capture did not take it"
    );
    term.push_input(b"\x1b[<0;8;6M\x1b[<0;8;6m");
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(
        other.presses.get(),
        1,
        "the press reached OTHER in the form"
    );
    assert_eq!(
        opener.presses.get(),
        1,
        "the form's stale capture did not steal it"
    );
}
