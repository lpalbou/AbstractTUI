//! effects — compositor effects on real overlay layers.
//!
//! Demonstrates: `app.overlays()` LayerHandles carrying RENDER's cell
//! shaders (`anim::shaders`): a Shimmer title, a Dissolve-in panel
//! (`d` replays), a HueDrift accent card, layer ColorTransforms
//! (`m` cycles none -> dim -> grayscale -> tint across the demo layers),
//! REACT's Toast (slide+fade, `n`), plus one component-level tween lane
//! (ramped progress) to contrast the two approaches. Shader clocks are
//! ANIMATIONS: one `reactive::after` loop advances `set_shader_t` at
//! 30 fps while running; `p` pauses it and the app goes fully idle.
//!
//! Keys: d dissolve replay · m cycle transform · n toast · p pause ·
//! q quit.
//!
//! OWNER: DESIGN.

mod common;

use std::time::Duration;

use abstracttui::anim::shaders::{Dissolve, HueDrift, Shimmer};
use abstracttui::app::Overlays;
use abstracttui::prelude::*;
use abstracttui::reactive::after;
use abstracttui::render::ColorTransform;

const FRAME: Duration = Duration::from_millis(33);
/// Demo layer z-band: above the root UI (0), below popups (1000+).
const FX_Z: i32 = 100;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("effects: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(90, 26));
    let quitter = app.quitter();
    let overlays = app.overlays();
    let viewport = app.viewport();

    app.mount(move |cx| {
        let theme = use_theme(cx);
        let t = theme.get_untracked().tokens;
        let running = cx.signal(true);
        let transform_ix = cx.signal(0usize);
        let toasts = cx.signal(0u32);

        // ---- the effect layers (created once; shaders animate them) ----
        let title = title_layer(&overlays, viewport, &t);
        title.set_shader(Some(Box::new(Shimmer {
            speed: 0.9,
            amplitude: 0.35,
            wavelength: 10.0,
        })));

        let panel = panel_layer(&overlays, viewport, &t);
        panel.set_shader(Some(Box::new(Dissolve {
            duration: 1.2,
            seed: 7,
        })));

        let card = card_layer(&overlays, viewport, &t);
        card.set_shader(Some(Box::new(HueDrift {
            speed: 0.6,
            strength: 0.45,
        })));

        // One clock, three shader timelines: the title/card run on wall
        // time; the panel's dissolve runs on time-since-replay.
        let clock_ms = cx.signal(0u64);
        let dissolve_from = cx.signal(0u64);
        {
            let (title, panel, card) = (title.clone(), panel.clone(), card.clone());
            fn tick(
                clock_ms: Signal<u64>,
                running: Signal<bool>,
                dissolve_from: Signal<u64>,
                layers: (
                    abstracttui::app::LayerHandle,
                    abstracttui::app::LayerHandle,
                    abstracttui::app::LayerHandle,
                ),
            ) {
                after(FRAME, move || {
                    if running.get_untracked() {
                        clock_ms.update(|t| *t += FRAME.as_millis() as u64);
                        let now = clock_ms.get_untracked();
                        let (title, panel, card) = &layers;
                        let secs = now as f32 / 1000.0;
                        title.set_shader_t(secs);
                        card.set_shader_t(secs);
                        let age = now.saturating_sub(dissolve_from.get_untracked());
                        panel.set_shader_t(age as f32 / 1000.0);
                    }
                    tick(clock_ms, running, dissolve_from, layers);
                });
            }
            tick(
                clock_ms,
                running,
                dissolve_from,
                (title.clone(), panel.clone(), card.clone()),
            );
        }

        let cycle_transform = {
            let layers = [title.clone(), panel.clone(), card.clone()];
            let error = t.error;
            move || {
                transform_ix.update(|i| *i = (*i + 1) % 4);
                let tf = match transform_ix.get_untracked() {
                    1 => ColorTransform::Dim(0.55),
                    2 => ColorTransform::Grayscale(1.0),
                    3 => ColorTransform::Tint(error, 0.35),
                    _ => ColorTransform::None,
                };
                for l in &layers {
                    l.set_color_transform(tf);
                }
            }
        };

        let notify = {
            let overlays = overlays.clone();
            move || {
                toasts.update(|n| *n += 1);
                Toast::show(
                    &overlays,
                    cx,
                    viewport,
                    format!(
                        "toast #{} — slide+fade by animate()",
                        toasts.get_untracked()
                    ),
                    Duration::from_secs(2),
                );
            }
        };

        // ---- the base UI (root layer): contrast lane + keys ------------
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('p')), move |_| {
                running.update(|r| *r = !*r)
            })
            .shortcut(KeyChord::plain(Key::Char('d')), move |_| {
                dissolve_from.set(clock_ms.get_untracked())
            })
            .shortcut(KeyChord::plain(Key::Char('m')), move |_| cycle_transform())
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| notify())
            // The layers cover the upper region; the root content lives
            // below them (layout spacer keeps the zones honest).
            .child(Element::new().style(LayoutStyle::default().h(15)).build())
            .child(
                Separator::horizontal()
                    .label("component lane — tween-driven ramps")
                    .element(&t)
                    .build(),
            )
            .child(dyn_view(LayoutStyle::column().gap(1).h(3), move || {
                let tk = theme.get().tokens;
                let phase = (clock_ms.get() % 4000) as f32 / 4000.0;
                let k = Easing::EaseInOut.eval(phase);
                Element::new()
                    .style(LayoutStyle::column().gap(1))
                    .child(Progress::new(k).element(&tk).build())
                    .child(Progress::new(k).ramp(true).element(&tk).build())
                    .build()
            }))
            .child(text(
                "d · dissolve replay    m · cycle transform    n · toast    p · pause    q · quit",
            ))
            .build()
    })?;
    app.run()
}

// ---------------------------------------------------------------------------
// Layer construction (drawn once; shaders do the moving)
// ---------------------------------------------------------------------------

fn title_layer(overlays: &Overlays, viewport: Size, t: &TokenSet) -> abstracttui::app::LayerHandle {
    let accent = t.accent;
    let muted = t.text_muted;
    let bounds = Rect::new(2, 1, (viewport.w - 4).max(10), 3);
    overlays.layer_draw(FX_Z, bounds, move |canvas, rect| {
        let title = "A B S T R A C T T U I";
        let x = rect.x + (rect.w - title.chars().count() as i32).max(0) / 2;
        canvas.print(Point::new(x, rect.y), title, accent, Rgba::TRANSPARENT);
        let sub = "compositor effects — shimmer is a cell shader on this layer";
        let x = rect.x + (rect.w - sub.chars().count() as i32).max(0) / 2;
        canvas.print(Point::new(x, rect.y + 2), sub, muted, Rgba::TRANSPARENT);
    })
}

fn panel_layer(overlays: &Overlays, viewport: Size, t: &TokenSet) -> abstracttui::app::LayerHandle {
    let (surface, text_c, ok) = (t.surface_raised, t.text, t.ok);
    let w = (viewport.w - 8).clamp(20, 64);
    let bounds = Rect::new(4, 5, w, 7);
    overlays.layer_draw(FX_Z + 1, bounds, move |canvas, rect| {
        canvas.fill(rect, ' ', text_c, surface);
        canvas.print(
            Point::new(rect.x + 2, rect.y + 1),
            "dissolve-in panel",
            ok,
            surface,
        );
        let lines = [
            "every cell owns a fixed hash threshold;",
            "the shader shows cells below t/duration —",
            "press d to run the reveal again.",
        ];
        for (i, line) in lines.iter().enumerate() {
            canvas.print(
                Point::new(rect.x + 2, rect.y + 3 + i as i32),
                line,
                text_c,
                surface,
            );
        }
    })
}

fn card_layer(overlays: &Overlays, viewport: Size, t: &TokenSet) -> abstracttui::app::LayerHandle {
    let (accent, surface) = (t.accent, t.surface);
    let x = (viewport.w - 26).max(2);
    let bounds = Rect::new(x, 5, 24, 7);
    overlays.layer_draw(FX_Z + 2, bounds, move |canvas, rect| {
        canvas.fill(rect, ' ', accent, surface);
        // A chunky accent frame: HueDrift makes it breathe.
        for cx in rect.x..rect.right() {
            canvas.put(Point::new(cx, rect.y), '▔', accent, surface);
            canvas.put(Point::new(cx, rect.bottom() - 1), '▁', accent, surface);
        }
        for cy in rect.y..rect.bottom() {
            canvas.put(Point::new(rect.x, cy), '▏', accent, surface);
            canvas.put(Point::new(rect.right() - 1, cy), '▕', accent, surface);
        }
        canvas.print(
            Point::new(rect.x + 2, rect.y + 2),
            "hue-drift card",
            accent,
            surface,
        );
        canvas.print(
            Point::new(rect.x + 2, rect.y + 4),
            "m cycles transforms",
            accent,
            surface,
        );
    })
}
