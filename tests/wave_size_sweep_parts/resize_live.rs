//! Axis (e) — LIVE resize across breakpoints with everything open:
//! pinned chrome + PageHost + an open (modal-focus) drawer + an open
//! oversized modal, resized 100x24 -> 60x16 -> 40x12 -> back up.
//!
//! The damage-correctness oracle: at every step the incumbent's screen
//! (incremental, damage-driven repaints applied over a GARBAGE-
//! prefilled referee) must equal a FRESH driver's first full paint of
//! the same scene at the same size, cell for cell. Any stale band,
//! missed re-emit, or un-reclamped overlay names itself as a diff.
//! The composed-frame screenshot must also equal the bytes-as-applied
//! referee (the screenshot roundtrip from 0.2.14 — presenter honesty
//! per size).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerHandle, DrawerSize};
use abstracttui::app::{App, Driver, Modal};
use abstracttui::base::Size;
use abstracttui::layout::{Direction, Style as LayoutStyle};
use abstracttui::reactive::Scope;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{text, Element};
use abstracttui::widgets::{PageHost, Scroll};

use crate::harness::{config, drive_to_idle, heavy_page, screen_diff};

/// One fully-loaded world: pinned chrome, a PageHost whose active page
/// scrolls heavy content, a right drawer, an oversized modal.
struct World {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    vt: VtScreen,
    // Held open for the scene's lifetime.
    _drawer: DrawerHandle,
    _modal: Modal,
}

fn build_world(size: Size) -> World {
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let ss = scope_slot.clone();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .child(text("HEADER — console"))
                    .build(),
            )
            .child(
                PageHost::new()
                    .page("main", "Main", |gcx| Scroll::new(heavy_page(300)).view(gcx))
                    .page("logs", "Logs", |_| text("PAGE-LOGS"))
                    .layout(LayoutStyle::default().grow(1.0))
                    .view(cx),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .child(text("FOOTER — status"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let scope = scope_slot.borrow().expect("scope");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let drawer = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Percent(0.42))
        .motion(Duration::ZERO)
        .title("Details")
        .overlays(&app.overlays())
        .install(scope, |_| text("DRAWER-BODY details pane"));
    drawer.open();
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    let modal = Modal::open(
        &app.overlays(),
        scope,
        app.viewport(),
        Size::new(78, 27),
        |mcx| {
            let body: String = (0..60)
                .map(|i| format!("mrow-{i:03} payload"))
                .collect::<Vec<_>>()
                .join("\n");
            Element::new()
                .style(LayoutStyle {
                    direction: Direction::Column,
                    ..LayoutStyle::fill()
                })
                .child(
                    Element::new()
                        .style(LayoutStyle::line(1))
                        .child(text("MODAL-TITLE"))
                        .build(),
                )
                .child(Scroll::new(text(body)).view(mcx))
                .child(
                    Element::new()
                        .style(LayoutStyle::line(3))
                        .child(text("[ OK ]  [ Cancel ]"))
                        .build(),
                )
                .build()
        },
    );
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    World {
        app,
        term,
        driver,
        vt,
        _drawer: drawer,
        _modal: modal,
    }
}

/// Assert the incumbent equals a fresh-paint oracle of the same scene
/// at `size`, and that the composed-frame screenshot equals the
/// bytes-as-applied referee.
///
/// The oracle builds ON A FRESH THREAD: the engine's per-thread
/// singletons (drawer one-per-edge registry, viewport/theme signals)
/// make two live worlds on one thread interfere — the oracle's drawer
/// open would `Replaced`-close the incumbent's drawer through the
/// shared registry (found live in this wave; the 12-cell "stale band"
/// it produced was the incumbent's vanished scrim, not a damage bug).
fn assert_step(world: &World, size: Size, step: &str) {
    // Screenshot roundtrip: composed frame == bytes-as-applied.
    assert_eq!(
        world.driver.screenshot().to_text(),
        world.vt.to_text(),
        "{step}: composed frame diverged from the bytes the terminal saw"
    );
    // Fresh-paint oracle.
    let oracle_vt = std::thread::spawn(move || build_world(size).vt)
        .join()
        .expect("oracle thread");
    let diff = screen_diff(&world.vt, &oracle_vt);
    assert!(
        diff.is_empty(),
        "{step}: {} stale/missing cells vs fresh-paint oracle at {size:?}\n\
         first mismatches:\n  {}\n--- incumbent ---\n{}\n--- oracle ---\n{}",
        diff.len(),
        diff.join("\n  "),
        world.vt.to_text(),
        oracle_vt.to_text()
    );
}

/// The breakpoint ladder, everything open, both directions — every
/// step byte-equal to a fresh paint (stale bands impossible), chrome
/// and overlays re-clamped, no panics anywhere.
#[test]
fn resize_ladder_with_everything_open_matches_fresh_paint() {
    let start = Size::new(100, 24);
    let mut w = build_world(start);
    assert_step(&w, start, "initial 100x24");
    assert!(
        w.vt.to_text().contains("MODAL-TITLE"),
        "modal open at start:\n{}",
        w.vt.to_text()
    );

    let ladder = [
        Size::new(60, 16),
        Size::new(40, 12),
        Size::new(60, 16),
        Size::new(100, 24),
    ];
    for (i, &size) in ladder.iter().enumerate() {
        w.term.push_resize(size);
        drive_to_idle(&mut w.driver, &mut w.app, &mut w.term, &mut w.vt);
        assert_step(&w, size, &format!("step {i} -> {size:?}"));
    }
}

/// Mid-cascade shrink: a second resize lands while the first is still
/// settling (one turn in) — the coalesced end state must still equal a
/// fresh paint at the LAST size.
#[test]
fn back_to_back_resizes_settle_to_fresh_paint_truth() {
    let start = Size::new(100, 24);
    let mut w = build_world(start);

    // Two resizes in one burst: only the final geometry matters.
    w.term.push_resize(Size::new(60, 16));
    w.term.push_resize(Size::new(40, 12));
    drive_to_idle(&mut w.driver, &mut w.app, &mut w.term, &mut w.vt);
    assert_step(&w, Size::new(40, 12), "burst shrink 100->60->40");

    // And straight back up in one burst.
    w.term.push_resize(Size::new(60, 16));
    w.term.push_resize(Size::new(100, 24));
    drive_to_idle(&mut w.driver, &mut w.app, &mut w.term, &mut w.vt);
    assert_step(&w, Size::new(100, 24), "burst grow 40->60->100");
}
