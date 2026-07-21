//! AbstractTUI brand constants: the boot splash's single source of truth.
//!
//! Art direction lives in `docs/design/theme-identity.md` (section 2 —
//! geometry, storyboard, frame-by-frame timing). This file is the
//! machine-readable half: timings, easing parameters, color ramp and the
//! fallback wordmark. GFX3D implements the 3D mark against these
//! constants; the 2D fallback splash (DESIGN, cycle 4) uses the same ones,
//! so the two splashes can never drift apart.
//!
//! The mark: **three ascending planes forming an "A"** — the compositor
//! story (layers) drawn as the product's initial. Not a generic spinning
//! cube; the geometry *is* the architecture pitch.
//!
//! OWNER: DESIGN (GFX3D consumes).

use crate::base::Rgba;

// ---------------------------------------------------------------------------
// Timeline (milliseconds from splash start)
// ---------------------------------------------------------------------------

/// Total splash duration. Product requirement: ~2 seconds, skippable.
pub const SPLASH_TOTAL_MS: u32 = 2000;

/// Phase A — "arrival": planes fly in from below-right, staggered.
pub const PHASE_ARRIVAL_START_MS: u32 = 0;
/// Phase A ends / phase B ("alignment") begins: planes overshoot toward
/// the A-silhouette and settle back (ease-out-back).
pub const PHASE_ALIGN_START_MS: u32 = 900;
/// Phase C — "reveal": wordmark letters fade in while letter-spacing
/// collapses; accent underline sweeps left to right.
pub const PHASE_REVEAL_START_MS: u32 = 1400;
/// Splash holds the finished composition briefly, then hands off.
pub const PHASE_HOLD_START_MS: u32 = 1850;

/// Per-plane arrival stagger: plane i starts at `i * STAGGER`.
pub const PLANE_STAGGER_MS: u32 = 120;
/// Duration of one plane's arrival tween.
pub const PLANE_ARRIVAL_MS: u32 = 780;

/// Glow/particle burst moment (alignment impact).
pub const BURST_AT_MS: u32 = 900;
/// Number of spark particles in the burst (cell-sized, additive).
pub const BURST_PARTICLES: u32 = 12;
/// Particle lifetime.
pub const BURST_LIFETIME_MS: u32 = 450;
/// Sparks kicked up when one plane/mark-line lands (cycle 7, the
/// ParticleField afterglow pass — additive to the locked timeline: no
/// existing constant moved, so GFX3D's drift pin is untouched).
pub const LAND_SPARKS: u32 = 5;

/// Afterglow: layer opacity of the trail buffer decays by this factor per
/// 100 ms (compositor multiplies the previous trail frame).
pub const AFTERGLOW_DECAY_PER_100MS: f32 = 0.72;

/// Skip: any key or mouse press. The splash never blocks input.
pub const SKIP_FADE_MS: u32 = 120;
/// Environment variable that disables the splash entirely (also
/// auto-disabled when stdout is not a TTY).
pub const SPLASH_DISABLE_ENV: &str = "ABSTRACTTUI_NO_SPLASH";

// ---------------------------------------------------------------------------
// Easing (cubic-bezier control points, CSS convention: x1, y1, x2, y2)
// ---------------------------------------------------------------------------
// Expressed as bezier parameters rather than `anim` types so this file has
// zero dependency on the animation layer's API surface; `anim` can build
// its curves from these numbers (CONTRACT(RENDER): see
// reviews/cycle1/design-requests.md — named easing constructors).

/// Plane fly-in: fast start, long soft landing.
pub const EASE_ARRIVAL: [f32; 4] = [0.16, 1.0, 0.30, 1.0]; // ease-out-expo-like
/// Alignment settle: overshoot then return (back-out). The `y > 1` control
/// point is the overshoot.
pub const EASE_SETTLE: [f32; 4] = [0.34, 1.56, 0.64, 1.0];
/// Wordmark tracking collapse: symmetric, calm.
pub const EASE_TRACKING: [f32; 4] = [0.83, 0.0, 0.17, 1.0]; // ease-in-out-quint-like
/// All fades (in, out, skip).
pub const EASE_FADE: [f32; 4] = [0.33, 1.0, 0.68, 1.0]; // ease-out-cubic

// ---------------------------------------------------------------------------
// Camera (3D mark; GFX3D)
// ---------------------------------------------------------------------------

/// Camera yaw sweep across the splash (degrees): starts angled, ends
/// nearly frontal so the "A" silhouette locks.
pub const CAMERA_YAW_DEG: (f32, f32) = (-35.0, -6.0);
/// Camera pitch (degrees), constant slight down-look.
pub const CAMERA_PITCH_DEG: f32 = 8.0;
/// Dolly distance (arbitrary scene units, start -> end): a slow push-in.
pub const CAMERA_DOLLY: (f32, f32) = (5.2, 4.4);

// ---------------------------------------------------------------------------
// Brand colors
// ---------------------------------------------------------------------------
// The splash renders on the ACTIVE theme's bg/text so it never clashes with
// the user's palette; the mark itself carries the Abstract house accents
// (theme.css :root values — the brand is the brand on every theme).

/// House accent (theme.css `--accent`): the A's leading plane.
pub const BRAND_ACCENT: Rgba = Rgba::rgb(0xe9, 0x45, 0x60);
/// House secondary accent (theme.css `--info`): the trailing plane.
pub const BRAND_ACCENT_ALT: Rgba = Rgba::rgb(0x60, 0xa5, 0xfa);
/// Deep field tint behind the mark (theme.css `--bg-tertiary`), blended
/// over the active theme's bg for the vignette.
pub const BRAND_FIELD: Rgba = Rgba::rgb(0x0f, 0x34, 0x60);

/// Curated 5-stop ramp for the plane gradient and particle colors.
/// Deliberately hand-picked, NOT lerped at runtime: sRGB midpoints between
/// the house red and blue desaturate to gray (see `theme::derive` docs), so
/// the middle stops route through violet on purpose.
pub const BRAND_RAMP: [Rgba; 5] = [
    Rgba::rgb(0xe9, 0x45, 0x60), // house red
    Rgba::rgb(0xc9, 0x53, 0x8f), // rose-violet bridge
    Rgba::rgb(0x9d, 0x6b, 0xc9), // violet bridge
    Rgba::rgb(0x7a, 0x86, 0xe8), // periwinkle bridge
    Rgba::rgb(0x60, 0xa5, 0xfa), // house blue
];

/// Ramp color for a normalized position `t` in 0..=1: nearest curated
/// stop pair, mixed only within the pair (short hue distance keeps the
/// blend honest at cell scale). Routed through `theme::derive::mix` — the
/// one audited home for color arithmetic (RT1-9b).
pub fn brand_ramp(t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0) * (BRAND_RAMP.len() - 1) as f32;
    let i = (t.floor() as usize).min(BRAND_RAMP.len() - 2);
    crate::theme::derive::mix(BRAND_RAMP[i], BRAND_RAMP[i + 1], t - i as f32)
}

// ---------------------------------------------------------------------------
// Wordmark
// ---------------------------------------------------------------------------

/// The product wordmark, revealed at `PHASE_REVEAL_START_MS`.
pub const WORDMARK: &str = "AbstractTUI";
/// Letter-spacing animation in cells: starts airy, collapses to snug.
pub const WORDMARK_TRACKING: (u16, u16) = (4, 1);
/// Tagline under the wordmark (text_muted, no animation, fades with the
/// wordmark's tail).
pub const TAGLINE: &str = "the terminal, composed";
/// Skip affordance shown bottom-right from 300 ms on (text_faint).
pub const SKIP_HINT: &str = "press any key to skip";

/// Fallback 2D mark for terminals without graphics/3D budget: three
/// ascending strokes forming an "A" (pure ASCII — the fallback targets the
/// dumbest terminals, so no box-drawing or shade glyphs). Rendered with
/// `brand_ramp` per line over the theme ground, wordmark alongside.
pub const MARK_ASCII: [&str; 5] = [
    r"      /\      ",
    r"     /  \     ",
    r"    / /\ \    ",
    r"   / /--\ \   ",
    r"  /_/    \_\  ",
];

/// Widest fallback-art line (cells) — layout helper for centering.
pub const MARK_ASCII_WIDTH: u16 = 14;

#[cfg(test)]
mod tests {
    use super::*;

    // Deliberate constant-relationship pins: these assertions exist to fail
    // when someone edits one identity constant without the others.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn timeline_is_ordered_and_fits_the_budget() {
        assert!(PHASE_ARRIVAL_START_MS < PHASE_ALIGN_START_MS);
        assert!(PHASE_ALIGN_START_MS < PHASE_REVEAL_START_MS);
        assert!(PHASE_REVEAL_START_MS < PHASE_HOLD_START_MS);
        assert!(PHASE_HOLD_START_MS < SPLASH_TOTAL_MS);
        // The last plane still lands before alignment begins.
        assert!(2 * PLANE_STAGGER_MS + PLANE_ARRIVAL_MS <= PHASE_ALIGN_START_MS + 200);
    }

    #[test]
    fn ramp_endpoints_are_the_house_accents() {
        assert_eq!(brand_ramp(0.0), BRAND_ACCENT);
        assert_eq!(brand_ramp(1.0), BRAND_ACCENT_ALT);
        // Middle stops keep saturation: no channel triple collapses to gray
        // (max-min channel spread stays wide).
        for c in BRAND_RAMP {
            let spread = c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b);
            assert!(
                spread > 60,
                "ramp stop {} desaturated (spread {spread})",
                c.to_hex()
            );
        }
    }

    #[test]
    fn mark_ascii_is_rectangular_and_pure_ascii() {
        for line in MARK_ASCII {
            assert_eq!(line.len(), MARK_ASCII_WIDTH as usize);
            assert!(line.is_ascii(), "fallback mark must be plain ASCII");
        }
    }

    // The overshoot line is a deliberate constant pin (see the timeline
    // test above for the rationale).
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn easings_are_valid_bezier_params() {
        // x control points must stay in 0..=1 for a well-defined curve;
        // y may overshoot (that IS the settle's overshoot).
        for e in [EASE_ARRIVAL, EASE_SETTLE, EASE_TRACKING, EASE_FADE] {
            assert!((0.0..=1.0).contains(&e[0]) && (0.0..=1.0).contains(&e[2]));
        }
        assert!(EASE_SETTLE[1] > 1.0, "settle must overshoot");
    }
}
