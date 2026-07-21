//! Cycle-2 acceptance: the whole charter bet, end to end, headless.
//!
//! A real `App` (reactive counter component, keymap, `Dyn` text region)
//! runs through the real `Driver` pipeline (batched dispatch -> layout ->
//! damaged-region draw -> flatten -> diff -> presenter) against REDTEAM's
//! `CaptureTerm`; assertions read the produced screen through the VT
//! model AND the raw bytes between frames:
//!
//! - the screen shows the right text after every key,
//! - a counter keypress repaints ONLY the counter region (the static
//!   header's bytes never re-emit),
//! - an idle turn emits ZERO bytes.

use crate::base::Size;
use crate::layout::Style;
use crate::term::{Capabilities, EnterOptions, MouseMode};
use crate::testing::CaptureTerm;
use crate::ui::{dyn_view, text, Element, Key, KeyChord};

use super::driver::{Driver, RunConfig};
use super::popups::Toast;
use super::App;

const W: i32 = 24;
const H: i32 = 4;

/// A fixed capability set so the host environment can never leak into
/// byte-level assertions (truecolor, no sync brackets for byte clarity).
fn test_caps() -> Capabilities {
    Capabilities {
        truecolor: true,
        colors_256: true,
        unicode_ok: true,
        ..Capabilities::default()
    }
}

fn test_config() -> RunConfig {
    RunConfig {
        caps: Some(test_caps()),
        // Minimal session: altscreen + hidden cursor. No mouse/paste/
        // focus modes — fewer moving parts in the byte log.
        enter: Some(EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: crate::term::KittyFlags(0),
        }),
        // The probe would write query bytes into the capture log; the
        // upgrade path has its own driver tests.
        probe: false,
    }
}

/// The demo component: a static header row and a reactive counter row.
/// `+`/`-` mutate the signal; `q` quits. The Dyn region is the ONLY thing
/// that should repaint on a counter change.
fn counter_app(app: &mut App) {
    let quitter = app.quitter();
    app.mount(move |cx| {
        let count = cx.signal(0i32);
        Element::new()
            .style(Style::column())
            .shortcut(KeyChord::plain(Key::Char('+')), move |_| {
                count.update(|c| *c += 1)
            })
            .shortcut(KeyChord::plain(Key::Char('-')), move |_| {
                count.update(|c| *c -= 1)
            })
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(text("counter demo"))
            .child(dyn_view(Style::default(), move || {
                text(format!("count: {}", count.get()))
            }))
            .build()
    })
    .expect("mount");
}

#[test]
fn headless_counter_end_to_end() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    counter_app(&mut app);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    assert!(term.is_entered(), "session entered (altscreen active)");
    let _enter_bytes = term.take_bytes();

    // ---- frame 1: initial paint --------------------------------------
    let turn = driver.turn(&mut app, &mut term).expect("turn 1");
    assert!(
        turn.rendered && turn.emitted,
        "mount damage renders frame 1: {turn:?}"
    );
    let frame1 = term.take_bytes();
    let text1 = String::from_utf8_lossy(&frame1).to_string();
    assert!(text1.contains("counter demo"), "header painted in frame 1");
    assert!(text1.contains("count: 0"), "counter painted in frame 1");
    assert_eq!(term.screen().to_text().trim_end(), "counter demo\ncount: 0");

    // ---- idle turn: ZERO bytes ----------------------------------------
    let idle = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(
        idle.idle && !idle.rendered,
        "no events, no damage: {idle:?}"
    );
    assert!(term.take_bytes().is_empty(), "idle emits zero bytes");

    // ---- frame 2: '+' repaints ONLY the counter region ----------------
    term.push_input(b"+");
    let turn = driver.turn(&mut app, &mut term).expect("turn 2");
    assert_eq!(turn.events, 1);
    assert!(turn.rendered && turn.emitted, "{turn:?}");
    let frame2 = term.take_bytes();
    let text2 = String::from_utf8_lossy(&frame2).to_string();
    // The pipeline narrowed the change to the single changed CELL: the
    // damage was the counter region (over-approximation), the diff
    // re-checked equality and only the digit differs — so the bytes
    // carry "1" but not even the unchanged "count: " prefix, and
    // certainly not the static header.
    assert!(text2.contains('1'), "fresh digit in frame 2: {text2:?}");
    assert!(
        !text2.contains("counter demo") && !text2.contains("count"),
        "unchanged cells must NOT re-emit: {text2:?}"
    );
    assert!(
        frame2.len() < frame1.len() / 2,
        "small change, small bytes: frame1={} frame2={}",
        frame1.len(),
        frame2.len()
    );
    assert_eq!(term.screen().to_text().trim_end(), "counter demo\ncount: 1");

    // ---- more keys: screen stays truthful frame by frame ---------------
    term.push_input(b"+");
    driver.turn(&mut app, &mut term).expect("turn 3");
    assert_eq!(term.screen().to_text().trim_end(), "counter demo\ncount: 2");
    term.push_input(b"-");
    driver.turn(&mut app, &mut term).expect("turn 4");
    assert_eq!(term.screen().to_text().trim_end(), "counter demo\ncount: 1");

    // ---- several keys in one turn coalesce into ONE frame --------------
    let before = term.flush_count();
    term.push_input(b"+");
    term.push_input(b"+");
    term.push_input(b"+");
    let turn = driver.turn(&mut app, &mut term).expect("burst turn");
    assert_eq!(turn.events, 3);
    assert_eq!(
        term.flush_count() - before,
        1,
        "one frame, one flush (RT1-16a)"
    );
    assert_eq!(term.screen().to_text().trim_end(), "counter demo\ncount: 4");

    // ---- 'q' quits through the app shortcut ----------------------------
    term.push_input(b"q");
    let turn = driver.turn(&mut app, &mut term).expect("quit turn");
    assert!(turn.quit, "{turn:?}");

    // ---- teardown restores the session ---------------------------------
    driver.finish(&mut term).expect("leave");
    assert!(!term.is_entered());
    assert!(!term.screen().modes().alt_screen(), "altscreen restored");
    assert_eq!(
        term.screen().unknown_seq_count(),
        0,
        "every emitted byte is modeled"
    );
}

#[test]
fn ctrl_c_quits_by_default_but_apps_can_override() {
    // Default: unhandled Ctrl+C ends the loop.
    let size = Size::new(W, H);
    let mut app = App::new(size);
    counter_app(&mut app);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("initial frame");
    term.push_input(b"\x03"); // raw ^C: parser decodes Ctrl+C
    let turn = driver.turn(&mut app, &mut term).expect("ctrl-c turn");
    assert!(turn.quit, "default Ctrl+C policy quits: {turn:?}");

    // Override: an app-level shortcut consumes Ctrl+C; no quit.
    let mut app2 = App::new(size);
    app2.mount(move |cx| {
        let hits = cx.signal(0i32);
        Element::new()
            .shortcut(crate::ui::KeyChord::ctrl(Key::Char('c')), move |_| {
                hits.update(|h| *h += 1)
            })
            .child(text("survivor"))
            .build()
    })
    .expect("mount");
    let mut term2 = CaptureTerm::new(size);
    let mut driver2 = Driver::new(&mut app2, &mut term2, test_config()).expect("driver");
    driver2.turn(&mut app2, &mut term2).expect("initial");
    term2.push_input(b"\x03");
    let turn = driver2
        .turn(&mut app2, &mut term2)
        .expect("overridden ctrl-c");
    assert!(!turn.quit, "consumed Ctrl+C must not quit: {turn:?}");
}

#[test]
fn resize_forces_full_repaint_with_fresh_geometry() {
    let mut app = App::new(Size::new(W, H));
    counter_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");
    let _ = term.take_bytes();

    term.push_resize(Size::new(30, 6));
    let turn = driver.turn(&mut app, &mut term).expect("resize turn");
    assert!(turn.rendered && turn.emitted, "{turn:?}");
    let bytes = term.take_bytes();
    let text = String::from_utf8_lossy(&bytes);
    // Full repaint: both rows re-emit (the terminal's post-resize content
    // is unknowable, so prev is poisoned and everything re-presents).
    assert!(
        text.contains("counter demo") && text.contains("count: 0"),
        "{text:?}"
    );
}

/// SGR mouse press+release at 0-based cell (x, y), as a terminal emits it.
fn sgr_click_bytes(x: i32, y: i32) -> (Vec<u8>, Vec<u8>) {
    (
        format!("\x1b[<0;{};{}M", x + 1, y + 1).into_bytes(),
        format!("\x1b[<0;{};{}m", x + 1, y + 1).into_bytes(),
    )
}

#[test]
fn mouse_click_through_widget_flows_to_minimal_repaint() {
    // The full interactive chain on real bytes: SGR wire bytes -> parser
    // -> ui hit test -> button handlers (press visual, release fires) ->
    // count signal -> label Dyn remount -> damage -> diff -> a repaint
    // that touches the changed regions only.
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        let count = cx.signal(0i32);
        let theme = super::use_theme(cx);
        let tokens = &theme.get_untracked().tokens;
        crate::ui::Element::new()
            .style(Style::column())
            .child(dyn_view(Style::default(), move || {
                text(format!("clicks: {}", count.get()))
            }))
            .child(
                crate::widgets::Button::new("Add")
                    .on_click(move || count.update(|c| *c += 1))
                    .element(cx, tokens)
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");

    driver.turn(&mut app, &mut term).expect("initial frame");
    assert_eq!(
        term.screen().to_text().lines().next().unwrap_or(""),
        "clicks: 0"
    );
    let frame1 = term.take_bytes();

    // Click the button (row 1; "Add" + padding occupies cols 0..5).
    let (press, release) = sgr_click_bytes(2, 1);
    term.push_input(&press);
    term.push_input(&release);
    let turn = driver.turn(&mut app, &mut term).expect("click turn");
    assert_eq!(turn.events, 2, "press + release routed");
    assert!(turn.rendered && turn.emitted, "{turn:?}");
    let frame2 = term.take_bytes();
    let text2 = String::from_utf8_lossy(&frame2).to_string();
    assert_eq!(
        term.screen().to_text().lines().next().unwrap_or(""),
        "clicks: 1",
        "the click reached the signal and the Dyn re-rendered"
    );
    assert!(
        text2.contains('1'),
        "fresh digit in the click frame: {text2:?}"
    );
    assert!(
        !text2.contains("clicks:"),
        "unchanged label prefix must not re-emit: {text2:?}"
    );
    assert!(
        frame2.len() < frame1.len() / 2,
        "click repaint stays regional: frame1={} frame2={}",
        frame1.len(),
        frame2.len()
    );

    // Idle after the interaction: zero bytes (hover state settled).
    let idle = driver.turn(&mut app, &mut term).expect("idle");
    assert!(!idle.rendered || term.take_bytes().is_empty(), "{idle:?}");
}

#[test]
fn toast_overlay_slides_fades_and_idle_returns_to_zero_bytes() {
    // The overlay acceptance: a toast layer slides+fades in over live
    // content, parks, dismisses itself, slides out, REMOVES its layer —
    // and after the dust settles an idle turn emits ZERO bytes. The
    // whole arc runs on an INJECTED clock (RunConfig::now): no sleeps,
    // no scheduler dependence — synthetic milliseconds drive the
    // transition and the dismiss timer deterministically.
    use std::cell::Cell as StdCell;
    use std::time::{Duration, Instant};
    let size = Size::new(30, 6);
    let mut app = App::new(size);
    let overlays = app.overlays();
    let scope_holder: std::rc::Rc<std::cell::RefCell<Option<crate::reactive::Scope>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let sh = scope_holder.clone();
    app.mount(move |cx| {
        *sh.borrow_mut() = Some(cx);
        crate::ui::Element::new()
            .style(Style::column())
            .child(text("main content"))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let clock: std::rc::Rc<StdCell<Instant>> = std::rc::Rc::new(StdCell::new(Instant::now()));
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.set_clock({
        let clock = clock.clone();
        move || clock.get()
    });
    let advance = |ms: u64| clock.set(clock.get() + Duration::from_millis(ms));
    driver.turn(&mut app, &mut term).expect("frame 1");
    assert!(term.screen().to_text().contains("main content"));
    let _ = term.take_bytes();

    // Show the toast: 30ms parked, 6ms flights.
    let cx = scope_holder.borrow().expect("scope");
    Toast::show_with_motion(
        &overlays,
        cx,
        size,
        "saved!",
        Duration::from_millis(30),
        Duration::from_millis(6),
    );
    // Slide-in on synthetic time: 2ms per frame, 6 frames covers the
    // 6ms flight with margin.
    let mut appeared = false;
    for _ in 0..6 {
        driver.turn(&mut app, &mut term).expect("slide-in turn");
        if term.screen().to_text().contains("saved!") {
            appeared = true;
        }
        advance(2);
    }
    assert!(
        appeared,
        "toast text must reach the screen:\n{}",
        term.screen().to_text()
    );
    assert!(
        term.screen().to_text().contains("main content"),
        "content still visible under the overlay"
    );
    assert_eq!(
        crate::reactive::frame_tasks_pending(),
        0,
        "transition landed"
    );
    let _ = term.take_bytes();

    // Parked: nothing animates, the dismiss timer sleeps — zero bytes.
    let parked = driver.turn(&mut app, &mut term).expect("parked turn");
    assert!(!parked.emitted, "{parked:?}");
    assert!(term.take_bytes().is_empty(), "parked toast costs nothing");

    // Jump synthetic time WELL past the dismissal (the `after` deadline
    // is real-now-at-registration + 30ms; the fat jump absorbs any real
    // setup time between clock creation and Toast::show on a loaded
    // CI), then tick out the slide-out the same way.
    advance(400);
    for _ in 0..8 {
        driver.turn(&mut app, &mut term).expect("slide-out turn");
        advance(2);
    }
    assert!(
        !term.screen().to_text().contains("saved!"),
        "toast removed:\n{}",
        term.screen().to_text()
    );
    assert!(
        term.screen().to_text().contains("main content"),
        "vacated region repainted"
    );

    // The charter line: after the whole show, idle is ZERO bytes.
    let _ = term.take_bytes();
    let idle = driver.turn(&mut app, &mut term).expect("idle");
    assert!(idle.idle || !idle.emitted, "{idle:?}");
    assert!(
        term.take_bytes().is_empty(),
        "idle after overlay teardown emits zero bytes"
    );
}

#[test]
fn modal_owns_input_and_close_restores_content() {
    let size = Size::new(30, 8);
    let mut app = App::new(size);
    let overlays = app.overlays();
    let scope_holder: std::rc::Rc<std::cell::RefCell<Option<crate::reactive::Scope>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let sh = scope_holder.clone();
    let root_keys: std::rc::Rc<std::cell::RefCell<u32>> =
        std::rc::Rc::new(std::cell::RefCell::new(0));
    let rk = root_keys.clone();
    app.mount(move |cx| {
        *sh.borrow_mut() = Some(cx);
        crate::ui::Element::new()
            .on(crate::ui::Phase::Bubble, move |_c, e| {
                if matches!(e, crate::ui::UiEvent::Key(_)) {
                    *rk.borrow_mut() += 1;
                }
            })
            .child(text("underneath"))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");

    let cx = scope_holder.borrow().expect("scope");
    let modal = crate::app::Modal::open(&overlays, cx, size, Size::new(16, 3), |_mcx| {
        text("dialog body")
    });
    driver.turn(&mut app, &mut term).expect("modal frame");
    let screen = term.screen().to_text();
    assert!(screen.contains("dialog body"), "modal painted:\n{screen}");

    // Keys route to the modal, never the root, while open.
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("key turn");
    assert_eq!(
        *root_keys.borrow(),
        0,
        "modal swallows keys from the root tree"
    );

    modal.close();
    driver.turn(&mut app, &mut term).expect("close frame");
    let screen = term.screen().to_text();
    assert!(!screen.contains("dialog body"), "modal gone:\n{screen}");
    assert!(
        screen.contains("underneath"),
        "vacated cells repainted from the root"
    );
    // Input flows to the root again.
    term.push_input(b"x");
    driver.turn(&mut app, &mut term).expect("key turn 2");
    assert_eq!(*root_keys.borrow(), 1);
}

#[test]
fn image_overlay_session_lifecycle_on_kitty_terminal() {
    // RT4-1 driver half: through the REAL driver on kitty caps, an
    // image overlay transmits once (a=T), MOVES by placement escape
    // only (a=p, no pixel retransmit), and REMOVAL deletes the upload
    // (a=d) — the session, not the raw renderer, owns the lifecycle.
    let size = Size::new(20, 6);
    let mut app = App::new(size);
    let overlays = app.overlays();
    app.mount(|_cx| Element::new().child(text("content")).build())
        .expect("mount");
    let mut term = CaptureTerm::new(size);
    let caps = Capabilities {
        truecolor: true,
        kitty_graphics: true,
        cell_pixel_size: Some(crate::base::PixelSize { w: 8, h: 16 }),
        ..Default::default()
    };
    let cfg = RunConfig {
        caps: Some(caps),
        probe: false,
        enter: Some(EnterOptions::default()),
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");
    let _ = term.take_bytes();

    let bitmap = crate::gfx::Bitmap::new(8, 8, crate::base::Rgba::new(200, 40, 40, 255));
    let img = overlays.image(crate::base::Rect::new(2, 1, 4, 2), bitmap);
    driver.turn(&mut app, &mut term).expect("transmit frame");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        bytes.contains("a=T"),
        "first sync transmits pixels: {bytes:?}"
    );

    // Move: same content version -> placement escape, no retransmit.
    img.set_rect(crate::base::Rect::new(8, 2, 4, 2));
    driver.turn(&mut app, &mut term).expect("move frame");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(bytes.contains("a=p"), "move re-places by id: {bytes:?}");
    assert!(
        !bytes.contains("a=T"),
        "move must NOT retransmit pixels: {bytes:?}"
    );

    // Remove: the terminal-side upload is freed (the RT4-1 leak).
    img.remove();
    driver.turn(&mut app, &mut term).expect("remove frame");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        bytes.contains("a=d"),
        "removal deletes the kitty upload: {bytes:?}"
    );

    // Teardown with a live slot also deletes (finish path).
    let bitmap2 = crate::gfx::Bitmap::new(4, 4, crate::base::Rgba::new(40, 200, 40, 255));
    let _img2 = overlays.image(crate::base::Rect::new(0, 0, 2, 1), bitmap2);
    driver.turn(&mut app, &mut term).expect("transmit 2");
    let _ = term.take_bytes();
    driver.finish(&mut term).expect("finish");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        bytes.contains("a=d"),
        "finish releases live uploads: {bytes:?}"
    );
}

#[test]
fn spatial_nav_actions_move_focus_between_panes() {
    // Acceptance for focus_next_in through the whole stack: Alt+arrows
    // registered as app ACTIONS (keys nothing consumed reach the
    // keymap), each calling the tree's spatial move; focus lands by
    // GEOMETRY and the a11y snapshot reports the movement.
    let size = Size::new(20, 4);
    let mut app = App::new(size);
    let pane = |name: &str| {
        crate::ui::Element::new()
            .style(
                Style::default()
                    .width(crate::layout::Dimension::Cells(10))
                    .height(crate::layout::Dimension::Cells(2)),
            )
            .focusable()
            .role(crate::ui::Role::Region)
            .access_label(name.to_string())
    };
    app.mount(move |_cx| {
        crate::ui::Element::new()
            .style(Style::column())
            .child(
                crate::ui::Element::new()
                    .style(Style::row().height(crate::layout::Dimension::Cells(2)))
                    .child(pane("nw").build())
                    .child(pane("ne").build())
                    .build(),
            )
            .child(
                crate::ui::Element::new()
                    .style(Style::row().height(crate::layout::Dimension::Cells(2)))
                    .child(pane("sw").build())
                    .child(pane("se").build())
                    .build(),
            )
            .build()
    })
    .expect("mount");
    // Alt+arrow actions drive spatial movement.
    for (chord_key, dir) in [
        (Key::Right, Key::Right),
        (Key::Down, Key::Down),
        (Key::Left, Key::Left),
        (Key::Up, Key::Up),
    ] {
        let mut tree = app.tree().handle();
        app.actions().register(
            format!("focus.{dir:?}"),
            Some(KeyChord::new(crate::ui::Mods::ALT, chord_key)),
            move || {
                tree.focus_next_in(dir);
            },
        );
    }
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");
    app.tree().focus_first();
    let at = |app: &mut App| {
        let snap = app.tree().a11y_tree();
        snap.focused().map(|e| e.label.clone()).unwrap_or_default()
    };
    assert_eq!(at(&mut app), "nw");
    // Alt+Right via raw kitty-style CSI? CaptureTerm feeds bytes; use
    // the legacy alt-arrow encoding ESC [ 1 ; 3 C (mods=3 => Alt).
    term.push_input(b"\x1b[1;3C");
    driver.turn(&mut app, &mut term).expect("alt-right");
    assert_eq!(at(&mut app), "ne", "Alt+Right moved focus east");
    term.push_input(b"\x1b[1;3B");
    driver.turn(&mut app, &mut term).expect("alt-down");
    assert_eq!(at(&mut app), "se", "Alt+Down moved focus south");
    term.push_input(b"\x1b[1;3D");
    driver.turn(&mut app, &mut term).expect("alt-left");
    assert_eq!(at(&mut app), "sw", "Alt+Left moved focus west");
}

#[test]
fn startup_notices_collect_caps_summary_and_custom_lines() {
    let size = Size::new(10, 3);
    let mut app = App::new(size);
    assert!(app.startup_notices().is_empty(), "clean start");
    app.push_startup_notice("input: degraded (stdin fallback)");
    let caps = test_caps();
    app.push_startup_notice(super::caps_summary(&caps));
    let notices = app.startup_notices();
    assert_eq!(notices[0], "input: degraded (stdin fallback)");
    assert!(notices[1].starts_with("caps: truecolor"), "{notices:?}");
}

#[test]
fn worker_failure_surfaces_as_app_error() {
    let mut app = App::new(Size::new(W, H));
    counter_app(&mut app);
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
    driver.turn(&mut app, &mut term).expect("frame 1");

    crate::reactive::spawn_worker("image-decoder", || panic!("bad png"))
        .join()
        .expect("worker thread contained its panic");
    let err = driver
        .turn(&mut app, &mut term)
        .expect_err("dead worker must surface");
    let msg = err.to_string();
    assert!(
        msg.contains("image-decoder") && msg.contains("bad png"),
        "{msg}"
    );
}
