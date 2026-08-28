//! The undeclared-capabilities branch: what a `Driver` assumes when
//! `RunConfig::caps` is `None` and the terminal it renders to is not a
//! terminal.
//!
//! The defect this pins was reported from the field, not found here. A
//! consumer's whole colour suite ran on `CaptureTerm` with `caps: None`,
//! so `Capabilities::detect_env` read *their shell* — `COLORTERM` unset,
//! therefore `ColorDepth::Xterm256`, therefore every colour the suite
//! asserted on had already been through the 256 cube. Nothing was red:
//! the tests compared a painted token against the same token, and both
//! sides quantise together. The suite's verdict simply depended on who
//! ran it.
//!
//! So these tests are about a NEGATIVE: not "colour works" (it did, at
//! the wrong depth, all along) but "the depth does not come from the
//! environment". Two independent assertions, because either alone is
//! satisfiable by an accident of the host:
//!
//!   1. the substitution announces itself (a notice only the gate can
//!      produce — red on any machine the moment the gate is removed);
//!   2. the emitted bytes carry an un-quantised colour (the consequence
//!      the notice is claiming).

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::{Capabilities, Terminal};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::Element;

const W: i32 = 24;
const H: i32 = 3;

/// A colour no xterm-256 cube level can express. The cube's channel
/// levels are 0/95/135/175/215/255, so every one of these three
/// components MUST move if the frame is quantised — there is no
/// coincidence available to make the assertion pass at 256 colours.
const OFF_CUBE: Rgba = Rgba::rgb(51, 102, 153);

/// The exact SGR run a truecolor presenter writes for `OFF_CUBE`.
const OFF_CUBE_SGR: &str = "38;2;51;102;153";

/// `caps: None` — the shape a harness reaches for before reading the
/// docs, and the one this whole file is about.
fn undeclared() -> RunConfig {
    RunConfig {
        caps: None,
        probe: false,
        ..RunConfig::default()
    }
}

fn mount_off_cube_text(app: &mut App) {
    app.mount(|_cx| {
        Element::new()
            .style(LayoutStyle::fill())
            .draw(|canvas, rect| {
                canvas.print(rect.origin(), "colour", OFF_CUBE, Rgba::BLACK);
            })
            .build()
    })
    .expect("mount");
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

// ---------------------------------------------------------------------------
// 1. The gate fires, and says so.
// ---------------------------------------------------------------------------

/// The deterministic half. `CaptureTerm` does not override
/// `Terminal::is_tty`, so it reports false; with `caps` undeclared the
/// driver must take the headless branch and leave a startup notice
/// naming it. No environment variable can produce this string, and no
/// environment variable can suppress it — remove the gate in
/// `Driver::new` and this fails on every machine.
#[test]
fn undeclared_caps_over_a_non_tty_substitute_headless_defaults_and_say_so() {
    let mut term = CaptureTerm::new(Size::new(W, H));
    assert!(
        !term.is_tty(),
        "premise: a capture terminal is not a terminal"
    );

    let mut app = App::new(Size::new(W, H));
    mount_off_cube_text(&mut app);
    let driver = Driver::new(&mut app, &mut term, undeclared()).expect("enter");

    assert_eq!(
        driver.caps(),
        &Capabilities::headless(),
        "the fixed set, not whatever this machine's shell advertises"
    );
    let notice = app
        .startup_notices()
        .iter()
        .find(|n| n.starts_with("caps: headless defaults"))
        .unwrap_or_else(|| {
            panic!(
                "the substitution must never be mute; notices were {:?}",
                app.startup_notices()
            )
        });
    assert!(
        notice.contains("RunConfig::caps") && notice.contains("Terminal::is_tty"),
        "the notice must name BOTH fixes — declaring caps, and the \
         third-party terminal that is real but never overrode is_tty: {notice}"
    );
}

/// The other side of the gate: a `Terminal` that says it IS one gets the
/// environment pass, untouched. This is what keeps the fix a
/// substitution for the headless case and not a global override — and it
/// is why the notice above has to exist, since `is_tty` defaults to
/// false and a real terminal that never overrode it lands in the
/// headless branch by accident.
#[test]
fn a_terminal_that_claims_a_tty_still_gets_the_environment_pass() {
    let mut term = CaptureTerm::new(Size::new(W, H));
    term.set_tty(true);
    assert!(term.is_tty(), "premise: this one claims to be a terminal");

    let mut app = App::new(Size::new(W, H));
    mount_off_cube_text(&mut app);
    let driver = Driver::new(&mut app, &mut term, undeclared()).expect("enter");

    assert_eq!(
        driver.caps(),
        &Capabilities::detect_env(),
        "a self-declared tty must keep the env pass, whatever it detects"
    );
    assert!(
        !app.startup_notices()
            .iter()
            .any(|n| n.starts_with("caps: headless defaults")),
        "no substitution happened, so nothing may claim one did: {:?}",
        app.startup_notices()
    );
}

// ---------------------------------------------------------------------------
// 2. The consequence: colour on the wire is not the shell's colour.
// ---------------------------------------------------------------------------

/// What the notice is actually promising. The frame paints a colour that
/// is not on the 256 cube; the bytes the presenter emitted must carry it
/// verbatim, with no `38;5;` fallback anywhere in the stream.
///
/// Under the old `unwrap_or_else(detect_env)` this test's verdict was a
/// property of the shell that ran it: green under `COLORTERM=truecolor`,
/// red with it unset. That instability WAS the bug — so read a pass here
/// as "the depth is pinned", never as "truecolor works".
#[test]
fn the_headless_frame_emits_the_colour_it_was_given_unquantised() {
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_off_cube_text(&mut app);
    let mut driver = Driver::new(&mut app, &mut term, undeclared()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let out = String::from_utf8_lossy(term.bytes()).into_owned();
    assert!(
        out.contains(OFF_CUBE_SGR),
        "the painted colour must reach the wire intact: expected {OFF_CUBE_SGR}"
    );
    assert!(
        !out.contains("38;5;"),
        "no cube fallback may appear in a headless frame — that is the \
         quantisation this branch exists to prevent"
    );
}

/// The guard on the guard. If the assertion above cannot tell the two
/// depths apart, it is decoration — so drive the SAME frame with
/// `Xterm256` declared and require the opposite bytes. A change that
/// broke the presenter's depth handling would take this test with it
/// instead of leaving a green suite behind.
#[test]
fn the_same_frame_at_256_colours_really_does_quantise() {
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_off_cube_text(&mut app);
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| c.colors_256 = true)),
        probe: false,
        ..RunConfig::default()
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let out = String::from_utf8_lossy(term.bytes()).into_owned();
    assert!(
        out.contains("38;5;"),
        "256-colour output must use the cube: {out:?}"
    );
    assert!(
        !out.contains(OFF_CUBE_SGR),
        "and must NOT carry the exact colour — if it did, the headless \
         test above would pass at either depth and prove nothing"
    );
}
