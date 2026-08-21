//! field-agora 0895, reduced: a bound `Scroll::offset_y` is DESTROYED
//! when the Scroll remounts over a `Feed`.
//!
//! The report blamed drawer "pages", so `wave_drawers.rs` chased the
//! Drawer and the PageHost first. Neither is involved. What is:
//!
//! 1. `Feed`'s content solves in TWO steps after a mount. The first
//!    frame publishes a partial extent — `(w, 1)`, the cross axis
//!    already correct and the scroll axis still a one-row placeholder —
//!    and the real `(w, 30)` lands on the next frame. A plain column of
//!    the same 30 rows publishes `(w, 30)` in ONE frame and is immune
//!    (`plain_column_offset_survives_a_remount`, which PASSES today and
//!    is the control that makes this file honest).
//! 2. `Scroll`'s offset repair (`scroll.rs`, "Offset repair
//!    (first-app/0281)") treats any extent that is not the `(0, 0)`
//!    sentinel as a real measurement. Against `(w, 1)` it computes
//!    `max_off = (1 - view_h).max(0) == 0` and writes the app's bound
//!    signal to 0. The true extent arrives one frame later, but the
//!    offset is already gone.
//!
//! It only bites on REMOUNT because a first mount usually starts at 0
//! anyway — there is nothing to destroy. That matches the field report
//! exactly: `agora-tui`'s reader panes rewind to the top every time
//! their page is rebuilt, which is what drove the app-side self-
//! windowing workaround (and cost it the native scrollbar with it).
//!
//! Note also that `Scroll::extent_signal` documents a warm start for
//! "a remounting caller" — that promise does NOT hold here either: the
//! partial `(w, 1)` solve overwrites the warm value before the repair
//! effect reads it (`extent_signal_warm_start_does_not_protect_offset`).
//!
//! OWNER: tui (abstracttui). Source: `plan/agora-ui.md` §4 item 1.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{Scope, Signal};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view_scoped, text, Element, View};
use abstracttui::widgets::{Feed, FeedItem, FeedState, Scroll};

const ROWS: i32 = 30;
const PARKED_AT: i32 = 9;

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.unicode_ok = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

/// Signals the test drives, captured out of the mount closure.
type Wires = Rc<RefCell<Option<(Signal<i32>, Signal<bool>)>>>;

/// A page builder. It gets the rebuilt GENERATION scope, plus two
/// signals created on the APP scope that outlive every rebuild: the
/// bound offset, and a spare extent signal for the warm-start case.
type PageFn = dyn Fn(Scope, Signal<i32>, Signal<(i32, i32)>) -> View;

/// Mount `build` behind a `show` toggle so flipping it off and on
/// disposes and rebuilds the whole subtree — the smallest possible
/// stand-in for a page switch, with no PageHost and no Drawer.
fn remountable(size: Size, build: Box<PageFn>) -> (App, CaptureTerm, Wires) {
    let mut app = App::new(size);
    let wires: Wires = Rc::new(RefCell::new(None));
    let w = wires.clone();
    app.mount(move |cx| {
        let oy = cx.signal(0i32);
        let show = cx.signal(true);
        // Created HERE, on the app scope — the rebuilt subtree below
        // never owns it, which is exactly the warm-start precondition.
        let ext = cx.signal((0i32, 0i32));
        *w.borrow_mut() = Some((oy, show));
        dyn_view_scoped(LayoutStyle::column(), move |gcx| {
            if show.get() {
                build(gcx, oy, ext)
            } else {
                text("HIDDEN")
            }
        })
    })
    .expect("mount");
    (app, CaptureTerm::new(size), wires)
}

fn feed(gcx: Scope) -> View {
    let feed = FeedState::new(gcx);
    for i in 0..ROWS {
        feed.push(format!("k{i}"), FeedItem::text(format!("item-{i:02}")));
    }
    Feed::new(&feed).gap(0).view(gcx)
}

fn feed_page(gcx: Scope, oy: Signal<i32>, _ext: Signal<(i32, i32)>) -> View {
    Scroll::new(feed(gcx)).offset_y(oy).view(gcx)
}

fn warm_feed_page(gcx: Scope, oy: Signal<i32>, ext: Signal<(i32, i32)>) -> View {
    Scroll::new(feed(gcx))
        .offset_y(oy)
        .extent_signal(ext)
        .view(gcx)
}

fn column_page(gcx: Scope, oy: Signal<i32>, _ext: Signal<(i32, i32)>) -> View {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..ROWS {
        col = col.child(text(format!("item-{i:02}")));
    }
    Scroll::new(col.build()).offset_y(oy).view(gcx)
}

/// Park the offset at `PARKED_AT`, unmount, remount, and report what
/// the bound signal holds plus the screen it produced.
fn park_and_remount(
    build: impl Fn(Scope, Signal<i32>, Signal<(i32, i32)>) -> View + 'static,
) -> (i32, String) {
    let size = Size::new(44, 14);
    let (mut app, mut term, wires) = remountable(size, Box::new(build));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let settle = |driver: &mut Driver, app: &mut App, term: &mut CaptureTerm| {
        for _ in 0..16 {
            if driver.turn(app, term).expect("turn").idle {
                return;
            }
        }
        panic!("loop failed to settle within 16 turns");
    };
    settle(&mut driver, &mut app, &mut term);
    let (oy, show) = wires.borrow().expect("wires");

    oy.set(PARKED_AT);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        oy.get_untracked(),
        PARKED_AT,
        "precondition: a mounted Scroll honours a bound offset write"
    );

    show.set(false);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        oy.get_untracked(),
        PARKED_AT,
        "disposal is clean — the unmount itself never writes the offset"
    );

    show.set(true);
    settle(&mut driver, &mut app, &mut term);
    (oy.get_untracked(), term.screen().to_text())
}

/// THE CONTROL. Same 30 rows, same bound offset, same remount — but a
/// plain column, which solves its extent in one frame. This passes
/// today, and it is what proves the failing test below is about the
/// two-step Feed measurement and not about remounting in general.
///
/// If this ever goes red, the offset repair broke for everyone and the
/// ignored test below stops being the interesting one.
#[test]
fn plain_column_offset_survives_a_remount() {
    let (offset, screen) = park_and_remount(column_page);
    assert_eq!(
        offset, PARKED_AT,
        "a one-frame extent leaves the bound offset alone"
    );
    assert!(
        screen.contains("item-09") && !screen.contains("item-00"),
        "the remounted column comes back where it was parked:\n{screen}"
    );
}

/// THE DEFECT (field-agora 0895), now fixed: `Scroll`'s offset repair
/// treats the first measurement after the unmeasured sentinel as
/// provisional and never clamps against it, so `Feed`'s one-row
/// placeholder can no longer destroy a bound offset on remount.
#[test]
fn feed_offset_survives_a_remount() {
    let (offset, screen) = park_and_remount(feed_page);
    assert_eq!(
        offset, PARKED_AT,
        "the remount must not rewrite the app's own offset signal"
    );
    assert!(
        screen.contains("item-09") && !screen.contains("item-00"),
        "the remounted feed comes back where it was parked:\n{screen}"
    );
}

/// A SEPARATE defect, uncovered by 0895 but not fixed with it — the
/// ignore reason is narrower now than it was, on purpose.
///
/// `Scroll::extent_signal` promises that a remounting caller warm-starts
/// from its last measurement. It does not. The provisional `(w, 1)`
/// solve overwrites the warm `(w, 30)` before anything can use it, so
/// the warm value buys nothing.
///
/// This is why the 0895 fix does not cover it: with a warm extent there
/// is no `(0,0)` sentinel, so the warm value ITSELF is the first
/// measurement and spends the one-shot trust exemption. The provisional
/// solve then arrives as a trusted second observation and clamps. The
/// bug is therefore in the publishing path — `size_probe` should not
/// clobber a warm extent with a provisional one — not in the repair,
/// and it wants fixing where the extent is written.
///
/// The offset is no longer LOST here in the way 0895 lost it: the
/// render-side clamp keeps the content visible throughout. What is
/// still wrong is that the app's signal gets rewritten to 0.
#[test]
#[ignore = "separate from 0895: size_probe overwrites extent_signal's warm \
            value with the provisional (w,1) solve, so the warm start the \
            rustdoc promises never takes effect — fix belongs in the \
            extent publishing path, not in the offset repair"]
fn extent_signal_warm_start_does_not_protect_offset() {
    let (offset, _screen) = park_and_remount(warm_feed_page);
    assert_eq!(
        offset, PARKED_AT,
        "a warm-started extent must not clamp the offset to 0"
    );
}
