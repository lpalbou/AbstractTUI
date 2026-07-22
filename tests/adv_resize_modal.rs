//! Backlog 0298: stale frame band after terminal resize combined with a
//! modal close (field report: workflow picker List-in-Modal, resize
//! around the close, half-stale screen).
//!
//! Method: drive the REAL pipeline (Driver + CaptureTerm) through every
//! interleaving of {resize, modal close}, then verify the final screen
//! BYTE-LEVEL against a fresh-paint oracle — a second driver taken to
//! the same app state at the same size. The incumbent's post-resize
//! bytes feed a VtScreen PRE-FILLED WITH GARBAGE ('X' cells), modeling
//! the honest worst case: the emulator's post-resize content is
//! arbitrary, so any cell the engine fails to re-emit surfaces as a
//! stale cell instead of hiding as a plausible blank.

use abstracttui::app::{App, Driver, Modal, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::{Capabilities, EnterOptions, MouseMode};
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::List;
use std::cell::RefCell;
use std::rc::Rc;

fn caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.unicode_ok = true;
    })
}

fn cfg() -> RunConfig {
    RunConfig {
        caps: Some(caps()),
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

/// Shared handles the fixture returns beside the app: the workflow
/// signal (what the modal's List sets) plus the slot the opened modal
/// parks in.
#[derive(Clone)]
struct Handles {
    scope: abstracttui::reactive::Scope,
    workflow: abstracttui::reactive::Signal<String>,
    modal: Rc<RefCell<Option<Modal>>>,
}

/// The field shape, reduced: a full-viewport column whose header and
/// body re-render from a "workflow" signal (dyn_view), plus enough
/// static rows that a stale band is visible anywhere on screen.
fn mount_fixture(size: Size, initial: &str) -> (App, Handles) {
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<abstracttui::reactive::Scope>>> = Rc::new(RefCell::new(None));
    let sig_slot: Rc<RefCell<Option<abstracttui::reactive::Signal<String>>>> =
        Rc::new(RefCell::new(None));
    let (ss, gs) = (scope_slot.clone(), sig_slot.clone());
    let initial = initial.to_string();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        let workflow = cx.signal(initial.clone());
        *gs.borrow_mut() = Some(workflow);
        let mut col = Element::new().style(LayoutStyle::column());
        col = col.child(dyn_view(LayoutStyle::line(1), move || {
            text(format!("header: {}", workflow.get()))
        }));
        for i in 0..40 {
            col = col.child(text(format!("row {i:02} ................")));
        }
        col.build()
    })
    .expect("mount");
    let scope = scope_slot.borrow().expect("scope");
    let workflow = sig_slot.borrow().expect("signal");
    (
        app,
        Handles {
            scope,
            workflow,
            modal: Rc::new(RefCell::new(None)),
        },
    )
}

/// Open the field modal: a List whose `on_activate` sets the workflow
/// signal and closes the modal from INSIDE the overlay tree's dispatch
/// (layer removed synchronously — the app path 0298 reports).
fn open_picker(app: &App, fx: &Handles, viewport: Size, panel: Size) {
    let overlays = app.overlays();
    let workflow = fx.workflow;
    let slot = fx.modal.clone();
    let slot_in = fx.modal.clone();
    let modal = Modal::open(&overlays, fx.scope, viewport, panel, move |mcx| {
        let items = vec!["alpha-agent".to_string(), "basic-agent".to_string()];
        List::new(items.clone())
            .on_activate(move |i| {
                workflow.set(items[i].clone());
                if let Some(m) = slot_in.borrow_mut().take() {
                    m.close();
                }
            })
            .view(mcx)
    });
    *slot.borrow_mut() = Some(modal);
}

/// Run turns until idle (bounded), feeding every emitted byte into `vt`.
/// On a viewport change the referee is REBUILT at the new size and
/// pre-filled with garbage — the post-resize screen is unknowable, and
/// a fresh blank grid would hide exactly the class of bug 0298 reports.
fn drive_to_idle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm, vt: &mut VtScreen) {
    for _ in 0..12 {
        let turn = driver.turn(app, term).expect("turn");
        let bytes = term.take_bytes();
        if vt.size() != app.viewport() {
            *vt = garbage_screen(app.viewport());
        }
        vt.feed(&bytes);
        if turn.idle {
            break;
        }
    }
}

/// A VtScreen modeling the honest post-resize worst case: every cell
/// holds a visible stale glyph ('X' on magenta), and the CURSOR sits at
/// the bottom-left of the NEW screen — emulators move the physical
/// cursor during reflow (macOS Terminal's bottom-anchored growth was
/// 0298's live incident), so an engine trusting its pre-resize virtual
/// cursor for relative motion paints the whole frame at an offset.
/// SGR is left at defaults (real terminals preserve pen state across a
/// resize, and the previous frame's trailer reset it — modeling a pen
/// hazard here would be inventing one).
fn garbage_screen(size: Size) -> VtScreen {
    let mut vt = VtScreen::new(size);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x1b[45;31m");
    for y in 0..size.h {
        bytes.extend_from_slice(format!("\x1b[{};1H", y + 1).as_bytes());
        bytes.extend(std::iter::repeat_n(b'X', size.w.max(0) as usize));
    }
    bytes.extend_from_slice(format!("\x1b[0m\x1b[{};1H", size.h).as_bytes());
    vt.feed(&bytes);
    vt
}

/// Cell-for-cell screen comparison (glyph + full paint). Returns the
/// mismatching cells (bounded sample) for the assert message.
fn screen_diff(a: &VtScreen, b: &VtScreen) -> Vec<String> {
    assert_eq!(a.size(), b.size(), "referee size mismatch (test bug)");
    let mut out = Vec::new();
    for y in 0..a.size().h {
        for x in 0..a.size().w {
            let ca = a.cell(x, y).expect("cell");
            let cb = b.cell(x, y).expect("cell");
            let glyphs_equal = ca.ch() == cb.ch();
            let paint_equal = ca.paint == cb.paint;
            if (!glyphs_equal || !paint_equal) && out.len() < 12 {
                out.push(format!(
                    "({x},{y}): got {:?}/{:?} want {:?}/{:?}",
                    ca.ch(),
                    ca.paint,
                    cb.ch(),
                    cb.paint
                ));
            }
        }
    }
    out
}

/// The oracle: a FRESH driver over the same app state (workflow already
/// switched, no modal) at `size`; its first full paint is the truth the
/// incumbent's final screen must equal byte-for-byte.
fn oracle_screen(size: Size, workflow: &str) -> VtScreen {
    let (mut app, _handles) = mount_fixture(size, workflow);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("oracle driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    vt
}

/// Activate the selected List row via Enter (the modal owns keys).
const ENTER: &[u8] = b"\r";

/// One scenario: open the picker at `start`, then run `script` steps
/// (resize / activate-close), then assert the final screen equals the
/// fresh-paint oracle at the final size.
enum Step {
    Resize(Size),
    ActivateClose,
    /// Resize + activate in the SAME turn (one input burst).
    ResizeAndActivate(Size),
}

fn run_scenario(name: &str, start: Size, panel: Size, script: &[Step]) {
    let (mut app, fx) = mount_fixture(start, "alpha-agent");
    let mut term = CaptureTerm::new(start);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(start);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    open_picker(&app, &fx, app.viewport(), panel);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert!(
        vt.to_text().contains("alpha-agent"),
        "{name}: modal list visible:\n{}",
        vt.to_text()
    );

    for step in script {
        match step {
            Step::Resize(size) => term.push_resize(*size),
            Step::ActivateClose => term.push_input(ENTER),
            Step::ResizeAndActivate(size) => {
                term.push_resize(*size);
                term.push_input(ENTER);
            }
        }
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    }
    assert!(
        fx.modal.borrow().is_none(),
        "{name}: the picker must have closed"
    );

    let final_size = app.viewport();
    let oracle = oracle_screen(final_size, &fx.workflow.get_untracked());
    let diff = screen_diff(&vt, &oracle);
    assert!(
        diff.is_empty(),
        "{name}: {} stale/missing cells vs fresh-paint oracle at {:?}\n\
         first mismatches:\n  {}\n--- incumbent ---\n{}\n--- oracle ---\n{}",
        diff.len(),
        final_size,
        diff.join("\n  "),
        vt.to_text(),
        oracle.to_text()
    );
}

const START: Size = Size { w: 60, h: 20 };
const TALLER: Size = Size { w: 60, h: 34 };
const SHORTER: Size = Size { w: 60, h: 12 };
const WIDER: Size = Size { w: 90, h: 20 };
const NARROWER: Size = Size { w: 42, h: 20 };
const PANEL: Size = Size { w: 30, h: 8 };

#[test]
fn resize_taller_while_modal_open_then_close() {
    run_scenario(
        "grow-then-close",
        START,
        PANEL,
        &[Step::Resize(TALLER), Step::ActivateClose],
    );
}

#[test]
fn resize_shorter_while_modal_open_then_close() {
    run_scenario(
        "shrink-then-close",
        START,
        PANEL,
        &[Step::Resize(SHORTER), Step::ActivateClose],
    );
}

#[test]
fn resize_wider_and_narrower_while_modal_open_then_close() {
    run_scenario(
        "wider-then-close",
        START,
        PANEL,
        &[Step::Resize(WIDER), Step::ActivateClose],
    );
    run_scenario(
        "narrower-then-close",
        START,
        PANEL,
        &[Step::Resize(NARROWER), Step::ActivateClose],
    );
}

#[test]
fn close_then_resize_each_direction() {
    run_scenario(
        "close-then-grow",
        START,
        PANEL,
        &[Step::ActivateClose, Step::Resize(TALLER)],
    );
    run_scenario(
        "close-then-shrink",
        START,
        PANEL,
        &[Step::ActivateClose, Step::Resize(SHORTER)],
    );
}

#[test]
fn resize_and_close_in_the_same_turn() {
    run_scenario(
        "same-turn grow+close",
        START,
        PANEL,
        &[Step::ResizeAndActivate(TALLER)],
    );
    run_scenario(
        "same-turn shrink+close",
        START,
        PANEL,
        &[Step::ResizeAndActivate(SHORTER)],
    );
}

#[test]
fn resize_shrink_then_grow_while_open_then_close() {
    run_scenario(
        "shrink-grow-close",
        START,
        PANEL,
        &[
            Step::Resize(SHORTER),
            Step::Resize(TALLER),
            Step::ActivateClose,
        ],
    );
}

/// The 0298 mechanism, pinned at the byte level: the first frame after
/// a resize must re-anchor ABSOLUTELY — its first cursor motion is a
/// CUP (`ESC [ H` / `ESC [ r;c H`), never relative motion (CUU/CUD/
/// CUF/CUB/CR) from the pre-resize parked cursor. The physical cursor
/// after an emulator reflow is unknowable, so a relative first hop
/// offsets the entire frame and leaves a stale band (the live
/// incident). Steady-state frames keep their relative-motion economy —
/// only the resize boundary re-anchors.
#[test]
fn first_post_resize_frame_re_anchors_absolutely() {
    let (mut app, _fx) = mount_fixture(START, "alpha-agent");
    let mut term = CaptureTerm::new(START);
    let mut driver = Driver::new(&mut app, &mut term, cfg()).expect("driver");
    let mut vt = VtScreen::new(START);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);

    for &size in &[TALLER, SHORTER, WIDER, NARROWER] {
        term.push_resize(size);
        driver.turn(&mut app, &mut term).expect("resize turn");
        let bytes = term.take_bytes();
        assert!(!bytes.is_empty(), "a resize must emit a frame");
        // First cursor-affecting sequence must be absolute. Scan the
        // head: SGR (`...m`) is allowed before it; any relative motion
        // final byte (A/B/C/D) or bare CR before the first `H` is the
        // ghost-cursor bug.
        let head = &bytes[..bytes.len().min(48)];
        let mut i = 0;
        let mut first_motion: Option<u8> = None;
        while i < head.len() {
            if head[i] == b'\r' {
                first_motion = Some(b'\r');
                break;
            }
            if head[i] == 0x1b && i + 1 < head.len() && head[i + 1] == b'[' {
                let mut j = i + 2;
                while j < head.len() && !head[j].is_ascii_alphabetic() {
                    j += 1;
                }
                if j < head.len() {
                    match head[j] {
                        b'A' | b'B' | b'C' | b'D' | b'H' => {
                            first_motion = Some(head[j]);
                            break;
                        }
                        _ => i = j, // SGR or mode flip: keep scanning
                    }
                }
            }
            i += 1;
        }
        assert_eq!(
            first_motion,
            Some(b'H'),
            "post-resize frame must open with absolute CUP, got {:?} in {:?}",
            first_motion.map(|b| b as char),
            String::from_utf8_lossy(head)
        );
        // Drain the rest of the resize cascade before the next size.
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    }
}
