//! Drawer wave acceptance (app-kits 0585): the drawer system through
//! the REAL driver pipeline against CaptureTerm on an injected clock —
//! the Toast acceptance standard applied to edge panels.
//!
//! The charter pins:
//! - the slide emits frames ONLY during the transition, then
//!   `frame_tasks_pending() == 0` and an idle turn emits ZERO bytes;
//! - mid-slide frames never re-emit content OUTSIDE the drawer band
//!   (`Layer::set_origin` bills old ∪ new bounds — the damage stays in
//!   the band);
//! - a full page (Scroll + Feed) inside the drawer scrolls;
//! - a Modal opened FROM a drawer layers above it and gives input back
//!   on close;
//! - both wire spellings (handle verbs, bound signal) drive one truth.

use std::cell::{Cell as StdCell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::drawer::{Drawer, DrawerCloseReason, DrawerEdge, DrawerFocus, DrawerSize};
use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{frame_tasks_pending, Scope};
use abstracttui::term::{Capabilities, EnterOptions, MouseMode};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{text, Element};
use abstracttui::widgets::{Feed, FeedItem, FeedState, Scroll};

fn test_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.unicode_ok = true;
    })
}

fn test_config() -> RunConfig {
    RunConfig {
        caps: Some(test_caps()),
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

/// App + captured scope + driver on an injected clock.
fn rig(size: Size, page: &'static str) -> (App, ScopeSlot, CaptureTerm, Rc<StdCell<Instant>>) {
    let mut app = App::new(size);
    let slot: ScopeSlot = Rc::new(RefCell::new(None));
    let s = slot.clone();
    app.mount(move |cx| {
        *s.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text(page))
            .build()
    })
    .expect("mount");
    let term = CaptureTerm::new(size);
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    (app, slot, term, clock)
}

#[test]
fn drawer_slide_frames_only_during_transition_then_idle_zero_bytes() {
    let size = Size::new(40, 10);
    let (mut app, slot, mut term, clock) = rig(size, "main page content");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    let advance = |ms: u64| clock.set(clock.get() + Duration::from_millis(ms));
    driver.turn(&mut app, &mut term).expect("frame 1");
    assert!(term.screen().to_text().contains("main page content"));
    let _ = term.take_bytes();

    let cx = slot.borrow().expect("scope");
    let reasons: Rc<RefCell<Vec<DrawerCloseReason>>> = Rc::new(RefCell::new(Vec::new()));
    let r = reasons.clone();
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(18))
        .title("Files")
        .motion(Duration::from_millis(12))
        .overlays(&overlays)
        .on_close(move |why| r.borrow_mut().push(why))
        .install(cx, |_| text("drawer page"));

    // ---- slide in: frames emit DURING the transition -------------------
    handle.open();
    let mut slide_frames = 0;
    for _ in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("slide-in turn");
        if turn.emitted {
            slide_frames += 1;
        }
        advance(4);
    }
    assert!(slide_frames >= 2, "the slide is animated: {slide_frames}");
    assert_eq!(frame_tasks_pending(), 0, "transition landed");
    let screen = term.screen().to_text();
    assert!(screen.contains("drawer page"), "panel content:\n{screen}");
    assert!(screen.contains("Files"), "header title:\n{screen}");

    // ---- parked: an idle turn emits ZERO bytes (the charter line) ------
    let _ = term.take_bytes();
    let parked = driver.turn(&mut app, &mut term).expect("parked turn");
    assert!(!parked.emitted, "{parked:?}");
    assert!(
        term.take_bytes().is_empty(),
        "open+settled drawer costs zero"
    );

    // ---- slide out: layer removed, page repainted, idle zero again -----
    handle.close();
    for _ in 0..8 {
        driver.turn(&mut app, &mut term).expect("slide-out turn");
        advance(4);
    }
    assert_eq!(frame_tasks_pending(), 0, "close landed");
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Api]);
    let screen = term.screen().to_text();
    assert!(!screen.contains("drawer page"), "panel gone:\n{screen}");
    assert!(
        screen.contains("main page content"),
        "vacated cells repainted from the page:\n{screen}"
    );
    let _ = term.take_bytes();
    let idle = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(idle.idle || !idle.emitted, "{idle:?}");
    assert!(
        term.take_bytes().is_empty(),
        "idle after a full open/close cycle emits zero bytes"
    );
}

#[test]
fn mid_slide_frames_stay_inside_the_drawer_band() {
    let size = Size::new(40, 10);
    let (mut app, slot, mut term, clock) = rig(size, "LEFTMARK anchored page");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    let advance = |ms: u64| clock.set(clock.get() + Duration::from_millis(ms));
    driver.turn(&mut app, &mut term).expect("frame 1");
    let _ = term.take_bytes();

    let cx = slot.borrow().expect("scope");
    // Scrim OFF isolates the band: the only damage during the slide is
    // the moving panel (old ∪ new bounds per frame).
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(12))
        .scrim(false)
        .motion(Duration::from_millis(12))
        .overlays(&overlays)
        .install(cx, |_| text("band"));
    handle.open();
    let mut emitted_any = false;
    for _ in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("slide turn");
        let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
        if turn.emitted {
            emitted_any = true;
        }
        assert!(
            !bytes.contains("LEFTMARK"),
            "content outside the drawer band must never re-emit: {bytes:?}"
        );
        advance(4);
    }
    assert!(emitted_any, "the slide painted frames");
    assert_eq!(frame_tasks_pending(), 0, "landed");
    assert!(
        term.screen().to_text().contains("LEFTMARK"),
        "page text still on screen (left of the band)"
    );
}

#[test]
fn feed_page_inside_the_drawer_scrolls() {
    let size = Size::new(40, 10);
    let (mut app, slot, mut term, clock) = rig(size, "page");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = slot.borrow().expect("scope");
    // A REAL page: a Feed of 30 rows inside a Scroll, hosted whole.
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(24))
        .motion(Duration::ZERO)
        .overlays(&overlays)
        .install(cx, |mount| {
            let feed = FeedState::new(mount);
            for i in 0..30 {
                feed.push(format!("k{i}"), FeedItem::text(format!("item-{i:02}")));
            }
            Scroll::new(Feed::new(&feed).gap(0).view(mount)).view(mount)
        });
    handle.open();
    for _ in 0..3 {
        driver.turn(&mut app, &mut term).expect("open turn");
        clock.set(clock.get() + Duration::from_millis(4));
    }
    let screen = term.screen().to_text();
    assert!(screen.contains("item-00"), "top of the feed:\n{screen}");
    assert!(!screen.contains("item-29"), "tail off-screen:\n{screen}");

    // Wheel down inside the panel (SGR 65 = wheel down, 1-based).
    for _ in 0..12 {
        term.push_input(b"\x1b[<65;30;5M");
        driver.turn(&mut app, &mut term).expect("wheel turn");
    }
    let screen = term.screen().to_text();
    assert!(
        !screen.contains("item-00"),
        "feed scrolled away from the top:\n{screen}"
    );
    assert!(
        screen.contains("item-1") || screen.contains("item-2"),
        "later items visible:\n{screen}"
    );
}

#[test]
fn modal_from_drawer_layers_above_and_returns_input_on_close() {
    let size = Size::new(40, 12);
    let (mut app, slot, mut term, clock) = rig(size, "page under");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = slot.borrow().expect("scope");
    let drawer_keys: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let dk = drawer_keys.clone();
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(20))
        .motion(Duration::ZERO)
        .overlays(&overlays)
        .install(cx, move |_| {
            let dk = dk.clone();
            // A realistic page carries a focusable: focus_init lands
            // there, so keys route through the page's handlers.
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(abstracttui::layout::Dimension::Percent(1.0))
                        .grow(1.0),
                )
                .focusable()
                .autofocus()
                .on(abstracttui::ui::Phase::Bubble, move |_c, ev| {
                    if matches!(ev, abstracttui::ui::UiEvent::Key(_)) {
                        *dk.borrow_mut() += 1;
                    }
                })
                .child(text("drawer host"))
                .build()
        });
    handle.open();
    driver.turn(&mut app, &mut term).expect("drawer open");
    driver.turn(&mut app, &mut term).expect("drawer settled");
    assert!(term.screen().to_text().contains("drawer host"));

    // A modal opened FROM the drawer sits at MODAL_Z (1000) above the
    // drawer band and owns the keyboard while open.
    let modal = Modal::open(&overlays, cx, size, Size::new(22, 3), |_| {
        text("modal above drawer")
    });
    driver.turn(&mut app, &mut term).expect("modal frame");
    let screen = term.screen().to_text();
    assert!(
        screen.contains("modal above drawer"),
        "modal on top:\n{screen}"
    );
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("key to modal");
    assert_eq!(
        *drawer_keys.borrow(),
        0,
        "the modal owns keys, not the drawer"
    );

    modal.close();
    driver.turn(&mut app, &mut term).expect("modal closed");
    let screen = term.screen().to_text();
    assert!(!screen.contains("modal above drawer"), "{screen}");
    assert!(
        screen.contains("drawer host"),
        "drawer repainted under the vacated modal:\n{screen}"
    );
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("key to drawer");
    assert_eq!(
        *drawer_keys.borrow(),
        1,
        "input returned to the drawer after the modal closed"
    );
    // Esc closes the drawer itself (through the whole driver stack;
    // kitty spelling — a lone \x1b parks in the parser's ambiguity
    // window until its deadline).
    term.push_input(b"\x1b[27u");
    driver.turn(&mut app, &mut term).expect("esc turn");
    assert!(!handle.is_open(), "Esc closed the drawer");
}

#[test]
fn handle_and_bound_signal_are_one_truth() {
    let size = Size::new(40, 10);
    let (mut app, slot, mut term, clock) = rig(size, "page");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = slot.borrow().expect("scope");
    let open = cx.signal(false);
    // 20 cells wide: the panel pads 3 (hairline + breathing), so the
    // 14-char label needs ≥ 17 columns to land unclipped.
    let handle = Drawer::new(DrawerEdge::Left)
        .size(DrawerSize::Cells(20))
        .motion(Duration::ZERO)
        .overlays(&overlays)
        .bind(open)
        .install(cx, |_| text("both spellings"));

    // Spelling 1: the handle. The signal follows.
    handle.open();
    driver.turn(&mut app, &mut term).expect("turn");
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(handle.is_open() && open.get_untracked());
    assert!(term.screen().to_text().contains("both spellings"));

    // Spelling 2: the signal. The drawer follows.
    open.set(false);
    driver.turn(&mut app, &mut term).expect("turn");
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(!handle.is_open(), "signal write closed the drawer");
    assert!(!term.screen().to_text().contains("both spellings"));

    open.set(true);
    driver.turn(&mut app, &mut term).expect("turn");
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(handle.is_open(), "signal write reopened the drawer");
    handle.toggle();
    driver.turn(&mut app, &mut term).expect("turn");
    assert!(
        !open.get_untracked(),
        "handle toggle reflected into the signal"
    );
}

/// F1 regression (cycle-3 acceptance): an INSTANT, scrimless close must
/// repaint the region the panel occupied. The close slides the panel to
/// its off-screen closed origin BEFORE removal, so `remove()`'s
/// current-bounds damage clips to empty — the visible cells never
/// repainted (a modal scrim's full-viewport removal hid this for modal
/// drawers; passive/scrimless drawers showed the frozen panel forever).
/// Cycle-1's own close test only asserted `layer().is_none()`, not the
/// pixels, so it slipped through.
#[test]
fn instant_scrimless_close_repaints_the_vacated_region() {
    let size = Size::new(40, 8);
    let (mut app, slot, mut term, clock) = rig(size, "PAGE UNDERNEATH");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");
    let cx = slot.borrow().expect("scope");
    let handle = Drawer::new(DrawerEdge::Left)
        .size(DrawerSize::Cells(16))
        .focus(DrawerFocus::Passive) // no scrim to mask the bug
        .title("Nav")
        .motion(Duration::ZERO) // instant: the panel teleports off-screen
        .overlays(&overlays)
        .install(cx, |_| text("NAV PANEL"));
    handle.open();
    for _ in 0..4 {
        driver.turn(&mut app, &mut term).expect("open");
    }
    assert!(term.screen().to_text().contains("NAV PANEL"));

    handle.close();
    for _ in 0..4 {
        driver.turn(&mut app, &mut term).expect("close");
    }
    assert!(handle.layer().is_none(), "layer removed");
    let s = term.screen().to_text();
    assert!(
        !s.contains("NAV PANEL"),
        "the closed panel's pixels must be gone:\n{s}"
    );
    assert!(
        s.contains("PAGE UNDERNEATH"),
        "the page under the vacated panel must repaint:\n{s}"
    );
}

/// F1 sibling (cycle-3): a scrimless RIGHT drawer that SHRINKS on resize
/// must not leave stale cells at its old leading edge. The panel's own
/// move/resize damage covers only the new (smaller) bounds; reclamp
/// names the old rect so the vacated strip recomposites.
#[test]
fn scrimless_right_drawer_shrink_on_resize_leaves_no_stale_edge() {
    let size = Size::new(60, 8);
    let (mut app, slot, mut term, clock) = rig(size, "PAGEBODY");
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");
    let cx = slot.borrow().expect("scope");
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(30))
        .scrim(false)
        .title("SIDE")
        .motion(Duration::ZERO)
        .overlays(&overlays)
        .install(cx, |_| text("RIGHTBODY"));
    handle.open();
    for _ in 0..4 {
        driver.turn(&mut app, &mut term).expect("open");
    }
    // Shrink the drawer WITHOUT shrinking the terminal: bind a smaller
    // size is not a builder knob, so drive it via a viewport change that
    // moves the right drawer's leading edge inward while keeping columns
    // that would hold any stale cells. A 30-wide right drawer at x=30
    // (cols 30..60); shrink the terminal to 50 → the drawer re-solves to
    // 30 at x=20 (cols 20..50). The old leading hairline at col 30 must
    // not persist as a doubled rule.
    term.push_resize(Size::new(50, 8));
    for _ in 0..4 {
        driver.turn(&mut app, &mut term).expect("resize");
    }
    let s = term.screen().to_text();
    let bar = s.lines().next().unwrap_or("");
    // Exactly one leading hairline '│' (the drawer's), never a stale twin.
    assert_eq!(
        bar.matches('│').count(),
        1,
        "a doubled/stale leading edge survived the shrink:\n{bar}"
    );
    assert!(
        bar.contains("SIDE"),
        "the drawer header still renders:\n{bar}"
    );
}
