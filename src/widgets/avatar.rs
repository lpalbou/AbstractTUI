//! Procedural, backend-independent activity avatar. OWNER: DESIGN.
//!
//! Halo rays and Hover's dome/eye-band/thrusters adapt the geometric vocabulary
//! of AbstractAssistant's watch Orb/OrbMachines scenes. The host supplies all
//! activity and audio state; this widget performs no I/O or inference.
use crate::{
    base::{Point, Rect, Rgba},
    gfx::{Bitmap, MosaicMode, MosaicRenderer},
    layout::{Dimension, Style as LayoutStyle},
    reactive::{batch, interval, on_cleanup, Scope, Signal},
    theme::TokenSet,
    ui::{dyn_view, Element, Role, View},
};

pub use abstract_avatar::{AvatarPose, AvatarStyle, Gaze as AvatarGaze};

/// Host-fed cursor activity. One cancellable idle expiry, no polling or input capture.
#[derive(Clone, Copy)]
pub struct AvatarPointer {
    point: Signal<Option<Point>>,
}
impl AvatarPointer {
    pub fn new(cx: Scope) -> Self {
        let point = cx.signal(None);
        if let Some(overlays) = cx.use_context::<crate::app::Overlays>() {
            overlays.require_pointer_motion();
        }
        cx.effect(move || {
            if point.get().is_some() {
                let timer = cx.child();
                interval(timer, std::time::Duration::from_secs(2), move || {
                    point.set_if_changed(None);
                });
                on_cleanup(move || timer.dispose());
            }
        });
        Self { point }
    }
    /// Call only for actual host pointer movement; repeated cells also count as activity.
    pub fn moved(self, position: Point) {
        self.point.set(Some(position));
    }
    pub fn clear(self) {
        self.point.set_if_changed(None);
    }
}

/// A small token-colored avatar with an optional, pose-paced activity clock.
/// Ready breathes at 2 fps, Waiting pulses at about 6 fps, and active poses use
/// at most 12 fps. Error stays static. Animation is opt-in. Hosts must set
/// `active` false while hidden; this disarms the clock and skips drawing.
/// Reduced motion is simply
/// `animated = false`; pose and level changes still update the static picture.
pub struct Avatar {
    pose: Signal<AvatarPose>,
    style: AvatarStyle,
    styles: Option<Signal<AvatarStyle>>,
    level: Option<Signal<f32>>,
    pointer: Option<AvatarPointer>,
    active: Option<Signal<bool>>,
    animated: Option<Signal<bool>>,
    label: bool,
    transparent_background: bool,
    layout: LayoutStyle,
}
impl Avatar {
    pub fn new(pose: Signal<AvatarPose>) -> Self {
        Self {
            pose,
            style: AvatarStyle::Halo,
            styles: None,
            level: None,
            pointer: None,
            active: None,
            animated: None,
            label: true,
            transparent_background: false,
            layout: LayoutStyle::default()
                .width(Dimension::Cells(28))
                .height(Dimension::Cells(15))
                .shrink(0.0),
        }
    }
    pub fn style(mut self, style: AvatarStyle) -> Self {
        self.style = style;
        self
    }
    pub fn style_signal(mut self, style: Signal<AvatarStyle>) -> Self {
        self.styles = Some(style);
        self
    }
    pub fn level(mut self, level: Signal<f32>) -> Self {
        self.level = Some(level);
        self
    }
    /// Ready eyes follow host cursor activity; reduced motion and other poses ignore it.
    pub fn follow_pointer(mut self, pointer: AvatarPointer) -> Self {
        self.pointer = Some(pointer);
        self
    }
    pub fn active(mut self, active: Signal<bool>) -> Self {
        self.active = Some(active);
        self
    }
    pub fn animated(mut self, enabled: Signal<bool>) -> Self {
        self.animated = Some(enabled);
        self
    }
    pub fn show_label(mut self, show: bool) -> Self {
        self.label = show;
        self
    }
    /// Preserve content outside the visible artwork instead of painting a card.
    /// Occupied terminal cells (including soft glow/edges) still replace the
    /// underlying glyph; empty cells leave its text and styling untouched.
    /// The host must repaint its background when the avatar moves or changes.
    pub fn transparent_background(mut self, transparent: bool) -> Self {
        self.transparent_background = transparent;
        self
    }
    pub fn layout(mut self, layout: LayoutStyle) -> Self {
        self.layout = layout;
        self
    }
    pub fn view(self, cx: Scope) -> View {
        self.build(cx, None).build()
    }
    pub fn element(self, cx: Scope, tokens: &TokenSet) -> Element {
        self.build(cx, Some(*tokens))
    }
    fn build(self, cx: Scope, tokens: Option<TokenSet>) -> Element {
        let pose = self.pose;
        let active = self.active;
        let enabled = self.animated;
        let phase = cx.signal(0.0f32);
        let sampled_level = cx.signal(0.0f32);
        let sampled_gaze = cx.signal(AvatarGaze::default());
        let pointer = self.pointer;
        let bounds = pointer.map(|_| cx.signal(None::<Rect>));
        let styles = self.styles;
        let style = self.style;
        let levels = self.level;
        // Producer signals may notify even for equal values. Cut them off before
        // clock ownership and painting: a 20-Hz audio callback must not keep
        // re-arming an 84-ms interval before its first frame can fire.
        let state = cx.memo(move || {
            (
                pose.get(),
                styles.map(|s| s.get()).unwrap_or(style),
                active.map(|s| s.get()).unwrap_or(true),
            )
        });
        let target_gaze = cx.memo(move || {
            if state.get().0 != AvatarPose::Ready
                || !state.get().2
                || !enabled.map(|s| s.get()).unwrap_or(false)
            {
                return AvatarGaze::default();
            }
            let Some(point) = pointer.and_then(|p| p.point.get()) else {
                return AvatarGaze::default();
            };
            let Some(rect) = bounds.and_then(|b| b.get()) else {
                return AvatarGaze::default();
            };
            AvatarGaze::new(
                (point.x - rect.x - rect.w / 2) as f32 / (rect.w as f32 / 2.0).max(1.0),
                (point.y - rect.y - rect.h / 2) as f32 / (rect.h as f32 / 2.0).max(1.0),
            )
        });
        let cadence = cx.memo(move || {
            let (pose, _, live) = state.get();
            if live && enabled.map(|s| s.get()).unwrap_or(false) {
                if pose == AvatarPose::Ready && target_gaze.get() != AvatarGaze::default() {
                    Some(std::time::Duration::from_millis(84))
                } else {
                    pose.period()
                }
            } else {
                None
            }
        });
        cx.effect(move || {
            if let Some(period) = cadence.get() {
                sampled_level
                    .set_if_changed(normalized(levels.map(|s| s.get_untracked()).unwrap_or(0.0)));
                sampled_gaze.set_if_changed(target_gaze.get_untracked());
                let clock_scope = cx.child();
                interval(clock_scope, period, move || {
                    batch(|| {
                        sampled_level.set_if_changed(normalized(
                            levels.map(|s| s.get_untracked()).unwrap_or(0.0),
                        ));
                        sampled_gaze.set_if_changed(target_gaze.get_untracked());
                        phase.update(|seconds| *seconds += period.as_secs_f32());
                    });
                });
                on_cleanup(move || clock_scope.dispose());
            }
        });
        let render_level = cx.memo(move || {
            let (pose, _, live) = state.get();
            if !live || !matches!(pose, AvatarPose::Listening | AvatarPose::Speaking) {
                0.0
            } else if cadence.get().is_some() {
                sampled_level.get()
            } else {
                // With reduced motion there is no frame clock, but deliberate
                // level changes still update the static listening/speaking pose.
                normalized(levels.map(|s| s.get()).unwrap_or(0.0))
            }
        });
        let image = Bitmap::default();
        let renderer = MosaicRenderer::new();
        // Buffers survive reactive frame rebuilds, and allocate only on resize.
        let buffers = std::rc::Rc::new(std::cell::RefCell::new((image, renderer)));
        let palette = cx.memo(move || tokens.unwrap_or_else(|| super::theme_tokens(cx)));
        let transparent_background = self.transparent_background;
        let materials = cx.memo(move || {
            let mut materials = Materials::new(&palette.get());
            materials.transparent_background = transparent_background;
            std::rc::Rc::new(materials)
        });
        let label = self.label;
        let child = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let t = palette.get();
                let materials = materials.get();
                let (p, style, live) = state.get();
                let level = render_level.get();
                let gaze = if cadence.get().is_some() {
                    sampled_gaze.get()
                } else {
                    AvatarGaze::default()
                };
                let phase = if cadence.get().is_some() {
                    phase.get()
                } else {
                    0.0
                };
                let buffers = buffers.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| {
                        if !live || rect.w <= 0 || rect.h <= 0 {
                            return;
                        }
                        let art_h = (rect.h - i32::from(label && rect.h > 1)).max(1);
                        let cols = rect.w.min(64) as u32;
                        let rows = art_h.min(32) as u32;
                        let mode = match super::Image::resolved_mode(None) {
                            MosaicMode::Braille => MosaicMode::Braille,
                            _ => MosaicMode::HalfBlock,
                        };
                        let (sx, sy) = mode.cell_pixels();
                        // Supersample the existing cell renderer; no pixel protocol
                        // or larger terminal resolution is assumed.
                        let (sx, sy) = (sx * 2, sy * 2);
                        let mut buffers = buffers.borrow_mut();
                        let (image, renderer) = &mut *buffers;
                        if image.width() != cols * sx || image.height() != rows * sy {
                            *image = Bitmap::new(cols * sx, rows * sy, Rgba::TRANSPARENT);
                        }
                        let radius = (cols as f32).min(rows as f32 * 2.0) * 0.5;
                        let frame = abstract_avatar::Frame::new(style, p, level, phase, gaze);
                        for y in 0..image.height() {
                            for x in 0..image.width() {
                                let nx =
                                    ((x as f32 + 0.5) / sx as f32 - cols as f32 * 0.5) / radius;
                                let ny = ((y as f32 + 0.5) / sy as f32 - rows as f32 * 0.5) * 2.0
                                    / radius;
                                image.set(x, y, materials.color(frame.sample(nx, ny)));
                            }
                        }
                        let grid = renderer.render(image, cols, rows, mode);
                        let origin = Point::new(
                            rect.x + (rect.w - cols as i32) / 2,
                            rect.y + (art_h - rows as i32) / 2,
                        );
                        for y in 0..rows {
                            for x in 0..cols {
                                let cell = grid.get(x, y).expect("rendered bounds");
                                // Even an alpha-zero put replaces the old glyph.
                                // Skip empty samples so overlay margins preserve
                                // both the text and styling beneath the artwork.
                                if cell.fg.is_transparent() && cell.bg.is_transparent() {
                                    continue;
                                }
                                // Prefer lower-half foreground: some fonts leave
                                // an ascender gap above ▀. An upper-only transparent
                                // edge must keep ▀ so the visible ink stays in fg;
                                // alpha-zero foreground can become terminal default.
                                let (ch, fg, bg) = if cell.ch == '▀'
                                    && !(transparent_background && cell.bg.is_transparent())
                                {
                                    ('▄', cell.bg, cell.fg)
                                } else {
                                    (cell.ch, cell.fg, cell.bg)
                                };
                                canvas.put(
                                    Point::new(origin.x + x as i32, origin.y + y as i32),
                                    ch,
                                    fg,
                                    bg,
                                );
                            }
                        }
                        if label && rect.h > 1 {
                            // The caption is a small independent surface, like
                            // the host's controls. Keep it readable over text
                            // without reviving the old full-width card/strip.
                            let pad = i32::from(transparent_background && rect.w >= 3);
                            let text = crate::text::truncate_ellipsis(p.label(), rect.w - pad * 2);
                            let width = crate::text::width(&text);
                            let x = rect.x + (rect.w - width).max(0) / 2;
                            if transparent_background {
                                canvas.fill(
                                    Rect::new(x - pad, rect.bottom() - 1, width + pad * 2, 1),
                                    ' ',
                                    t.text_muted,
                                    t.bg,
                                );
                            }
                            canvas.print(
                                Point::new(x, rect.bottom() - 1),
                                &text,
                                t.text_muted,
                                Rgba::TRANSPARENT,
                            );
                        }
                    })
                    .build()
            },
        );
        let mut element = Element::new()
            .style(self.layout)
            .role(Role::Region)
            .access_label("Activity avatar")
            .access_value(move || pose.get().label().into())
            .child(child);
        if let Some(bounds) = bounds {
            element = element.rect_signal(bounds);
        }
        element
    }
}
fn normalized(level: f32) -> f32 {
    if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
// All colors come from theme-owned material ramps, prepared once per theme.
struct Materials {
    tokens: TokenSet,
    warm: crate::theme::derive::MaterialRamp,
    eye: crate::theme::derive::MaterialRamp,
    dim: crate::theme::derive::MaterialRamp,
    transparent_background: bool,
}
impl PartialEq for Materials {
    fn eq(&self, other: &Self) -> bool {
        self.tokens == other.tokens && self.transparent_background == other.transparent_background
    }
}
impl Materials {
    fn new(t: &TokenSet) -> Self {
        let ramp = crate::theme::derive::material_ramp;
        Self {
            tokens: *t,
            warm: ramp(t.bg, t.warn, t.text),
            eye: ramp(t.bg, t.error, t.text),
            dim: ramp(t.bg, t.border, t.text_muted),
            transparent_background: false,
        }
    }
    fn color(&self, sample: abstract_avatar::Sample) -> Rgba {
        if sample.coverage == 0.0 {
            return self.ground();
        }
        let ramp = match sample.material {
            abstract_avatar::Material::Warm => &self.warm,
            abstract_avatar::Material::Eye => &self.eye,
            abstract_avatar::Material::Dim => &self.dim,
        };
        self.paint(ramp, sample.light, sample.coverage)
    }
    fn ground(&self) -> Rgba {
        if self.transparent_background {
            Rgba::TRANSPARENT
        } else {
            self.tokens.bg
        }
    }
    fn paint(&self, ramp: &crate::theme::derive::MaterialRamp, light: f32, coverage: f32) -> Rgba {
        if self.transparent_background {
            let ink = ramp.sample(light, 1.0);
            ink.with_alpha((coverage.clamp(0.0, 1.0) * 255.0).round() as u8)
        } else {
            ramp.sample(light, coverage)
        }
    }
}
#[cfg(test)]
fn sample(
    style: AvatarStyle,
    pose: AvatarPose,
    level: f32,
    time: f32,
    x: f32,
    y: f32,
    m: &Materials,
) -> Rgba {
    m.color(abstract_avatar::sample(style, pose, level, time, x, y))
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
