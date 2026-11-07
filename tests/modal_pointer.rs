//! Modal pointer defaults, driven through the real driver:
//!
//! - the WHEEL scrolls the modal's content: over its scroll region it
//!   scrolls that region; over the title, the padding or outside the
//!   panel it scrolls the modal's main scroll region; it never scrolls
//!   the page under the modal;
//! - a PRESS OUTSIDE the panel closes exactly the top modal and is not
//!   also delivered to the widget under the pointer; a modal can opt out
//!   (`Modal::set_close_on_outside(false)`).

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{Scope, Signal};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element, MouseButton, MouseKind, Phase, UiEvent, View};
use abstracttui::widgets::{Button, Scroll};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const SCREEN: Size = Size::new(40, 12);
/// The modal panel: 30x8 centred in 40x12 sits at (5,2); its 1-cell
/// padding puts the content at (6,3): TITLE on row 3, the body scroll
/// region on rows 4..=8.
const PANEL: Size = Size::new(30, 8);

fn tall(label: &str, n: usize) -> View {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..n {
        col = col.child(text(format!("{label} {i}")));
    }
    col.build()
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

/// SGR mouse report at 0-based cell (x, y).
fn sgr(code: u8, x: i32, y: i32, press: bool) -> Vec<u8> {
    format!(
        "\x1b[<{code};{};{}{}",
        x + 1,
        y + 1,
        if press { 'M' } else { 'm' }
    )
    .into_bytes()
}

fn wheel_down(term: &mut CaptureTerm, x: i32, y: i32) {
    term.push_input(&sgr(65, x, y, true));
}

fn left_click(term: &mut CaptureTerm, x: i32, y: i32) {
    let mut bytes = sgr(0, x, y, true);
    bytes.extend(sgr(0, x, y, false));
    term.push_input(&bytes);
}

/// The modal body: TITLE over a 30-row scroll region bound to `y`.
fn body(mcx: Scope, y: Signal<i32>) -> View {
    Element::new()
        .style(LayoutStyle::column().grow(1.0))
        .child(text("TITLE"))
        .child(Scroll::new(tall("m", 30)).offset_y(y).view(mcx))
        .build()
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    page_y: Signal<i32>,
    modal_y: Signal<i32>,
    modal: Modal,
}

/// A page that scrolls (bound to `page_y`) under one open modal whose
/// body scrolls (bound to `modal_y`).
fn rig() -> Rig {
    let mut app = App::new(SCREEN);
    let overlays = app.overlays();
    type Out = Option<(Signal<i32>, Signal<i32>, Modal)>;
    let out: Rc<RefCell<Out>> = Rc::new(RefCell::new(None));
    let o = out.clone();
    app.mount(move |cx| {
        let page_y = cx.signal(0);
        let mut modal_y = None;
        let modal = Modal::open(&overlays, cx, SCREEN, PANEL, |mcx| {
            let y = mcx.signal(0);
            modal_y = Some(y);
            body(mcx, y)
        });
        *o.borrow_mut() = Some((page_y, modal_y.expect("modal built"), modal));
        Scroll::new(tall("p", 40)).offset_y(page_y).view(cx)
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SCREEN);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    let (page_y, modal_y, modal) = out.borrow_mut().take().expect("rig");
    Rig {
        app,
        term,
        driver,
        page_y,
        modal_y,
        modal,
    }
}

impl Rig {
    fn wheel(&mut self, x: i32, y: i32) {
        wheel_down(&mut self.term, x, y);
        turn(&mut self.driver, &mut self.app, &mut self.term);
    }
}

#[test]
fn the_wheel_over_the_modal_body_scrolls_it_and_never_the_page() {
    let mut r = rig();
    r.wheel(10, 6); // over the body scroll region
    assert_eq!(r.modal_y.get_untracked(), 3, "the modal body scrolled");
    assert_eq!(r.page_y.get_untracked(), 0, "the page under it did not");
}

#[test]
fn the_wheel_anywhere_else_scrolls_the_modal_body_and_never_the_page() {
    let mut r = rig();
    r.wheel(10, 3); // the title row
    assert_eq!(r.modal_y.get_untracked(), 3, "a wheel on the title");
    r.wheel(5, 2); // the panel's padding (its top-left cell)
    assert_eq!(r.modal_y.get_untracked(), 6, "a wheel on the padding");
    r.wheel(1, 0); // outside the panel, over the page
    assert_eq!(r.modal_y.get_untracked(), 9, "a wheel outside the panel");
    assert_eq!(r.page_y.get_untracked(), 0, "the page never scrolled");
    assert!(r.modal.layer().is_alive(), "a wheel outside never closes");
    let screen = r.term.screen().to_text();
    assert!(
        screen.contains("m 9"),
        "the body shows its scrolled rows:\n{screen}"
    );
}

/// A widget under the pointer that counts raw presses and clicks.
#[derive(Clone, Default)]
struct Under {
    presses: Rc<Cell<u32>>,
    clicks: Rc<Cell<u32>>,
}

fn under_view(cx: Scope, under: &Under) -> View {
    let (p, c) = (under.presses.clone(), under.clicks.clone());
    Element::new()
        .style(LayoutStyle::line(1))
        .on(Phase::Capture, move |_ctx, ev| {
            if matches!(ev, UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)))
            {
                p.set(p.get() + 1);
            }
        })
        .child(
            Button::new("UNDER")
                .on_click(move || c.set(c.get() + 1))
                .view(cx),
        )
        .build()
}

struct PressRig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    under: Under,
    modals: Vec<Modal>,
}

/// UNDER on row 0 of the page, `n` modals stacked over it (each opened
/// after the previous one, all at the same z).
fn press_rig(n: usize) -> PressRig {
    let mut app = App::new(SCREEN);
    let overlays = app.overlays();
    let under = Under::default();
    let out: Rc<RefCell<Vec<Modal>>> = Rc::new(RefCell::new(Vec::new()));
    let (u, o) = (under.clone(), out.clone());
    app.mount(move |cx| {
        for i in 0..n {
            let m = Modal::open(&overlays, cx, SCREEN, PANEL, move |_| {
                text(format!("modal {i}"))
            });
            o.borrow_mut().push(m);
        }
        Element::new()
            .style(LayoutStyle::column())
            .child(under_view(cx, &u))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SCREEN);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    let modals = out.borrow_mut().drain(..).collect();
    PressRig {
        app,
        term,
        driver,
        under,
        modals,
    }
}

impl PressRig {
    fn click_under(&mut self) {
        left_click(&mut self.term, 1, 0); // UNDER, outside every panel
        turn(&mut self.driver, &mut self.app, &mut self.term);
    }
    fn open(&self, i: usize) -> bool {
        self.modals[i].layer().is_alive()
    }
}

#[test]
fn a_press_outside_closes_the_modal_and_never_reaches_the_widget_under_it() {
    let mut r = press_rig(1);
    assert!(r.open(0));
    r.click_under();
    assert!(!r.open(0), "the outside press closed the modal");
    assert_eq!(r.under.presses.get(), 0, "the press never reached UNDER");
    assert_eq!(r.under.clicks.get(), 0, "UNDER did not activate");
    let screen = r.term.screen().to_text();
    assert!(!screen.contains("modal 0"), "the panel is gone:\n{screen}");
    // With the modal gone, the same click is UNDER's.
    r.click_under();
    assert_eq!(r.under.presses.get(), 1);
    assert_eq!(r.under.clicks.get(), 1);
}

#[test]
fn a_press_outside_closes_exactly_the_top_modal() {
    let mut r = press_rig(2);
    r.click_under();
    assert!(!r.open(1), "the top modal closed");
    assert!(r.open(0), "the modal under it stays open");
    assert_eq!(r.under.presses.get(), 0);
    let screen = r.term.screen().to_text();
    assert!(
        screen.contains("modal 0"),
        "the lower modal shows:\n{screen}"
    );
    r.click_under();
    assert!(!r.open(0), "the next outside press closes the next modal");
    assert_eq!(r.under.presses.get(), 0, "neither press reached UNDER");
    assert_eq!(r.under.clicks.get(), 0);
}

#[test]
fn a_modal_that_opts_out_stays_open_and_still_swallows_the_press() {
    let mut r = press_rig(1);
    r.modals[0].set_close_on_outside(false);
    r.click_under();
    assert!(r.open(0), "an opted-out modal stays open");
    assert_eq!(r.under.presses.get(), 0, "the press is still swallowed");
    assert_eq!(r.under.clicks.get(), 0);
    // The flag is the modal's, shared by every handle.
    r.modals[0].share().set_close_on_outside(true);
    r.click_under();
    assert!(!r.open(0), "opting back in restores the default");
}

#[test]
fn the_wheel_goes_to_the_top_modal_only() {
    let mut app = App::new(SCREEN);
    let overlays = app.overlays();
    type Out = Vec<Signal<i32>>;
    let out: Rc<RefCell<Out>> = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    app.mount(move |cx| {
        for _ in 0..2 {
            let o = o.clone();
            let _ = Modal::open(&overlays, cx, SCREEN, PANEL, move |mcx| {
                let y = mcx.signal(0);
                o.borrow_mut().push(y);
                body(mcx, y)
            });
        }
        text("page")
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SCREEN);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    let ys = out.borrow().clone();
    wheel_down(&mut term, 10, 6);
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(ys[1].get_untracked(), 3, "the top modal scrolled");
    assert_eq!(ys[0].get_untracked(), 0, "the modal under it did not");
}

/// The rule stands down over a scroll region: a small pane that cannot
/// move keeps the wheel (nothing moves), the main region is not scrolled
/// behind the user's back.
#[test]
fn a_wheel_over_a_pane_that_cannot_move_does_not_scroll_the_main_region() {
    let mut app = App::new(SCREEN);
    let overlays = app.overlays();
    type Out = Option<(Signal<i32>, Signal<i32>)>;
    let out: Rc<RefCell<Out>> = Rc::new(RefCell::new(None));
    let o = out.clone();
    app.mount(move |cx| {
        let _ = Modal::open(&overlays, cx, SCREEN, PANEL, |mcx| {
            let (small, big) = (mcx.signal(0), mcx.signal(0));
            *o.borrow_mut() = Some((small, big));
            Element::new()
                .style(LayoutStyle::column().grow(1.0))
                // Row 3: a one-row pane whose one row fits.
                .child(
                    Scroll::new(tall("s", 1))
                        .layout(LayoutStyle::line(1))
                        .offset_y(small)
                        .view(mcx),
                )
                .child(Scroll::new(tall("m", 30)).offset_y(big).view(mcx))
                .build()
        });
        text("page")
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SCREEN);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    turn(&mut driver, &mut app, &mut term);
    let (small, big) = out.borrow().expect("built");
    wheel_down(&mut term, 10, 3);
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(small.get_untracked(), 0, "the small pane has nowhere to go");
    assert_eq!(big.get_untracked(), 0, "the main region was not scrolled");
    wheel_down(&mut term, 10, 6);
    turn(&mut driver, &mut app, &mut term);
    assert_eq!(
        big.get_untracked(),
        3,
        "the main region scrolls under the pointer"
    );
}
