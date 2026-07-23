//! Wave-8 cycle-2 cross-review: DRAWER attacks TABS' PageHost (0545)
//! through the real Driver/CaptureTerm pipeline. Companion findings:
//! reviews/wave8/drawer-on-tabs.md.
//!
//! Charter: badge-heavy idle cost, live streaming across switches, 20
//! tabs on 60 cols, controlled writes mid-press, unknown-id healing,
//! action/chord routing collisions (the shell's drawer letters), the
//! duplicate-id and panicking-builder contracts, hit-test-vs-pixels
//! staleness, and the nested compositions (PageHost inside a modal
//! Drawer, PageHost inside a Modal).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerSize};
use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::ui::text;
use abstracttui::widgets::{Feed, FeedState, PageHost};

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
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
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

use abstracttui::testing::CaptureTerm;

// ---------------------------------------------------------------------------
// Badge cost: a parked badge-heavy bar is free; one badge tick repaints
// the bar only (no page bytes, no page remount).
// ---------------------------------------------------------------------------

#[test]
fn parked_badge_heavy_bar_idles_at_zero_and_one_tick_repaints_bar_only() {
    let size = Size::new(80, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let probe: Rc<RefCell<Option<Signal<u32>>>> = Rc::new(RefCell::new(None));
    let probe_in = probe.clone();
    let builds: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let b = builds.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        // Nine live badge signals — the badge-heavy bar.
        let counts: Vec<Signal<u32>> = (0..9).map(|_| cx.signal(0u32)).collect();
        *probe_in.borrow_mut() = Some(counts[6]);
        let mut host = PageHost::new();
        for (i, count) in counts.iter().enumerate().take(9) {
            let id = format!("p{i}");
            let count = *count;
            let b = b.clone();
            let active_page = i == 0;
            host = host
                .page(id.clone(), format!("P{i}"), move |_| {
                    if active_page {
                        *b.borrow_mut() += 1;
                    }
                    text(format!("BODY {i}"))
                })
                .badge(&id, move || {
                    let n = count.get();
                    (n > 0).then(|| n.to_string())
                });
        }
        host.element(cx, &t).build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*builds.borrow(), 1);
    let _ = term.take_bytes();

    // Parked: nine tracked badge getters cost NOTHING while quiet.
    for _ in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("idle turn");
        assert!(turn.idle && !turn.rendered, "{turn:?}");
    }
    assert!(term.take_bytes().is_empty(), "parked badge bar wrote bytes");

    // One badge tick: the bar repaints, the page region does not, the
    // page never remounts.
    probe.borrow().expect("probe").set(7);
    settle(&mut driver, &mut app, &mut term);
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        bytes.contains('7'),
        "badge digit reached the wire: {bytes:?}"
    );
    assert!(
        !bytes.contains("BODY"),
        "a badge tick must never re-emit page content: {bytes:?}"
    );
    assert_eq!(*builds.borrow(), 1, "badge tick remounted the page");

    // And it parks again at zero.
    let turn = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(turn.idle && term.take_bytes().is_empty(), "{turn:?}");
}

// ---------------------------------------------------------------------------
// The state recipe under a LIVE stream: an app-owned FeedState keeps
// ingesting while its page is hidden (at zero render cost), and the
// remounted page shows the whole stream.
// ---------------------------------------------------------------------------

#[test]
fn live_stream_survives_switch_away_and_back_via_app_owned_state() {
    let size = Size::new(44, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let feed_probe: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let fp = feed_probe.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        // App-owned: the stream's data outlives every page generation.
        let feed = FeedState::new(cx);
        *fp.borrow_mut() = Some(feed.clone());
        let feed_for_page = feed.clone();
        PageHost::new()
            .page("live", "Live", move |gcx| {
                let follow = gcx.signal(true);
                Scroll::new(Feed::new(&feed_for_page).gap(0).view(gcx))
                    .follow_tail(follow)
                    .view(gcx)
            })
            .page("other", "Other", |_| text("BODY OTHER"))
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let feed = feed_probe.borrow().clone().expect("probe");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    feed.push_stream("run");
    feed.stream_append("run", "alpha ");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("alpha"), "{}", screen(&term));

    // Switch away; the page's view (and its follow signal) die.
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY OTHER"));
    let _ = term.take_bytes();

    // Stream INTO THE HIDDEN page's state: data lands, pixels do not —
    // hidden ingestion must not render anything.
    feed.stream_append("run", "beta ");
    feed.stream_append("run", "gamma");
    let turn = driver.turn(&mut app, &mut term).expect("hidden turn");
    assert!(
        !turn.rendered,
        "hidden-page stream writes must not render: {turn:?}"
    );
    assert!(
        term.take_bytes().is_empty(),
        "hidden-page stream writes emitted bytes"
    );

    // Back: the fresh mount windows the WHOLE stream (follow-tail).
    term.push_input(b"\x1b[5;5~");
    settle(&mut driver, &mut app, &mut term);
    feed.stream_finish("run");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(
        s.contains("gamma"),
        "the mid-hidden chunk is there after remount:\n{s}"
    );
}

// ---------------------------------------------------------------------------
// 20 tabs on 60 cols: the sticky window keeps the ACTIVE tab visible
// through a full forward and backward walk; the indicators stay honest
// at both ends.
// ---------------------------------------------------------------------------

#[test]
fn twenty_tabs_on_sixty_cols_keep_the_active_tab_visible_and_indicators_honest() {
    let size = Size::new(60, 6);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let mut host = PageHost::new();
        for i in 1..=20 {
            host = host.page(format!("p{i:02}"), format!("T{i:02}"), move |_| {
                text(format!("BODY {i:02}"))
            });
        }
        host.element(cx, &t).build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let bar = screen(&term).lines().next().unwrap_or("").to_string();
    assert!(bar.contains("T01"), "start shows the first tab: {bar}");
    assert!(
        !bar.contains('‹'),
        "nothing hidden left at the start: {bar}"
    );
    assert!(bar.contains('›'), "hidden tabs on the right: {bar}");

    // Forward walk: the active tab must be in the window every step.
    for i in 2..=20 {
        term.push_input(b"\x1b[6;5~");
        settle(&mut driver, &mut app, &mut term);
        let s = screen(&term);
        let bar = s.lines().next().unwrap_or("");
        assert!(
            bar.contains(&format!("T{i:02}")),
            "active tab T{i:02} left the window: {bar}"
        );
        assert!(
            s.contains(&format!("BODY {i:02}")),
            "page {i:02} not mounted"
        );
    }
    let bar = screen(&term).lines().next().unwrap_or("").to_string();
    assert!(
        bar.contains('‹'),
        "hidden tabs on the left at the end: {bar}"
    );
    assert!(!bar.contains('›'), "nothing hidden right at the end: {bar}");

    // Backward walk home.
    for i in (1..=19).rev() {
        term.push_input(b"\x1b[5;5~");
        settle(&mut driver, &mut app, &mut term);
        let s = screen(&term);
        let bar = s.lines().next().unwrap_or("");
        assert!(
            bar.contains(&format!("T{i:02}")),
            "active tab T{i:02} left the window on the way back: {bar}"
        );
    }
    let bar = screen(&term).lines().next().unwrap_or("").to_string();
    assert!(!bar.contains('‹'), "back home: nothing hidden left: {bar}");
}

// ---------------------------------------------------------------------------
// Controlled mode: an external write landing between press and release
// sticks (the release is inert — no snap-back to the pressed tab).
// ---------------------------------------------------------------------------

#[test]
fn controlled_external_write_between_press_and_release_sticks() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let probe: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
    let probe_in = probe.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let page = cx.signal("one".to_string());
        *probe_in.borrow_mut() = Some(page);
        PageHost::new()
            .page("one", "One", |_| text("BODY ONE"))
            .page("two", "Two", |_| text("BODY TWO"))
            .page("three", "Three", |_| text("BODY THREE"))
            .active(page)
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let page = probe.borrow().expect("probe");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Press (NO release) on " Two " (segs: One 0..5, Two 6..11 -> col 8).
    term.push_input(b"\x1b[<0;8;1M");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY TWO"));

    // External write while the button is still down.
    page.set("three".to_string());
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY THREE"));

    // Release over the pressed tab: inert — the external win stands.
    term.push_input(b"\x1b[<0;8;1m");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(page.get_untracked(), "three");
    assert!(screen(&term).contains("BODY THREE"));
}

// ---------------------------------------------------------------------------
// Unknown controlled ids: fold to the first page for RENDERING, and the
// next host-driven step writes a REAL id back (the signal heals).
// ---------------------------------------------------------------------------

#[test]
fn unknown_id_mid_session_folds_to_first_and_the_next_chord_heals_the_signal() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let probe: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
    let probe_in = probe.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let page = cx.signal("b".to_string());
        *probe_in.borrow_mut() = Some(page);
        PageHost::new()
            .page("a", "Aaa", |_| text("BODY A"))
            .page("b", "Bbb", |_| text("BODY B"))
            .active(page)
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let page = probe.borrow().expect("probe");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY B"));

    page.set("ghost".to_string());
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("BODY A"),
        "unknown id renders the first page (the documented fold)"
    );
    assert_eq!(
        page.get_untracked(),
        "ghost",
        "the fold never edits the signal"
    );

    // The next host step resolves from the folded index and writes a
    // REAL id — the signal heals through normal navigation.
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(page.get_untracked(), "b", "navigation healed the ghost id");
    assert!(screen(&term).contains("BODY B"));
}

// ---------------------------------------------------------------------------
// Routing collision (the shell composition): drawer toggle letters ride
// the GLOBAL action registry; page digits ride the host shortcut table.
// Both stay live without stealing from each other — and a modal drawer
// owns the digits while open.
// ---------------------------------------------------------------------------

#[test]
fn drawer_toggle_letters_and_page_digits_route_without_collision() {
    let size = Size::new(60, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let drawer_slot: Rc<RefCell<Option<abstracttui::app::drawer::DrawerHandle>>> =
        Rc::new(RefCell::new(None));
    let slot = drawer_slot.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let handle = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(24))
            .title("Inspector")
            .motion(std::time::Duration::ZERO)
            .install(cx, |_| text("DRAWER PAGE"));
        *slot.borrow_mut() = Some(handle);
        PageHost::new()
            .page("one", "One", |_| text("BODY ONE"))
            .page("two", "Two", |_| text("BODY TWO"))
            .number_jump(true)
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let handle = drawer_slot.borrow().clone().expect("drawer installed");
    {
        let handle = handle.clone();
        app.actions().register(
            "drawer.inspector",
            Some(KeyChord::plain(Key::Char('i'))),
            move || handle.toggle(),
        );
    }
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Digits jump pages (host shortcut table).
    term.push_input(b"2");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY TWO"));

    // 'i' falls through the host to the GLOBAL action: drawer opens.
    term.push_input(b"i");
    settle(&mut driver, &mut app, &mut term);
    assert!(handle.is_open(), "the letter action opened the drawer");
    assert!(screen(&term).contains("DRAWER PAGE"));

    // While the MODAL drawer is open, digits belong to it (unconsumed,
    // and no digit ACTION exists) — the page must not switch.
    term.push_input(b"1");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("BODY TWO"),
        "a modal drawer owns digits; the page switched:\n{}",
        screen(&term)
    );

    // 'i' is still a global action even over a modal drawer (unconsumed
    // keys fall through to the registry) — the toggle closes it. This
    // is the driver's deliberate actions-last seam, pinned here.
    term.push_input(b"i");
    settle(&mut driver, &mut app, &mut term);
    assert!(!handle.is_open(), "the toggle letter closed the drawer");

    // Digits work again after the drawer closed.
    term.push_input(b"1");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY ONE"));
}

// ---------------------------------------------------------------------------
// Contracts: duplicate ids are a DEBUG panic; a panicking page builder
// fails loud without wedging the thread's reactive runtime.
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "duplicate page id")]
fn duplicate_page_ids_panic_in_debug_builds() {
    let _ = PageHost::new()
        .page("same", "First", |_| text("A"))
        .page("same", "Second", |_| text("B"));
}

#[test]
fn panicking_page_builder_fails_loud_without_wedging_the_runtime() {
    let size = Size::new(30, 6);
    // The panic unwinds out of the mount — loud, uncontained by design
    // (the engine's panic posture: restore the terminal, die).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut app = App::new(size);
        app.mount(move |cx| {
            let t = abstracttui::theme::default_theme().tokens;
            PageHost::new()
                .page("boom", "Boom", |_| -> View { panic!("builder exploded") })
                .element(cx, &t)
                .build()
        })
        .expect("mount");
    }));
    assert!(result.is_err(), "the builder panic must propagate");

    // The thread's reactive runtime survives: a fresh app on the SAME
    // thread mounts, renders and idles normally.
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        PageHost::new()
            .page("ok", "Ok", |_| text("BODY OK"))
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("BODY OK"),
        "a builder panic must not poison later apps on this thread:\n{}",
        screen(&term)
    );
}

// ---------------------------------------------------------------------------
// Surface 2 (pinned cost): a chord pressed while the BAR is focused
// re-anchors focus to the host root — bar arrows go dead until the user
// clicks/Tabs back. Their argued rule ("one predictable re-anchor")
// stands; this pin makes its price visible.
// ---------------------------------------------------------------------------

#[test]
fn chord_while_bar_focused_moves_focus_off_the_bar_pinned_tradeoff() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        PageHost::new()
            .page("one", "One", |_| text("BODY ONE"))
            .page("two", "Two", |_| text("BODY TWO"))
            .page("three", "Three", |_| text("BODY THREE"))
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Click the active tab: focuses the bar without switching.
    term.push_input(&sgr_click(3, 1));
    settle(&mut driver, &mut app, &mut term);
    // Arrows cycle while the bar is focused.
    term.push_input(b"\x1b[C"); // Right
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY TWO"));

    // A chord switch re-anchors focus on the host ROOT (documented).
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("BODY THREE"));

    // The pinned price: the bar lost focus, so arrows are dead now.
    term.push_input(b"\x1b[C");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("BODY THREE"),
        "pinned: arrows are dead after a chord until focus returns:\n{}",
        screen(&term)
    );
}

// ---------------------------------------------------------------------------
// Surface 1 (the escape hatch): empty chord sets disarm the capture
// interceptor entirely — the reserved keys go back to the content
// (a terminal-emulator-class page's opt-out today).
// ---------------------------------------------------------------------------

#[test]
fn empty_chord_sets_hand_the_reserved_keys_back_to_the_content() {
    let size = Size::new(44, 10);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        PageHost::new()
            .page("feed", "Feed", move |gcx| {
                let mut col = Element::new().style(LayoutStyle::column());
                for i in 0..40 {
                    col = col.child(text(format!("line{i:02}")));
                }
                Scroll::new(col.build()).view(gcx)
            })
            .page("other", "Other", |_| text("BODY OTHER"))
            .chords(&[], &[]) // the reservation OFF switch
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("line00"));

    // Focus the scroll (click into the page region), then Ctrl+PgDn:
    // with the reservation off, the modifier-blind Scroll consumes it —
    // the content scrolls and the page does NOT switch.
    term.push_input(&sgr_click(5, 4));
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(
        !s.contains("BODY OTHER"),
        "reservation off: the chord must not switch pages:\n{s}"
    );
    assert!(
        !s.contains("line00 ") && s.contains("line"),
        "the content consumed the chord and scrolled:\n{s}"
    );
}

// ---------------------------------------------------------------------------
// Hit-test truth: a click must act on what the user SEES. A badge that
// widens in the same batch as the click used to shift the hit geometry
// under the pointer (model-new vs pixels-old) — the press now resolves
// against the last-DRAWN plan.
// ---------------------------------------------------------------------------

#[test]
fn click_resolves_against_the_drawn_bar_not_a_newer_undrawn_plan() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let probe: Rc<RefCell<Option<Signal<Option<String>>>>> = Rc::new(RefCell::new(None));
    let probe_in = probe.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let badge = cx.signal(None::<String>);
        *probe_in.borrow_mut() = Some(badge);
        PageHost::new()
            .page("one", "One", |_| text("BODY ONE"))
            .page("two", "Two", |_| text("BODY TWO"))
            .page("three", "Three", |_| text("BODY THREE"))
            .badge("one", move || badge.get())
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let badge = probe.borrow().expect("probe");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    // Drawn geometry: " One "(0..5) " Two "(6..11) — the user sees Two
    // under column 8 (0-based).
    assert!(screen(&term).lines().next().unwrap_or("").contains("Two"));

    // The badge widens tab One BEFORE the next draw (same-batch model
    // change), shifting Two's segment to x=12 in the NEW plan.
    badge.set(Some("88888".to_string()));
    // Click where the user still SEES "Two" (pixels are pre-badge).
    term.push_input(&sgr_click(9, 1));
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("BODY TWO"),
        "the click must hit the tab the user saw at that column:\n{}",
        screen(&term)
    );
}

// ---------------------------------------------------------------------------
// Composition: a PageHost INSIDE a modal Drawer (nested navigation) and
// inside a Modal. The inner host owns the chords while the overlay owns
// input; the outer host resumes when it closes. Same default chords on
// both hosts — the fight is resolved by overlay input ownership.
// ---------------------------------------------------------------------------

#[test]
fn page_host_inside_a_modal_drawer_owns_chords_while_open() {
    let size = Size::new(60, 12);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let drawer_slot: Rc<RefCell<Option<abstracttui::app::drawer::DrawerHandle>>> =
        Rc::new(RefCell::new(None));
    let slot = drawer_slot.clone();
    let outer_probe: Rc<RefCell<Option<Signal<String>>>> = Rc::new(RefCell::new(None));
    let op = outer_probe.clone();
    app.mount(move |cx| {
        let t = abstracttui::theme::default_theme().tokens;
        let outer = cx.signal("oa".to_string());
        *op.borrow_mut() = Some(outer);
        let handle = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Percent(0.6))
            .title("Nested")
            .motion(std::time::Duration::ZERO)
            .install(cx, move |mount| {
                let t = abstracttui::theme::default_theme().tokens;
                // The INNER host, same default chords as the outer one.
                PageHost::new()
                    .page("ia", "InA", |_| text("INNER ALPHA"))
                    .page("ib", "InB", |_| text("INNER BETA"))
                    .element(mount, &t)
                    .build()
            });
        *slot.borrow_mut() = Some(handle);
        PageHost::new()
            .page("oa", "OutA", |_| text("OUTER ALPHA"))
            .page("ob", "OutB", |_| text("OUTER BETA"))
            .active(outer)
            .element(cx, &t)
            .build()
    })
    .expect("mount");
    let handle = drawer_slot.borrow().clone().expect("drawer");
    let outer = outer_probe.borrow().expect("outer probe");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("OUTER ALPHA"));

    // Open the drawer: the modal tree focus-inits on the inner bar, so
    // the inner host's chords are live from frame one.
    handle.open();
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("INNER ALPHA"));

    term.push_input(b"\x1b[6;5~"); // Ctrl+PgDn
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("INNER BETA"),
        "the INNER host switched:\n{}",
        screen(&term)
    );
    assert_eq!(
        outer.get_untracked(),
        "oa",
        "the outer host must not see chords while the modal drawer is open"
    );

    // Esc closes the drawer (bubbles past the inner host untouched);
    // the outer host resumes chord duty (root-mounted, no focus needed).
    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert!(!handle.is_open(), "Esc closed the drawer");
    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        outer.get_untracked(),
        "ob",
        "the outer host resumed after the drawer closed"
    );
}

#[test]
fn page_host_inside_a_modal_dialog_switches_pages() {
    let size = Size::new(50, 12);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let overlays = app.overlays();
    let slot: Rc<RefCell<Option<Scope>>> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    app.mount(move |cx| {
        *s.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("UNDERNEATH"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    let cx = slot.borrow().expect("scope");
    let modal = Modal::open(&overlays, cx, size, Size::new(40, 8), |mcx| {
        let t = abstracttui::theme::default_theme().tokens;
        PageHost::new()
            .page("ma", "MoA", |_| text("MODAL ALPHA"))
            .page("mb", "MoB", |_| text("MODAL BETA"))
            .element(mcx, &t)
            .build()
    });
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("MODAL ALPHA"));

    term.push_input(b"\x1b[6;5~");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("MODAL BETA"),
        "the host inside the modal switched:\n{}",
        screen(&term)
    );
    modal.close();
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("UNDERNEATH"));
}
