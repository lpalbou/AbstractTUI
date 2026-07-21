//! images — the mosaic showcase: one image, four glyph families.
//!
//! Demonstrates: the unicode-mosaic ladder side by side (halfblock 1x2 ·
//! quadrant 2x2 · sextant 2x3 · braille 2x4) with aspect-correct fitting,
//! Floyd–Steinberg dithering (`d` toggles a 16-color pre-quantize — watch
//! gradients band and un-band), and the pixel-protocol path (`p` places
//! the image through `overlays.image`, labeled with the channel the caps
//! ladder actually chose: kitty / iterm2 / sixel / mosaic).
//!
//! Usage: `cargo run --example images -- photo.png` — PNG today (JPEG
//! joins when the decoder lands); without an argument a procedural test
//! card is generated (hue bars, luminance ramp, circle, checkerboard —
//! aliasing and banding probes).
//!
//! Keys: d dither · p protocol placement · t theme · q quit.
//!
//! OWNER: DESIGN.

mod common;

use std::rc::Rc;
use std::sync::Arc;

use abstracttui::app::{ImageHandle, Overlays};
use abstracttui::gfx::{choose_channel, dither, quantize, Channel, MosaicMode, MosaicRenderer};
use abstracttui::prelude::*;
use abstracttui::theme::themes;

const PANELS: [(MosaicMode, &str); 4] = [
    (MosaicMode::HalfBlock, "halfblock 1x2"),
    (MosaicMode::Quadrant, "quadrant 2x2"),
    (MosaicMode::Sextant, "sextant 2x3"),
    (MosaicMode::Braille, "braille 2x4"),
];

fn main() -> abstracttui::base::Result<()> {
    // Diagnostic surface: `--caps` prints the capability report and
    // exits — works everywhere, no tty required.
    if std::env::args().any(|a| a == "--caps") {
        println!(
            "{}",
            abstracttui::term::Capabilities::detect_env().summary()
        );
        return Ok(());
    }
    if !abstracttui::term::have_tty() {
        println!("images: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let (source_label, bitmap) = load_or_generate();
    let bitmap = Arc::new(bitmap);
    // Dithered twin, prepared once: 16-color median-cut palette +
    // Floyd–Steinberg (the engine's own dither pipeline).
    let dithered = Arc::new({
        let mut copy = (*bitmap).clone();
        let palette = quantize::median_cut(copy.pixels(), 16);
        let _ = dither::floyd_steinberg(&mut copy, &palette);
        copy
    });

    let caps = abstracttui::term::Capabilities::detect_env();
    let channel = choose_channel(&caps.graphics());
    let channel_label = match channel {
        Channel::Kitty => "kitty",
        Channel::Iterm2 => "iterm2",
        Channel::Sixel => "sixel",
        Channel::Mosaic => "mosaic (no pixel protocol here)",
    };

    let mut app = App::new(Size::new(110, 30));
    let quitter = app.quitter();
    let overlays = app.overlays();

    app.mount(move |cx| {
        let theme = use_theme(cx);
        let dithering = cx.signal(false);
        let protocol_on = cx.signal(false);
        let theme_ix = cx.signal(0usize);
        let placed: Rc<std::cell::RefCell<Option<ImageHandle>>> =
            Rc::new(std::cell::RefCell::new(None));

        let toggle_protocol = {
            let overlays: Overlays = overlays.clone();
            let placed = placed.clone();
            let bitmap = bitmap.clone();
            move || {
                protocol_on.update(|p| *p = !*p);
                let mut slot = placed.borrow_mut();
                if let Some(h) = slot.take() {
                    h.remove();
                    return;
                }
                // Center-right placement, sized by the image's cell
                // aspect at half-block density.
                let w = 36.min((bitmap.width() / 2).max(8) as i32);
                let h = (w / 2).max(4);
                *slot = Some(overlays.image(Rect::new(60, 4, w, h), (*bitmap).clone()));
            }
        };

        let title = format!("images — {source_label}");
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('d')), move |_| {
                dithering.update(|d| *d = !*d)
            })
            .shortcut(KeyChord::plain(Key::Char('p')), move |_| toggle_protocol())
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .child(dyn_view(LayoutStyle::default().h(1), {
                let title = title.clone();
                move || {
                    let _ = theme.get();
                    text(title.clone())
                }
            }))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let src = if dithering.get() {
                    dithered.clone()
                } else {
                    bitmap.clone()
                };
                let mut row = Element::new().style(LayoutStyle::row().gap(1));
                for (mode, label) in PANELS {
                    row = row.child(panel(&t, src.clone(), mode, label));
                }
                row.build()
            }))
            .child(dyn_view(LayoutStyle::default().h(2), move || {
                let _ = theme.get();
                let d = if dithering.get() {
                    "on (16-color FS)"
                } else {
                    "off (truecolor)"
                };
                let p = if protocol_on.get() {
                    format!("placed via {channel_label}")
                } else {
                    format!("off — would use {channel_label}")
                };
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text(format!("dither: {d}    protocol: {p}")))
                    .child(text("d dither · p protocol · t theme · q quit"))
                    .build()
            }))
            .build()
    })?;
    app.run()
}

/// One labeled mosaic panel, aspect-fit for its mode's subpixel density.
fn panel(t: &TokenSet, img: Arc<Bitmap>, mode: MosaicMode, label: &'static str) -> View {
    let ground = t.bg;
    Block::new()
        .title(label)
        .fill(t.surface)
        .layout(LayoutStyle::column().grow(1.0))
        .child(
            Element::new()
                .style(LayoutStyle::default().grow(1.0))
                .draw(move |canvas, rect| {
                    if rect.w <= 1 || rect.h <= 1 {
                        return;
                    }
                    let (sx, sy) = mode.cell_pixels();
                    // Fit: preserve the SOURCE pixel aspect through the
                    // mode's cell density (cells are sx x sy px).
                    let iw = img.width().max(1) as f32;
                    let ih = img.height().max(1) as f32;
                    let scale =
                        ((rect.w as f32 * sx as f32) / iw).min((rect.h as f32 * sy as f32) / ih);
                    let cw = ((iw * scale / sx as f32).floor() as i32).clamp(1, rect.w);
                    let ch = ((ih * scale / sy as f32).floor() as i32).clamp(1, rect.h);
                    let origin = Point::new(rect.x + (rect.w - cw) / 2, rect.y + (rect.h - ch) / 2);
                    let mut renderer = MosaicRenderer::new();
                    let grid = renderer.render(&img, cw as u32, ch as u32, mode);
                    for (pos, chr, fg, bg) in grid.cell_patches(origin) {
                        let bg = if bg.is_transparent() { ground } else { bg };
                        canvas.put(pos, chr, fg, bg);
                    }
                })
                .build(),
        )
        .element(t)
        .build()
}

fn load_or_generate() -> (String, Bitmap) {
    if let Some(path) = std::env::args().nth(1) {
        match std::fs::read(&path) {
            Ok(bytes) => match abstracttui::gfx::png::decode(&bytes) {
                Ok(bmp) => {
                    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                    return (format!("{name} ({}x{} px)", bmp.width(), bmp.height()), bmp);
                }
                Err(e) => {
                    eprintln!("images: {path}: {e:?} — showing the test card instead");
                }
            },
            Err(e) => eprintln!("images: cannot read {path}: {e} — showing the test card"),
        }
    }
    (
        "procedural test card (160x96 px)".to_string(),
        test_card(160, 96),
    )
}

/// Banding/aliasing probe: hue bars over a luminance ramp, a circle and
/// a checkerboard corner. Brand-ramp hues so the card is on-identity.
fn test_card(w: u32, h: u32) -> Bitmap {
    use abstracttui::boot::identity::brand_ramp;
    Bitmap::from_fn(w, h, |x, y| {
        let fx = x as f32 / (w - 1) as f32;
        let fy = y as f32 / (h - 1) as f32;
        // Circle probe, centered right third.
        let (cx, cy, r) = (w as f32 * 0.72, h as f32 * 0.38, h as f32 * 0.26);
        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        if d < r {
            let k = 1.0 - (d / r).powi(2);
            return brand_ramp(fx).lerp(Rgba::WHITE, k * 0.55);
        }
        // Checkerboard corner (dither probe).
        if fx > 0.82 && fy > 0.7 {
            return if (x / 2 + y / 2) % 2 == 0 {
                Rgba::rgb(20, 20, 24)
            } else {
                Rgba::rgb(235, 235, 240)
            };
        }
        // Hue bars over a vertical luminance ramp.
        let band = (fx * 5.0).floor() / 5.0;
        brand_ramp(band).lerp(Rgba::BLACK, fy * 0.85)
    })
}
