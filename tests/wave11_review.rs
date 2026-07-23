//! Wave-11 quality-audit pins (CODE seat): API-consistency additions
//! and the injected-clock arm/fire coherence regression.
//!
//! 1. CANONICAL BUILD UNIFORMITY — every element-only widget gained the
//!    documented canonical `.view(cx)` form (docs/api.md: "the
//!    canonical build is `.view(cx)`"); this drives all eleven through
//!    the REAL driver and asserts they render. If a future widget ships
//!    element-only again, extend this scene — the api.md sentence is a
//!    contract, not a wish.
//! 2. ARM-CLOCK COHERENCE — `after`/`interval` deadlines planted inside
//!    a turn measure from the turn's published clock (injected via
//!    `Driver::set_clock`), never a fresh `Instant::now()`. Before the
//!    fix, real time racing ahead of a test's injected clock (a loaded
//!    machine running the full suite) planted deadlines the injected
//!    timeline never reached — the wave-11 `wave_drawers::
//!    feed_page_inside_the_drawer_scrolls` flake: the Scroll width
//!    probe's `after(0)` never fired, the Feed stayed one item tall,
//!    and the wheel had nothing to scroll.
//!
//! OWNER: CODE (wave 11).

use std::cell::{Cell as StdCell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::reactive::Scope;
use abstracttui::render::{RichLine, RichText, Span};
use abstracttui::term::{Capabilities, EnterOptions, MouseMode};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::Element;
use abstracttui::widgets::{
    Badge, BarChart, CodeView, LineChart, Logo, MarkdownView, Progress, RichTextView, Separator,
    Sparkline, Spinner,
};

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

fn rig(
    size: Size,
    build: impl FnOnce(Scope) -> abstracttui::ui::View + 'static,
) -> (App, CaptureTerm) {
    let mut app = App::new(size);
    app.mount(build).expect("mount");
    let term = CaptureTerm::new(size);
    (app, term)
}

// ---------------------------------------------------------------------------
// 1. Canonical `.view(cx)` builds for the element-only widgets
// ---------------------------------------------------------------------------

#[test]
fn canonical_view_builds_render_for_every_element_only_widget() {
    let size = Size::new(80, 40);
    let (mut app, mut term) = rig(size, |cx| {
        let row = |h: i32| {
            LayoutStyle::default()
                .height(Dimension::Cells(h))
                .shrink(0.0)
        };
        Element::new()
            .style(LayoutStyle::column())
            .child(Badge::new("BADGE-11").layout(row(1)).view(cx))
            .child(Progress::new(0.5).layout(row(1)).view(cx))
            .child(Spinner::new().label("spin-11").layout(row(1)).view(cx))
            .child(
                Separator::horizontal()
                    .label("SEP-11")
                    .layout(row(1))
                    .view(cx),
            )
            .child(
                Sparkline::new(vec![0.0, 0.5, 1.0, 0.25])
                    .layout(row(2))
                    .view(cx),
            )
            .child(
                LineChart::new(vec![vec![0.0, 1.0, 0.5]])
                    .layout(row(4))
                    .view(cx),
            )
            .child(BarChart::new(vec![1.0, 3.0, 2.0]).layout(row(4)).view(cx))
            .child(
                RichTextView::new(RichText::from_lines(vec![RichLine::from_spans(vec![
                    Span::plain("RICH-11"),
                ])]))
                .layout(row(1))
                .view(cx),
            )
            .child(CodeView::new("let code_11 = true;").layout(row(3)).view(cx))
            .child(
                MarkdownView::new("# MD-11\n\nbody text")
                    .layout(row(4))
                    .view(cx),
            )
            .child(Logo::new().layout(row(8)).view(cx))
            .build()
    });
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");

    let screen = term.screen().to_text();
    for needle in [
        "BADGE-11", "spin-11", "SEP-11", "RICH-11", "code_11", "MD-11",
    ] {
        assert!(
            screen.contains(needle),
            "{needle} missing from the canonical-build scene:\n{screen}"
        );
    }
    // The chart family carries no text; the pin is that all three built
    // through `.view(cx)` and the frame rendered without panicking. The
    // bar chart at these values must paint SOMETHING non-blank in its
    // band (row-level presence, not glyph-exact — golden tests own
    // glyph fidelity).
    let lines: Vec<&str> = screen.lines().collect();
    let bar_band = lines[10..14].join("");
    assert!(
        bar_band.chars().any(|c| !c.is_whitespace()),
        "bar chart band is blank:\n{screen}"
    );
}

// ---------------------------------------------------------------------------
// 2. Timer arm/fire coherence under an injected clock
// ---------------------------------------------------------------------------

/// `after(ZERO)` armed INSIDE a turn (a key handler — the same phase-U
/// slot event handlers, effects and paint probes occupy) must come due
/// on the injected timeline even when real time has raced far ahead of
/// the injected clock. The 25 ms sleep is not a race window: it
/// guarantees real-now > injected-now at arm time by a margin the
/// injected clock's +1 ms steps never close — before the fix this test
/// fails deterministically, after it it passes deterministically.
///
/// (An arm from TEST code — outside any turn — deliberately keeps
/// real-time arming: the loop clock is turn-scoped, so bare-rig and
/// mount-time behavior is byte-identical to before the fix.)
#[test]
fn after_armed_inside_a_turn_fires_on_the_injected_timeline() {
    let size = Size::new(20, 4);
    let fired: Rc<StdCell<bool>> = Rc::new(StdCell::new(false));
    let armed: Rc<StdCell<bool>> = Rc::new(StdCell::new(false));

    let (mut app, mut term) = rig(size, {
        let fired = fired.clone();
        let armed = armed.clone();
        move |_cx| {
            Element::new()
                .style(LayoutStyle::column())
                .shortcut(
                    abstracttui::ui::KeyChord::plain(abstracttui::ui::Key::Char('a')),
                    move |_| {
                        armed.set(true);
                        let fired = fired.clone();
                        abstracttui::reactive::after(Duration::ZERO, move || fired.set(true));
                    },
                )
                .child(abstracttui::ui::text("clock rig"))
                .build()
        }
    });
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    // Real time races ahead of the injected clock (the loaded-machine
    // shape: the full workspace suite saturating every core).
    std::thread::sleep(Duration::from_millis(25));

    term.push_input(b"a");
    clock.set(clock.get() + Duration::from_millis(1));
    driver.turn(&mut app, &mut term).expect("arm turn");
    assert!(
        armed.get(),
        "the key handler armed the one-shot inside the turn"
    );

    clock.set(clock.get() + Duration::from_millis(1));
    driver.turn(&mut app, &mut term).expect("fire turn");
    assert!(
        fired.get(),
        "after(ZERO) armed inside a turn must fire on the next injected-clock \
         turn — an arm from real `Instant::now()` would sit ~25 ms in the \
         injected future and never come due"
    );
}

// ---------------------------------------------------------------------------
// 2b. Feature-coverage gap fills: theme switching and Viewport3D had no
//     test through the REAL Driver (the wave-11 coverage table).
// ---------------------------------------------------------------------------

/// A live theme switch retints a theme-tracking scene on the next turn
/// — pinned through the Driver + the 0.2.14 screenshot surface (two
/// features, one proof), and the app returns to a clean idle after.
#[test]
fn theme_switch_through_the_driver_retints_the_scene() {
    use abstracttui::widgets::Block;

    let size = Size::new(30, 8);
    let (mut app, mut term) = rig(size, |cx| {
        let theme = abstracttui::app::use_theme(cx);
        abstracttui::ui::dyn_view(LayoutStyle::default().grow(1.0), move || {
            let t = theme.get().tokens;
            Block::new()
                .title("T-BLOCK")
                .fill(t.surface)
                .layout(LayoutStyle::column().grow(1.0))
                .element(&t)
                .build()
        })
    });
    // Anchor the starting theme (test binaries share a process-global
    // current theme across tests on other threads' rigs — anchor, don't
    // assume).
    assert!(abstracttui::app::set_theme_by_id("abstract-dark"));
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");
    let before = driver.screenshot();

    assert!(
        abstracttui::app::set_theme_by_id("rose-pine"),
        "registry theme exists"
    );
    // The switch lands through the reactive graph: next turns rebuild
    // the dyn scene and repaint.
    for _ in 0..3 {
        driver.turn(&mut app, &mut term).expect("retint turn");
    }
    let after = driver.screenshot();
    assert!(
        before.to_ansi() != after.to_ansi(),
        "a theme switch must repaint the scene in new inks"
    );
    assert!(
        after.to_text().contains("T-BLOCK"),
        "content survives the retint:\n{}",
        after.to_text()
    );
    // Restore for neighbors sharing the process-global theme.
    assert!(abstracttui::app::set_theme_by_id("abstract-dark"));
}

/// Viewport3D through the real Driver: the widget renders a model into
/// its band, reports orbit deltas for drags and zoom steps for wheel —
/// the interaction slice no integration test drove before wave 11.
#[test]
fn viewport3d_renders_and_reports_orbit_and_zoom_through_the_driver() {
    use std::sync::Arc;

    use abstracttui::testing::glb_mutate;
    use abstracttui::three::Model;
    use abstracttui::widgets::Viewport3D;

    let size = Size::new(40, 12);
    let orbits: Rc<StdCell<(f32, f32)>> = Rc::new(StdCell::new((0.0, 0.0)));
    let zooms: Rc<StdCell<f32>> = Rc::new(StdCell::new(0.0));

    let model = Arc::new(Model::load(&glb_mutate::minimal_glb()).expect("minimal GLB loads"));
    let (mut app, mut term) = rig(size, {
        let orbits = orbits.clone();
        let zooms = zooms.clone();
        move |_cx| {
            let t = abstracttui::theme::default_theme().tokens;
            Element::new()
                .style(LayoutStyle::column().grow(1.0))
                .child(
                    Viewport3D::new(model)
                        .orbit(0.6, 0.35, 1.0)
                        .on_orbit(move |dy, dp| {
                            let (y, p) = orbits.get();
                            orbits.set((y + dy, p + dp));
                        })
                        .on_zoom(move |steps| zooms.set(zooms.get() + steps))
                        .element(&t)
                        .build(),
                )
                .build()
        }
    });
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");

    // The triangle rasterizes into the band: the half-block mosaic
    // emits GLYPH patches (▀-family) for lit cells. Glyph presence is
    // the honest check — every cell carries the root surface's theme
    // background, so a bg-colored check is vacuously true on a blank
    // screen (this assertion originally passed against an INVISIBLE
    // widget that way — the zero-height default this test then caught).
    let shot = driver.screenshot();
    let mut inked = false;
    for y in 0..size.h {
        for x in 0..size.w {
            let cell = shot.cell(x, y).expect("in bounds");
            if cell.text() != " " && !cell.text().is_empty() {
                inked = true;
            }
        }
    }
    assert!(inked, "the model rendered nothing:\n{}", shot.to_text());

    // Left-drag across the widget: Down at (6,6), drag right 4 cells,
    // release — the widget reports a positive yaw delta.
    term.push_input(b"\x1b[<0;6;6M");
    driver.turn(&mut app, &mut term).expect("press");
    term.push_input(b"\x1b[<32;10;6M");
    driver.turn(&mut app, &mut term).expect("drag");
    term.push_input(b"\x1b[<0;10;6m");
    driver.turn(&mut app, &mut term).expect("release");
    assert!(
        orbits.get().0 < 0.0,
        "the grab-the-object convention: a rightward drag orbits toward the \
         model's left — a NEGATIVE yaw delta (got {:?})",
        orbits.get()
    );

    // Wheel up inside the widget: one zoom step reported.
    term.push_input(b"\x1b[<64;6;6M");
    driver.turn(&mut app, &mut term).expect("wheel");
    assert!(
        zooms.get() != 0.0,
        "wheel over the viewport reports zoom steps"
    );
}

// ---------------------------------------------------------------------------
// 3. Drawer close releases the input trap (the dead-keys window)
// ---------------------------------------------------------------------------

/// A key pressed DURING a modal drawer's closing slide must reach the
/// app — the exit flight is cosmetics, not an input owner. Before the
/// fix the panel's modal tree kept swallowing every key until the
/// slide landed (~160 ms): the pty smoke's `i`, Esc, `q` script hung
/// because `q` arrived ~150 ms into the close and died in the dying
/// panel (found live by `live_smoke::live_drawers`).
#[test]
fn keys_reach_the_app_the_instant_a_modal_drawer_begins_closing() {
    use abstracttui::app::drawer::{Drawer, DrawerEdge};

    let size = Size::new(40, 10);
    let slot: ScopeSlot = Rc::new(RefCell::new(None));
    let hits: Rc<StdCell<u32>> = Rc::new(StdCell::new(0));

    let s = slot.clone();
    let (mut app, mut term) = rig(size, {
        let hits = hits.clone();
        move |cx| {
            *s.borrow_mut() = Some(cx);
            Element::new()
                .style(LayoutStyle::column())
                .shortcut(
                    abstracttui::ui::KeyChord::plain(abstracttui::ui::Key::Char('x')),
                    move |_| hits.set(hits.get() + 1),
                )
                .child(abstracttui::ui::text("main surface"))
                .build()
        }
    });
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = slot.borrow().expect("scope");
    let handle = Drawer::new(DrawerEdge::Right)
        .motion(Duration::from_millis(160))
        .overlays(&overlays)
        .install(cx, |_| abstracttui::ui::text("panel"));
    handle.open();
    for _ in 0..12 {
        clock.set(clock.get() + Duration::from_millis(16));
        driver.turn(&mut app, &mut term).expect("open turn");
    }
    // Trapped while open: `x` never reaches the app shortcut.
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("trapped key");
    assert_eq!(hits.get(), 0, "modal drawer owns keys while open");

    // Begin the close (Esc inside the panel is the same path — this
    // exercises the verb) and press `x` MID-FLIGHT.
    handle.close();
    clock.set(clock.get() + Duration::from_millis(16));
    driver.turn(&mut app, &mut term).expect("close turn 1");
    assert!(
        handle.layer().is_some(),
        "the close is still in flight (the window under test exists)"
    );
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("mid-close key");
    assert_eq!(
        hits.get(),
        1,
        "a key during the closing slide belongs to the APP, not the dying panel"
    );

    // Let the slide land; the drawer tears down clean.
    for _ in 0..20 {
        clock.set(clock.get() + Duration::from_millis(16));
        driver.turn(&mut app, &mut term).expect("landing turn");
    }
    assert!(handle.layer().is_none(), "close landed");
}

/// Reopening a modal drawer WHILE it slides closed re-arms the trap:
/// the reversal is a fresh open from the user's view, and an untrapped
/// "open" modal would leak every key to the app behind the scrim.
#[test]
fn reopen_mid_close_restores_the_modal_trap() {
    use abstracttui::app::drawer::{Drawer, DrawerEdge};

    let size = Size::new(40, 10);
    let slot: ScopeSlot = Rc::new(RefCell::new(None));
    let hits: Rc<StdCell<u32>> = Rc::new(StdCell::new(0));

    let s = slot.clone();
    let (mut app, mut term) = rig(size, {
        let hits = hits.clone();
        move |cx| {
            *s.borrow_mut() = Some(cx);
            Element::new()
                .style(LayoutStyle::column())
                .shortcut(
                    abstracttui::ui::KeyChord::plain(abstracttui::ui::Key::Char('x')),
                    move |_| hits.set(hits.get() + 1),
                )
                .child(abstracttui::ui::text("main surface"))
                .build()
        }
    });
    let overlays = app.overlays();
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = slot.borrow().expect("scope");
    let handle = Drawer::new(DrawerEdge::Right)
        .motion(Duration::from_millis(160))
        .overlays(&overlays)
        .install(cx, |_| abstracttui::ui::text("panel"));
    handle.open();
    for _ in 0..12 {
        clock.set(clock.get() + Duration::from_millis(16));
        driver.turn(&mut app, &mut term).expect("open turn");
    }

    handle.close();
    clock.set(clock.get() + Duration::from_millis(16));
    driver.turn(&mut app, &mut term).expect("close turn");
    handle.open(); // reverse the flight — same mount continues
    clock.set(clock.get() + Duration::from_millis(16));
    driver.turn(&mut app, &mut term).expect("reopen turn");

    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("key after reopen");
    assert_eq!(
        hits.get(),
        0,
        "a reopened modal drawer owns the keyboard again (trap re-armed)"
    );
    assert!(handle.is_open(), "the drawer heads open after the reversal");
}

/// The same coherence for `interval`'s FIRST deadline (later ones
/// already re-armed from the fire pass's clock — the pre-existing
/// `timer_fire_now` rule; the first arm was the hole).
#[test]
fn interval_first_deadline_measures_from_the_injected_clock() {
    let size = Size::new(20, 4);
    let slot: ScopeSlot = Rc::new(RefCell::new(None));
    let ticks: Rc<StdCell<u32>> = Rc::new(StdCell::new(0));
    let started: Rc<StdCell<bool>> = Rc::new(StdCell::new(false));

    let s = slot.clone();
    let (mut app, mut term) = rig(size, {
        let ticks = ticks.clone();
        let started = started.clone();
        move |cx| {
            *s.borrow_mut() = Some(cx);
            Element::new()
                .style(LayoutStyle::column())
                .shortcut(
                    abstracttui::ui::KeyChord::plain(abstracttui::ui::Key::Char('i')),
                    move |_| {
                        started.set(true);
                        let ticks = ticks.clone();
                        // The handle drops unrecorded deliberately: the
                        // installing scope owns cleanup.
                        let _ = abstracttui::reactive::interval(
                            cx,
                            Duration::from_millis(10),
                            move || ticks.set(ticks.get() + 1),
                        );
                    },
                )
                .child(abstracttui::ui::text("interval rig"))
                .build()
        }
    });
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    let clock: Rc<StdCell<Instant>> = Rc::new(StdCell::new(Instant::now()));
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    driver.turn(&mut app, &mut term).expect("frame 1");

    std::thread::sleep(Duration::from_millis(25));
    term.push_input(b"i");
    driver.turn(&mut app, &mut term).expect("arm turn");
    assert!(started.get(), "interval installed inside the turn");

    // +10 ms on the injected clock = exactly one period.
    clock.set(clock.get() + Duration::from_millis(10));
    driver.turn(&mut app, &mut term).expect("first period");
    assert_eq!(
        ticks.get(),
        1,
        "the interval's first deadline must be injected-now + period, not \
         real-now + period"
    );
}
