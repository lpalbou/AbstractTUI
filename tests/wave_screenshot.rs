//! control-plane/0370 — the screenshot capability, attacked end to end.
//!
//! The load-bearing proof is the FIDELITY ROUNDTRIP: a capture exported
//! with `to_ansi()`, fed back through the testing rig's independent VT
//! interpreter and re-captured, must equal the original — the exporter
//! can lie about nothing the model can see. On top: cross-source
//! agreement (composed frame vs replayed bytes through the REAL driver),
//! byte-stable goldens for all three exporters, the protocol-image
//! honesty stamp, the component-reachable verb, and measured 300x100
//! export numbers.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rect, Rgba, Size};
use abstracttui::gfx::Bitmap;
use abstracttui::prelude::*;
use abstracttui::render::{Attrs, Cell, Screenshot, Style};
use abstracttui::term::Capabilities;
use abstracttui::testing::{assert_snapshot, time_median, CaptureTerm, Rng, VtScreen};
use abstracttui::ui::text;

// ---------------------------------------------------------------- helpers

fn config(caps: Capabilities) -> RunConfig {
    RunConfig {
        caps: Some(caps),
        enter: None,
        probe: false,
    }
}

fn truecolor_caps() -> Capabilities {
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

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// Replay ANSI bytes on a fresh VT model and re-capture.
fn replay(ansi: &str, size: Size) -> Screenshot {
    let mut vt = VtScreen::new(size);
    vt.feed(ansi.as_bytes());
    assert_eq!(
        vt.unknown_seq_count(),
        0,
        "the ANSI export must stay inside the modeled set: {:?}",
        vt.unknown_samples()
    );
    vt.screenshot()
}

/// A small themed scene through the REAL driver — the same pipeline
/// production uses (theme block, border, CJK, emoji, ZWJ cluster).
fn themed_scene(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let t = theme.get().tokens;
        Block::new()
            .border(BorderKind::Rounded)
            .title("shot")
            .fill(t.surface)
            .layout(LayoutStyle::column().padding(Edges::all(1)))
            .child(text("status: ready 世界 🚀"))
            .child(text(
                "family 👨\u{200D}👩\u{200D}👧\u{200D}👦 flag 🇫🇷 tone 👍🏽",
            ))
            .child(text("plain tail"))
            .element(&t)
            .build()
    })
    .expect("mount");
    app
}

// ---------------------------------------------------- (a) the roundtrip

#[test]
fn roundtrip_hand_built_style_zoo() {
    let size = Size::new(40, 8);
    let mut s = abstracttui::render::Surface::new(size, Cell::EMPTY);
    s.draw_text(0, 0, "bold", Style::new().fg(Rgba::rgb(255, 80, 80)).bold());
    s.draw_text(5, 0, "dim", Style::new().dim());
    s.draw_text(9, 0, "it", Style::new().italic());
    s.draw_text(
        12,
        0,
        "ul",
        Style::new()
            .underline()
            .underline_color(Rgba::rgb(0, 0, 255)),
    );
    s.draw_text(15, 0, "curl", Style::new().attrs(Attrs::UNDERCURL));
    s.draw_text(
        20,
        0,
        "both",
        Style::new().attrs(Attrs::UNDERLINE | Attrs::UNDERCURL),
    );
    s.draw_text(
        0,
        1,
        "rev",
        Style::new()
            .fg(Rgba::rgb(1, 2, 3))
            .bg(Rgba::rgb(200, 200, 0))
            .reverse(),
    );
    s.draw_text(4, 1, "strike", Style::new().strike());
    s.draw_text(11, 1, "blink", Style::new().attrs(Attrs::BLINK));
    s.draw_text(17, 1, "hidden", Style::new().attrs(Attrs::HIDDEN));
    s.draw_text(0, 2, "世界 é e\u{0301} ❤\u{FE0F} 🇫🇷 👍🏽", Style::new());
    s.draw_text(
        0,
        3,
        "👨\u{200D}👩\u{200D}👧\u{200D}👦 pooled cluster",
        Style::new(),
    );
    // Colored trailing run (must survive the trim) + wide glyph at the
    // right edge (wrap-pending hazard on replay).
    s.fill_rect(
        Rect::new(34, 4, 6, 1),
        Cell::EMPTY.with_bg(Rgba::rgb(0, 60, 0)),
    );
    s.draw_text(38, 5, "世", Style::new());
    // Underline color WITHOUT an underline: wire state a terminal keeps.
    s.draw_text(
        0,
        6,
        "ul58",
        Style::new().underline_color(Rgba::rgb(9, 9, 9)),
    );

    let shot = Screenshot::from_surface(&s);
    let back = replay(&shot.to_ansi(), size);
    assert_eq!(shot, back, "ANSI replay must reproduce the capture exactly");
}

#[test]
fn roundtrip_seeded_style_fuzz() {
    let glyphs: &[&str] = &[
        " ",
        "a",
        "Z",
        "0",
        "~",
        "|",
        "é",
        "ß",
        "世",
        "界",
        "🚀",
        "❤\u{FE0F}",
        "🇫🇷",
        "👍🏽",
        "e\u{0301}",
        "👨\u{200D}👩\u{200D}👧\u{200D}👦",
        "\u{FFFD}",
        "🇫",
        "a\u{200D}",
    ];
    let colors: &[Option<Rgba>] = &[
        None,
        Some(Rgba::rgb(255, 0, 0)),
        Some(Rgba::rgb(0, 255, 128)),
        Some(Rgba::rgb(10, 10, 10)),
        Some(Rgba::rgb(240, 240, 240)),
    ];
    let size = Size::new(24, 6);
    for seed in 0..24u64 {
        let mut rng = Rng::new(0xC0FFEE ^ seed);
        let mut s = abstracttui::render::Surface::new(size, Cell::EMPTY);
        for _ in 0..80 {
            let x = rng.below(size.w as usize) as i32;
            let y = rng.below(size.h as usize) as i32;
            let mut style =
                Style::new().attrs(Attrs::from_bits_truncate((rng.next_u32() & 0x01FF) as u16));
            if let Some(fg) = rng.pick(colors) {
                style = style.fg(*fg);
            }
            if let Some(bg) = rng.pick(colors) {
                style = style.bg(*bg);
            }
            if rng.chance(1, 4) {
                style = style.underline_color(Rgba::rgb(0, 0, 255));
            }
            s.draw_text(x, y, rng.pick(glyphs), style);
        }
        let shot = Screenshot::from_surface(&s);
        let back = replay(&shot.to_ansi(), size);
        assert_eq!(shot, back, "seed {seed}: replay diverged");
    }
}

#[test]
fn adjacent_fusion_attackers_do_not_fuse_on_replay() {
    // First, the hazard is REAL: without a re-anchor, a terminal (and
    // the VT model, which models exactly this) fuses a trailing-ZWJ
    // cluster with the glyph that follows it.
    let mut naive = VtScreen::new(Size::new(12, 1));
    naive.feed("\x1b[0ma\u{200D}x".as_bytes());
    assert_eq!(
        naive.cell(0, 0).unwrap().display(),
        "a\u{200D}x",
        "premise: unanchored adjacency fuses"
    );

    // The exporter's CUP re-anchor keeps every cell its own: trailing
    // ZWJ before a glyph, lone regional indicators side by side, an
    // ambiguous-width Latin-1 char — all same-style, so no SGR escape
    // accidentally breaks the join state for us.
    let size = Size::new(12, 3);
    let mut s = abstracttui::render::Surface::new(size, Cell::EMPTY);
    s.draw_text(0, 0, "a\u{200D}", Style::new());
    s.draw_text(1, 0, "x", Style::new());
    s.draw_text(0, 1, "🇫", Style::new());
    s.draw_text(1, 1, "🇷", Style::new());
    s.draw_text(0, 2, "é", Style::new());
    s.draw_text(1, 2, "b", Style::new());
    let shot = Screenshot::from_surface(&s);
    let back = replay(&shot.to_ansi(), size);
    assert_eq!(shot, back, "anchored replay must not fuse cells");
    assert_eq!(back.cell(1, 0).unwrap().text(), "x");
    assert_eq!(back.cell(0, 1).unwrap().text(), "🇫");
    assert_eq!(back.cell(1, 1).unwrap().text(), "🇷");
}

// -------------------------------- cross-source agreement (real driver)

#[test]
fn driver_and_vt_model_capture_the_same_screen() {
    let size = Size::new(44, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = themed_scene(size);
    let mut driver = Driver::new(&mut app, &mut term, config(truecolor_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(term.screen().unknown_seq_count(), 0);

    let from_frame = driver.screenshot();
    let from_bytes = term.screen().screenshot();
    assert_eq!(
        from_frame, from_bytes,
        "the composed frame and the replayed bytes are one truth"
    );
    assert!(from_frame.to_text().contains("status: ready 世界 🚀"));

    // And the roundtrip holds for a REAL presenter-produced screen too.
    let back = replay(&from_bytes.to_ansi(), size);
    assert_eq!(from_bytes, back);
}

// ------------------------------------------- (e) goldens, byte-stable

#[test]
fn golden_scene_pins_all_three_exports() {
    let prev = abstracttui::app::set_theme(abstracttui::theme::get("nord").expect("nord"));
    let size = Size::new(40, 7);
    let mut term = CaptureTerm::new(size);
    let mut app = themed_scene(size);
    let mut driver = Driver::new(&mut app, &mut term, config(truecolor_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let shot = driver.screenshot();
    abstracttui::app::set_theme(prev); // restore for sibling tests

    assert_snapshot("screenshot_scene_text", &shot.to_text());
    assert_snapshot("screenshot_scene_ansi", &shot.to_ansi());
    assert_snapshot("screenshot_scene_svg", &shot.to_svg());
}

// ------------------------------------- (f) protocol-image honesty case

#[test]
fn byte_channel_images_stamp_labeled_pixel_regions() {
    let size = Size::new(32, 6);
    let mut term = CaptureTerm::new(size);
    let mut app = themed_scene(size);
    let mut driver = Driver::new(&mut app, &mut term, config(kitty_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let overlays = app.overlays();
    let rect = Rect::new(20, 1, 8, 3);
    let _img = overlays.image(
        rect,
        Bitmap::from_fn(16, 12, |x, _| {
            if x < 8 {
                Rgba::rgb(255, 0, 0)
            } else {
                Rgba::rgb(0, 0, 255)
            }
        }),
    );
    settle(&mut driver, &mut app, &mut term);
    assert!(
        term.take_bytes().windows(3).any(|w| w == b"\x1b_G"),
        "precondition: the image rode the kitty byte channel"
    );

    let shot = driver.screenshot();
    assert_eq!(
        shot.pixel_regions(),
        &[rect],
        "the placement bookkeeping must stamp the capture"
    );
    let svg = shot.to_svg();
    assert!(
        svg.contains("image (pixels)"),
        "the SVG labels the region instead of pretending cells are the picture"
    );
    // The VT-side capture honestly carries NO regions (the rig consumes
    // protocol payloads as counted, unmodeled string frames).
    assert!(term.screen().screenshot().pixel_regions().is_empty());

    // Mosaic images are cells and must NOT stamp a region.
    drop(_img);
    settle(&mut driver, &mut app, &mut term);
    let mut term2 = CaptureTerm::new(size);
    let mut app2 = themed_scene(size);
    let mut driver2 = Driver::new(&mut app2, &mut term2, config(truecolor_caps())).expect("enter");
    settle(&mut driver2, &mut app2, &mut term2);
    let overlays2 = app2.overlays();
    let _img2 = overlays2.image(rect, Bitmap::from_fn(16, 12, |_, _| Rgba::rgb(0, 200, 0)));
    settle(&mut driver2, &mut app2, &mut term2);
    assert!(
        driver2.screenshot().pixel_regions().is_empty(),
        "mosaic lives in the cell model — no veil"
    );
}

// ------------------------------------------ the verb, end to end

#[test]
fn request_screenshot_serves_key_handler_same_turn_then_idles() {
    let size = Size::new(40, 7);
    let mut term = CaptureTerm::new(size);
    let captured: Rc<RefCell<Option<Screenshot>>> = Rc::new(RefCell::new(None));
    let sink = captured.clone();
    let mut app = App::new(size);
    app.mount(move |_cx| {
        let sink = sink.clone();
        Element::new()
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                let sink = sink.clone();
                abstracttui::app::request_screenshot(move |shot| {
                    *sink.borrow_mut() = Some(shot);
                });
            })
            .child(text("scene for the verb"))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config(truecolor_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    term.push_input(b"s");
    let turn = driver.turn(&mut app, &mut term).expect("verb turn");
    assert_eq!(turn.events, 1);
    let shot = captured.borrow_mut().take().expect("served the same turn");
    assert!(
        shot.to_text().contains("scene for the verb"),
        "the callback sees the screen as last presented:\n{}",
        shot.to_text()
    );
    assert!(
        !turn.emitted && term.take_bytes().is_empty(),
        "a pure capture emits no bytes: {turn:?}"
    );

    // Zero-idle preserved: nothing pending after the serve.
    for _ in 0..4 {
        let turn = driver.turn(&mut app, &mut term).expect("post turn");
        assert!(turn.idle && !turn.rendered, "{turn:?}");
    }
    assert!(term.take_bytes().is_empty());

    // The write conveniences produce the files a test artifact needs.
    let dir = std::env::temp_dir().join("abstracttui-wave-screenshot");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let svg_path = dir.join("verb.svg");
    let shot2 = driver.screenshot();
    shot2.write_svg(&svg_path).expect("write svg");
    let written = std::fs::read_to_string(&svg_path).expect("read back");
    assert_eq!(written, shot2.to_svg());
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------- (g) determinism on an unchanged screen

#[test]
fn unchanged_screen_captures_byte_identically() {
    let size = Size::new(40, 7);
    let mut term = CaptureTerm::new(size);
    let mut app = themed_scene(size);
    let mut driver = Driver::new(&mut app, &mut term, config(truecolor_caps())).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let first = driver.screenshot();
    let turn = driver.turn(&mut app, &mut term).expect("idle turn");
    assert!(turn.idle);
    let second = driver.screenshot();
    assert_eq!(first, second);
    assert_eq!(first.to_text(), second.to_text());
    assert_eq!(first.to_ansi(), second.to_ansi());
    assert_eq!(first.to_svg(), second.to_svg());
}

// ------------------------------- (c) big-screen numbers, printed

#[test]
fn huge_screen_export_costs_are_printed_and_sane() {
    let size = Size::new(300, 100);
    let mut s = abstracttui::render::Surface::new(size, Cell::EMPTY);
    // Realistically busy: full text coverage, a style change every ~10
    // columns, wide glyphs sprinkled on every 7th row.
    for y in 0..size.h {
        for chunk in 0..(size.w / 10) {
            let x = chunk * 10;
            let style = Style::new()
                .fg(Rgba::rgb(
                    (chunk * 23 % 255) as u8,
                    128,
                    (y * 11 % 255) as u8,
                ))
                .bg(Rgba::rgb(16, (chunk * 5 % 64) as u8, 32));
            let word = if y % 7 == 0 && chunk % 3 == 0 {
                "世界 data 界"
            } else {
                "abcdefghij"
            };
            s.draw_text(
                x,
                y,
                word,
                if chunk % 4 == 0 { style.bold() } else { style },
            );
        }
    }
    let capture = time_median("screenshot 300x100 from_surface", 1, 3, 2, |_| {
        abstracttui::testing::sink(Screenshot::from_surface(&s));
    });
    let shot = Screenshot::from_surface(&s);
    let text = time_median("screenshot 300x100 to_text", 1, 3, 2, |_| {
        abstracttui::testing::sink(shot.to_text());
    });
    let ansi = time_median("screenshot 300x100 to_ansi", 1, 3, 2, |_| {
        abstracttui::testing::sink(shot.to_ansi());
    });
    let svg = time_median("screenshot 300x100 to_svg", 1, 3, 2, |_| {
        abstracttui::testing::sink(shot.to_svg());
    });
    println!("{}", capture.report());
    println!("{}", text.report());
    println!("{}", ansi.report());
    println!("{}", svg.report());
    println!(
        "sizes: text {} B, ansi {} B, svg {} B",
        shot.to_text().len(),
        shot.to_ansi().len(),
        shot.to_svg().len()
    );
    // Generous CI budgets — these are captures, not frame paths; the
    // point is "fast enough to sprinkle through a test suite".
    let budget = std::time::Duration::from_millis(250);
    capture.assert_under(budget);
    text.assert_under(budget);
    ansi.assert_under(budget);
    svg.assert_under(budget);

    // Fidelity at scale, too.
    let back = replay(&shot.to_ansi(), size);
    assert_eq!(shot, back, "300x100 replay diverged");
}
