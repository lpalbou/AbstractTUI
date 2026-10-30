//! Procedural, backend-independent activity avatar. OWNER: DESIGN.
//!
//! Halo rays and Hover's dome/eye-band/thrusters adapt the geometric vocabulary
//! of AbstractAssistant's watch Orb/OrbMachines scenes. The host supplies all
//! activity and audio state; this widget performs no I/O or inference.
use crate::{
    base::{Point, Rgba},
    gfx::{Bitmap, MosaicRenderer},
    layout::{Dimension, Style as LayoutStyle},
    reactive::{interval, on_cleanup, Scope, Signal},
    theme::TokenSet,
    ui::{dyn_view, Element, Role, View},
};
use std::time::Duration;

/// Observed activity, supplied by the application rather than inferred here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AvatarPose {
    #[default]
    Ready,
    Thinking,
    Working,
    Listening,
    Speaking,
    Waiting,
    Error,
}
impl AvatarPose {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Thinking => "Thinking",
            Self::Working => "Working",
            Self::Listening => "Listening",
            Self::Speaking => "Speaking",
            Self::Waiting => "Waiting",
            Self::Error => "Error",
        }
    }
    fn moves(self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::Working | Self::Listening | Self::Speaking
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AvatarStyle {
    #[default]
    Halo,
    Hover,
}

/// A small token-colored avatar with an optional 12-fps activity clock.
/// Animation is off unless bound to an enabled signal. Ready/waiting/error are
/// static even when animation is enabled. Hosts must set `active` false while
/// hidden; this disarms the clock and skips drawing. Reduced motion is simply
/// `animated = false`; pose and level changes still update the static picture.
pub struct Avatar {
    pose: Signal<AvatarPose>,
    style: AvatarStyle,
    styles: Option<Signal<AvatarStyle>>,
    level: Option<Signal<f32>>,
    active: Option<Signal<bool>>,
    animated: Option<Signal<bool>>,
    label: bool,
    layout: LayoutStyle,
}
impl Avatar {
    pub fn new(pose: Signal<AvatarPose>) -> Self {
        Self {
            pose,
            style: AvatarStyle::Halo,
            styles: None,
            level: None,
            active: None,
            animated: None,
            label: true,
            layout: LayoutStyle::default()
                .width(Dimension::Cells(18))
                .height(Dimension::Cells(10))
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
        let frame = cx.signal(0u64);
        cx.effect(move || {
            if pose.get().moves()
                && active.map(|s| s.get()).unwrap_or(true)
                && enabled.map(|s| s.get()).unwrap_or(false)
            {
                let clock_scope = cx.child();
                interval(clock_scope, Duration::from_millis(84), move || {
                    frame.update(|n| *n = n.wrapping_add(1))
                });
                on_cleanup(move || clock_scope.dispose());
            }
        });
        let image = Bitmap::default();
        let renderer = MosaicRenderer::new();
        // Buffers survive reactive frame rebuilds, and allocate only on resize.
        let buffers = std::rc::Rc::new(std::cell::RefCell::new((image, renderer)));
        let label = self.label;
        let styles = self.styles;
        let style = self.style;
        let levels = self.level;
        let child = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let t = tokens.unwrap_or_else(|| super::theme_tokens(cx));
                let p = pose.get();
                let live = active.map(|s| s.get()).unwrap_or(true);
                let style = styles.map(|s| s.get()).unwrap_or(style);
                let level = normalized(levels.map(|s| s.get()).unwrap_or(0.0));
                let moving = live && p.moves() && enabled.map(|s| s.get()).unwrap_or(false);
                let phase = if moving {
                    frame.get() as f32 * 0.084
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
                        let cols = rect.w.min(32) as u32;
                        let rows = art_h.min(16) as u32;
                        let mode = super::Image::resolved_mode(None);
                        let (sx, sy) = mode.cell_pixels();
                        let mut buffers = buffers.borrow_mut();
                        let (image, renderer) = &mut *buffers;
                        if image.width() != cols * sx || image.height() != rows * sy {
                            *image = Bitmap::new(cols * sx, rows * sy, Rgba::TRANSPARENT);
                        }
                        let radius = (cols as f32).min(rows as f32 * 2.0) * 0.5;
                        for y in 0..image.height() {
                            for x in 0..image.width() {
                                let nx =
                                    ((x as f32 + 0.5) / sx as f32 - cols as f32 * 0.5) / radius;
                                let ny = ((y as f32 + 0.5) / sy as f32 - rows as f32 * 0.5) * 2.0
                                    / radius;
                                image.set(x, y, sample(style, p, level, phase, nx, ny, &t));
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
                                canvas.put(
                                    Point::new(origin.x + x as i32, origin.y + y as i32),
                                    cell.ch,
                                    cell.fg,
                                    cell.bg,
                                );
                            }
                        }
                        if label && rect.h > 1 {
                            let text = crate::text::truncate_ellipsis(p.label(), rect.w);
                            canvas.print(
                                Point::new(
                                    rect.x + (rect.w - crate::text::width(&text)).max(0) / 2,
                                    rect.bottom() - 1,
                                ),
                                &text,
                                t.text_muted,
                                Rgba::TRANSPARENT,
                            );
                        }
                    })
                    .build()
            },
        );
        Element::new()
            .style(self.layout)
            .role(Role::Region)
            .access_label("Activity avatar")
            .access_value(move || pose.get().label().into())
            .child(child)
    }
}
fn normalized(level: f32) -> f32 {
    if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn ink(pose: AvatarPose, t: &TokenSet) -> Rgba {
    match pose {
        AvatarPose::Error => t.error,
        AvatarPose::Waiting => t.warn,
        AvatarPose::Listening => t.info,
        AvatarPose::Working => t.ok,
        _ => t.accent,
    }
}
fn sample(
    style: AvatarStyle,
    pose: AvatarPose,
    level: f32,
    time: f32,
    x: f32,
    y: f32,
    t: &TokenSet,
) -> Rgba {
    let tint = ink(pose, t);
    match style {
        AvatarStyle::Halo => {
            let r = (x * x + y * y).sqrt();
            let a = y.atan2(x);
            let voice = if matches!(pose, AvatarPose::Speaking | AvatarPose::Listening) {
                level
            } else {
                0.0
            };
            let radius = 0.48 + voice * 0.07;
            if r < radius {
                let z = (1.0 - r * r / (radius * radius)).max(0.0).sqrt();
                let light = -x * 0.6 - y * 0.8 + z * 0.7;
                if light > 0.85 {
                    t.text
                } else if light > 0.35 {
                    tint
                } else if light > 0.0 {
                    t.accent_alt
                } else {
                    t.border
                }
            } else {
                let orbit = 0.69 + 0.06 * (a * 3.0 + time).sin();
                let rays = 0.66 + voice * 0.22 * (a * 12.0 - time * 2.0).sin().abs();
                let moving = matches!(pose, AvatarPose::Thinking | AvatarPose::Working);
                if (r - orbit).abs() < 0.025 && (!moving || (a - time * 1.8).sin() > -0.25) {
                    tint
                } else if r > radius + 0.05 && r < rays && (a * 16.0 + time).sin() > 0.86 {
                    t.accent_alt
                } else if (r - 0.85).abs() < 0.018 && pose == AvatarPose::Waiting {
                    t.warn
                } else {
                    Rgba::TRANSPARENT
                }
            }
        }
        AvatarStyle::Hover => {
            // Watch Hover: domed lid, dark eye band, rounded lower hull, two
            // warm eyes and small thrusters. State changes amplitude, not pace.
            let bob = if pose.moves() {
                0.04 * (time * 1.05).sin()
            } else {
                0.0
            };
            let y = y - bob;
            let lean = if pose == AvatarPose::Thinking {
                0.08
            } else if pose == AvatarPose::Listening {
                -0.05
            } else {
                0.0
            };
            let y = y + lean * x;
            let dome = (x / 0.7).powi(2) + ((y + 0.10) / 0.53).powi(2);
            if y < -0.10 && dome < 1.0 {
                if (dome - 0.82).abs() < 0.06 {
                    t.text
                } else if x + y < -0.32 {
                    t.text_muted
                } else {
                    t.accent_alt
                }
            } else if (-0.10..0.19).contains(&y) && x.abs() < 0.7 {
                let ey = if pose == AvatarPose::Waiting {
                    0.07
                } else {
                    0.035
                };
                let eye = ((x.abs() - 0.27) / 0.115).powi(2)
                    + ((y - ey) / (0.10 + level * 0.035)).powi(2);
                if eye < 0.35 {
                    t.text
                } else if eye < 1.0 {
                    if pose == AvatarPose::Error {
                        t.error
                    } else {
                        t.warn
                    }
                } else {
                    t.surface
                }
            } else if (0.19..0.56).contains(&y)
                && (x / 0.7).powi(2) + ((y - 0.24) / 0.34).powi(2) < 1.0
            {
                if y < 0.25 || y > 0.46 {
                    t.text_muted
                } else if pose == AvatarPose::Speaking && ((x * 18.0).sin() > 0.2) {
                    tint
                } else {
                    t.accent_alt
                }
            } else if (y - 0.26).abs() < 0.19 && (x.abs() - 0.77).abs() < 0.07 {
                t.text_muted
            } else if y > 0.54
                && y < 0.70
                    + if pose == AvatarPose::Working {
                        0.12
                    } else {
                        0.0
                    }
                && (x.abs() - 0.37).abs() < 0.045
            {
                tint
            } else {
                Rgba::TRANSPARENT
            }
        }
    }
}
#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
