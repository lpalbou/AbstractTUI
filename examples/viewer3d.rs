//! viewer3d — the 3D flagship: orbit a GLB in themed chrome.
//!
//! Demonstrates: `three` GLB loading (textures included once decoders
//! allow), `Viewport3D` in a titled Block, drag-to-orbit + wheel zoom,
//! mosaic mode switching, live light steering, a MEASURED fps readout
//! (frames actually painted over each 1 s window — not a wish), theme
//! cycling, auto-spin.
//!
//! Usage: `cargo run --example viewer3d -- path/to/model.glb`
//! (without an argument it tries the workspace's test assets and prints
//! friendly instructions when none exist).
//!
//! Keys: drag orbit · wheel zoom · space spin · 1/2/3/4 mode
//! (half/quadrant/sextant/braille) · l/L light · r reset · t theme ·
//! q quit.
//!
//! OWNER: DESIGN.

mod common;

use std::cell::Cell as StdCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use abstracttui::gfx::MosaicMode;
use abstracttui::prelude::*;
use abstracttui::reactive::after;
use abstracttui::theme::themes;
use abstracttui::three::{Light, Model, Vec3};

/// Workspace assets the engine's own tests render (tried in order when
/// no path is given).
const DEFAULT_ASSETS: [&str; 2] = [
    "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb",
    "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb",
];

const MODES: [(MosaicMode, &str); 4] = [
    (MosaicMode::HalfBlock, "halfblock"),
    (MosaicMode::Quadrant, "quadrant"),
    (MosaicMode::Sextant, "sextant"),
    (MosaicMode::Braille, "braille"),
];

const SPIN_STEP: Duration = Duration::from_millis(33);

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
        println!("viewer3d: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let Some((path, bytes)) = find_asset() else {
        println!("viewer3d: no model found.");
        println!("  usage: cargo run --example viewer3d -- path/to/model.glb");
        println!("  (GLB with embedded buffers; the engine's test assets live at)");
        for p in DEFAULT_ASSETS {
            println!("    {p}");
        }
        return Ok(());
    };
    let model = match Model::load(&bytes) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            println!("viewer3d: could not load {path}: {e:?}");
            return Ok(());
        }
    };
    let title = format!(
        "{} — {} triangles",
        path.rsplit('/').next().unwrap_or(&path),
        model.triangle_count()
    );

    let mut app = App::new(Size::new(100, 30));
    // Honest-degradation UX: degradations fold into one warn-ink footer
    // line rather than toasts — a viewer wants its canvas quiet. The
    // line is reactive (REACT's notices bridge), so engine notices
    // pushed AFTER mount (input path) appear too. `caps: …` summary
    // lines stay off the glass — `--caps` prints the full report.
    let caps = abstracttui::term::Capabilities::detect_env();
    if !caps.truecolor {
        app.push_startup_notice("render: 256-color quantization (no truecolor)");
    }
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let yaw = cx.signal(0.6f32);
        let pitch = cx.signal(0.35f32);
        let zoom = cx.signal(1.0f32);
        let spin = cx.signal(0.0f32);
        let spinning = cx.signal(true);
        let mode_ix = cx.signal(0usize);
        let light_az = cx.signal(0.9f32);
        let theme_ix = cx.signal(0usize);
        // Painted-frame counter (draw-side Cell) folded into fps once a
        // second — the number is measured, never assumed.
        let frames = Rc::new(StdCell::new(0u32));
        let fps = cx.signal(0u32);

        {
            let frames = frames.clone();
            fn fps_loop(frames: Rc<StdCell<u32>>, fps: Signal<u32>) {
                after(Duration::from_secs(1), move || {
                    fps.set(frames.replace(0));
                    fps_loop(frames, fps);
                });
            }
            fps_loop(frames.clone(), fps);
        }
        {
            fn spin_loop(spin: Signal<f32>, spinning: Signal<bool>) {
                after(SPIN_STEP, move || {
                    if spinning.get_untracked() {
                        spin.update(|s| *s += 0.02);
                    }
                    spin_loop(spin, spinning);
                });
            }
            spin_loop(spin, spinning);
        }

        let title = title.clone();
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                spinning.update(|s| *s = !*s)
            })
            .shortcut(KeyChord::plain(Key::Char('r')), move |_| {
                yaw.set(0.6);
                pitch.set(0.35);
                zoom.set(1.0);
                spin.set(0.0);
            })
            .shortcut(KeyChord::plain(Key::Char('l')), move |_| {
                light_az.update(|a| *a += 0.26)
            })
            .shortcut(KeyChord::plain(Key::Char('L')), move |_| {
                light_az.update(|a| *a -= 0.26)
            })
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .shortcut(KeyChord::plain(Key::Char('1')), move |_| mode_ix.set(0))
            .shortcut(KeyChord::plain(Key::Char('2')), move |_| mode_ix.set(1))
            .shortcut(KeyChord::plain(Key::Char('3')), move |_| mode_ix.set(2))
            .shortcut(KeyChord::plain(Key::Char('4')), move |_| mode_ix.set(3))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let (mode, mode_name) = MODES[mode_ix.get() % MODES.len()];
                let az = light_az.get();
                let frames = frames.clone();
                let status = format!(
                    "{mode_name}  ·  {} fps  ·  zoom {:.2}x  ·  light {:.0}°  ·  {}",
                    fps.get(),
                    zoom.get(),
                    az.to_degrees() % 360.0,
                    if spinning.get() { "spinning" } else { "paused" },
                );
                let status_fg = t.text_faint;
                Block::new()
                    .title(title.clone())
                    .fill(t.bg)
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(
                        Viewport3D::new(model.clone())
                            .orbit(yaw.get(), pitch.get(), zoom.get())
                            .spin(spin.get())
                            .mode(mode)
                            .light(Light {
                                direction: Vec3::new(-az.cos(), -0.55, -az.sin()),
                                ambient: 0.30,
                                diffuse: 0.75,
                            })
                            .background(t.bg)
                            .on_orbit(move |dyaw, dpitch| {
                                yaw.update(|v| *v += dyaw);
                                pitch.update(|v| *v = (*v + dpitch).clamp(-1.45, 1.45));
                            })
                            .on_zoom(move |steps| {
                                zoom.update(|z| *z = (*z * (1.0 - steps * 0.1)).clamp(0.25, 6.0))
                            })
                            .layout(LayoutStyle::default().grow(1.0))
                            .element(&t)
                            .build(),
                    )
                    // Status row doubles as the painted-frame probe: this
                    // closure runs exactly when the panel region paints.
                    .child(
                        Element::new()
                            .style(LayoutStyle::default().h(1))
                            .draw(move |canvas, rect| {
                                frames.set(frames.get() + 1);
                                canvas.print(
                                    Point::new(rect.x + 1, rect.y),
                                    &status,
                                    status_fg,
                                    Rgba::TRANSPARENT,
                                );
                            })
                            .build(),
                    )
                    .element(&t)
                    .build()
            }))
            .child(dyn_view(LayoutStyle::default().h(1), move || {
                let _ = theme.get();
                text("drag orbit · wheel zoom · space spin · 1-4 mode · l/L light · r reset · t theme · q quit")
            }))
            .child(dyn_view(LayoutStyle::default().h(1), {
                let notices = use_startup_notices(cx);
                move || {
                    let t = theme.get().tokens;
                    let line = notices.with(|list| {
                        list.iter()
                            .filter(|n| !n.starts_with("caps:"))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("  ·  ")
                    });
                    if line.is_empty() {
                        return Element::new().build();
                    }
                    let ink = t.warn;
                    Element::new()
                        .style(LayoutStyle::default().h(1))
                        .draw(move |canvas, rect| {
                            canvas.print(
                                Point::new(rect.x + 1, rect.y),
                                &line,
                                ink,
                                Rgba::TRANSPARENT,
                            );
                        })
                        .build()
                }
            }))
            .build()
    })?;
    app.run()
}

fn find_asset() -> Option<(String, Vec<u8>)> {
    let mut candidates: Vec<String> = std::env::args().skip(1).collect();
    if candidates.is_empty() {
        candidates = DEFAULT_ASSETS.iter().map(|s| s.to_string()).collect();
    }
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((path, bytes));
        }
    }
    None
}
