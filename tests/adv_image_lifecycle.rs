//! MEDIA study-2 attack: image-overlay LIFECYCLE truth on every channel.
//!
//! The cycle-4 test (`adv_overlay::image_overlay_renders_moves_and_clears`)
//! asserted that a moved image PAINTS at its new rect — never that the OLD
//! rect restores. These tests close that hole for each channel class:
//!
//! - mosaic: patches live in the ROOT SURFACE; a move/remove must repaint
//!   the vacated cells from tree truth (screen-assertable, VtScreen models
//!   the cells);
//! - sixel / iTerm2 (cursor-paint byte channels): the image overdraws
//!   terminal cells the diff model never changed — a move/remove must
//!   FORCE re-emission of the vacated cells (byte-assertable: the row text
//!   under the image reappears in the frame bytes);
//! - kitty: a move must produce exactly ONE live placement (placement-id
//!   replace semantics), refereed by the KittyModel.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{PixelSize, Rect, Rgba, Size};
use abstracttui::gfx::Bitmap;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::Signal;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, KittyModel, VtScreen};
use abstracttui::ui::{dyn_view, text, Element};

const W: i32 = 40;
const H: i32 = 12;

/// Base app: every row carries distinct text so a vacated image rect is
/// distinguishable from "blank happened to be there".
fn rows_app(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(move |_cx| {
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..H {
            col = col.child(text(format!("row{i:02} ##########################")));
        }
        col.build()
    })
    .expect("mount");
    app
}

fn config(caps: Capabilities) -> RunConfig {
    RunConfig {
        caps: Some(caps),
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
    panic!("loop failed to settle");
}

fn img() -> Bitmap {
    Bitmap::from_fn(16, 12, |x, _| {
        if x < 8 {
            Rgba::rgb(255, 0, 0)
        } else {
            Rgba::rgb(0, 0, 255)
        }
    })
}

// ---------------------------------------------------------------------------
// Mosaic: vacated cells restore from tree truth (screen-level proof).
// ---------------------------------------------------------------------------

#[test]
fn mosaic_move_and_remove_restore_the_vacated_cells() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let before_r4 = term.screen().to_text().lines().nth(4).unwrap().to_string();
    assert!(before_r4.starts_with("row04"), "{before_r4:?}");

    let overlays = app.overlays();
    let handle = overlays.image(Rect::new(2, 3, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let covered = term.screen().to_text().lines().nth(4).unwrap().to_string();
    assert_ne!(covered, before_r4, "image must overdraw row 4");

    // MOVE: the old rect must restore to the row text.
    handle.set_rect(Rect::new(24, 6, 10, 4));
    settle(&mut driver, &mut app, &mut term);
    let after_move_r4 = term.screen().to_text().lines().nth(4).unwrap().to_string();
    assert_eq!(
        after_move_r4,
        before_r4,
        "vacated mosaic cells must repaint from tree truth:\n{}",
        term.screen().to_styled_dump()
    );

    // REMOVE: the second rect restores too.
    let before_r7 = "row07";
    handle.remove();
    settle(&mut driver, &mut app, &mut term);
    let after_remove_r7 = term.screen().to_text().lines().nth(7).unwrap().to_string();
    assert!(
        after_remove_r7.starts_with(before_r7),
        "removed mosaic image must restore cells: {after_remove_r7:?}"
    );
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

// ---------------------------------------------------------------------------
// Sixel (cursor-paint byte channel): vacated cells must RE-EMIT even though
// the cell model never changed (the terminal shows image pixels there).
// ---------------------------------------------------------------------------

#[test]
fn sixel_move_and_remove_force_cell_reemission_under_the_old_rect() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.sixel = true;
        c.cell_pixel_size = Some(PixelSize::new(8, 16));
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    let overlays = app.overlays();
    let handle = overlays.image(Rect::new(2, 3, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let placed = term.take_bytes();
    assert!(
        placed.windows(2).any(|w| w == b"\x1bP"),
        "sixel DCS emitted on placement"
    );

    // MOVE: the frame bytes must repaint the text under the OLD rect —
    // the terminal shows sixel pixels there and only a re-emission
    // erases them (diff equality would otherwise suppress it).
    handle.set_rect(Rect::new(24, 6, 10, 4));
    settle(&mut driver, &mut app, &mut term);
    let moved = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    assert!(
        moved.contains("row04") || moved.contains("w04"),
        "vacated sixel cells must re-emit row text; bytes: {moved:?}"
    );

    // REMOVE: same contract for the final rect (x=24..34, rows 6..10 —
    // those columns hold only the '#' fill, so the proof is the CUP
    // targets: every vacated row must be re-addressed and repainted).
    handle.remove();
    settle(&mut driver, &mut app, &mut term);
    let removed = String::from_utf8_lossy(&term.take_bytes()).into_owned();
    for row in 7..=10 {
        // 1-based CUP rows for screen rows 6..10, columns ≥ 25.
        assert!(
            removed.contains(&format!("\u{1b}[{row};25H")),
            "vacated row {row} must repaint after remove; bytes: {removed:?}"
        );
    }
    assert!(
        removed.contains("####"),
        "vacated cells repaint their true content; bytes: {removed:?}"
    );
    assert_eq!(
        term.screen().unknown_seq_count(),
        0,
        "DCS is modeled traffic"
    );
}

// ---------------------------------------------------------------------------
// Kitty: a session-driven move keeps exactly ONE live placement.
// ---------------------------------------------------------------------------

#[test]
fn kitty_session_move_keeps_exactly_one_visible_placement() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_graphics = true;
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    let overlays = app.overlays();
    let handle = overlays.image(Rect::new(2, 3, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let mut model = KittyModel::new();
    model.feed(&term.take_bytes());

    // Three moves: placements must REPLACE, never accumulate — an
    // accumulated placement is a visible ghost at the old rect on any
    // spec-correct terminal (kitty, ghostty).
    for rect in [
        Rect::new(20, 2, 10, 4),
        Rect::new(5, 7, 10, 4),
        Rect::new(28, 1, 10, 4),
    ] {
        handle.set_rect(rect);
        settle(&mut driver, &mut app, &mut term);
        model.feed(&term.take_bytes());
        let placed = model.placed_ids();
        assert_eq!(placed.len(), 1, "one image id placed: {placed:?}");
        let st = model.image(placed[0]).expect("state");
        assert_eq!(
            st.placements, 1,
            "moves must replace the placement, not accumulate ghosts"
        );
        assert_eq!(st.transmits, 1, "moves must never retransmit pixels");
    }

    handle.remove();
    settle(&mut driver, &mut app, &mut term);
    model.feed(&term.take_bytes());
    assert!(model.live_data_ids().is_empty(), "upload freed on remove");
    assert!(
        model.placed_ids().is_empty(),
        "no placement survives remove"
    );
    assert!(model.violations.is_empty(), "{:?}", model.violations);
}

// ---------------------------------------------------------------------------
// Kitty, TWO images: the fixed placement id `p=1` is scoped per image id
// `i` (placements key on the (i, p) PAIR per the spec), so concurrent
// images never collide — each move replaces only its own placement.
// ---------------------------------------------------------------------------

#[test]
fn kitty_two_images_move_and_remove_independently() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = rows_app(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_graphics = true;
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    let overlays = app.overlays();
    let a = overlays.image(Rect::new(1, 1, 8, 3), img());
    let b = overlays.image(Rect::new(12, 1, 8, 3), img());
    settle(&mut driver, &mut app, &mut term);
    let mut model = KittyModel::new();
    model.feed(&term.take_bytes());
    let ids = model.live_data_ids();
    assert_eq!(ids.len(), 2, "two uploads expected: {ids:?}");

    // Both images move each round: every image id must keep EXACTLY one
    // live placement. A p=1 collision ACROSS ids would fold both images
    // onto one placement slot; pid-less puts would accumulate ghosts —
    // either failure shows up in the per-id placement census.
    for (ra, rb) in [
        (Rect::new(1, 5, 8, 3), Rect::new(12, 5, 8, 3)),
        (Rect::new(22, 1, 8, 3), Rect::new(30, 5, 8, 3)),
        (Rect::new(2, 8, 8, 3), Rect::new(14, 8, 8, 3)),
    ] {
        a.set_rect(ra);
        b.set_rect(rb);
        settle(&mut driver, &mut app, &mut term);
        model.feed(&term.take_bytes());
        assert_eq!(model.placed_ids(), ids, "both images must stay placed");
        for &id in &ids {
            let st = model.image(id).expect("state");
            assert_eq!(
                st.placements, 1,
                "image {id}: p=1 must replace within its OWN id, never leak to the sibling"
            );
            assert_eq!(st.transmits, 1, "image {id}: a move must never retransmit");
        }
    }

    // Remove ONE: the other image's upload and placement must survive
    // (a delete keyed on the wrong id would kill the sibling).
    a.remove();
    settle(&mut driver, &mut app, &mut term);
    model.feed(&term.take_bytes());
    assert_eq!(
        model.live_data_ids().len(),
        1,
        "exactly one upload survives: {:?}",
        model.live_data_ids()
    );
    let placed = model.placed_ids();
    assert_eq!(
        placed.len(),
        1,
        "exactly one placement survives: {placed:?}"
    );
    assert_eq!(model.image(placed[0]).unwrap().placements, 1);

    b.remove();
    settle(&mut driver, &mut app, &mut term);
    model.feed(&term.take_bytes());
    assert!(model.live_data_ids().is_empty(), "everything freed");
    assert!(model.placed_ids().is_empty());
    assert!(model.violations.is_empty(), "{:?}", model.violations);
}

// ---------------------------------------------------------------------------
// Scroll guard scoping: live BYTE-channel images force the plain diff
// (terminals scroll protocol pixels WITH the text — kitty mandates it —
// which would desync the session's placement bookkeeping); MOSAIC images
// live in the cell model and must NOT disable the optimization
// (`ImageSession::live_byte_slots` filters them out — session.rs).
// ---------------------------------------------------------------------------

/// Log-scroll app: `top` selects the H-line window over a synthetic log;
/// each increment is a pure one-row shift — the scroll-detection shape
/// (full-width damage, one vertical band).
fn log_app(size: Size) -> (App, Signal<usize>) {
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let top = cx.signal(0usize);
        wiring = Some(top);
        dyn_view(LayoutStyle::default().grow(1.0), move || {
            let top = top.get();
            let mut col = Element::new().style(LayoutStyle::column());
            for i in 0..size.h as usize {
                col = col.child(text(format!("line {:04} ######################", top + i)));
            }
            col.build()
        })
    })
    .expect("mount");
    (app, wiring.expect("top signal"))
}

/// Advance the log by one row, settle, return the frame's bytes.
fn scroll_step(
    driver: &mut Driver,
    app: &mut App,
    term: &mut CaptureTerm,
    top: &Signal<usize>,
) -> Vec<u8> {
    top.update(|t| *t += 1);
    settle(driver, app, term);
    term.take_bytes()
}

/// The scroll optimization's unforgeable signature: `emit_shift` ends
/// with the bare DECSTBM reset `ESC [ r` — no other presenter path
/// emits it (CUP/SGR always carry parameters before their final byte).
fn has_scroll_sequence(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|w| w == b"\x1b[r")
}

#[test]
fn scroll_guard_scopes_to_byte_channel_images_only() {
    let size = Size::new(W, H);

    // Leg 1 — no image: the log scroll must engage DECSTBM + SU.
    let (mut app, top) = log_app(size);
    let mut term = CaptureTerm::new(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_graphics = true; // caps alone must not disable anything
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();
    let mut leg1 = Vec::new();
    for i in 0..6 {
        let bytes = scroll_step(&mut driver, &mut app, &mut term, &top);
        assert!(
            has_scroll_sequence(&bytes),
            "leg1 frame {i}: scroll optimization must engage with no image live"
        );
        leg1.push(bytes.len());
    }

    // Leg 2 — parked KITTY image: the guard must force the plain diff
    // (no shift sequence), and the PARKED image must add zero bytes.
    let (mut app, top) = log_app(size);
    let mut term = CaptureTerm::new(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
        c.kitty_graphics = true;
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let overlays = app.overlays();
    let _img = overlays.image(Rect::new(28, 2, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let placed = term.take_bytes();
    assert!(
        placed.windows(3).any(|w| w == b"\x1b_G"),
        "precondition: the image went through the kitty byte channel"
    );
    let mut leg2 = Vec::new();
    for i in 0..6 {
        let bytes = scroll_step(&mut driver, &mut app, &mut term, &top);
        assert!(
            !has_scroll_sequence(&bytes),
            "leg2 frame {i}: a live byte-channel image must force the plain diff \
             (a terminal-executed scroll would move the placement out from under \
             the session's bookkeeping)"
        );
        assert!(
            !bytes.windows(3).any(|w| w == b"\x1b_G"),
            "leg2 frame {i}: a PARKED image must not re-emit protocol bytes"
        );
        assert!(!bytes.is_empty(), "leg2 frame {i}: the rows did change");
        leg2.push(bytes.len());
    }

    // Leg 3 — parked MOSAIC image: lives in the cell model, scrolls with
    // the diff like any other cells — the optimization must stay ON.
    let (mut app, top) = log_app(size);
    let mut term = CaptureTerm::new(size);
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true; // no protocol bits -> mosaic channel
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let overlays = app.overlays();
    let _img = overlays.image(Rect::new(28, 2, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();
    let mut leg3 = Vec::new();
    for i in 0..6 {
        let bytes = scroll_step(&mut driver, &mut app, &mut term, &top);
        assert!(
            has_scroll_sequence(&bytes),
            "leg3 frame {i}: a mosaic image must NOT disable the scroll optimization"
        );
        leg3.push(bytes.len());
    }

    let med = |v: &[usize]| {
        let mut s = v.to_vec();
        s.sort_unstable();
        s[s.len() / 2]
    };
    eprintln!(
        "scroll-guard bytes/frame @ {W}x{H}: no image {} B (scrolled) | parked kitty {} B (plain) | parked mosaic {} B (scrolled)",
        med(&leg1),
        med(&leg2),
        med(&leg3)
    );
}

// ---------------------------------------------------------------------------
// Beneath-repaint survival: a PARKED mosaic image must outlive content
// changes under its rect. The tree's clear+redraw erases the blitted
// patches in the root surface; the driver must re-blit the placement
// (its cells are engine-owned) or a parked image corrodes row by row.
// ---------------------------------------------------------------------------

/// Dump glyph + paint for a cell range of one row (screen truth).
fn cells_dump(screen: &VtScreen, y: i32, x0: i32, x1: i32) -> String {
    let mut out = String::new();
    for x in x0..x1 {
        let c = screen.cell(x, y).unwrap();
        out.push_str(&format!("{}:{:?}:{:?};", c.ch(), c.paint.fg, c.paint.bg));
    }
    out
}

#[test]
fn mosaic_image_survives_content_repaint_beneath_it() {
    let size = Size::new(W, H);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let tick = cx.signal(0usize);
        wiring = Some(tick);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..H {
            if i == 4 {
                // The ticking row: its counter sits OUTSIDE the image
                // columns so the repaint is observable beside the image.
                col = col.child(dyn_view(LayoutStyle::default().h(1), move || {
                    text(format!("############ tick {:03}", tick.get()))
                }));
            } else {
                col = col.child(text(format!("row{i:02} ##########################")));
            }
        }
        col.build()
    })
    .expect("mount");
    let tick = wiring.expect("tick signal");
    let caps = Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true; // mosaic channel
    });
    let mut driver = Driver::new(&mut app, &mut term, config(caps)).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let before_image = cells_dump(term.screen(), 4, 2, 12);

    let overlays = app.overlays();
    // Rows 3..7, columns 2..12 — covering the ticking row 4.
    let _img = overlays.image(Rect::new(2, 3, 10, 4), img());
    settle(&mut driver, &mut app, &mut term);
    let with_image = cells_dump(term.screen(), 4, 2, 12);
    assert_ne!(
        with_image, before_image,
        "precondition: the image overdraws row 4"
    );

    for t in 1..=3usize {
        tick.set(t);
        settle(&mut driver, &mut app, &mut term);
        // The repaint really happened beside the image...
        let row = term.screen().to_text().lines().nth(4).unwrap().to_string();
        assert!(
            row.contains(&format!("tick {t:03}")),
            "tick {t}: the row under the image must repaint from truth: {row:?}"
        );
        // ...and the image cells above it survived.
        assert_eq!(
            cells_dump(term.screen(), 4, 2, 12),
            with_image,
            "tick {t}: a parked mosaic image must survive a repaint beneath it \
             (the tree redraw erased the blitted patches and nothing re-blitted them)"
        );
    }
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
