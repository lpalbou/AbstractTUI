//! Procedural, backend-independent activity avatar. OWNER: DESIGN.
//!
//! Halo rays and Hover's dome/eye-band/thrusters adapt the geometric vocabulary
//! of AbstractAssistant's watch Orb/OrbMachines scenes. The host supplies all
//! activity and audio state; this widget performs no I/O or inference.
use crate::{
    base::{Point, Rgba},
    gfx::{Bitmap, MosaicMode, MosaicRenderer},
    layout::{Dimension, Style as LayoutStyle},
    reactive::{batch, interval, on_cleanup, Scope, Signal},
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
        let sampled_level = cx.signal(0.0f32);
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
        let moving = cx.memo(move || {
            let (pose, _, live) = state.get();
            live && pose.moves() && enabled.map(|s| s.get()).unwrap_or(false)
        });
        cx.effect(move || {
            if moving.get() {
                sampled_level
                    .set_if_changed(normalized(levels.map(|s| s.get_untracked()).unwrap_or(0.0)));
                let clock_scope = cx.child();
                interval(clock_scope, Duration::from_millis(84), move || {
                    batch(|| {
                        sampled_level.set_if_changed(normalized(
                            levels.map(|s| s.get_untracked()).unwrap_or(0.0),
                        ));
                        frame.update(|n| *n = n.wrapping_add(1));
                    });
                });
                on_cleanup(move || clock_scope.dispose());
            }
        });
        let render_level = cx.memo(move || {
            let (pose, _, live) = state.get();
            if !live || !matches!(pose, AvatarPose::Listening | AvatarPose::Speaking) {
                0.0
            } else if moving.get() {
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
        let materials = cx.memo(move || std::rc::Rc::new(Materials::new(&palette.get())));
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
                let phase = if moving.get() {
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
                        for y in 0..image.height() {
                            for x in 0..image.width() {
                                let nx =
                                    ((x as f32 + 0.5) / sx as f32 - cols as f32 * 0.5) / radius;
                                let ny = ((y as f32 + 0.5) / sy as f32 - rows as f32 * 0.5) * 2.0
                                    / radius;
                                image.set(x, y, sample(style, p, level, phase, nx, ny, &materials));
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
                                // Use lower-half foreground consistently: some
                                // terminal/SVG fonts leave an ascender gap above ▀.
                                // Putting the brighter lower edge in the background
                                // would expose an orphan stripe above the silhouette.
                                let (ch, fg, bg) = if cell.ch == '▀' {
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
// All colors come from theme-owned material ramps, prepared once per theme.
struct Materials {
    tokens: TokenSet,
    warm: crate::theme::derive::MaterialRamp,
    eye: crate::theme::derive::MaterialRamp,
    dim: crate::theme::derive::MaterialRamp,
}
impl PartialEq for Materials {
    fn eq(&self, other: &Self) -> bool {
        self.tokens == other.tokens
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
        }
    }
    fn state(&self, p: AvatarPose) -> &crate::theme::derive::MaterialRamp {
        if p == AvatarPose::Error {
            &self.eye
        } else {
            &self.warm
        }
    }
}
fn soft(distance: f32, width: f32) -> f32 {
    (0.5 - distance / width.max(0.001)).clamp(0.0, 1.0)
}
fn glow(distance: f32, width: f32) -> f32 {
    (-(distance / width).powi(2)).exp()
}
fn segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let u = ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy);
    let u = u.clamp(0.0, 1.0);
    ((x - ax - u * dx).powi(2) + (y - ay - u * dy).powi(2)).sqrt()
}
fn sample(
    style: AvatarStyle,
    pose: AvatarPose,
    level: f32,
    time: f32,
    x: f32,
    y: f32,
    m: &Materials,
) -> Rgba {
    match style {
        AvatarStyle::Halo => halo(pose, level, time, x, y, m),
        AvatarStyle::Hover => hover(pose, level, time, x, y, m),
    }
}
fn halo(p: AvatarPose, level: f32, time: f32, x: f32, y: f32, m: &Materials) -> Rgba {
    let r = (x * x + y * y).sqrt();
    let a = y.atan2(x);
    let voice = if matches!(p, AvatarPose::Speaking | AvatarPose::Listening) {
        level
    } else {
        0.0
    };
    let radius = 0.49 + voice * 0.035;
    let rim = glow(r - radius, 0.075);
    if r < radius + 0.015 {
        let nx = x / radius;
        let ny = y / radius;
        let z = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
        let diffuse = (-0.45 * nx - 0.5 * ny + 0.72 * z).max(0.0);
        let highlight = glow(((nx + 0.30).powi(2) + (ny + 0.35).powi(2)).sqrt(), 0.22);
        let veins = (a * 5.0 + r * 22.0 - time * 0.6).sin().abs().powi(9) * 0.035;
        let heart = glow(r, radius * 0.58) * 0.18;
        return m.warm.sample(
            (0.18 + 0.65 * diffuse + 0.24 * highlight + veins + heart).clamp(0.0, 1.0),
            soft(r - radius, 0.04),
        );
    }
    // The watch Halo's seven broad streamers remain soft, not isolated dots.
    let turn = if p == AvatarPose::Thinking {
        time * 0.25
    } else {
        time * 0.07
    };
    let streamers = (0.5
        + 0.5 * (0.62 * (5.0 * (a - turn)).cos() + 0.38 * (8.0 * (a - turn) + 1.7).cos()))
    .max(0.0);
    let reach = 0.12 + voice * 0.18 + if p == AvatarPose::Thinking { 0.08 } else { 0.0 };
    let crown = glow(r - radius, 0.08 + reach * streamers) * 0.42;
    let rotation = if p == AvatarPose::Working {
        time * 0.65
    } else {
        time * 0.35
    };
    let (c, s) = (rotation.cos(), rotation.sin());
    let (rx, ry) = (x * c - y * s, x * s + y * c);
    let orbit = ((rx / 0.79).powi(2) + (ry / 0.31).powi(2)).sqrt();
    let arc = if matches!(p, AvatarPose::Thinking | AvatarPose::Working) {
        glow(orbit - 1.0, 0.07) * (0.32 + 0.16 * (a - time).cos())
    } else {
        glow(r - 0.76, 0.065) * 0.12
    };
    let outer = if p == AvatarPose::Waiting {
        glow(r - 0.83, 0.045) * 0.48
    } else {
        0.0
    };
    let intensity = (rim * 0.38 + crown + arc + outer).clamp(0.0, 0.88);
    if intensity < 0.012 {
        m.tokens.bg
    } else {
        m.state(p).sample(intensity, 1.0)
    }
}
fn hover(p: AvatarPose, level: f32, time: f32, x: f32, y: f32, m: &Materials) -> Rgba {
    let bob = if p == AvatarPose::Working {
        0.10 * (time * 1.6).sin() + 0.012 * (time * 3.1).sin()
    } else if p.moves() {
        0.025 * (time * 1.05).sin()
    } else {
        0.0
    };
    let mut x = x;
    let mut y = y - bob;
    if p == AvatarPose::Listening {
        // Cock toward the user's conversation, with a raised listening hand.
        // This is a real head/body roll, not an almost invisible shear.
        x += 0.075;
        let (s, c) = (-0.24f32).sin_cos();
        (x, y) = (c * x + s * y, -s * x + c * y);
    } else if p == AvatarPose::Thinking {
        y += 0.065 * x;
    } else if p == AvatarPose::Waiting {
        y -= 0.065;
    }
    let dome = ((x / 0.68).powi(2) + ((y + 0.11) / 0.50).powi(2)).sqrt();
    let dome_edge = (dome - 1.0) * 0.5;
    if y < -0.11 && dome_edge < 0.025 {
        let nx = x / 0.68;
        let ny = (y + 0.11) / 0.50;
        let z = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
        let light = (-0.45 * nx - 0.40 * ny + 0.73 * z).max(0.0);
        let reflection = glow(((x + 0.24).powi(2) + (y + 0.42).powi(2)).sqrt(), 0.115);
        let rim = glow(dome - 0.98, 0.04) * 0.10;
        return m.warm.sample(
            (0.52 + light * 0.36 + reflection * 0.23 + rim).clamp(0.0, 1.0),
            soft(dome_edge, 0.05),
        );
    }
    // Curved polished rim along the lower dome, following the watch barrel.
    let top_rim = -0.10 + 0.055 * (x / 0.68).powi(2);
    if x.abs() < 0.69 && (y - top_rim).abs() < 0.022 {
        return m.warm.sample(0.90, soft(x.abs() - 0.68, 0.04));
    }
    if (-0.10..0.20).contains(&y) && x.abs() < 0.68 {
        let eye_y = if p == AvatarPose::Waiting {
            0.07
        } else {
            0.035
        };
        let eye_x = 0.27
            + if p == AvatarPose::Thinking {
                0.025
            } else {
                0.0
            };
        let open = if p == AvatarPose::Thinking {
            0.78
        } else if p == AvatarPose::Error {
            0.40
        } else if p == AvatarPose::Waiting {
            0.55
        } else {
            1.0
        };
        let gaze = if p == AvatarPose::Waiting { 0.045 } else { 0.0 };
        let d = (((x - gaze).abs() - eye_x).powi(2) + ((y - eye_y) / open).powi(2)).sqrt();
        let radius = 0.088
            + if p == AvatarPose::Speaking {
                level * 0.018
            } else {
                0.0
            };
        let eye = &m.eye;
        if d < radius {
            let core = 1.0 - d / radius;
            let brightness = if p == AvatarPose::Waiting {
                0.52 + 0.26 * core.powf(0.35)
            } else {
                0.66 + 0.34 * core.powf(0.35)
            };
            return eye.sample(brightness, 1.0);
        }
        let shine = glow(d - radius, 0.055) * 0.54;
        return if shine > 0.03 {
            eye.sample(shine, 1.0)
        } else {
            m.dim.sample(0.12 + 0.08 * (1.0 - x.abs()), 1.0)
        };
    }
    let hull = ((x / 0.69).powi(2) + ((y - 0.24) / 0.34).powi(2)).sqrt();
    if (0.20..0.60).contains(&y) && hull < 1.045 {
        let nx = x / 0.69;
        let z = (1.0 - nx * nx).max(0.0).sqrt();
        let lit = (0.62 + 0.27 * (-0.4 * nx + 0.7 * z) - 0.12 * (y - 0.2)).clamp(0.0, 1.0);
        let band1 = 0.245 + 0.055 * (x / 0.69).powi(2);
        let band2 = 0.455 + 0.045 * (x / 0.69).powi(2);
        let band = glow(y - band1, 0.012).max(glow(y - band2, 0.016));
        if p == AvatarPose::Speaking
            && x.abs() < 0.20
            && (y - 0.35).abs() < 0.055
            && (x * 55.0).sin() > 0.3
        {
            return m.state(p).sample(0.45 + level * 0.4, 1.0);
        }
        return m.warm.sample(
            (lit + band * 0.18).clamp(0.0, 1.0),
            soft((hull - 1.0) * 0.35, 0.05),
        );
    }
    // Rounded arms, extending a little while tools are working.
    let reach = if p == AvatarPose::Working { 0.12 } else { 0.0 };
    let arm = if p == AvatarPose::Listening && x < 0.0 {
        segment(x, y, -0.72, 0.06, -0.82, -0.20) - 0.065
    } else if p == AvatarPose::Waiting {
        segment(x.abs(), y, 0.72, 0.06, 0.69, 0.42) - 0.055
    } else {
        segment(x.abs(), y, 0.72, 0.06, 0.75 + reach, 0.40 - reach) - 0.055
    };
    if arm < 0.025 {
        return m
            .warm
            .sample((0.75 - x.abs() * 0.1).clamp(0.0, 1.0), soft(arm, 0.05));
    }
    if p == AvatarPose::Waiting {
        let dots = [-0.15f32, 0.0, 0.15]
            .iter()
            .map(|dx| glow(x - dx, 0.035) * glow(y - 0.76, 0.035))
            .fold(0.0f32, f32::max);
        if dots > 0.04 {
            return m.warm.sample(0.55 * dots, 1.0);
        }
    }
    let plume_len = 0.14
        + if p == AvatarPose::Working {
            0.14 + 0.025 * (time * 4.0).sin()
        } else {
            0.0
        };
    let plume = glow(x.abs() - 0.32, 0.045) * glow(y - 0.59, plume_len);
    let pool = glow(x, 0.48) * glow(y - 0.91, 0.04) * 0.16;
    let mut particles = 0.0f32;
    if p == AvatarPose::Working {
        for side in [-1.0f32, 1.0] {
            for i in 0..3 {
                let age = (time * 0.7 + i as f32 * 0.31).fract();
                let px = side * 0.32 + side * 0.12 * age;
                let py = 0.65 + 0.34 * age;
                particles += glow(x - px, 0.025) * glow(y - py, 0.035) * (1.0 - age) * 0.65;
            }
        }
    }
    let intensity = (plume * 0.65 + pool + particles).min(0.85);
    if intensity < 0.015 {
        m.tokens.bg
    } else {
        m.eye.sample(intensity, 1.0)
    }
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
