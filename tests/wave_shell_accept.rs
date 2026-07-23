//! Wave-8 cycle-3 acceptance: the STRANGER's proof. A third app author
//! who knows only docs/api.md composes one realistic shell — a
//! PageHost (Feed page, form page, Table page), a right MODAL drawer
//! (inspector with scrollable content), a left PASSIVE drawer (nav), a
//! Modal opened FROM the inspector, and a Toast — and drives the whole
//! thing through raw wire bytes on the real Driver: chord + click
//! navigation, typing, both drawers, modal-from-drawer, the Esc
//! unwinding ladder (modal → drawer → nothing), a theme switch and a
//! resize while EVERYTHING is open, then parks at zero idle bytes.
//!
//! Every API spelling below is the documented one (api.md: PageHost,
//! Drawer, Modal, Toast, widgets). If the stranger trips, the doc or
//! the code is wrong — findings ride reviews/ + the cycle-3 report.

use std::cell::{Cell as StdCell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerFocus, DrawerSize};
use abstracttui::app::{App, Driver, Modal, RunConfig, Toast};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;
use abstracttui::widgets::{Button, ColWidth, Column, Feed, FeedItem, FeedState, PageHost, Table};

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: None,
        probe: false,
    }
}

fn settle_on(
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    clock: &std::cell::Cell<Instant>,
) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
        // Animations (the toast flight) ride the injected clock.
        clock.set(clock.get() + Duration::from_millis(20));
    }
    panic!("loop failed to settle within 64 turns");
}

/// SGR left click (press + release) at 1-BASED terminal coordinates.
fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

/// App-owned state shared across pages/drawers (the documented recipe:
/// everything that must survive remounts lives OUTSIDE the builders).
#[derive(Clone)]
struct Shell {
    inspector: abstracttui::app::drawer::DrawerHandle,
    nav: abstracttui::app::drawer::DrawerHandle,
    draft: Signal<String>,
    saves: Signal<u32>,
    page: Signal<String>,
    detail_modal: Rc<RefCell<Option<Modal>>>,
}

#[test]
fn a_stranger_composes_the_whole_shell_from_the_docs() {
    let size = Size::new(100, 30);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let shell_slot: Rc<RefCell<Option<Shell>>> = Rc::new(RefCell::new(None));
    let slot = shell_slot.clone();

    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let overlays = cx
            .use_context::<abstracttui::app::Overlays>()
            .expect("App provides the overlay store as context");

        // ---- the app store (survives every remount) -----------------
        let draft = cx.signal(String::new());
        let saves = cx.signal(0u32);
        // Controlled navigation (the documented router shape): the app
        // owns the active-page signal; the host renders and writes it.
        let page = cx.signal("feed".to_string());
        let feed = FeedState::new(cx);
        for i in 0..12 {
            feed.push(
                format!("m{i}"),
                FeedItem::markdown(format!("**msg {i}** hello")),
            );
        }
        let detail_modal: Rc<RefCell<Option<Modal>>> = Rc::new(RefCell::new(None));

        // ---- the right MODAL inspector (scrollable full page) -------
        let dm = detail_modal.clone();
        let ov = overlays.clone();
        let inspector = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Percent(0.45))
            .title("Inspector")
            .motion(Duration::ZERO)
            .install(cx, move |mount| {
                let dm = dm.clone();
                let ov = ov.clone();
                let vp = use_viewport(mount);
                let mut lines = Element::new().style(LayoutStyle::column());
                for i in 0..40 {
                    lines = lines.child(text(format!("inspect-{i:02}")));
                }
                Element::new()
                    .style(LayoutStyle::column().gap(1).grow(1.0))
                    .child(
                        Button::new("Details")
                            .on_click(move || {
                                // Modal-from-drawer: layers above the
                                // drawer band by the documented z laws.
                                // Modal closes EXPLICITLY (api.md), so
                                // the app wires Esc itself through a
                                // shared slot (the closer pattern —
                                // the handle only exists after open).
                                let dm_esc = dm.clone();
                                let modal = Modal::open(
                                    &ov,
                                    mount,
                                    vp.get_untracked(),
                                    Size::new(44, 8),
                                    move |_mcx| {
                                        Element::new()
                                            .style(LayoutStyle::fill())
                                            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                                                if let Some(m) = dm_esc.borrow_mut().take() {
                                                    m.close();
                                                }
                                            })
                                            .child(text("MODAL DETAIL — esc closes"))
                                            .build()
                                    },
                                );
                                *dm.borrow_mut() = Some(modal);
                            })
                            .view(mount),
                    )
                    .child(Scroll::new(lines.build()).view(mount))
                    .build()
            });

        // ---- the left PASSIVE nav (glanceable) ----------------------
        let nav = Drawer::new(DrawerEdge::Left)
            .size(DrawerSize::Cells(22))
            .focus(DrawerFocus::Passive)
            .title("Navigate")
            .motion(Duration::ZERO)
            .install(cx, |_| {
                Element::new()
                    .style(LayoutStyle::column().gap(1).grow(1.0))
                    .child(text("· feed\n· form\n· table"))
                    .build()
            });

        *slot.borrow_mut() = Some(Shell {
            inspector: inspector.clone(),
            nav: nav.clone(),
            draft,
            saves,
            page,
            detail_modal: detail_modal.clone(),
        });

        // ---- the page host ------------------------------------------
        let feed_for_page = feed.clone();
        let ov_toast = overlays.clone();
        let host = PageHost::new()
            .page("feed", "Feed", move |gcx| {
                let follow = gcx.signal(false);
                Scroll::new(Feed::new(&feed_for_page).gap(0).view(gcx))
                    .follow_tail(follow)
                    .view(gcx)
            })
            .page("form", "Form", move |gcx| {
                let vp = use_viewport(gcx);
                let ov = ov_toast.clone();
                Element::new()
                    .style(LayoutStyle::column().gap(0).grow(1.0))
                    .child(text("display name:"))
                    .child(TextInput::new().value(draft).element(gcx, &t).build())
                    .child(
                        Button::new("Save")
                            .on_click(move || {
                                saves.update(|n| *n += 1);
                                Toast::show(
                                    &ov,
                                    gcx,
                                    vp.get_untracked(),
                                    "saved!",
                                    Duration::from_millis(500),
                                );
                            })
                            .view(gcx),
                    )
                    .build()
            })
            .page("table", "Table", move |gcx| {
                Table::new(vec![
                    Column::new("name", ColWidth::Cells(12)),
                    Column::new("state", ColWidth::Flex(1.0)),
                ])
                .rows(vec![
                    vec!["alpha".into(), "ready".into()],
                    vec!["beta".into(), "busy".into()],
                    vec!["gamma".into(), "ready".into()],
                ])
                .view(gcx)
            })
            .active(page)
            .view(cx);

        // ---- root: toggles ride shortcuts (focused widgets win) -----
        let ins = inspector.clone();
        let nv = nav.clone();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('i')), move |_| ins.toggle())
            .shortcut(KeyChord::plain(Key::Char('g')), move |_| nv.toggle())
            .child(host)
            .build()
    })
    .expect("mount");

    let shell = shell_slot.borrow().clone().expect("shell built");
    // The host lives under a wrapper root, so the documented rule
    // applies (api.md, Chords): establish focus or chords are dead —
    // the shell example's own `focus_first` line. THIS TRIPPED the
    // stranger once before reading to the end of the paragraph.
    app.tree().focus_first();
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    let advance = |ms: u64| clock.set(clock.get() + Duration::from_millis(ms));

    // ---- 1. boot: feed page under the tab bar -----------------------
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let s = screen(&term);
    assert!(s.contains("Feed") && s.contains("msg 0"), "boot:\n{s}");
    let _ = term.take_bytes();

    // ---- 2. park: the empty shell idles at zero bytes ---------------
    for _ in 0..4 {
        let turn = driver.turn(&mut app, &mut term).expect("idle");
        assert!(turn.idle && !turn.rendered, "{turn:?}");
    }
    assert!(term.take_bytes().is_empty(), "parked shell wrote bytes");

    // ---- 3. chord to the form page; type; save (toast) --------------
    term.push_input(b"\x1b[6;5~"); // Ctrl+PgDn
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(screen(&term).contains("display name:"));
    // Click into the input (bar rows 1-2; label row 3; input row 4).
    term.push_input(&sgr_click(4, 4));
    settle_on(&mut driver, &mut app, &mut term, &clock);
    term.push_input(b"hello");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(screen(&term).contains("hello"), "{}", screen(&term));
    assert_eq!(shell.draft.get_untracked(), "hello");
    // Click Save (the button row sits under the input).
    term.push_input(&sgr_click(3, 5));
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert_eq!(shell.saves.get_untracked(), 1, "the save click landed");
    assert!(
        screen(&term).contains("saved!"),
        "toast visible:\n{}",
        screen(&term)
    );
    // The toast dismisses on its own clock and the shell re-parks.
    advance(60_000);
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(!screen(&term).contains("saved!"), "toast gone");
    let _ = term.take_bytes();
    let turn = driver.turn(&mut app, &mut term).expect("idle");
    assert!(turn.idle && term.take_bytes().is_empty(), "{turn:?}");

    // ---- 4. click the Table tab; table renders -----------------------
    // Bar segs: " Feed "(0..6) " Form "(7..13) " Table "(14..21).
    term.push_input(&sgr_click(16, 1));
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let s = screen(&term);
    assert!(
        s.contains("alpha") && s.contains("beta"),
        "table page:\n{s}"
    );

    // ---- 5. passive nav drawer: glanceable, keys stay with the app ---
    term.push_input(b"g");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(shell.nav.is_open());
    assert!(screen(&term).contains("Navigate"));
    // Chords still drive the PAGE HOST while a passive drawer shows.
    // (The page's own text sits UNDER the nav drawer's band — overlay
    // truth — so the page identity is asserted through the controlled
    // signal, the documented router shape.)
    term.push_input(b"\x1b[5;5~"); // Ctrl+PgUp -> Form
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert_eq!(
        shell.page.get_untracked(),
        "form",
        "passive drawer left the keyboard with the app"
    );

    // ---- 6. modal inspector over everything; scroll its page ---------
    term.push_input(b"i");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(shell.inspector.is_open());
    let s = screen(&term);
    assert!(s.contains("Inspector") && s.contains("inspect-00"), "{s}");
    // Wheel-down inside the panel (right 45% band): content scrolls.
    for _ in 0..6 {
        term.push_input(b"\x1b[<65;75;12M");
        settle_on(&mut driver, &mut app, &mut term, &clock);
    }
    let s = screen(&term);
    assert!(
        !s.contains("inspect-00") && s.contains("inspect-"),
        "the inspector page scrolled:\n{s}"
    );

    // ---- 7. modal-from-drawer: focus-init sits on the content --------
    // The drawer's first focusable IS the Details button (chrome never
    // steals focus), so Enter opens the detail modal directly.
    term.push_input(b"\r"); // Enter
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(
        screen(&term).contains("MODAL DETAIL"),
        "modal above the drawer:\n{}",
        screen(&term)
    );

    // ---- 8. theme switch while EVERYTHING is open ---------------------
    abstracttui::app::set_theme_by_id("nord");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let s = screen(&term);
    assert!(
        s.contains("MODAL DETAIL") && s.contains("Inspector") && s.contains("Navigate"),
        "everything survived the theme switch:\n{s}"
    );

    // ---- 9. resize while everything is open ---------------------------
    term.push_resize(Size::new(90, 28));
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let s = screen(&term);
    assert!(
        s.contains("MODAL DETAIL") && s.contains("Inspector") && s.contains("Navigate"),
        "everything survived the resize (drawers re-clamp):\n{s}"
    );

    // ---- 10. the Esc unwinding ladder ---------------------------------
    // Esc #1: the topmost modal closes (the app wired Esc to close it —
    // Modal's documented contract is close-explicitly).
    term.push_input(b"\x1b[27u");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let s = screen(&term);
    assert!(!s.contains("MODAL DETAIL"), "modal closed first:\n{s}");
    assert!(
        shell.inspector.is_open() && s.contains("Inspector"),
        "the drawer is still up under the closed modal:\n{s}"
    );
    // Esc #2: the modal drawer closes (built-in).
    term.push_input(b"\x1b[27u");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(!shell.inspector.is_open(), "inspector closed second");
    assert!(shell.nav.is_open(), "the passive nav is untouched by Esc");
    // 'g' toggles the passive nav away; Esc #3 lands on nothing.
    term.push_input(b"g");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert!(!shell.nav.is_open());
    // The passive nav vacated its band cleanly (F1 fix: an instant/
    // scrimless close must repaint the region the panel occupied).
    assert!(
        !screen(&term).contains("Navigate"),
        "closed nav left stale pixels:\n{}",
        screen(&term)
    );
    let before = screen(&term);
    term.push_input(b"\x1b[27u");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    assert_eq!(screen(&term), before, "Esc on nothing changes nothing");

    // ---- 11. state survived the whole journey -------------------------
    assert!(
        screen(&term).contains("hello"),
        "the form draft survived every overlay/theme/resize:\n{}",
        screen(&term)
    );
    assert_eq!(shell.draft.get_untracked(), "hello");
    let _ = shell.detail_modal; // (slot exercised through the button path)

    // ---- 12. final park: zero idle bytes (the charter line) -----------
    abstracttui::app::set_theme_by_id("abstract-dark");
    settle_on(&mut driver, &mut app, &mut term, &clock);
    let _ = term.take_bytes();
    assert_eq!(
        abstracttui::reactive::frame_tasks_pending(),
        0,
        "no animation survives the journey"
    );
    for _ in 0..4 {
        let turn = driver.turn(&mut app, &mut term).expect("final idle");
        assert!(turn.idle && !turn.rendered, "{turn:?}");
    }
    assert!(
        term.take_bytes().is_empty(),
        "the whole composed shell parks at zero idle bytes"
    );
}
