//! Does the ground assignment reach a REAL app?
//!
//! `tests/theme_quantisation_grounds.rs` proves the policy and proves the
//! presenter honours it when someone installs one. Neither proves anyone
//! ever does. These drive `App` + `Driver` + `CaptureTerm` — the whole
//! stack an application actually runs — at 256 colours, feed the emitted
//! bytes to the VT model, and read the colours back off the screen.
//!
//! The distinction matters because the two halves failed differently
//! while this was being built: the policy was right and unreached for a
//! slice, and a fix that is correct in `color.rs` and never consulted is
//! not a fix. Every assertion here is on painted output, never on the
//! driver's internal state.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::palette::XTERM_256;
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::render::color::nearest_xterm256;
use abstracttui::term::Capabilities;
use abstracttui::testing::{xterm_256, CaptureTerm, VtScreen};
use abstracttui::ui::Element;

const W: i32 = 8;
const H: i32 = 2;

/// 256 colours, declared rather than detected — `adv_headless_caps`
/// exists because inheriting the developer's shell here is how a whole
/// suite silently ran at the wrong depth.
fn caps_256() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = false;
            c.colors_256 = true;
        })),
        probe: false,
        ..RunConfig::default()
    }
}

fn caps_truecolor() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        probe: false,
        ..RunConfig::default()
    }
}

/// Paint cell (0,0) in `a` and cell (1,0) in `b`, as GROUNDS — the case
/// the pair quantiser cannot see, because the two never share a cell.
fn mount_two_grounds(app: &mut App, a: Rgba, b: Rgba) {
    app.mount(move |_cx| {
        Element::new()
            .style(LayoutStyle::fill())
            .draw(move |canvas, rect| {
                canvas.print(rect.origin(), " ", Rgba::WHITE, a);
                canvas.print(Point::new(rect.x + 1, rect.y), " ", Rgba::WHITE, b);
            })
            .build()
    })
    .expect("mount");
}

/// The palette entry `surface` actually renders as once the theme's own
/// five grounds have been separated — which is NOT its natural entry in
/// a theme where the grounds collide, because the assignment displaces
/// it. Anything testing a consumer collision has to collide with this.
fn surface_entry(t: &abstracttui::theme::TokenSet) -> u8 {
    let g = t.grounds();
    let idx = abstracttui::render::color::quantize_set_256(g.map(|(_, c)| c));
    let k = g
        .iter()
        .position(|(id, _)| *id == abstracttui::theme::TokenId::Surface)
        .expect("surface is a ground");
    idx[k]
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// What the terminal ends up showing at (0,0) and (1,0).
fn painted(term: &mut CaptureTerm) -> (Option<Rgba>, Option<Rgba>) {
    let mut screen = VtScreen::new(Size::new(W, H));
    screen.feed(term.bytes());
    assert_eq!(screen.unknown_seq_count(), 0, "unmodeled bytes");
    let bg = |x: i32| screen.cell(x, 0).expect("in bounds").paint.bg;
    (bg(0), bg(1))
}

/// **The end-to-end case.** The engine's default theme has `bg` and
/// `surface` on one xterm-256 entry, so before this wiring a panel over
/// the app ground reached a 256-colour terminal as a single flat colour.
/// Rendered through the real driver, the two now arrive distinct.
///
/// The theme is read rather than hardcoded: if `abstract-dark` is
/// retuned so its grounds stop colliding, the premise assertion fails
/// loudly instead of the test passing for the wrong reason.
#[test]
fn a_real_app_at_256_colours_keeps_the_default_themes_two_grounds_apart() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    assert_eq!(
        nearest_xterm256(t.bg),
        nearest_xterm256(t.surface),
        "premise: these two grounds collapse to one entry at 256 colours"
    );

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_two_grounds(&mut app, t.bg, t.surface);
    let mut driver = Driver::new(&mut app, &mut term, caps_256()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let (ground, panel) = painted(&mut term);
    assert!(ground.is_some() && panel.is_some(), "both cells painted");
    assert_ne!(
        ground, panel,
        "the app ground and the panel reached the terminal as ONE colour — \
         the assignment is not being installed by the driver"
    );
    assert_eq!(
        ground,
        Some(xterm_256(nearest_xterm256(t.bg))),
        "the ground that did not move must keep exactly its natural entry"
    );
}

/// Truecolor is untouched: no assignment, no displacement, the authored
/// colours on the wire. The fix must be invisible where there is no
/// defect — otherwise it moves colours on terminals that render them
/// exactly, which was the objection that killed the token-adjusting
/// design.
#[test]
fn truecolor_is_left_alone() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_two_grounds(&mut app, t.bg, t.surface);
    let mut driver = Driver::new(&mut app, &mut term, caps_truecolor()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let (ground, panel) = painted(&mut term);
    assert_eq!(ground, Some(t.bg), "authored ground, verbatim");
    assert_eq!(panel, Some(t.surface), "authored panel, verbatim");
}

/// **A ground the THEME does not know about.** A consumer minting its
/// own fill gets no protection unless the separator is handed it — that
/// is what `set_extra_grounds` is for, and this is the case it exists
/// for: a colour chosen to land on the same palette entry as a theme
/// ground.
///
/// Both halves are asserted. Undeclared, the consumer fill collapses
/// into the theme ground (the defect, live, through the whole stack);
/// declared, it separates. Without the first half the second could pass
/// on a colour that never collided.
#[test]
fn a_consumer_ground_collapses_until_it_is_declared() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    // The colliding fill has to be built against the entry `surface`
    // ENDS UP on, not the one it starts on. Two earlier attempts got
    // this wrong and are worth recording: a +2 channel nudge lands on a
    // different entry entirely, and `surface`'s own natural entry is
    // already vacated — the theme's five-ground assignment displaces
    // `surface` off 234 before any consumer arrives. So the premise is
    // computed from the assignment, not guessed.
    let consumer = XTERM_256[surface_entry(&t) as usize];
    assert_ne!(consumer, t.surface, "premise: a different colour");

    let undeclared = {
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut app = App::new(Size::new(W, H));
        mount_two_grounds(&mut app, t.surface, consumer);
        let mut driver = Driver::new(&mut app, &mut term, caps_256()).expect("enter");
        settle(&mut driver, &mut app, &mut term);
        painted(&mut term)
    };
    assert_eq!(
        undeclared.0, undeclared.1,
        "premise: an undeclared consumer ground is not protected — if this \
         now differs, something else is separating it and the second half \
         of this test proves nothing"
    );

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_two_grounds(&mut app, t.surface, consumer);
    let mut driver = Driver::new(&mut app, &mut term, caps_256()).expect("enter");
    driver.set_extra_grounds(&[consumer]);
    settle(&mut driver, &mut app, &mut term);

    let (theme_ground, consumer_ground) = painted(&mut term);
    assert_ne!(
        theme_ground, consumer_ground,
        "the consumer ground was declared and still collapsed into a theme \
         ground — set_extra_grounds is not reaching the assignment"
    );
}

/// `set_extra_grounds` owns its repaint. Declaring a ground mid-session
/// changes the bytes an already-painted cell resolves to, and the frame
/// diff will not re-emit a cell that did not change — so without the
/// damage the screen would keep the old indices while every internal
/// structure said otherwise.
///
/// Asserted on what the TERMINAL shows after the call, from a settled
/// screen that had already been painted.
#[test]
fn declaring_a_ground_mid_session_repaints_what_is_already_on_screen() {
    let t = abstracttui::theme::get("abstract-dark")
        .expect("house default")
        .tokens;
    let consumer = XTERM_256[surface_entry(&t) as usize];

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut app = App::new(Size::new(W, H));
    mount_two_grounds(&mut app, t.surface, consumer);
    let mut driver = Driver::new(&mut app, &mut term, caps_256()).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let before = painted(&mut term);
    assert_eq!(
        before.0, before.1,
        "premise: collapsed on the settled screen"
    );

    // Everything the terminal shows from here on is what THIS call
    // caused: the previous bytes are dropped, not accumulated.
    term.take_bytes();
    driver.set_extra_grounds(&[consumer]);
    settle(&mut driver, &mut app, &mut term);

    let mut screen = VtScreen::new(Size::new(W, H));
    screen.feed(term.bytes());
    let bg = |x: i32| screen.cell(x, 0).expect("in bounds").paint.bg;
    assert!(
        bg(0).is_some() && bg(1).is_some(),
        "both cells must be RE-EMITTED after the declaration, not left \
         standing at their old indices: the diff sees no cell change, so \
         only the damage forces them out"
    );
    assert_ne!(bg(0), bg(1), "and they must arrive separated");
}
