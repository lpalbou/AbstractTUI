//! Viewport3D: an orbiting software-rendered view of a `three::Model`.
//!
//! ```no_run
//! use std::sync::Arc;
//! use abstracttui::gfx::MosaicMode;
//! use abstracttui::theme::default_theme;
//! use abstracttui::three;
//! use abstracttui::widgets::Viewport3D;
//!
//! let theme = default_theme();
//! let model = Arc::new(three::load_glb("model.glb").unwrap());
//! // Camera state lives app-side (signals in a real app; the widget
//! // is pure over its props — same props, same pixels).
//! let (yaw, pitch, zoom) = (0.6_f32, 0.35_f32, 1.0_f32);
//! let vp = Viewport3D::new(model)
//!     .orbit(yaw, pitch, zoom)
//!     .mode(MosaicMode::HalfBlock)
//!     .animate(0, 1.25) // play clip 0 at t=1.25s (loops; static = rest)
//!     .on_orbit(move |dy, dp| { let _ = (dy, dp); /* write yaw/pitch signals */ })
//!     .on_zoom(move |steps| { let _ = steps; /* write zoom signal */ })
//!     .element(&theme.tokens);
//! # let _ = vp;
//! ```
//!
//! (`element` takes only `&TokenSet` — no `Scope`: the widget holds no
//! reactive state of its own; see RT8-3.)
//!
//! The widget is PURE over its props: camera angles arrive as plain
//! floats each view build (signals live app-side), auto-rotation is
//! driven by a caller-supplied time value (`spin(t)`) so animation
//! wiring — frame requests, clocks — stays the app's business and two
//! builds with the same props paint the same pixels.
//!
//! Interaction: left-drag orbits (pointer captured for the drag so
//! fast drags keep steering even when the cursor leaves the rect —
//! REACT's `EventCtx::capture_pointer`, landed this cycle), wheel
//! zooms. The widget only REPORTS deltas through `on_orbit`/`on_zoom`;
//! the app owns the camera state and clamping policy.
//!
//! Each draw renders scene -> `Framebuffer` (sized rect x the mosaic
//! mode's subpixel density: half-block = 2 px per cell vertically) ->
//! mosaic -> canvas cells. Buffers persist inside the FnMut draw
//! closure; a steady-state repaint reallocates nothing.
//!
//! OWNER: GFX3D.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::base::{Point, Rgba};
use crate::gfx::mosaic::{MosaicMode, MosaicRenderer};
use crate::layout::Style as LayoutStyle;
use crate::theme::TokenSet;
use crate::three::raster::Framebuffer;
use crate::three::scene::{Camera, Light, Scene, SceneRenderer};
use crate::three::{Model, Pose};
use crate::ui::{Element, MouseKind, UiEvent};

/// Orbit sensitivity: radians per cell of drag. Cells are ~1:2, so the
/// vertical rate is doubled to make diagonal drags feel isotropic.
const DRAG_YAW_PER_CELL: f32 = 0.05;
const DRAG_PITCH_PER_CELL: f32 = 0.10;

/// The default orbit (yaw, pitch, zoom) — a viewer's "reset camera"
/// writes exactly these into its signals. Zoom 1.0 = fit-to-bounds
/// (the widget frames the model's AABB every draw, so a freshly
/// loaded model is always fully in view at defaults).
pub const DEFAULT_ORBIT: (f32, f32, f32) = (0.6, 0.35, 1.0);

pub struct Viewport3D {
    model: Arc<Model>,
    fog: f32,
    yaw: f32,
    pitch: f32,
    /// Camera distance as a multiple of the auto-framing distance
    /// (1.0 = exactly framed). Dimensionless so callers need no
    /// knowledge of model units.
    zoom: f32,
    spin: f32,
    /// Animation playback: (index into `Model::animations()`, time in
    /// seconds). Time LOOPS over the clip duration inside the widget;
    /// play/pause/speed live app-side as signal policy (pause = stop
    /// advancing the time signal, speed = scale the delta) — the same
    /// purity contract as `spin`.
    animation: Option<(usize, f32)>,
    mode: MosaicMode,
    light: Light,
    background: Rgba,
    double_sided: bool,
    on_orbit: Option<Box<dyn FnMut(f32, f32)>>,
    on_zoom: Option<Box<dyn FnMut(f32)>>,
    layout: LayoutStyle,
}

impl Viewport3D {
    pub fn new(model: Arc<Model>) -> Viewport3D {
        Viewport3D {
            model,
            fog: 0.0,
            yaw: DEFAULT_ORBIT.0,
            pitch: DEFAULT_ORBIT.1,
            zoom: DEFAULT_ORBIT.2,
            spin: 0.0,
            animation: None,
            mode: MosaicMode::HalfBlock,
            light: Light::default(),
            background: Rgba::TRANSPARENT,
            double_sided: true,
            on_orbit: None,
            on_zoom: None,
            layout: LayoutStyle::default(),
        }
    }

    /// Current camera orbit (radians, radians, zoom multiple).
    pub fn orbit(mut self, yaw: f32, pitch: f32, zoom: f32) -> Viewport3D {
        self.yaw = yaw;
        self.pitch = pitch;
        self.zoom = zoom.max(0.05);
        self
    }

    /// Extra yaw from the caller's clock: pass `t * speed` for a
    /// turntable. Kept separate from `orbit` so drag deltas and the
    /// animation never fight over one number.
    pub fn spin(mut self, extra_yaw: f32) -> Viewport3D {
        self.spin = extra_yaw;
        self
    }

    /// Play animation `index` at time `t` seconds (from the app's
    /// clock signal). Loops over the clip duration. Unknown indices
    /// and static models draw the rest pose — a viewer can wire
    /// `space=play` without pre-checking.
    pub fn animate(mut self, index: usize, t: f32) -> Viewport3D {
        self.animation = Some((index, t));
        self
    }

    pub fn mode(mut self, mode: MosaicMode) -> Viewport3D {
        self.mode = mode;
        self
    }

    pub fn light(mut self, light: Light) -> Viewport3D {
        self.light = light;
        self
    }

    /// Key-light direction from viewer-friendly angles (radians):
    /// azimuth around the model, elevation above the horizon. Keeps
    /// the default ambient/diffuse balance.
    pub fn light_angles(mut self, azimuth: f32, elevation: f32) -> Viewport3D {
        self.light = Light {
            direction: Light::from_angles(azimuth, elevation).direction,
            ..self.light
        };
        self
    }

    /// Depth fog toward the widget background (0.0 = off, up to 1.0):
    /// far geometry recedes into the ground — the storyboard cue,
    /// viewer-grade. Only visible with an opaque `.background()`.
    pub fn fog(mut self, strength: f32) -> Viewport3D {
        self.fog = strength.clamp(0.0, 1.0);
        self
    }

    /// Ground behind the model (pass a theme surface token; default
    /// keeps whatever is beneath the widget).
    pub fn background(mut self, ground: Rgba) -> Viewport3D {
        self.background = ground;
        self
    }

    /// Cull back faces (false — the default — renders both sides;
    /// real-world exports are not consistently wound).
    pub fn cull_backfaces(mut self, cull: bool) -> Viewport3D {
        self.double_sided = !cull;
        self
    }

    /// Drag steering: receives (yaw delta, pitch delta) in radians,
    /// signed so that adding them to your orbit signals makes the model
    /// follow the pointer (grab-the-object convention).
    pub fn on_orbit(mut self, f: impl FnMut(f32, f32) + 'static) -> Viewport3D {
        self.on_orbit = Some(Box::new(f));
        self
    }

    /// Wheel zoom: receives +1 per wheel-up notch (toward the model),
    /// -1 per wheel-down.
    pub fn on_zoom(mut self, f: impl FnMut(f32) + 'static) -> Viewport3D {
        self.on_zoom = Some(Box::new(f));
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Viewport3D {
        self.layout = style;
        self
    }

    /// Canonical one-call build (RT8-3 uniformity): same shape as the
    /// interactive widgets — tokens resolve from the app's theme
    /// context, the finished `View` comes back. `element(&tokens)`
    /// remains the explicit-theming door.
    pub fn view(self, cx: crate::reactive::Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(&t).build()
    }

    pub fn element(self, _t: &TokenSet) -> Element {
        let model = self.model;
        let (yaw, pitch, zoom, spin) = (self.yaw, self.pitch, self.zoom, self.spin);
        let (mode, light, background, double_sided, fog) = (
            self.mode,
            self.light,
            self.background,
            self.double_sided,
            self.fog,
        );
        let animation = self.animation;

        // Draw state persists inside the FnMut closure.
        let mut fb: Option<Framebuffer> = None;
        let mut mosaic = MosaicRenderer::new();
        let mut scene_renderer = SceneRenderer::new();
        let mut pose = Pose::default();
        let draw_model = model.clone();

        let mut el = Element::new().style(self.layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let (subw, subh) = mode.cell_pixels();
            let (pw, ph) = (rect.w as u32 * subw, rect.h as u32 * subh);
            let fb = match &mut fb {
                Some(f) if f.width() == pw && f.height() == ph => f,
                slot => slot.insert(Framebuffer::new(pw, ph)),
            };

            let Some((min, max)) = draw_model.bounds() else {
                return; // no finite geometry: nothing honest to draw
            };
            let mut camera = Camera::framing(min, max, yaw + spin, pitch);
            camera.distance *= zoom;
            let mut scene = Scene::new(&draw_model, camera);
            scene.light = light;
            scene.background = background;
            scene.double_sided = double_sided;
            // Animation playback: loop `t` over the clip and sample.
            // A failed sample (static model, bad index) draws rest.
            if let Some((index, t)) = animation {
                let duration = draw_model
                    .animations()
                    .get(index)
                    .map(|a| a.duration())
                    .unwrap_or(0.0);
                let looped = if duration > 1e-6 {
                    t.rem_euclid(duration)
                } else {
                    0.0
                };
                if draw_model.sample_pose_full(index, looped, &mut pose) {
                    scene.pose = Some(&pose);
                }
            }
            scene_renderer.render(&scene, fb);
            if fog > 0.0 && !background.is_transparent() {
                fb.depth_fog(background, fog);
            }

            let grid = mosaic.render(fb.bitmap(), rect.w as u32, rect.h as u32, mode);
            for (pos, ch, fg, bg) in grid.cell_patches(rect.origin()) {
                canvas.put(pos, ch, fg, bg);
            }
        });

        // Interaction: drag-to-orbit with pointer capture, wheel zoom.
        if self.on_orbit.is_some() || self.on_zoom.is_some() {
            let mut on_orbit = self.on_orbit;
            let mut on_zoom = self.on_zoom;
            // Last drag position; shared with nothing (one closure), so
            // plain closure state would do — Rc<RefCell> keeps it
            // borrowable if a future builder splits the handlers.
            let last: Rc<RefCell<Option<Point>>> = Rc::new(RefCell::new(None));
            el = el.on_event(move |ctx, event| {
                let UiEvent::Mouse(m) = event else { return };
                match m.kind {
                    MouseKind::Down(crate::ui::MouseButton::Left) => {
                        *last.borrow_mut() = Some(m.pos);
                        // Capture: fast drags keep steering outside the
                        // rect until release.
                        if let Some(id) = ctx.target() {
                            ctx.capture_pointer(id);
                        }
                    }
                    MouseKind::Drag(crate::ui::MouseButton::Left) => {
                        let mut anchor = last.borrow_mut();
                        if let (Some(prev), Some(cb)) = (*anchor, on_orbit.as_mut()) {
                            let dx = (m.pos.x - prev.x) as f32;
                            let dy = (m.pos.y - prev.y) as f32;
                            if dx != 0.0 || dy != 0.0 {
                                // Grab-the-object convention: the surface
                                // under the pointer follows the drag. A
                                // rightward drag must orbit the camera
                                // toward the model's LEFT side (yaw
                                // decreases — `Camera::eye` grows yaw
                                // toward +X), hence the negation; the
                                // pitch axis already matches (drag down
                                // raises the camera, revealing the top).
                                cb(-dx * DRAG_YAW_PER_CELL, dy * DRAG_PITCH_PER_CELL);
                            }
                        }
                        *anchor = Some(m.pos);
                    }
                    MouseKind::Up(crate::ui::MouseButton::Left) => {
                        *last.borrow_mut() = None;
                        ctx.release_pointer();
                    }
                    MouseKind::ScrollUp => {
                        if let Some(cb) = on_zoom.as_mut() {
                            cb(1.0);
                        }
                        ctx.stop_propagation();
                    }
                    MouseKind::ScrollDown => {
                        if let Some(cb) = on_zoom.as_mut() {
                            cb(-1.0);
                        }
                        ctx.stop_propagation();
                    }
                    _ => {}
                }
            });
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::three::primitives::{cube, model_of};
    use crate::widgets::test_util::draw_into;

    fn cube_model() -> Arc<Model> {
        // Token-rule-safe color: plain white base (materials are model
        // data, not widget chrome — but the lint scans this file, so
        // stay on constants anyway).
        Arc::new(model_of(cube(2.0), [1.0, 1.0, 1.0, 1.0]))
    }

    fn coverage(c: &crate::ui::BufferCanvas, size: Size) -> usize {
        let mut n = 0;
        for y in 0..size.h {
            for x in 0..size.w {
                let cell = c.cell(Point::new(x, y)).unwrap();
                if cell.0 != ' ' || cell.2.a != 0 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn renders_cube_with_nonzero_coverage() {
        let t = default_theme().tokens;
        let size = Size::new(30, 15);
        let el = Viewport3D::new(cube_model()).element(&t);
        let c = draw_into(el, size);
        let n = coverage(&c, size);
        assert!(n > 20, "cube coverage {n} cells");
    }

    #[test]
    fn orbit_changes_pixels_deterministically() {
        let t = default_theme().tokens;
        let size = Size::new(24, 12);
        let paint = |yaw: f32| {
            let el = Viewport3D::new(cube_model())
                .orbit(yaw, 0.35, 1.0)
                .element(&t);
            let c = draw_into(el, size);
            (0..size.h).map(|y| c.row_text(y)).collect::<Vec<_>>()
        };
        let a = paint(0.6);
        let b = paint(0.6);
        assert_eq!(a, b, "same props, same pixels");
        let c = paint(1.4);
        assert_ne!(a, c, "orbit must change the frame");
    }

    #[test]
    fn spin_prop_drives_turntable() {
        let t = default_theme().tokens;
        let size = Size::new(24, 12);
        let frame = |spin: f32| {
            let el = Viewport3D::new(cube_model()).spin(spin).element(&t);
            let c = draw_into(el, size);
            (0..size.h).map(|y| c.row_text(y)).collect::<Vec<_>>()
        };
        assert_ne!(frame(0.0), frame(0.9));
    }

    #[test]
    fn zoom_scales_apparent_size() {
        let t = default_theme().tokens;
        let size = Size::new(30, 15);
        let cover = |zoom: f32| {
            let el = Viewport3D::new(cube_model())
                .orbit(0.6, 0.35, zoom)
                .element(&t);
            let c = draw_into(el, size);
            coverage(&c, size)
        };
        assert!(
            cover(0.6) > cover(2.0),
            "closer camera must cover more cells ({} vs {})",
            cover(0.6),
            cover(2.0)
        );
    }

    #[test]
    fn fog_and_light_angles_change_the_frame() {
        let t = default_theme().tokens;
        let size = Size::new(24, 12);
        let dump = |el: Element| {
            let c = draw_into(el, size);
            (0..size.h)
                .flat_map(|y| (0..size.w).map(move |x| (x, y)))
                .map(|(x, y)| c.cell(Point::new(x, y)).unwrap())
                .collect::<Vec<_>>()
        };
        // Fog needs an opaque ground: use a theme token.
        let base = dump(
            Viewport3D::new(cube_model())
                .background(t.surface)
                .element(&t),
        );
        let fogged = dump(
            Viewport3D::new(cube_model())
                .background(t.surface)
                .fog(0.9)
                .element(&t),
        );
        assert_ne!(base, fogged, "fog must recolor depth");
        let relit = dump(
            Viewport3D::new(cube_model())
                .background(t.surface)
                .light_angles(2.4, 0.1)
                .element(&t),
        );
        assert_ne!(base, relit, "light angles must change shading");
    }

    #[test]
    fn default_orbit_is_the_reset_target() {
        // new() must start exactly at DEFAULT_ORBIT so "reset camera"
        // (writing DEFAULT_ORBIT into the app's signals) reproduces
        // the initial framing bit-for-bit.
        let t = default_theme().tokens;
        let size = Size::new(20, 10);
        let fresh = draw_into(Viewport3D::new(cube_model()).element(&t), size);
        let reset = draw_into(
            Viewport3D::new(cube_model())
                .orbit(DEFAULT_ORBIT.0, DEFAULT_ORBIT.1, DEFAULT_ORBIT.2)
                .element(&t),
            size,
        );
        for y in 0..size.h {
            assert_eq!(fresh.row_text(y), reset.row_text(y));
        }
    }

    #[test]
    fn animation_playback_moves_pixels_and_loops() {
        let t = default_theme().tokens;
        let size = Size::new(30, 15);
        let (json, bin) = crate::three::skin_tests::skinned_bar_glb();
        let model = Arc::new(
            Model::load(&crate::testing::glb_mutate::assemble(
                json.as_bytes(),
                Some(&bin),
            ))
            .unwrap(),
        );
        // Full-cell dump (glyph + fg + bg): halfblock paints solid
        // interiors as ' ' with a BACKGROUND color, so a glyph-only
        // comparison is blind to most of the bar.
        let frame = |el: Element| {
            let c = draw_into(el, size);
            (0..size.h)
                .flat_map(|y| (0..size.w).map(move |x| (x, y)))
                .map(|(x, y)| c.cell(Point::new(x, y)).unwrap())
                .collect::<Vec<_>>()
        };
        let rest = frame(Viewport3D::new(model.clone()).element(&t));
        // Mid-clip (45° bend). NOT t=1.0: the widget LOOPS time over
        // the clip, so t == duration wraps back to the bind pose.
        let bent = frame(Viewport3D::new(model.clone()).animate(0, 0.5).element(&t));
        assert_ne!(rest, bent, "playback must move pixels");
        // The loop wrap itself: t = duration + 0.5 lands on the same
        // frame as t = 0.5.
        let wrapped = frame(Viewport3D::new(model.clone()).animate(0, 1.5).element(&t));
        assert_eq!(bent, wrapped, "time must loop over the clip duration");
        // Unknown animation index: honest rest pose, no panic.
        let bad = frame(Viewport3D::new(model.clone()).animate(9, 0.5).element(&t));
        assert_eq!(rest, bad);
    }

    #[test]
    fn all_four_mosaic_modes_render_the_scene() {
        let t = default_theme().tokens;
        let size = Size::new(30, 15);
        let mut frames = Vec::new();
        for mode in [
            MosaicMode::HalfBlock,
            MosaicMode::Quadrant,
            MosaicMode::Sextant,
            MosaicMode::Braille,
        ] {
            let el = Viewport3D::new(cube_model()).mode(mode).element(&t);
            let c = draw_into(el, size);
            let n = coverage(&c, size);
            assert!(n > 10, "{mode:?} coverage {n}");
            frames.push(
                (0..size.h)
                    .flat_map(|y| (0..size.w).map(move |x| (x, y)))
                    .map(|(x, y)| c.cell(Point::new(x, y)).unwrap())
                    .collect::<Vec<_>>(),
            );
        }
        // Distinct glyph vocabularies: each mode paints differently.
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                assert_ne!(frames[i], frames[j], "modes {i} and {j} identical");
            }
        }
    }

    #[test]
    fn degenerate_rects_and_empty_models_never_panic() {
        let t = default_theme().tokens;
        for size in [Size::new(0, 0), Size::new(1, 1)] {
            let el = Viewport3D::new(cube_model()).element(&t);
            let _ = draw_into(el, size);
        }
        // A model with no instances has no bounds: draws nothing.
        let empty = Arc::new(Model::default());
        let el = Viewport3D::new(empty).element(&t);
        let c = draw_into(el, Size::new(8, 4));
        assert_eq!(coverage(&c, Size::new(8, 4)), 0);
    }
}
