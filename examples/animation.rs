//! animation — play a moving picture in the terminal.
//!
//! Demonstrates the animation lane end to end: decode a frame sequence
//! (animated GIF or APNG), play it with `widgets::AnimatedImage`, pause
//! and restart it, and see what the playback actually costs.
//!
//! What the engine decodes ITSELF is deliberately bounded to formats
//! that are permissively licensed, patent-free, and small enough to
//! decode honestly. VIDEO IS NOT among them: point this at a `.mp4` or
//! a `.mov` and you get the other half of the design — a labeled
//! refusal naming the container and carrying the `ffmpeg` line that
//! converts it into something playable. That is what a user sees, so
//! the example shows it as a first-class state.
//!
//! Usage:
//!   cargo run --example animation                 # a generated clip
//!   cargo run --example animation -- loading.gif  # a GIF or an APNG
//!
//! Keys: space play/pause · r restart · f/s family + fit · t theme ·
//! q quit.
//!
//! Docs: docs/graphics-and-3d.md § "Animated pictures".
//!
//! OWNER: GFX3D.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::Duration;

use abstracttui::base::Rgba;
use abstracttui::gfx::{decode_animation, Animation, Bitmap, Frame, MosaicMode};
use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::widgets::{AnimatedImage, ImageFit};

/// The generated clip: a hue wheel sweeping a bar around a ring, so
/// motion is obvious at any mosaic density.
fn generated_clip() -> Animation {
    const W: u32 = 160;
    const H: u32 = 120;
    const N: usize = 24;
    let frames = (0..N)
        .map(|i| {
            let phase = i as f32 / N as f32 * TAU;
            let image = Bitmap::from_fn(W, H, |x, y| {
                let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                let r = (dx * dx + dy * dy).sqrt() / cx;
                let a = dy.atan2(dx) - phase;
                let sweep = ((a.sin() + 1.0) * 0.5).powf(6.0);
                let ring = (1.0 - (r - 0.62).abs() * 6.0).clamp(0.0, 1.0);
                let v = (sweep * ring * 255.0) as u8;
                Rgba::rgb(
                    v.saturating_add((r * 60.0) as u8),
                    (v as f32 * 0.5) as u8 + ((1.0 - r) * 40.0) as u8,
                    255u8.saturating_sub(v / 2),
                )
            });
            Frame {
                image,
                delay: Duration::from_millis(60),
            }
        })
        .collect::<Vec<_>>();
    Animation {
        frames,
        loop_count: None, // forever
        width: W,
        height: H,
    }
}

fn main() -> abstracttui::base::Result<()> {
    // Diagnostic surface (like `images`): print the capability report
    // and exit — no tty required.
    if std::env::args().any(|a| a == "--caps") {
        println!(
            "{}",
            abstracttui::term::Capabilities::detect_env().summary()
        );
        return Ok(());
    }
    if !abstracttui::term::have_tty() {
        println!("animation: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }

    // Decode ONCE, outside the build closure: rebuilds are frequent and
    // a movie is not cheap to decode. `Arc<Animation>` makes every
    // rebuild a pointer clone.
    let path = std::env::args().nth(1).filter(|a| a != "--caps");
    let source: Result<Arc<Animation>, String> = match &path {
        None => Ok(Arc::new(generated_clip())),
        Some(p) => std::fs::read(p)
            .map_err(|e| format!("unreadable: {e}"))
            .and_then(|b| {
                decode_animation(&b)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            }),
    };
    let source_label = match (&path, &source) {
        (None, _) => "generated clip".to_string(),
        (Some(p), _) => p.clone(),
    };
    let detail = match &source {
        Ok(a) => format!(
            "{} frames · {}x{} · {:?}/pass · {}",
            a.len(),
            a.width,
            a.height,
            a.duration(),
            match a.loop_count {
                None => "loops".to_string(),
                Some(n) => format!("plays {n}x"),
            }
        ),
        // The refusal IS the interesting state for a codec we do not
        // decode: it names the codec and carries the fix.
        Err(e) => e.clone(),
    };

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let playing = cx.signal(true);
        let generation = cx.signal(0u32);
        let family = cx.signal(0usize);
        let contain = cx.signal(true);
        let theme_ix = cx.signal(0usize);
        let source = source.clone();
        let title = format!("animation — {source_label}");
        let detail = detail.clone();

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                playing.update(|p| *p = !*p)
            })
            .shortcut(KeyChord::plain(Key::Char('r')), move |_| {
                generation.update(|g| *g += 1);
                playing.set(true);
            })
            .shortcut(KeyChord::plain(Key::Char('f')), move |_| {
                family.update(|f| *f += 1)
            })
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                contain.update(|c| *c = !*c)
            })
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
            // The player lives in a SCOPED dyn_view: restarting is a
            // scope rebuild, which disposes the old clock (its timers
            // stop with it) and starts a fresh one at frame 0.
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |vcx| {
                    let _ = generation.get();
                    match &source {
                        Err(e) => {
                            let _ = vcx;
                            text(format!("⌧ {e}"))
                        }
                        Ok(a) => AnimatedImage::from_animation(a.clone())
                            .fit(if contain.get() {
                                ImageFit::Contain
                            } else {
                                ImageFit::Cover
                            })
                            .mode(FAMILIES[family.get() % FAMILIES.len()].0)
                            .playing(playing)
                            .layout(LayoutStyle::default().grow(1.0))
                            .view(vcx),
                    }
                },
            ))
            .child(dyn_view(LayoutStyle::default().h(2), move || {
                let _ = theme.get();
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text(format!(
                        "{detail} · {} · {} · {}",
                        if playing.get() { "playing" } else { "paused" },
                        FAMILIES[family.get() % FAMILIES.len()].1,
                        if contain.get() { "contain" } else { "cover" }
                    )))
                    .child(text(
                        "space play/pause · r restart · f family · s fit · t theme · q quit",
                    ))
                    .build()
            }))
            .build()
    })?;
    app.run()
}

const FAMILIES: [(MosaicMode, &str); 4] = [
    (MosaicMode::Quadrant, "quadrant 2x2"),
    (MosaicMode::Sextant, "sextant 2x3"),
    (MosaicMode::HalfBlock, "halfblock 1x2"),
    (MosaicMode::Braille, "braille 2x4"),
];
