//! Probe-driven capability truth (backlog 0293 + 0295 / media-av 0685),
//! end to end through the real driver against `CaptureTerm`:
//!
//! - the kitty keyboard enter-flags FOLLOW the probe: a terminal the env
//!   pass could not claim (iTerm2 ≥ 3.5, VS Code/Cursor, Warp) gets
//!   `CSI > flags u` the moment its probe reply proves the protocol, and
//!   the pop bookkeeping stays balanced through finish (0293);
//! - Shift+Enter arrives as a DISTINCT key event once the flags are live
//!   (the parser side of the same chain);
//! - `app::use_caps` is the reactive post-probe view: capability-derived
//!   UI (the images example's channel label, composer key hints)
//!   re-renders truthfully as probe replies fold in (0295/0685);
//! - an explicit `RunConfig::enter` posture is the embedder's own: the
//!   upgrade never touches it.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{current_caps, use_caps, Driver, RunConfig};
use abstracttui::gfx::{choose_channel, Channel};
use abstracttui::prelude::*;
use abstracttui::term::{Capabilities, EnterOptions, KittyFlags, MouseMode};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{Phase, UiEvent};

const SIZE: Size = Size::new(40, 6);

/// A terminal the env pass cannot claim the kitty protocol for (the
/// iTerm2/VS Code/Warp shape): real colors, no kitty keyboard.
fn plain_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.unicode_ok = true;
    })
}

fn key_log_app(app: &mut App) -> Rc<RefCell<Vec<(Key, Mods)>>> {
    let log: Rc<RefCell<Vec<(Key, Mods)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    app.mount(move |_cx| {
        Element::new()
            .on(Phase::Bubble, move |_ctx, ev| {
                if let UiEvent::Key(k) = ev {
                    sink.borrow_mut().push((k.key, k.mods));
                }
            })
            .child(text("probe rig"))
            .build()
    })
    .expect("mount");
    log
}

/// 0293 acceptance: probe proof pushes the standard flags; Shift+Enter
/// then decodes as a distinct key; finish pops the entry.
#[test]
fn probe_proof_pushes_kitty_flags_then_shift_enter_works_and_finish_pops() {
    let mut app = App::new(SIZE);
    let log = key_log_app(&mut app);
    let mut term = CaptureTerm::new(SIZE);
    let cfg = RunConfig {
        caps: Some(plain_caps()),
        enter: None, // derived posture: the driver owns the kitty flags
        probe: true,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    driver.turn(&mut app, &mut term).expect("first frame");
    assert_eq!(
        term.screen().counters().kitty_push_depth,
        0,
        "env pass could not claim the protocol: nothing pushed at enter"
    );
    let _ = term.take_bytes();

    // The terminal answers the probe: kitty keyboard reply, then the
    // DA1 sentinel (one burst, as real terminals answer).
    term.push_input(b"\x1b[?1u\x1b[?62c");
    driver.turn(&mut app, &mut term).expect("probe fold turn");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        bytes.contains("\x1b[>3u"),
        "probe proof must push the standard flags: {bytes:?}"
    );
    assert_eq!(term.screen().counters().kitty_push_depth, 1);
    assert!(
        current_caps().kitty_keyboard,
        "the reactive caps view carries the proven protocol"
    );

    // Shift+Enter now arrives as a DISTINCT key event (CSI 13;2u — the
    // chord the whole 0293 chain exists for).
    term.push_input(b"\x1b[13;2u");
    driver.turn(&mut app, &mut term).expect("shift+enter turn");
    assert!(
        log.borrow()
            .iter()
            .any(|(k, m)| *k == Key::Enter && m.contains(Mods::SHIFT)),
        "Shift+Enter must reach the app as Enter+SHIFT: {:?}",
        log.borrow()
    );

    // Finish pops the runtime push — the terminal's own bookkeeping,
    // no driver-side pop needed.
    driver.finish(&mut term).expect("finish");
    assert_eq!(
        term.screen().counters().kitty_push_depth,
        0,
        "leave must pop exactly the one runtime push"
    );
}

/// An explicit `RunConfig::enter` posture is authoritative: the probe
/// upgrade reports the capability but never re-arms the session.
#[test]
fn explicit_enter_posture_is_never_upgraded() {
    let mut app = App::new(SIZE);
    let _log = key_log_app(&mut app);
    let mut term = CaptureTerm::new(SIZE);
    let cfg = RunConfig {
        caps: Some(plain_caps()),
        enter: Some(EnterOptions {
            mouse: MouseMode::Off,
            kitty_keyboard: KittyFlags(0), // embedder said: no enhancement
            ..EnterOptions::default()
        }),
        probe: true,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    driver.turn(&mut app, &mut term).expect("first frame");
    let _ = term.take_bytes();

    term.push_input(b"\x1b[?1u\x1b[?62c");
    driver.turn(&mut app, &mut term).expect("probe fold turn");
    let bytes = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        !bytes.contains("\x1b[>"),
        "explicit posture: the upgrade must not push flags: {bytes:?}"
    );
    assert_eq!(term.screen().counters().kitty_push_depth, 0);
    // The FACT still reaches the app (posture and capability are
    // separate truths — hint text can say what the terminal speaks).
    assert!(current_caps().kitty_keyboard);
    driver.finish(&mut term).expect("finish");
    assert_eq!(term.screen().counters().kitty_push_depth, 0);
}

/// 0295/0685 acceptance: a `dyn_view` reading `use_caps` re-renders
/// with the truthful graphics-channel label as probe replies fold in —
/// the images example's footer, as a pinned contract.
#[test]
fn use_caps_dyn_view_rerenders_channel_label_on_probe_upgrade() {
    let mut app = App::new(SIZE);
    app.mount(move |cx| {
        let caps = use_caps(cx);
        Element::new()
            .child(dyn_view(LayoutStyle::line(1), move || {
                let c = caps.get();
                let label = match choose_channel(&c.graphics()) {
                    Channel::Kitty => "kitty",
                    Channel::Iterm2 => "iterm2",
                    Channel::Sixel => "sixel",
                    Channel::Mosaic => "mosaic",
                };
                let kbd = if c.kitty_keyboard {
                    "Shift+Enter newline"
                } else {
                    "Alt+Enter newline"
                };
                text(format!("via {label} · {kbd}"))
            }))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(SIZE);
    let cfg = RunConfig {
        caps: Some(plain_caps()),
        enter: None,
        probe: true,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    driver.turn(&mut app, &mut term).expect("first frame");
    let screen = term.screen().to_text();
    assert!(
        screen.contains("via mosaic") && screen.contains("Alt+Enter"),
        "env-pass truth on frame 1:\n{screen}"
    );

    // Kitty graphics proof + keyboard proof + sentinel fold in; the
    // label and the key hint both flip to the upgraded truth.
    term.push_input(b"\x1b_Gi=4242;OK\x1b\\\x1b[?1u\x1b[?62c");
    driver.turn(&mut app, &mut term).expect("probe fold turn");
    driver.turn(&mut app, &mut term).expect("re-render turn");
    let screen = term.screen().to_text();
    assert!(
        screen.contains("via kitty") && screen.contains("Shift+Enter"),
        "post-probe truth re-rendered:\n{screen}"
    );
    driver.finish(&mut term).expect("finish");
}
