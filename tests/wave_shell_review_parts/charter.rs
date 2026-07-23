//! The reviewer-charter half of the wave-8 cross-review (`#[path]`
//! child of `wave_shell_review.rs`, file-size budget — shares its
//! Rig; lives in a subdirectory so cargo does not compile it as its
//! own test crate): Esc routing with a focused editor, drawer ×
//! PageHost composition (the contract verdict), tiny viewports,
//! Percent rounding, sticky scrims, toggle storms, and interval scope
//! semantics under the zero-idle law.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::drawer::{Drawer, DrawerCloseReason, DrawerEdge, DrawerFocus, DrawerSize};
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::prelude::interval;
use abstracttui::reactive::{frame_tasks_pending, Scope, Signal};
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::{Button, PageHost, TextInput};

use super::{reason_log, HandleSlot, Rig, Size};

// ---------------------------------------------------------------------------
// Charter — Esc with a focused editor inside a modal drawer: the
// documented contract is substrate-owned Escape (content-first, then
// the panel): TextInput leaves Esc unconsumed, so Esc CLOSES the
// drawer even mid-edit. Pinned; the draft survives via the state
// recipe. (Tension with the 0515 layered-Esc idiom filed as a finding,
// not a defect — the contract is documented.)
// ---------------------------------------------------------------------------

#[test]
fn esc_in_a_modal_drawer_with_a_focused_editor_closes_the_drawer() {
    let mut rig = Rig::page(Size::new(44, 10), "page");
    let draft = rig.cx.signal(String::new());
    let reasons = reason_log();
    let r = reasons.clone();
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(24))
        .title("Compose")
        .motion(Duration::ZERO)
        .on_close(move |why| r.borrow_mut().push(why))
        .install(rig.cx, move |mount| {
            let t = abstracttui::theme::default_theme().tokens;
            TextInput::new().value(draft).element(mount, &t).build()
        });
    handle.open();
    rig.settle();
    // focus_init landed on the header's ✕ (first focusable); Tab moves
    // into the editor.
    rig.keys(b"\t");
    rig.keys(b"hi");
    assert_eq!(draft.get_untracked(), "hi", "typed into the editor");

    rig.keys(b"\x1b[27u");
    assert!(!handle.is_open(), "Esc closed the drawer around the editor");
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Escape]);
    assert_eq!(
        draft.get_untracked(),
        "hi",
        "the draft survived (install-scope signal, the state recipe)"
    );
}

// ---------------------------------------------------------------------------
// Charter — drawer × PageHost composition (the contract verdict).
// ---------------------------------------------------------------------------

fn shell_view(cx: Scope) -> abstracttui::ui::View {
    PageHost::new()
        .page("alpha", "Alpha", |_| text("BODY ALPHA"))
        .page("beta", "Beta", |_| text("BODY BETA"))
        .element(cx, &abstracttui::theme::default_theme().tokens)
        .build()
}

#[test]
fn page_switch_under_a_passive_drawer_keeps_it_open() {
    let mut rig = Rig::new(Size::new(44, 12), shell_view);
    let drawer = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(14))
        .focus(DrawerFocus::Passive)
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("notes"));
    drawer.open();
    rig.settle();
    assert!(rig.screen().contains("BODY ALPHA"));
    assert!(rig.screen().contains("notes"));

    // Passive drawer unfocused: the chord falls through to the page
    // host — the page switches UNDER the open drawer, which survives
    // (its host is the app scope, not the page generation).
    rig.keys(b"\x1b[6;5~");
    assert!(rig.screen().contains("BODY BETA"), "{}", rig.screen());
    assert!(drawer.is_open(), "app-scoped drawer survives page switches");
    assert!(rig.screen().contains("notes"));
}

#[test]
fn drawer_installed_inside_a_page_closes_with_host_gone_on_switch() {
    let slot: HandleSlot = Rc::new(RefCell::new(None));
    let reasons = reason_log();
    let (s, r) = (slot.clone(), reasons.clone());
    let mut rig = Rig::new(Size::new(44, 12), move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let (s, r) = (s.clone(), r.clone());
        PageHost::new()
            .page("alpha", "Alpha", move |gcx| {
                // A drawer whose HOST is the page's generation scope:
                // it lives exactly as long as the page.
                let r = r.clone();
                let h = Drawer::new(DrawerEdge::Right)
                    .size(DrawerSize::Cells(12))
                    .focus(DrawerFocus::Passive)
                    .motion(Duration::ZERO)
                    .on_close(move |why| r.borrow_mut().push(why))
                    .install(gcx, |_| text("page tool"));
                *s.borrow_mut() = Some(h);
                text("BODY ALPHA")
            })
            .page("beta", "Beta", |_| text("BODY BETA"))
            .element(cx, &t)
            .build()
    });
    let handle = slot.borrow().clone().expect("installed at page mount");
    handle.open();
    rig.settle();
    assert!(rig.screen().contains("page tool"));

    rig.keys(b"\x1b[6;5~"); // switch away: the page generation dies
    assert!(rig.screen().contains("BODY BETA"));
    assert!(!handle.is_open(), "page-scoped drawer died with its page");
    assert!(handle.layer().is_none());
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::HostGone]);
    assert!(!rig.screen().contains("page tool"));
    assert_eq!(frame_tasks_pending(), 0);
}

#[test]
fn modal_drawer_blocks_page_chords_while_open_by_design() {
    let mut rig = Rig::new(Size::new(44, 12), shell_view);
    let drawer = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(14))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("modal pane"));
    drawer.open();
    rig.settle();

    // A modal drawer owns every key: the page chord never reaches the
    // host (documented modal contract) — the page does NOT switch.
    rig.keys(b"\x1b[6;5~");
    assert!(rig.screen().contains("BODY ALPHA"), "{}", rig.screen());
    assert!(drawer.is_open());

    // Close it; the chord works again.
    rig.keys(b"\x1b[27u");
    assert!(!drawer.is_open());
    rig.keys(b"\x1b[6;5~");
    assert!(rig.screen().contains("BODY BETA"), "{}", rig.screen());
}

// ---------------------------------------------------------------------------
// Charter — tiny viewports and Percent rounding.
// ---------------------------------------------------------------------------

#[test]
fn top_and_bottom_drawers_survive_tiny_viewports() {
    let mut rig = Rig::page(Size::new(6, 4), "pg");
    let top = Drawer::new(DrawerEdge::Top)
        .size(DrawerSize::Percent(0.5))
        .title("T")
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("t"));
    let bottom = Drawer::new(DrawerEdge::Bottom)
        .size(DrawerSize::Cells(9)) // oversize: clamps to the viewport
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("b"));
    top.open();
    rig.settle();
    assert_eq!(
        top.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(0, 0, 6, 2)
    );
    bottom.open();
    rig.settle();
    assert_eq!(
        bottom.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(0, 0, 6, 4),
        "oversize clamps to the whole axis"
    );
    top.close();
    bottom.close();
    rig.settle();

    // Degenerate 2x2: open/close without panic; floor is one cell.
    let mut tiny = Rig::page(Size::new(2, 2), "");
    let t2 = Drawer::new(DrawerEdge::Top)
        .size(DrawerSize::Percent(0.1))
        .motion(Duration::ZERO)
        .install(tiny.cx, |_| text("x"));
    t2.open();
    tiny.settle();
    assert_eq!(
        t2.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(0, 0, 2, 1),
        "size floors at one cell"
    );
    t2.close();
    tiny.settle();
    assert_eq!(frame_tasks_pending(), 0);
}

#[test]
fn percent_sizes_round_and_clamp_at_odd_axes() {
    let mut rig = Rig::page(Size::new(41, 11), "pg");
    let right = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Percent(0.5))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("r"));
    let left = Drawer::new(DrawerEdge::Left)
        .size(DrawerSize::Percent(0.25))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("l"));
    let bottom = Drawer::new(DrawerEdge::Bottom)
        .size(DrawerSize::Percent(0.5))
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("b"));
    right.open();
    left.open();
    bottom.open();
    rig.settle();
    // 41 * 0.5 = 20.5 rounds away from zero: 21 cells, x = 20.
    assert_eq!(
        right.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(20, 0, 21, 11)
    );
    // 41 * 0.25 = 10.25 rounds to 10.
    assert_eq!(
        left.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(0, 0, 10, 11)
    );
    // 11 * 0.5 = 5.5 rounds to 6, hugging the bottom.
    assert_eq!(
        bottom.layer().unwrap().bounds().unwrap(),
        abstracttui::base::Rect::new(0, 5, 41, 6)
    );
}

// ---------------------------------------------------------------------------
// Charter — a sticky modal scrim never leaks presses to the page.
// ---------------------------------------------------------------------------

#[test]
fn sticky_modal_scrim_never_leaks_presses_to_the_page() {
    let presses: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let p = presses.clone();
    let mut rig = Rig::new(Size::new(40, 10), move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let p = p.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Button::new("hit me")
                    .on_click(move || *p.borrow_mut() += 1)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let drawer = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .close_on_outside(false)
        .motion(Duration::ZERO)
        .install(rig.cx, |_| text("sticky"));
    rig.click(3, 1);
    assert_eq!(*presses.borrow(), 1, "control: the button works");

    drawer.open();
    rig.settle();
    rig.click(3, 1); // on the scrim, over the button
    assert_eq!(
        *presses.borrow(),
        1,
        "the scrim swallowed the press — nothing reached the page"
    );
    assert!(drawer.is_open(), "close_on_outside(false) held");

    rig.keys(b"\x1b[27u");
    assert!(!drawer.is_open());
    rig.click(3, 1);
    assert_eq!(*presses.borrow(), 2, "input returned to the page");
}

// ---------------------------------------------------------------------------
// Charter — toggle storms: reopen-reversals and replace churn across
// consecutive turns drain to zero tasks with one coherent truth.
// ---------------------------------------------------------------------------

#[test]
fn toggle_storms_drain_to_zero_and_keep_one_truth() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let reasons = reason_log();
    let r = reasons.clone();
    let a = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::from_millis(40))
        .on_close(move |why| r.borrow_mut().push(why))
        .install(rig.cx, |_| text("AAA"));
    // Same-mount wobble: a GENUINE mid-flight close (the follower has
    // left zero), reversed, then closed for real — exactly one landed
    // close on the ledger.
    a.open();
    rig.turn(); // stamp the flight
    rig.advance(20);
    rig.turn(); // sample: mid-flight now
    a.close(); // caught mid-flight: no synchronous finish
    rig.turn();
    rig.advance(5);
    a.open(); // REVERSE the closing flight (same mount)
    rig.settle();
    assert!(a.is_open(), "reversal kept the mount");
    assert!(
        reasons.borrow().is_empty(),
        "the reversed close never landed: {:?}",
        reasons.borrow()
    );
    a.close();
    rig.settle();
    assert!(!a.is_open());
    assert_eq!(
        *reasons.borrow(),
        vec![DrawerCloseReason::Api],
        "one landed close for the whole wobble"
    );

    // Replace churn: two drawers alternate claims on one edge.
    let b = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .motion(Duration::from_millis(40))
        .install(rig.cx, |_| text("BBB"));
    for _ in 0..3 {
        a.open();
        rig.turn();
        rig.advance(10);
        b.open();
        rig.turn();
        rig.advance(10);
    }
    rig.settle();
    assert!(
        b.is_open() && !a.is_open(),
        "the last claimant owns the edge"
    );
    assert_eq!(
        frame_tasks_pending(),
        0,
        "every follower generation drained"
    );
    let screen = rig.screen();
    assert!(screen.contains("BBB"), "{screen}");
    assert!(!screen.contains("AAA"), "{screen}");
    let ledger = reasons.borrow();
    assert_eq!(
        ledger
            .iter()
            .filter(|w| **w == DrawerCloseReason::Replaced)
            .count(),
        3,
        "each churn round replaced A exactly once: {ledger:?}"
    );
}

// ---------------------------------------------------------------------------
// Charter — zero idle with an interval-driven page: the timer belongs
// to the MOUNT scope, so it ticks only while the drawer is open.
// ---------------------------------------------------------------------------

#[test]
fn interval_page_in_a_drawer_ticks_only_while_open() {
    let mut rig = Rig::page(Size::new(40, 10), "page");
    let ticks: Signal<u32> = rig.cx.signal(0u32);
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(16))
        .motion(Duration::ZERO)
        .install(rig.cx, move |mount| {
            // Per-open work rides the MOUNT scope: it dies at close.
            interval(mount, Duration::from_millis(200), move || {
                ticks.update(|t| *t += 1)
            });
            Element::new()
                .style(LayoutStyle::column().width(Dimension::Percent(1.0)))
                .child(dyn_view(LayoutStyle::line(1), move || {
                    text(format!("tick {}", ticks.get()))
                }))
                .build()
        });
    handle.open();
    rig.settle();
    assert!(rig.screen().contains("tick 0"));

    let mut rendered = 0;
    for _ in 0..3 {
        rig.advance(220);
        let turn = rig.turn();
        if turn.rendered {
            rendered += 1;
        }
        rig.turn();
    }
    assert!(rendered >= 2, "the interval drove frames while open");
    let while_open = ticks.get_untracked();
    assert!(while_open >= 2, "ticked while open: {while_open}");

    handle.close();
    rig.settle();
    let at_close = ticks.get_untracked();
    let _ = rig.term.take_bytes();
    for _ in 0..5 {
        rig.advance(220);
        let turn = rig.turn();
        assert!(turn.idle, "closed drawer: turns are idle ({turn:?})");
        assert!(!turn.rendered);
    }
    assert!(
        rig.term.bytes().is_empty(),
        "closed drawer costs zero bytes"
    );
    assert_eq!(
        ticks.get_untracked(),
        at_close,
        "the interval died with the mount scope"
    );
}
