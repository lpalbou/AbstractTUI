//! Color derivation helpers for theme construction.
//!
//! The AbstractUIC `theme.css` source defines backgrounds, text tiers,
//! accents and semantic states as concrete hex values — those are ported
//! verbatim (never re-invented). Tokens the CSS expresses as *alpha washes*
//! (borders at `rgba(white, 0.12)`, selection tints, focus rings) cannot be
//! copied as-is: an alpha wash only has a color once composited over a
//! ground. These helpers perform that compositing deterministically at
//! theme-build time, producing opaque tokens that a contrast audit can
//! reason about.
//!
//! ## Perceptual honesty (documented limits)
//!
//! All mixing here is a straight sRGB-space lerp (`Rgba::lerp`). sRGB lerp
//! is not perceptually uniform: midpoints between two saturated, hue-distant
//! colors desaturate toward gray (e.g. `#e94560` -> `#60a5fa` passes through
//! a muddy violet-gray). That is acceptable for what this module does —
//! small nudges (t <= ~0.45) between a ground and an ink of the *same
//! theme*, where hue distance is modest and drift is invisible at cell
//! scale. Long decorative gradients must NOT be built by lerping endpoints;
//! they use curated intermediate stops (see `boot::identity::brand_ramp`).
//!
//! OWNER: DESIGN.

use crate::base::Rgba;
use crate::theme::contrast::contrast_ratio;

/// Mix `a` toward `b` by `t` (0.0 = `a`, 1.0 = `b`), sRGB-space.
///
/// Thin alias over [`Rgba::lerp`] so theme code reads as intent
/// (`mix(bg, text, 0.16)`) rather than mechanism.
pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    a.lerp(b, t)
}

/// Lighten `c` by mixing toward white. `t` is the mix amount, not a
/// luminance delta — see the module notes on perceptual limits.
pub fn lighten(c: Rgba, t: f32) -> Rgba {
    c.lerp(Rgba::WHITE, t)
}

/// Darken `c` by mixing toward black.
pub fn darken(c: Rgba, t: f32) -> Rgba {
    c.lerp(Rgba::BLACK, t)
}

/// Smallest-step upward walk: mix `base` toward `ink`, starting at `t0` and
/// increasing by `step`, until the result reaches `floor` contrast against
/// `anchor` (or t hits 1.0, returning `ink` — the loudest possible answer).
///
/// Used for tokens whose CSS source is an alpha wash with a contrast floor
/// to honor, e.g. borders: `theme.css` draws borders at 12% ink, but a 12%
/// wash over a very dark ground lands under the 1.5:1 border floor. The
/// walk keeps the smallest mix that satisfies the floor, so faithful themes
/// stay close to the source alpha and only the grounds that need more get
/// more.
pub fn mix_until_contrast(
    base: Rgba,
    ink: Rgba,
    anchor: Rgba,
    t0: f32,
    step: f32,
    floor: f32,
) -> Rgba {
    debug_assert!(step > 0.0, "mix_until_contrast requires a positive step");
    let mut t = t0.clamp(0.0, 1.0);
    loop {
        let candidate = base.lerp(ink, t);
        if contrast_ratio(candidate, anchor) >= floor || t >= 1.0 {
            return candidate;
        }
        t = (t + step).min(1.0);
    }
}

/// Downward walk for *readable tints*: mix `base` toward `tint`, starting at
/// `t0` and decreasing by `step` (never below `t_min`), until `fg` reaches
/// `floor` contrast on the result.
///
/// Used for selection backgrounds: the strongest accent tint that still
/// keeps the selected text readable. Walking down (toward the ground) always
/// converges because `fg` already clears the floor against the ground
/// itself (test-pinned per theme).
pub fn tint_until_readable(
    base: Rgba,
    tint: Rgba,
    fg: Rgba,
    t0: f32,
    step: f32,
    t_min: f32,
    floor: f32,
) -> Rgba {
    debug_assert!(step > 0.0, "tint_until_readable requires a positive step");
    let mut t = t0.clamp(0.0, 1.0);
    loop {
        let candidate = base.lerp(tint, t);
        if contrast_ratio(fg, candidate) >= floor || t <= t_min {
            return candidate;
        }
        t = (t - step).max(t_min);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_endpoints_are_exact() {
        let a = Rgba::rgb(0x1a, 0x1a, 0x2e);
        let b = Rgba::rgb(0xee, 0xee, 0xee);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
    }

    #[test]
    fn lighten_darken_move_luminance_monotonically() {
        let c = Rgba::rgb(0x88, 0x40, 0xa0);
        assert!(lighten(c, 0.3).luminance() > c.luminance());
        assert!(darken(c, 0.3).luminance() < c.luminance());
    }

    #[test]
    fn upward_walk_reaches_floor_from_a_dark_ground() {
        // The motivating case: 12% ink over the abstract-dark ground is
        // ~1.38:1; the walk must land at or above the 1.5:1 border floor.
        let bg = Rgba::rgb(0x1a, 0x1a, 0x2e);
        let text = Rgba::rgb(0xee, 0xee, 0xee);
        let border = mix_until_contrast(bg, text, bg, 0.12, 0.02, 1.5);
        assert!(contrast_ratio(border, bg) >= 1.5);
        // ...and stays a subtle stroke, nowhere near full ink.
        assert!(contrast_ratio(border, bg) < 3.0);
    }

    #[test]
    fn upward_walk_returns_ink_when_floor_is_unreachable() {
        // Gray ink over a gray ground can never reach 21:1; the walk must
        // terminate at t=1.0 (the ink itself) instead of spinning.
        let g = Rgba::rgb(0x80, 0x80, 0x80);
        let ink = Rgba::rgb(0x90, 0x90, 0x90);
        assert_eq!(mix_until_contrast(g, ink, g, 0.1, 0.05, 21.0), ink);
    }

    #[test]
    fn downward_walk_keeps_fg_readable() {
        // A light ground with a mid-luminance accent: the strong tint fails
        // 4.5:1 for the dark text, so the walk retreats toward the ground.
        let bg = Rgba::rgb(0xef, 0xf1, 0xf5);
        let accent = Rgba::rgb(0x88, 0x39, 0xef);
        let fg = Rgba::rgb(0x4c, 0x4f, 0x69);
        let sel = tint_until_readable(bg, accent, fg, 0.60, 0.02, 0.04, 4.5);
        assert!(contrast_ratio(fg, sel) >= 4.5);
        // The tint must remain visible against the plain ground.
        assert!(contrast_ratio(sel, bg) > 1.05);
    }
}
