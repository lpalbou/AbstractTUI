//! The 3D brandmark as a splash frame source — DESIGN's thin wrapper over
//! GFX3D's renderer.
//!
//! `three::brandmark::BrandmarkRenderer::render` is deliberately
//! signature-identical to [`SplashFrameSource::render`], but `three` sits
//! BELOW `boot` in the layer map and must not implement an upper layer's
//! trait; the adapter lives here instead (one line of forwarding, zero
//! logic — pacing, skip, fade and cutoff all stay player-owned).
//!
//! OWNER: DESIGN (adapter) — GFX3D owns the renderer behind it.

use crate::base::Size;
use crate::render::Surface;
use crate::theme::Theme;
use crate::three::brandmark::{BrandmarkParams, BrandmarkRenderer};

use super::identity;
use super::player::SplashFrameSource;

/// The three-planes "A" (storyboard §2.1-2.2), pluggable wherever the 2D
/// fallback goes.
pub struct Brandmark3d(BrandmarkRenderer);

/// The storyboard as data: every `BrandmarkParams` field built from its
/// `boot::identity` constant (R4-1 — the layer map holds: three defines
/// the struct, boot fills it, identity stays DESIGN-owned; GFX3D's
/// `identity_drift_pin` test names each pairing).
pub fn identity_params() -> BrandmarkParams {
    BrandmarkParams {
        align_start_ms: identity::PHASE_ALIGN_START_MS,
        reveal_start_ms: identity::PHASE_REVEAL_START_MS,
        hold_start_ms: identity::PHASE_HOLD_START_MS,
        plane_stagger_ms: identity::PLANE_STAGGER_MS,
        plane_arrival_ms: identity::PLANE_ARRIVAL_MS,
        burst_at_ms: identity::BURST_AT_MS,
        burst_particles: identity::BURST_PARTICLES,
        burst_lifetime_ms: identity::BURST_LIFETIME_MS,
        afterglow_decay_per_100ms: identity::AFTERGLOW_DECAY_PER_100MS,
        ease_arrival: identity::EASE_ARRIVAL,
        ease_settle: identity::EASE_SETTLE,
        ease_tracking: identity::EASE_TRACKING,
        ease_fade: identity::EASE_FADE,
        camera_yaw_deg: identity::CAMERA_YAW_DEG,
        camera_pitch_deg: identity::CAMERA_PITCH_DEG,
        camera_dolly: identity::CAMERA_DOLLY,
        ramp: identity::BRAND_RAMP,
        field: identity::BRAND_FIELD,
        wordmark: identity::WORDMARK,
        tagline: identity::TAGLINE,
        skip_hint: identity::SKIP_HINT,
        wordmark_tracking: identity::WORDMARK_TRACKING,
    }
}

impl Brandmark3d {
    pub fn new() -> Brandmark3d {
        Brandmark3d(BrandmarkRenderer::with_params(identity_params()))
    }
}

impl Default for Brandmark3d {
    fn default() -> Self {
        Brandmark3d::new()
    }
}

impl SplashFrameSource for Brandmark3d {
    fn render(&mut self, t: f32, size: Size, theme: &Theme) -> &Surface {
        self.0.render(t, size, theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::fallback2d::FallbackSplash;
    use crate::boot::identity;
    use crate::theme::default_theme;

    #[test]
    fn adapter_forwards_and_honors_size() {
        let mut src = Brandmark3d::new();
        let s = SplashFrameSource::render(&mut src, 1.0, Size::new(40, 12), default_theme());
        assert_eq!(s.size(), Size::new(40, 12));
    }

    /// Timeline drift guard: the 2D and 3D sources must express the SAME
    /// identity beats — skip hint after 0.3 s, wordmark only after the
    /// reveal phase. Both read `identity` constants by construction; this
    /// pins that neither grows a private timeline.
    #[test]
    fn both_sources_share_the_identity_beats() {
        let theme = default_theme();
        let size = Size::new(80, 24);
        let reveal_s = identity::PHASE_REVEAL_START_MS as f32 / 1000.0;

        let hint =
            |s: &crate::render::Surface| row_text(s, size.h - 1).contains(identity::SKIP_HINT);
        let wordmark_cells = |s: &crate::render::Surface| {
            (0..size.h).any(|y| {
                let row = row_text(s, y);
                let squeezed: String = row.chars().filter(|c| !c.is_whitespace()).collect();
                squeezed.contains("AbstractTUI") || squeezed.contains("Abstr")
            })
        };

        // Per-beat FRESH sources: the 3D trail is history-dependent by
        // design, and beat semantics must hold from a cold start.
        type SourceFactory = fn() -> Box<dyn SplashFrameSource>;
        fn make_2d() -> Box<dyn SplashFrameSource> {
            Box::new(FallbackSplash::new())
        }
        fn make_3d() -> Box<dyn SplashFrameSource> {
            Box::new(Brandmark3d::new())
        }
        let sources: [(&str, SourceFactory); 2] = [("2d", make_2d), ("3d", make_3d)];
        for (label, make) in sources {
            let mut early = make();
            let s = early.render(0.1, size, theme);
            assert!(!hint(s), "[{label}] hint must respect the 0.3s grace");
            assert!(
                !wordmark_cells(s),
                "[{label}] wordmark must wait for the reveal phase"
            );

            let mut pre_reveal = make();
            let s = pre_reveal.render(reveal_s - 0.1, size, theme);
            assert!(hint(s), "[{label}] hint visible before reveal");
            assert!(
                !wordmark_cells(s),
                "[{label}] no wordmark before {reveal_s}s"
            );

            let mut done = make();
            let s = done.render(1.95, size, theme);
            assert!(wordmark_cells(s), "[{label}] wordmark landed by 1.95s");
        }
    }

    fn row_text(s: &crate::render::Surface, y: i32) -> String {
        (0..s.size().w)
            .map(|x| {
                s.get(x, y)
                    .map(|c| c.glyph.as_str(s.pool()))
                    .filter(|t| !t.is_empty())
                    .and_then(|t| t.chars().next())
                    .unwrap_or(' ')
            })
            .collect()
    }
}
