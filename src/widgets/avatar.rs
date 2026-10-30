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
    fn period(self) -> Option<Duration> {
        match self {
            Self::Ready => Some(Duration::from_millis(500)),
            Self::Waiting => Some(Duration::from_millis(167)),
            Self::Error => None,
            _ => Some(Duration::from_millis(84)),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AvatarStyle {
    #[default]
    Halo,
    Hover,
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
        let cadence = cx.memo(move || {
            let (pose, _, live) = state.get();
            if live && enabled.map(|s| s.get()).unwrap_or(false) {
                pose.period()
            } else {
                None
            }
        });
        cx.effect(move || {
            if let Some(period) = cadence.get() {
                sampled_level
                    .set_if_changed(normalized(levels.map(|s| s.get_untracked()).unwrap_or(0.0)));
                let clock_scope = cx.child();
                interval(clock_scope, period, move || {
                    batch(|| {
                        sampled_level.set_if_changed(normalized(
                            levels.map(|s| s.get_untracked()).unwrap_or(0.0),
                        ));
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
    let breath = if p == AvatarPose::Ready {
        0.014 * (time * 0.9).sin()
    } else {
        0.0
    };
    let expression = if p == AvatarPose::Speaking {
        0.018 * speech(time)
    } else if p == AvatarPose::Listening {
        -0.010 * (0.5 + 0.5 * (time * 1.2).sin())
    } else {
        0.0
    };
    let radius = 0.49 + voice * 0.035 + breath + expression;
    let rim = glow(r - radius, 0.075);
    if r < radius + 0.015 {
        let nx = x / radius;
        let ny = y / radius;
        let z = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
        let diffuse = (-0.45 * nx - 0.5 * ny + 0.72 * z).max(0.0);
        let scan = if p == AvatarPose::Thinking {
            0.12 * (time * 0.8).sin()
        } else {
            0.0
        };
        let highlight = glow(
            ((nx + 0.30 - scan).powi(2) + (ny + 0.35).powi(2)).sqrt(),
            0.22,
        );
        let veins = (a * 5.0 + r * 22.0 - time * if p == AvatarPose::Thinking { 1.7 } else { 0.6 })
            .sin()
            .abs()
            .powi(9)
            * if p == AvatarPose::Thinking {
                0.08
            } else {
                0.035
            };
        let heart = glow(r, radius * 0.58) * 0.18;
        return m.paint(
            &m.warm,
            (0.18 + 0.65 * diffuse + 0.24 * highlight + veins + heart).clamp(0.0, 1.0),
            soft(r - radius, 0.04),
        );
    }
    // The watch Halo's seven broad streamers remain soft, not isolated dots.
    let turn = if p == AvatarPose::Thinking {
        time * 0.25
    } else if p == AvatarPose::Listening {
        -time * 0.24
    } else if p == AvatarPose::Speaking {
        time * 0.45
    } else {
        time * 0.07
    };
    let streamers = (0.5
        + 0.5 * (0.62 * (5.0 * (a - turn)).cos() + 0.38 * (8.0 * (a - turn) + 1.7).cos()))
    .max(0.0);
    let reach = 0.12 + voice * 0.18 + if p == AvatarPose::Thinking { 0.08 } else { 0.0 };
    let crown = glow(r - radius, 0.08 + reach * streamers) * 0.42;
    // Intake travels toward the core; spoken syllables send a wider wave out.
    // The distinction remains visible with a constant or quiet audio level.
    let flow = if matches!(p, AvatarPose::Listening | AvatarPose::Speaking) {
        let progress = (time
            * if p == AvatarPose::Listening {
                0.55
            } else {
                0.90
            })
        .fract();
        let distance = if p == AvatarPose::Listening {
            0.28 * (1.0 - progress)
        } else {
            0.32 * progress
        };
        glow(r - radius - distance, 0.028) * (std::f32::consts::PI * progress).sin().powi(2) * 0.28
    } else {
        0.0
    };
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
        let pulse = 0.5 + 0.5 * (time * 2.4).sin();
        glow(r - (0.83 + 0.018 * (time * 2.4).sin()), 0.045) * (0.30 + 0.25 * pulse)
            + glow(r - 0.83, 0.035) * (0.5 + 0.5 * (a - time * 1.1).cos()).powi(6) * 0.30
    } else {
        0.0
    };
    let intensity = (rim * 0.38 + crown + arc + outer + flow).clamp(0.0, 0.88);
    if intensity < 0.012 {
        m.ground()
    } else {
        m.paint(m.state(p), intensity, 1.0)
    }
}
fn blink(time: f32, period: f32, duration: f32) -> f32 {
    let phase = time.rem_euclid(period);
    if phase < duration {
        1.0 - 0.88 * (std::f32::consts::PI * phase / duration).sin().powi(2)
    } else {
        1.0
    }
}
fn speech(time: f32) -> f32 {
    0.6 * (0.5 + 0.5 * (time * 8.0).sin()) + 0.4 * (0.5 + 0.5 * (time * 13.0 + 1.0).sin())
}
fn hover(p: AvatarPose, level: f32, time: f32, x: f32, y: f32, m: &Materials) -> Rgba {
    let bob = if p == AvatarPose::Working {
        0.10 * (time * 1.6).sin() + 0.012 * (time * 3.1).sin()
    } else if p == AvatarPose::Ready {
        0.018 * (time * 0.85).sin()
    } else if p == AvatarPose::Thinking {
        -0.025 + 0.026 * (time * 0.9).sin()
    } else if matches!(p, AvatarPose::Listening | AvatarPose::Speaking) {
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
        let (s, c) = (-0.24 + 0.055 * (time * 1.1).sin()).sin_cos();
        (x, y) = (c * x + s * y, -s * x + c * y);
    } else if p == AvatarPose::Thinking {
        y += (0.065 + 0.045 * (time * 0.8).sin()) * x;
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
        return m.paint(
            &m.warm,
            (0.52 + light * 0.36 + reflection * 0.23 + rim).clamp(0.0, 1.0),
            soft(dome_edge, 0.05),
        );
    }
    // Curved polished rim along the lower dome, following the watch barrel.
    let top_rim = -0.10 + 0.055 * (x / 0.68).powi(2);
    if x.abs() < 0.69 && (y - top_rim).abs() < 0.022 {
        return m.paint(&m.warm, 0.90, soft(x.abs() - 0.68, 0.04));
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
        let blink = if p == AvatarPose::Ready {
            blink(time, 5.0, 0.8)
        } else if p == AvatarPose::Thinking {
            blink(time + 1.0, 4.7, 0.35)
        } else if p == AvatarPose::Listening {
            blink(time + 2.0, 6.3, 0.3)
        } else {
            1.0
        };
        let open = open * blink;
        let gaze = if p == AvatarPose::Waiting {
            0.045
        } else if p == AvatarPose::Thinking {
            0.075 * (time * 1.1).sin()
        } else {
            0.0
        };
        let d = (((x - gaze).abs() - eye_x).powi(2) + ((y - eye_y) / open).powi(2)).sqrt();
        let radius = 0.088
            + if p == AvatarPose::Speaking {
                level * 0.018 + 0.008 * speech(time)
            } else if p == AvatarPose::Listening {
                level * 0.012
            } else {
                0.0
            };
        let eye = if p == AvatarPose::Waiting {
            &m.warm
        } else {
            &m.eye
        };
        if d < radius {
            let core = 1.0 - d / radius;
            let brightness = if p == AvatarPose::Waiting {
                0.42 + 0.22 * core.powf(0.35) + 0.15 * (0.5 + 0.5 * (time * 2.4).sin())
            } else {
                0.66 + 0.34 * core.powf(0.35)
            };
            return m.paint(eye, brightness, 1.0);
        }
        let shine = glow(d - radius, 0.055) * 0.54;
        return if shine > 0.03 {
            m.paint(eye, shine, 1.0)
        } else {
            m.paint(&m.dim, 0.12 + 0.08 * (1.0 - x.abs()), 1.0)
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
            && x.abs() < 0.23
            && (y - 0.35).abs() < 0.025 + (0.035 + level * 0.045) * speech(time)
        {
            return if (x * 55.0).sin() > 0.1 {
                m.paint(&m.eye, 0.38 + 0.35 * level + 0.17 * speech(time), 1.0)
            } else {
                m.paint(&m.dim, 0.10, 1.0)
            };
        }
        return m.paint(
            &m.warm,
            (lit + band * 0.18).clamp(0.0, 1.0),
            soft((hull - 1.0) * 0.35, 0.05),
        );
    }
    // Rounded arms, extending a little while tools are working.
    let reach = if p == AvatarPose::Working { 0.12 } else { 0.0 };
    let arm = if p == AvatarPose::Listening && x < 0.0 {
        segment(x, y, -0.72, 0.06, -0.82 - 0.025 * (time * 1.4).sin(), -0.20) - 0.065
    } else if p == AvatarPose::Waiting {
        segment(x.abs(), y, 0.72, 0.06, 0.69, 0.42) - 0.055
    } else {
        segment(x.abs(), y, 0.72, 0.06, 0.75 + reach, 0.40 - reach) - 0.055
    };
    if arm < 0.025 {
        return m.paint(
            &m.warm,
            (0.75 - x.abs() * 0.1).clamp(0.0, 1.0),
            soft(arm, 0.05),
        );
    }
    if p == AvatarPose::Waiting {
        let hot = (time * 1.5).floor() as usize % 3;
        let dots = [-0.15f32, 0.0, 0.15]
            .iter()
            .enumerate()
            .map(|(i, dx)| {
                glow(x - dx, 0.035) * glow(y - 0.76, 0.035) * if i == hot { 0.72 } else { 0.25 }
            })
            .fold(0.0f32, f32::max);
        if dots > 0.02 {
            return m.paint(&m.warm, dots, 1.0);
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
        m.ground()
    } else {
        m.paint(&m.eye, intensity, 1.0)
    }
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
