//! Wave-8 adversarial cross-review (TABS attacking DRAWER's 0585):
//! the six self-named surfaces — same-instant close ordering, on_close
//! re-entrancy vs a mid-claim replacement, per-thread registry residue,
//! at-open token staleness, passive-over-modal input ownership, the
//! `opening` latch — plus the reviewer charter: Esc with a focused
//! editor, drawer × PageHost composition, tiny viewports, Percent
//! rounding, sticky scrims, toggle storms, and interval scope
//! semantics under the zero-idle law. Findings + dispositions:
//! reviews/wave8/tabs-on-drawer.md.

use std::cell::{Cell as StdCell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::drawer::{
    Drawer, DrawerCloseReason, DrawerEdge, DrawerFocus, DrawerHandle, DrawerSize,
};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{frame_tasks_pending, Scope};
use abstracttui::term::{Capabilities, EnterOptions, MouseMode};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element};

fn test_config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: Some(EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: abstracttui::term::KittyFlags(0),
        }),
        probe: false,
    }
}

type ScopeSlot = Rc<RefCell<Option<Scope>>>;
type HandleSlot = Rc<RefCell<Option<DrawerHandle>>>;
type Reasons = Rc<RefCell<Vec<DrawerCloseReason>>>;

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    clock: Rc<StdCell<Instant>>,
    cx: Scope,
}

impl Rig {
    /// App + driver on an injected clock, mounting `build`'s view.
    fn new(size: Size, build: impl FnOnce(Scope) -> abstracttui::ui::View + 'static) -> Rig {
        let mut app = App::new(size);
        let slot: ScopeSlot = Rc::new(RefCell::new(None));
        let s = slot.clone();
        app.mount(move |cx| {
            *s.borrow_mut() = Some(cx);
            build(cx)
        })
        .expect("mount");
        let mut term = CaptureTerm::new(size);
        let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
        let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
        let c = clock.clone();
        driver.set_clock(move || c.get());
        driver.turn(&mut app, &mut term).expect("frame 1");
        let cx = slot.borrow().expect("scope");
        Rig {
            app,
            term,
            driver,
            clock,
            cx,
        }
    }

    fn page(size: Size, label: &'static str) -> Rig {
        Rig::new(size, move |_| {
            Element::new()
                .style(LayoutStyle::column())
                .child(text(label))
                .build()
        })
    }

    fn turn(&mut self) -> abstracttui::app::Turn {
        self.driver
            .turn(&mut self.app, &mut self.term)
            .expect("turn")
    }

    fn advance(&mut self, ms: u64) {
        self.clock.set(self.clock.get() + Duration::from_millis(ms));
    }

    /// Drive turns (advancing the clock) until idle.
    fn settle(&mut self) {
        for _ in 0..64 {
            let turn = self.turn();
            self.advance(8);
            if turn.idle && frame_tasks_pending() == 0 {
                return;
            }
        }
        panic!("failed to settle");
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    fn click(&mut self, col: i32, row: i32) {
        self.term
            .push_input(format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").as_bytes());
        self.settle();
    }

    fn keys(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
    }
}

fn reason_log() -> Reasons {
    Rc::new(RefCell::new(Vec::new()))
}

// ---------------------------------------------------------------------------
// Surface 1 — begin_close ordering: open-then-close (and close-then-
// reopen) in the SAME instant, before any frame ran, must never touch a
// dead signal and must land with the right ledger.
// ---------------------------------------------------------------------------

#[test]
fn same_instant_open_close_and_reopen_touch_no_dead_signal() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let reasons = reason_log();
    let r = reasons.clone();
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(14))
        .motion(Duration::from_millis(200))
        .on_close(move |why| r.borrow_mut().push(why))
        .install(rig.cx, |_| text("blink"));

    // Open + close before ANY effect flush: eased progress is still 0,
    // so the close lands synchronously through the slide effect — the
    // documented progress-before-closing order is what keeps the
    // second write off a disposed signal.
    handle.open();
    handle.close();
    rig.settle();
    assert!(!handle.is_open());
    assert!(handle.layer().is_none(), "mount torn down");
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Api]);
    assert!(!rig.screen().contains("blink"));

    // Open + close + open in one instant: the zero-progress close is
    // SYNCHRONOUS by design (the documented ordering comment), so the
    // second open is a FRESH mount and the ledger honestly records the
    // landed close. (Reopen-reversal applies only to closes caught
    // MID-FLIGHT — pinned in the toggle-storm test below.)
    handle.open();
    handle.close();
    handle.open();
    rig.settle();
    assert!(handle.is_open(), "ends open on a fresh mount");
    assert_eq!(
        *reasons.borrow(),
        vec![DrawerCloseReason::Api, DrawerCloseReason::Api],
        "the instant close landed; the ledger stays honest"
    );
    assert!(rig.screen().contains("blink"));
}

// ---------------------------------------------------------------------------
// Surface 2 — on_close re-entrancy vs a mid-claim replacement: the
// incumbent's observer reopens it WHILE the replacement is between its
// registry claim and its mount. One drawer per edge must survive.
// ---------------------------------------------------------------------------

#[test]
fn reopen_from_on_close_during_replacement_keeps_one_drawer_per_edge() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let a_slot: HandleSlot = Rc::new(RefCell::new(None));
    let reopened = Rc::new(StdCell::new(false));
    let (slot, once) = (a_slot.clone(), reopened.clone());
    let a = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::ZERO)
        .on_close(move |why| {
            // The pinned-drawer reflex: "if something replaces me,
            // come back" — fires INSIDE the replacement's claim.
            if why == DrawerCloseReason::Replaced && !once.get() {
                once.set(true);
                if let Some(h) = slot.borrow().as_ref() {
                    h.open();
                }
            }
        })
        .install(rig.cx, |_| text("AAA"));
    *a_slot.borrow_mut() = Some(a.clone());
    let b = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("BBB"));

    a.open();
    rig.settle();
    b.open(); // claims the edge -> A closes -> A's observer reopens A
    rig.settle();

    assert!(
        !(a.is_open() && b.is_open()),
        "TWO drawers open on one edge — the one-per-edge law broke"
    );
    // The LAST claim owns the slot: A reopened inside the claim window,
    // so B's open aborted before mounting (it never opened — no close
    // reason for B).
    assert!(a.is_open() && !b.is_open());
    let screen = rig.screen();
    assert!(screen.contains("AAA"), "{screen}");
    assert!(!screen.contains("BBB"), "{screen}");
}

#[test]
fn open_on_the_same_edge_from_inside_a_build_stays_single() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let y_slot: HandleSlot = Rc::new(RefCell::new(None));
    let y = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("YYY"));
    *y_slot.borrow_mut() = Some(y.clone());
    let once = Rc::new(StdCell::new(false));
    let (slot, guard) = (y_slot.clone(), once.clone());
    let x = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::ZERO)
        .install(rig.cx, move |_| {
            // User code mid-open steals the edge for another drawer.
            if !guard.get() {
                guard.set(true);
                if let Some(h) = slot.borrow().as_ref() {
                    h.open();
                }
            }
            text("XXX")
        });

    x.open();
    rig.settle();
    assert!(
        !(x.is_open() && y.is_open()),
        "both drawers mounted on one edge"
    );
    assert!(y.is_open() && !x.is_open(), "the LAST claim owns the slot");
    let screen = rig.screen();
    assert!(screen.contains("YYY"), "{screen}");
    assert!(!screen.contains("XXX"), "{screen}");
}

// ---------------------------------------------------------------------------
// Surface 6 — the `opening` latch: recursive open() from the drawer's
// own build is a no-op; close() from inside the build is SWALLOWED
// (pinned: the open completes) — review INFO #6.
// ---------------------------------------------------------------------------

#[test]
fn recursive_open_from_own_build_is_latched_and_close_is_swallowed() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let z_slot: HandleSlot = Rc::new(RefCell::new(None));
    let slot = z_slot.clone();
    let z = Drawer::new(DrawerEdge::Left)
        .size(DrawerSize::Cells(12))
        .motion(Duration::ZERO)
        .install(rig.cx, move |_| {
            if let Some(h) = slot.borrow().as_ref() {
                h.open(); // latched: Plan::Nothing
                h.close(); // no mount yet: swallowed (pinned behavior)
                h.toggle(); // is_open false -> open -> latched again
            }
            text("ZZZ")
        });
    *z_slot.borrow_mut() = Some(z.clone());
    z.open();
    rig.settle();
    assert!(z.is_open(), "the open completed; in-build verbs were inert");
    assert!(rig.screen().contains("ZZZ"));
}

// ---------------------------------------------------------------------------
// Surface 3 — per-thread registry across sequential worlds: a world
// dropped with a drawer OPEN MID-FLIGHT must leave nothing behind that
// breaks the next world on the same thread (stale weak claims are
// inert; the orphaned animate flight cancels through the new guard).
// ---------------------------------------------------------------------------

#[test]
fn sequential_app_worlds_leave_no_registry_or_task_residue() {
    let size = Size::new(40, 10);
    {
        let mut w1 = Rig::page(size, "world one");
        let h = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(12))
            .motion(Duration::from_millis(300))
            .install(w1.cx, |_| text("w1 drawer"));
        h.open();
        w1.turn(); // flight is LIVE
                   // World 1 dies here: app, driver, term, handle all drop with
                   // the drawer open mid-slide.
    }
    let mut w2 = Rig::page(size, "world two");
    let h2 = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::from_millis(20))
        .install(w2.cx, |_| text("w2 drawer"));
    h2.open();
    w2.settle();
    assert!(h2.is_open(), "the same edge claims cleanly in a new world");
    assert!(w2.screen().contains("w2 drawer"));
    assert_eq!(
        frame_tasks_pending(),
        0,
        "world 1's orphaned flight cancelled quietly"
    );
}

// ---------------------------------------------------------------------------
// Surface 4 — token staleness: a theme switch while open neither
// crashes nor restyles the open drawer (tokens resolve at open); the
// NEXT open wears the new theme. (The scrim's resize-repaint half is
// unit-pinned in drawer_tests.rs — it read the CURRENT theme, a
// verified defect, fixed.)
// ---------------------------------------------------------------------------

#[test]
fn theme_switch_while_open_is_calm_and_lands_at_next_open() {
    let before = abstracttui::app::current_theme().id;
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(16))
        .title("Files")
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("themed"));
    handle.open();
    rig.settle();
    assert!(rig.screen().contains("themed"));

    assert!(abstracttui::app::set_theme_by_id("nord"));
    rig.settle();
    assert!(handle.is_open(), "theme switch never closes a drawer");
    assert!(
        rig.screen().contains("themed"),
        "panel still renders (at-open tokens):\n{}",
        rig.screen()
    );

    handle.close();
    rig.settle();
    handle.open();
    rig.settle();
    assert!(
        rig.screen().contains("themed"),
        "reopen renders on the new theme"
    );
    abstracttui::app::set_theme_by_id(before);
}

// ---------------------------------------------------------------------------
// Surface 5 — passive above modal: a FOCUSED passive drawer on a
// higher edge slot must not keep the keyboard once a MODAL drawer
// opens — Esc belongs to the modal (found stealing pre-fix; the modal
// open now blurs passive drawer trees; an explicit click back into the
// unveiled passive re-steals, the engine's one focus story).
// ---------------------------------------------------------------------------

#[test]
fn modal_drawer_takes_keys_from_a_focused_passive_drawer_above_it() {
    let mut rig = Rig::page(Size::new(40, 12), "page");
    let passive = Drawer::new(DrawerEdge::Top)
        .size(DrawerSize::Cells(3))
        .focus(DrawerFocus::Passive)
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("glance strip"));
    let reasons = reason_log();
    let r = reasons.clone();
    let modal = Drawer::new(DrawerEdge::Left)
        .size(DrawerSize::Cells(14))
        .motion(Duration::ZERO)
        .on_close(move |why| r.borrow_mut().push(why))
        .install(rig.cx, |_| text("modal work"));

    passive.open();
    rig.settle();
    rig.click(30, 2); // focus the passive strip (click-to-focus)
    modal.open();
    rig.settle();
    assert!(passive.is_open() && modal.is_open());

    // Esc must close the MODAL (it owns input while open), not the
    // passive strip that happened to hold focus at a higher z slot.
    rig.keys(b"\x1b[27u");
    assert!(
        modal.is_open() != passive.is_open() || !modal.is_open(),
        "someone closed"
    );
    assert!(
        !modal.is_open(),
        "Esc belongs to the open modal drawer, not the passive strip"
    );
    assert!(
        passive.is_open(),
        "the passive drawer neither closed nor consumed the key"
    );
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Escape]);

    // Deliberate re-entry: clicking the (unveiled) passive strip after
    // the modal closed re-steals the keyboard — Esc now closes IT.
    rig.click(30, 2);
    rig.keys(b"\x1b[27u");
    assert!(!passive.is_open(), "explicit click-in re-owns the keys");
}

// The reviewer-charter half (Esc routing, PageHost composition, tiny
// viewports, Percent rounding, sticky scrims, storms, interval scope
// semantics) — sibling module for the size budget, sharing this rig
// (a SUBDIRECTORY file: top-level tests/*.rs are their own crates).
#[path = "wave_shell_review_parts/charter.rs"]
mod charter;
