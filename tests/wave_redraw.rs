//! first-app/0299 — the public full-redraw verb + the focus-regain
//! repaint opt-in, through the REAL frame loop (`Driver::turn` against
//! `CaptureTerm`).
//!
//! The scenario these pins encode: the terminal's content was
//! destroyed EXTERNALLY (Cmd+K, `printf '\033c'`) — the engine's
//! model still believes every cell is painted, so ordinary repaints
//! emit nothing. The verb must re-emit EVERY cell (byte evidence: the
//! verb frame's bytes alone rebuild the whole screen on a fresh VT
//! model), re-place byte-channel images, and cost nothing once the
//! healing frame is done (idle returns to zero bytes).

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rect, Rgba, Size};
use abstracttui::gfx::Bitmap;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::text;

const W: i32 = 32;
const H: i32 = 6;

fn config(caps: Capabilities) -> RunConfig {
    RunConfig {
        caps: Some(caps),
        enter: None,
        probe: false,
    }
}

fn plain_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
    })
}

fn kitty_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_graphics = true;
    })
}

/// Drive turns until idle (bounded).
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// Rows app with DISTINCT text per row (so "every cell re-emitted" is
/// checkable line by line) and Ctrl+L bound to the public verb — the
/// end-to-end component-reachable path the item asks for.
fn rows_app_with_ctrl_l(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(move |_cx| {
        let mut col = Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('l')), |_| {
                request_full_redraw()
            });
        for i in 0..size.h {
            col = col.child(text(format!("row{i:02} ####################")));
        }
        col.build()
    })
    .expect("mount");
    app
}

/// Every row's text must be reconstructible from `bytes` ALONE on a
/// fresh VT model — the proof that the frame re-emitted every cell
/// with absolute anchoring (a diff frame against a live model would
/// leave a fresh screen mostly blank).
fn assert_bytes_rebuild_whole_screen(bytes: &[u8], size: Size) {
    let mut replay = VtScreen::new(size);
    replay.feed(bytes);
    let text = replay.to_text();
    for i in 0..size.h {
        assert!(
            text.contains(&format!("row{i:02} ")),
            "row {i} missing from the replayed verb frame:\n{text}"
        );
    }
}

#[test]
fn ctrl_l_full_redraw_re_emits_every_cell_then_idles() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app_with_ctrl_l(size);
    let mut driver = Driver::new(&mut app, &mut term, config(plain_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // Steady state: an idle turn emits nothing (the damage contract).
    let turn = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(turn.idle && !turn.rendered);
    assert!(term.take_bytes().is_empty(), "idle turns must stay silent");

    // Ctrl+L (0x0c on the legacy wire) reaches the app's shortcut,
    // which calls `request_full_redraw()`; the driver drains the
    // request the SAME turn and the frame re-emits everything.
    term.push_input(b"\x0c");
    let turn = driver.turn(&mut app, &mut term).expect("verb turn");
    assert!(
        turn.rendered && turn.emitted,
        "the verb frame must render and emit: {turn:?}"
    );
    let bytes = term.take_bytes();
    assert!(!bytes.is_empty());
    assert_bytes_rebuild_whole_screen(&bytes, size);
    assert_eq!(
        term.screen().unknown_seq_count(),
        0,
        "every emitted byte is modeled"
    );

    // Idle cost returns to zero after the healing frame.
    for _ in 0..4 {
        let turn = driver.turn(&mut app, &mut term).expect("post turn");
        assert!(turn.idle && !turn.rendered, "post-verb turn: {turn:?}");
    }
    assert!(
        term.take_bytes().is_empty(),
        "the verb must not leave residual damage"
    );
}

#[test]
fn full_redraw_re_places_byte_channel_images() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app_with_ctrl_l(size);
    let mut driver = Driver::new(&mut app, &mut term, config(kitty_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let overlays = app.overlays();
    let _img = overlays.image(
        Rect::new(20, 1, 8, 3),
        Bitmap::from_fn(16, 12, |x, _| {
            if x < 8 {
                Rgba::rgb(255, 0, 0)
            } else {
                Rgba::rgb(0, 0, 255)
            }
        }),
    );
    settle(&mut driver, &mut app, &mut term);
    let placed = term.take_bytes();
    assert!(
        placed.windows(3).any(|w| w == b"\x1b_G"),
        "precondition: the image went through the kitty byte channel"
    );

    // Parked: nothing re-emits on its own.
    let turn = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(turn.idle);
    assert!(term.take_bytes().is_empty());

    // The verb straight from app-thread code (no keybinding needed).
    request_full_redraw();
    let turn = driver.turn(&mut app, &mut term).expect("verb turn");
    assert!(turn.rendered && turn.emitted, "{turn:?}");
    let bytes = term.take_bytes();
    assert!(
        bytes.windows(3).any(|w| w == b"\x1b_G"),
        "the verb must re-place byte-channel images"
    );
    // The cells re-emit too (rows visible around the image rect).
    let mut replay = VtScreen::new(size);
    replay.feed(&bytes);
    assert!(
        replay.to_text().contains("row00 "),
        "cells re-emitted alongside the image:\n{}",
        replay.to_text()
    );
}

#[test]
fn redraw_on_focus_gained_is_opt_in() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app_with_ctrl_l(size);
    let mut driver = Driver::new(&mut app, &mut term, config(plain_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // Default OFF: a focus round-trip emits nothing (existing sessions
    // stay byte-identical).
    term.push_input(b"\x1b[O\x1b[I");
    let turn = driver.turn(&mut app, &mut term).expect("focus turn");
    assert!(!turn.rendered, "default: focus-in must not repaint");
    assert!(term.take_bytes().is_empty());

    // Opt in: focus-in heals the (externally damaged) screen with one
    // full re-present.
    abstracttui::app::set_redraw_on_focus_gained(true);
    term.push_input(b"\x1b[O\x1b[I");
    let turn = driver.turn(&mut app, &mut term).expect("heal turn");
    assert!(
        turn.rendered && turn.emitted,
        "opted in: focus-in re-presents: {turn:?}"
    );
    let bytes = term.take_bytes();
    assert_bytes_rebuild_whole_screen(&bytes, size);
    // And back to zero once healed.
    let turn = driver.turn(&mut app, &mut term).expect("post turn");
    assert!(turn.idle && term.take_bytes().is_empty());

    // Restore for sibling tests on this thread (house discipline).
    abstracttui::app::set_redraw_on_focus_gained(false);
}
