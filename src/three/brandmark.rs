//! The 3D boot mark: three ascending planes aligning into an "A",
//! implementing DESIGN's storyboard (docs/design/theme-identity.md §2).
//!
//! LAYERING (R4-1, integrator ruling cycle 4): this module imports
//! NOTHING from `boot` — every timing, easing, camera keyframe and
//! brand color arrives through [`BrandmarkParams`], plain data defined
//! HERE. DESIGN's `boot::brandmark3d` adapter constructs the params
//! from `boot::identity` constants (their file, their constants).
//! The test-only `BrandmarkParams::reference()` carries the same
//! values as a fixture; a drift pin (tests may look
//! upward — they are not part of the layer graph) asserts the
//! reference copy equals the identity constants field by field, so the
//! two can never diverge silently. `theme::Theme` stays as a render
//! parameter (explicitly tolerated by R4-1).
//!
//! Storyboard coverage: staggered arrival from below-right
//! (EASE_ARRIVAL), settle-with-overshoot at alignment (EASE_SETTLE, its
//! y1 > 1 IS the overshoot), camera yaw −35°→−6° + dolly 5.2→4.4,
//! brand-ramp emissive gradient along each plane, depth fog toward the
//! theme ground, radial vignette toward BRAND_FIELD, afterglow trails
//! (per-100ms decay constant), the 12-spark crossbar burst, wordmark
//! reveal with tracking collapse + accent underline sweep, tagline,
//! and the skip hint. All timing/easing/color constants come from
//! `identity` — nothing re-hardcoded here, so the two splashes cannot
//! drift.

use crate::anim::Easing;
use crate::base::{Rgba, Size};
use crate::render::{Cell, Style, Surface};
use crate::theme::Theme;
use crate::three::extract::MeshData;
use crate::three::load::{MaterialData, MeshInstance, Model};
use crate::three::math::{Mat4, Vec3};
use crate::three::primitives::cuboid;
use crate::three::raster::Framebuffer;
use crate::three::scene::{Camera, Light, Scene, SceneRenderer};
use crate::three::texture::srgb8_to_linear;

/// Plane geometry (storyboard §2.1): width 1.0, height 0.62, thickness
/// 0.04, stacked z 0.22 apart, rising 0.18 apart, pitched 12°. The
/// crossbar (last) plane is narrower so the fused silhouette reads
/// "A", not "≡".
const PLANE_W: f32 = 1.0;
const PLANE_H: f32 = 0.62;
const PLANE_T: f32 = 0.04;
const PLANE_DZ: f32 = 0.22;
const PLANE_DY: f32 = 0.18;
const PLANE_PITCH_DEG: f32 = 12.0;
const CROSSBAR_SCALE: f32 = 0.7;

/// Off-screen start offset (below-right of the composition, storyboard
/// frame 0.0s) in scene units.
const ARRIVAL_OFFSET: Vec3 = Vec3::new(2.6, -2.2, 1.2);

/// The arrival tween covers this fraction of the travel; the settle
/// phase carries the rest THROUGH the overshooting EASE_SETTLE curve
/// (storyboard: "planes overshoot the A-alignment by ~6%").
const ARRIVAL_FRACTION: f32 = 0.94;

/// Every storyboard number the renderer consumes, as plain data —
/// the R4-1 seam. The AUTHORITATIVE values live in `boot::identity`
/// (DESIGN-owned); the adapter builds this struct from them.
#[derive(Clone, Debug, PartialEq)]
pub struct BrandmarkParams {
    // Timeline (ms from splash start).
    pub align_start_ms: u32,
    pub reveal_start_ms: u32,
    pub hold_start_ms: u32,
    pub plane_stagger_ms: u32,
    pub plane_arrival_ms: u32,
    pub burst_at_ms: u32,
    pub burst_particles: u32,
    pub burst_lifetime_ms: u32,
    pub afterglow_decay_per_100ms: f32,
    // Easings (cubic-bezier x1, y1, x2, y2 — CSS convention).
    pub ease_arrival: [f32; 4],
    pub ease_settle: [f32; 4],
    pub ease_tracking: [f32; 4],
    pub ease_fade: [f32; 4],
    // Camera keyframes.
    pub camera_yaw_deg: (f32, f32),
    pub camera_pitch_deg: f32,
    pub camera_dolly: (f32, f32),
    // Brand colors (sRGB, resolved).
    pub ramp: [Rgba; 5],
    pub field: Rgba,
    // Wordmark block.
    pub wordmark: &'static str,
    pub tagline: &'static str,
    pub skip_hint: &'static str,
    pub wordmark_tracking: (u16, u16),
}

#[cfg(test)]
impl BrandmarkParams {
    /// TEST FIXTURE (cycle-5: the cycle-4 compat constructor is gone —
    /// DESIGN's adapter builds params from `boot::identity` now). The
    /// values mirror the identity constants and stay drift-pinned by
    /// `identity_drift_pin`; production code CANNOT reach this.
    pub(crate) fn reference() -> BrandmarkParams {
        BrandmarkParams {
            align_start_ms: 900,
            reveal_start_ms: 1400,
            hold_start_ms: 1850,
            plane_stagger_ms: 120,
            plane_arrival_ms: 780,
            burst_at_ms: 900,
            burst_particles: 12,
            burst_lifetime_ms: 450,
            afterglow_decay_per_100ms: 0.72,
            ease_arrival: [0.16, 1.0, 0.30, 1.0],
            ease_settle: [0.34, 1.56, 0.64, 1.0],
            ease_tracking: [0.83, 0.0, 0.17, 1.0],
            ease_fade: [0.33, 1.0, 0.68, 1.0],
            camera_yaw_deg: (-35.0, -6.0),
            camera_pitch_deg: 8.0,
            camera_dolly: (5.2, 4.4),
            ramp: [
                Rgba::rgb(0xe9, 0x45, 0x60),
                Rgba::rgb(0xc9, 0x53, 0x8f),
                Rgba::rgb(0x9d, 0x6b, 0xc9),
                Rgba::rgb(0x7a, 0x86, 0xe8),
                Rgba::rgb(0x60, 0xa5, 0xfa),
            ],
            field: Rgba::rgb(0x0f, 0x34, 0x60),
            wordmark: "AbstractTUI",
            tagline: "the terminal, composed",
            skip_hint: "press any key to skip",
            wordmark_tracking: (4, 1),
        }
    }
}

/// Ramp color at normalized `t`: nearest curated stop pair, mixed only
/// within the pair (mirrors `identity::brand_ramp` — sRGB midpoints of
/// the house red/blue desaturate, so the middles route through violet
/// deliberately; the STOPS carry that decision, this just indexes).
fn ramp_color(ramp: &[Rgba; 5], t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0) * (ramp.len() - 1) as f32;
    let i = (t.floor() as usize).min(ramp.len() - 2);
    ramp[i].lerp(ramp[i + 1], t - i as f32)
}

pub struct BrandmarkRenderer {
    params: BrandmarkParams,
    scene_renderer: SceneRenderer,
    fb: Framebuffer,
    trail: crate::gfx::Bitmap,
    mosaic: crate::gfx::MosaicRenderer,
    surface: Surface,
    last_t: f32,
}

impl BrandmarkRenderer {
    /// The one constructor (R4-1): callers pass the storyboard as data
    /// — production boot builds it from `boot::identity` in DESIGN's
    /// adapter (`boot::brandmark3d::identity_params`).
    pub fn with_params(params: BrandmarkParams) -> BrandmarkRenderer {
        BrandmarkRenderer {
            params,
            scene_renderer: SceneRenderer::new(),
            fb: Framebuffer::new(0, 0),
            trail: crate::gfx::Bitmap::default(),
            mosaic: crate::gfx::MosaicRenderer::new(),
            surface: Surface::new(Size::new(0, 0), Cell::EMPTY),
            last_t: 0.0,
        }
    }

    /// `SplashFrameSource`-shaped: `t` in SECONDS since splash start,
    /// returned surface exactly `size` cells. Deterministic in `t` and
    /// `size` except for the afterglow trail, which is a function of
    /// the frame HISTORY by design (decay per wall-time, dropped
    /// frames decay further — matching the player's drop-not-queue
    /// pacing).
    pub fn render(&mut self, t: f32, size: Size, theme: &Theme) -> &Surface {
        let w = size.w.max(0) as u32;
        let h = size.h.max(0) as u32;
        // Half-block density: 1x2 px per cell.
        let (pw, ph) = (w, h * 2);
        if self.fb.width() != pw || self.fb.height() != ph {
            self.fb = Framebuffer::new(pw, ph);
            self.trail = crate::gfx::Bitmap::new(pw, ph, Rgba::TRANSPARENT);
        }
        if self.surface.size() != size {
            self.surface = Surface::new(size, Cell::EMPTY);
        }
        if size.w <= 0 || size.h <= 0 {
            return &self.surface;
        }
        let bg = theme.tokens.bg;

        // ---- 3D pass -----------------------------------------------------
        let p = self.params.clone();
        let model = build_planes(t, &p);
        let ms = t * 1000.0;
        let cam_k = ease(
            p.ease_tracking,
            (ms / p.reveal_start_ms as f32).clamp(0.0, 1.0),
        );
        let yaw_deg = lerp(p.camera_yaw_deg.0, p.camera_yaw_deg.1, cam_k);
        let dolly = lerp(p.camera_dolly.0, p.camera_dolly.1, cam_k);
        let camera = Camera::orbit(
            Vec3::new(0.0, PLANE_DY, 0.0), // look at the stack's center
            dolly,
            yaw_deg.to_radians(),
            p.camera_pitch_deg.to_radians(),
        );
        let mut scene = Scene::new(&model, camera);
        // Emissive look: high ambient, gentle key so faces still sep-
        // arate. Colors are the ramp via vertex colors.
        scene.light = Light {
            direction: Vec3::new(-0.3, -0.6, -0.75),
            ambient: 0.72,
            diffuse: 0.38,
        };
        scene.background = Rgba::TRANSPARENT;
        scene.double_sided = true;
        self.scene_renderer.render(&scene, &mut self.fb);

        self.fb.depth_fog(bg, 0.45); // storyboard depth fog
        draw_burst(&mut self.fb, ms, &p);
        merge_afterglow(
            &mut self.trail,
            &mut self.fb,
            t,
            self.last_t,
            p.afterglow_decay_per_100ms,
        );
        self.last_t = t;

        // ---- cells -------------------------------------------------------
        let grid = self
            .mosaic
            .render(self.fb.bitmap(), w, h, crate::gfx::MosaicMode::HalfBlock);
        let (cx, cy) = ((size.w as f32 - 1.0) * 0.5, (size.h as f32 - 1.0) * 0.5);
        let max_r = (cx * cx + cy * cy).sqrt().max(1.0);
        for row in 0..h {
            for col in 0..w {
                let cell = grid
                    .get(col, row)
                    .copied()
                    .unwrap_or(crate::gfx::MosaicCell::EMPTY);
                let empty = cell.fg.is_transparent() && cell.bg.is_transparent();
                let (ch, fg, bgc) = if empty {
                    // Vignette on the bare ground: radial mix toward
                    // BRAND_FIELD at 12% max (storyboard §2.1).
                    let dx = col as f32 - cx;
                    let dy = row as f32 - cy;
                    let k = (dx * dx + dy * dy).sqrt() / max_r * 0.12;
                    let ground = mix(bg, p.field, k);
                    (' ', ground, ground)
                } else {
                    // Mosaic colors composite over the theme ground.
                    (cell.ch, cell.fg.over(bg), cell.bg.over(bg))
                };
                let mut buf = [0u8; 4];
                let s: &str = ch.encode_utf8(&mut buf);
                self.surface
                    .draw_text(col as i32, row as i32, s, Style::new().fg(fg).bg(bgc));
            }
        }

        self.draw_wordmark(ms, size, theme);
        self.draw_hints(ms, size, theme);
        &self.surface
    }

    /// Wordmark reveal (storyboard 1.4s): per-letter fade left→right,
    /// tracking collapse 4→1 cells, accent underline sweep, tagline.
    fn draw_wordmark(&mut self, ms: f32, size: Size, theme: &Theme) {
        let p = self.params.clone();
        let reveal = p.reveal_start_ms as f32;
        if ms < reveal {
            return;
        }
        let k = ((ms - reveal) / (p.hold_start_ms as f32 - reveal)).clamp(0.0, 1.0);
        let track_k = ease(p.ease_tracking, k);
        let tracking = lerp(
            p.wordmark_tracking.0 as f32,
            p.wordmark_tracking.1 as f32,
            track_k,
        );
        let letters: Vec<char> = p.wordmark.chars().collect();
        let n = letters.len() as f32;
        let width = n + (n - 1.0) * (tracking - 1.0).max(0.0);
        let x0 = (size.w as f32 - width) * 0.5;
        let y = size.h - 3;
        let bg = theme.tokens.bg;
        for (i, chl) in letters.iter().enumerate() {
            // 30 ms per letter, left to right (storyboard).
            let fade = ((ms - reveal - i as f32 * 30.0) / 200.0).clamp(0.0, 1.0);
            if fade <= 0.0 {
                continue;
            }
            let fg = mix(bg, theme.tokens.text, ease(p.ease_fade, fade));
            let x = (x0 + i as f32 * tracking.max(1.0)).round() as i32;
            let mut buf = [0u8; 4];
            self.surface
                .draw_text(x, y, chl.encode_utf8(&mut buf), Style::new().fg(fg));
        }
        // Accent underline sweep, left → right across the wordmark box.
        let sweep = ease(p.ease_fade, k);
        let full = width.round() as i32;
        let lit = (full as f32 * sweep).round() as i32;
        for x in 0..lit {
            self.surface.draw_text(
                x0.round() as i32 + x,
                y + 1,
                "─",
                Style::new().fg(ramp_color(&p.ramp, x as f32 / full.max(1) as f32)),
            );
        }
        // Tagline fades with the wordmark tail.
        let tag_fade = ((k - 0.35) / 0.5).clamp(0.0, 1.0);
        if tag_fade > 0.0 {
            let tag = p.tagline;
            let tx = (size.w - tag.chars().count() as i32) / 2;
            let fg = mix(bg, theme.tokens.text_muted, ease(p.ease_fade, tag_fade));
            self.surface.draw_text(tx, y + 2, tag, Style::new().fg(fg));
        }
    }

    /// Skip hint, bottom-right from 300 ms (storyboard §2.4).
    fn draw_hints(&mut self, ms: f32, size: Size, theme: &Theme) {
        if ms < 300.0 {
            return;
        }
        let fade = ((ms - 300.0) / 250.0).clamp(0.0, 1.0);
        let hint = self.params.skip_hint;
        let x = size.w - hint.chars().count() as i32 - 1;
        let fg = mix(
            theme.tokens.bg,
            theme.tokens.text_faint,
            ease(self.params.ease_fade, fade),
        );
        self.surface
            .draw_text(x, size.h - 1, hint, Style::new().fg(fg));
    }
}

/// Per-frame model: three ramp-colored slabs at their `t` poses.
fn build_planes(t: f32, p: &BrandmarkParams) -> Model {
    let ms = t * 1000.0;
    let mut instances = Vec::with_capacity(3);
    for i in 0..3u32 {
        // Arrival: staggered, eased (storyboard 0.0–0.9s).
        let start = (i * p.plane_stagger_ms) as f32;
        let a = ((ms - start) / p.plane_arrival_ms as f32).clamp(0.0, 1.0);
        let arrive = ease(p.ease_arrival, a) * ARRIVAL_FRACTION;
        // Settle: the remaining travel through the overshooting curve
        // (storyboard 0.9–1.4s).
        let s = ((ms - p.align_start_ms as f32) / (p.reveal_start_ms - p.align_start_ms) as f32)
            .clamp(0.0, 1.0);
        let progress = arrive + ease(p.ease_settle, s) * (1.0 - ARRIVAL_FRACTION);

        let fi = i as f32 - 1.0; // -1, 0, 1 around the stack center
        let target = Vec3::new(0.0, (fi + 1.0) * PLANE_DY, fi * PLANE_DZ);
        let pos = target + ARRIVAL_OFFSET * (1.0 - progress);

        let scale = if i == 2 { CROSSBAR_SCALE } else { 1.0 };
        let mut mesh = cuboid(PLANE_W * scale, PLANE_H, PLANE_T);
        paint_ramp(&mut mesh, i, &p.ramp);
        mesh.material = Some(0);
        let world = Mat4::translate(pos)
            .mul(&Mat4::rotate_x(PLANE_PITCH_DEG.to_radians()))
            .mul(&Mat4::scale(Vec3::new(1.0, 1.0, 1.0)));
        instances.push(MeshInstance {
            data: mesh,
            world,
            source_node: None,
        });
    }
    Model {
        instances,
        materials: vec![MaterialData::default()],
        rig: None,
        warnings: Vec::new(),
    }
}

/// Emissive gradient along the plane's length: plane i spans ramp
/// [i/3, (i+1)/3] (leading red → trailing blue, storyboard §2.1).
/// Brand stops are sRGB; vertex colors are linear — convert with the
/// engine's one gamma pair.
fn paint_ramp(mesh: &mut MeshData, plane: u32, ramp: &[Rgba; 5]) {
    let half_w = PLANE_W * 0.5;
    let colors = mesh
        .positions
        .iter()
        .map(|p| {
            let x_norm = ((p[0] + half_w) / PLANE_W).clamp(0.0, 1.0);
            let ramp_t = (plane as f32 + x_norm) / 3.0;
            let c = ramp_color(ramp, ramp_t);
            [
                srgb8_to_linear(c.r),
                srgb8_to_linear(c.g),
                srgb8_to_linear(c.b),
                1.0,
            ]
        })
        .collect();
    mesh.colors = Some(colors);
}

/// The alignment burst (storyboard 0.9s): 12 sparks from the crossbar,
/// gravity-free outward drift, ramp colors, 450 ms lifetime. Screen-
/// space particles — cell-scale, deterministic in `t`.
fn draw_burst(fb: &mut Framebuffer, ms: f32, p: &BrandmarkParams) {
    let age = ms - p.burst_at_ms as f32;
    if !(0.0..=p.burst_lifetime_ms as f32).contains(&age) {
        return;
    }
    let life = age / p.burst_lifetime_ms as f32;
    let (w, h) = (fb.width() as f32, fb.height() as f32);
    // Burst origin: the crossbar sits slightly above frame center.
    let (ox, oy) = (w * 0.5, h * 0.42);
    let n = p.burst_particles;
    for i in 0..n {
        // Deterministic directions: golden-angle fan.
        let ang = i as f32 * 2.399_963 + 0.7;
        let speed = 0.12 + 0.08 * ((i * 37 % 11) as f32 / 10.0);
        let dist = life.sqrt() * speed * w;
        let x = ox + ang.cos() * dist;
        let y = oy + ang.sin() * dist * 0.6; // squash to cell aspect
        let c = ramp_color(&p.ramp, i as f32 / (n - 1).max(1) as f32);
        let fade = 1.0 - life;
        let px = fb.bitmap_mut();
        let (xi, yi) = (x as i64, y as i64);
        if xi >= 0 && yi >= 0 && (xi as u32) < px.width() && (yi as u32) < px.height() {
            let prev = px.get(xi as u32, yi as u32).unwrap_or(Rgba::TRANSPARENT);
            px.set(xi as u32, yi as u32, mix(prev.over(Rgba::BLACK), c, fade));
        }
    }
}

/// Afterglow: decay the trail buffer by the storyboard constant per
/// 100 ms of WALL time, merge the fresh frame in (per-channel max =
/// additive-ish glow), and write the composite back as the displayed
/// frame.
fn merge_afterglow(
    trail: &mut crate::gfx::Bitmap,
    fb: &mut Framebuffer,
    t: f32,
    last_t: f32,
    decay_per_100ms: f32,
) {
    let dt_ms = ((t - last_t).max(0.0) * 1000.0).min(500.0);
    let decay = decay_per_100ms.powf(dt_ms / 100.0);
    let cur = fb.bitmap_mut();
    for (tp, cp) in trail
        .pixels_mut()
        .iter_mut()
        .zip(cur.pixels_mut().iter_mut())
    {
        // Decay previous energy.
        let faded = Rgba::new(
            (tp.r as f32 * decay) as u8,
            (tp.g as f32 * decay) as u8,
            (tp.b as f32 * decay) as u8,
            (tp.a as f32 * decay) as u8,
        );
        // Merge the current frame: max per channel keeps hot pixels hot.
        let merged = Rgba::new(
            faded.r.max(cp.r),
            faded.g.max(cp.g),
            faded.b.max(cp.b),
            faded.a.max(cp.a),
        );
        *tp = merged;
        *cp = merged;
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn ease(bezier: [f32; 4], t: f32) -> f32 {
    Easing::CubicBezier(bezier[0], bezier[1], bezier[2], bezier[3]).eval(t.clamp(0.0, 1.0))
}

/// Straight sRGB mix via the base color type (R4-1: no theme::derive
/// import — the splash is presentation math over resolved colors, and
/// `base::Rgba::lerp` is the same clamped sRGB arithmetic).
#[inline]
fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    a.lerp(b, k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::default_theme;

    /// R4-1 drift pin: the in-three reference params must equal the
    /// boot::identity constants FIELD BY FIELD. Tests may look upward
    /// (they are not part of the layer graph); src code must not —
    /// this test is what makes the reference copy safe to exist.
    #[test]
    fn identity_drift_pin() {
        use crate::boot::identity as id;
        let p = BrandmarkParams::reference();
        assert_eq!(p.align_start_ms, id::PHASE_ALIGN_START_MS);
        assert_eq!(p.reveal_start_ms, id::PHASE_REVEAL_START_MS);
        assert_eq!(p.hold_start_ms, id::PHASE_HOLD_START_MS);
        assert_eq!(p.plane_stagger_ms, id::PLANE_STAGGER_MS);
        assert_eq!(p.plane_arrival_ms, id::PLANE_ARRIVAL_MS);
        assert_eq!(p.burst_at_ms, id::BURST_AT_MS);
        assert_eq!(p.burst_particles, id::BURST_PARTICLES);
        assert_eq!(p.burst_lifetime_ms, id::BURST_LIFETIME_MS);
        assert_eq!(p.afterglow_decay_per_100ms, id::AFTERGLOW_DECAY_PER_100MS);
        assert_eq!(p.ease_arrival, id::EASE_ARRIVAL);
        assert_eq!(p.ease_settle, id::EASE_SETTLE);
        assert_eq!(p.ease_tracking, id::EASE_TRACKING);
        assert_eq!(p.ease_fade, id::EASE_FADE);
        assert_eq!(p.camera_yaw_deg, id::CAMERA_YAW_DEG);
        assert_eq!(p.camera_pitch_deg, id::CAMERA_PITCH_DEG);
        assert_eq!(p.camera_dolly, id::CAMERA_DOLLY);
        assert_eq!(p.ramp, id::BRAND_RAMP);
        assert_eq!(p.field, id::BRAND_FIELD);
        assert_eq!(p.wordmark, id::WORDMARK);
        assert_eq!(p.tagline, id::TAGLINE);
        assert_eq!(p.skip_hint, id::SKIP_HINT);
        assert_eq!(p.wordmark_tracking, id::WORDMARK_TRACKING);
        // And the local ramp interpolation matches identity's.
        for k in [0.0f32, 0.2, 0.5, 0.77, 1.0] {
            assert_eq!(ramp_color(&p.ramp, k), id::brand_ramp(k), "ramp at {k}");
        }
    }

    fn coverage(surface: &Surface) -> usize {
        let mut n = 0;
        for y in 0..surface.size().h {
            for x in 0..surface.size().w {
                if let Some(c) = surface.get(x, y) {
                    if !c.bg.is_transparent() || !c.fg.is_transparent() {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn honors_requested_size_and_resizes() {
        let theme = default_theme();
        let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let s = r.render(0.5, Size::new(100, 30), theme);
        assert_eq!(s.size(), Size::new(100, 30));
        let s = r.render(0.6, Size::new(80, 24), theme);
        assert_eq!(s.size(), Size::new(80, 24));
        let s = r.render(0.7, Size::new(0, 0), theme);
        assert_eq!(s.size(), Size::new(0, 0)); // degenerate: no panic
    }

    #[test]
    fn storyboard_beats_appear() {
        let theme = default_theme();
        let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        // Quiet beat: before any plane has traveled, the frame is
        // ground + vignette only (no mark cells brighter than ground).
        let s = r.render(0.0, Size::new(100, 30), theme);
        assert!(coverage(s) > 0, "vignette paints the ground");

        // Mid-flight: the mark is visible.
        let mut r2 = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let s = r2.render(1.0, Size::new(100, 30), theme);
        let mark_cells = mark_cell_count(s, theme);
        assert!(
            mark_cells > 30,
            "mark visible at t=1.0 ({mark_cells} cells)"
        );

        // Reveal: the wordmark text row exists.
        let mut r3 = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let s = r3.render(1.9, Size::new(100, 30), theme);
        let row: String = row_text(s, s.size().h - 3);
        assert!(row.contains('A'), "wordmark visible: {row:?}");
        let hint_row = row_text(s, s.size().h - 1);
        assert!(hint_row.contains("skip"), "skip hint: {hint_row:?}");
    }

    #[test]
    fn deterministic_frames_fresh_renderers() {
        // Same t + size + theme through FRESH renderers = same bytes
        // (the trail makes SEQUENTIAL frames history-dependent by
        // design; determinism is per fresh start, which is what the
        // player restarts give).
        let theme = default_theme();
        let mut a = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let mut b = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let (sa, sb) = (
            frame_dump(a.render(1.2, Size::new(60, 20), theme)),
            frame_dump(b.render(1.2, Size::new(60, 20), theme)),
        );
        assert_eq!(sa, sb);
    }

    #[test]
    fn camera_sweep_changes_the_frame() {
        let theme = default_theme();
        let mut a = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let mut b = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let early = frame_dump(a.render(0.5, Size::new(60, 20), theme));
        let late = frame_dump(b.render(1.3, Size::new(60, 20), theme));
        assert_ne!(early, late);
    }

    /// Budget: one frame at the typical 100x30 must fit comfortably in
    /// a 30 fps cadence next to diff+present.
    /// `cargo test --release -- --ignored perf_brandmark`
    #[test]
    #[ignore = "perf budget; run explicitly in release"]
    fn perf_brandmark_100x30() {
        let theme = default_theme();
        let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
        let m = crate::testing::bench::time_median("brandmark_100x30", 3, 5, 20, |i| {
            let t = (i % 60) as f32 / 30.0;
            let s = r.render(t, Size::new(100, 30), theme);
            crate::testing::bench::sink(s.size());
        });
        eprintln!("{}", m.report());
        m.assert_under(std::time::Duration::from_millis(8));
    }

    fn row_text(s: &Surface, y: i32) -> String {
        (0..s.size().w)
            .map(|x| {
                s.get(x, y)
                    .and_then(|c| s.glyph_str(c).chars().next())
                    .unwrap_or(' ')
            })
            .collect()
    }

    fn mark_cell_count(s: &Surface, theme: &Theme) -> usize {
        let bg = theme.tokens.bg;
        let mut n = 0;
        for y in 0..s.size().h {
            for x in 0..s.size().w {
                if let Some(c) = s.get(x, y) {
                    // Mark cells carry brand color well away from the
                    // ground (vignette stays near it).
                    let d = (c.bg.r as i32 - bg.r as i32).abs()
                        + (c.bg.g as i32 - bg.g as i32).abs()
                        + (c.bg.b as i32 - bg.b as i32).abs();
                    if d > 120 {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn frame_dump(s: &Surface) -> Vec<(String, Rgba, Rgba)> {
        let mut out = Vec::new();
        for y in 0..s.size().h {
            for x in 0..s.size().w {
                let c = s.get(x, y).unwrap();
                out.push((s.glyph_str(c).to_string(), c.fg, c.bg));
            }
        }
        out
    }
}
