//! Paint helpers over [`Surface`]: gradients and shadow layers — ground
//! decoration that isn't per-widget logic. All fills here are ONE-TIME
//! paints: they damage the touched rect once and cost nothing per frame
//! afterwards (the per-frame budgets never see them).

use crate::base::{Point, Rect, Rgba};

use super::cell::Cell;
use super::layer::Layer;
use super::surface::Surface;

/// Terminal cells are ~twice as tall as wide; visual-angle math uses this
/// constant so a 45° gradient LOOKS 45° (documented approximation — real
/// fonts vary 1.8–2.2).
const CELL_ASPECT: f32 = 2.0;

/// Fill direction for [`fill_gradient_axis`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left → right.
    Horizontal,
    /// Top → bottom.
    Vertical,
}

/// A multi-stop gradient. Stops are `(position 0..=1, color)`; positions
/// are sorted at fill time (builders may list them freely), the ends
/// clamp, and colors between stops use the engine's one sRGB lerp so
/// gradients and tweens meeting at a seam agree.
#[derive(Clone, Debug)]
pub struct GradientSpec {
    /// `(position 0..=1, color)` stops; order-free, sorted at fill time.
    pub stops: Vec<(f32, Rgba)>,
    /// Linear (angled) or radial shape.
    pub kind: GradientKind,
    /// Ordered 4x4 Bayer dither on the sub-step remainder: breaks the
    /// visible bands a big rect with a small color delta produces at cell
    /// resolution. ON by default; turn off for exact-color assertions.
    pub dither: bool,
}

/// Gradient shape (see the variant fields for coordinate conventions).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GradientKind {
    /// Straight ramp along an angle.
    Linear {
        /// Visual angle in degrees: 0° = left→right, 90° = top→bottom
        /// (screen-y grows downward), after cell-aspect correction.
        angle_deg: f32,
    },
    /// Circular ramp out from a center.
    Radial {
        /// Center in unit rect coordinates ((0.5, 0.5) = middle); t
        /// reaches 1.0 at the farthest corner.
        center: (f32, f32),
    },
}

impl GradientSpec {
    /// A dithered linear gradient at `angle_deg` (0° = left→right).
    pub fn linear(angle_deg: f32, stops: Vec<(f32, Rgba)>) -> GradientSpec {
        GradientSpec {
            stops,
            kind: GradientKind::Linear { angle_deg },
            dither: true,
        }
    }

    /// A dithered radial gradient from `center` (unit rect coordinates).
    pub fn radial(center: (f32, f32), stops: Vec<(f32, Rgba)>) -> GradientSpec {
        GradientSpec {
            stops,
            kind: GradientKind::Radial { center },
            dither: true,
        }
    }

    /// The common two-color case.
    pub fn two(from: Rgba, to: Rgba, kind: GradientKind) -> GradientSpec {
        GradientSpec {
            stops: vec![(0.0, from), (1.0, to)],
            kind,
            dither: true,
        }
    }

    /// Disables dithering (exact per-cell colors, banding accepted).
    pub fn without_dither(mut self) -> GradientSpec {
        self.dither = false;
        self
    }
}

/// Bracketing stops for `t`: (low color, high color, fraction between).
/// Before the first stop / past the last, both sides are that stop.
fn bracket(sorted: &[(f32, Rgba)], t: f32) -> (Rgba, Rgba, f32) {
    let t = t.clamp(0.0, 1.0);
    let (Some(first), Some(last)) = (sorted.first(), sorted.last()) else {
        return (Rgba::TRANSPARENT, Rgba::TRANSPARENT, 0.0);
    };
    if t <= first.0 {
        return (first.1, first.1, 0.0);
    }
    if t >= last.0 {
        return (last.1, last.1, 0.0);
    }
    for w in sorted.windows(2) {
        if t <= w[1].0 {
            let span = (w[1].0 - w[0].0).max(1e-6);
            return (w[0].1, w[1].1, ((t - w[0].0) / span).clamp(0.0, 1.0));
        }
    }
    (last.1, last.1, 0.0)
}

/// 4x4 Bayer matrix, thresholds in 0..16 (classic ordered dither).
const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Fills `rect` (clipped) with `spec` as BACKGROUND color. Glyphs, attrs
/// and links in the rect are preserved (gradients are ground, text is
/// content; both draw orders compose). Wide pairs keep one bg (leader's,
/// mirrored). One-time paint: damages the rect once.
pub fn fill_gradient(s: &mut Surface, rect: Rect, spec: &GradientSpec) {
    let r = rect.intersect(s.bounds());
    if r.is_empty() || spec.stops.is_empty() {
        return;
    }
    let mut sorted = spec.stops.clone();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Projection frame: cell centers, aspect-corrected so visual angles
    // are honest. Linear gradients normalize the direction-projection
    // over the rect's corner extremes (correct for every angle); radial
    // reaches t=1 at the farthest corner from the center.
    let (w, h) = (r.w as f32, r.h as f32 * CELL_ASPECT);
    let proj = |x: i32, y: i32| -> f32 {
        let px = (x - r.x) as f32 + 0.5;
        let py = ((y - r.y) as f32 + 0.5) * CELL_ASPECT;
        match spec.kind {
            GradientKind::Linear { angle_deg } => {
                let a = angle_deg.to_radians();
                let (dx, dy) = (a.cos(), a.sin());
                let corners = [0.0, w * dx, h * dy, w * dx + h * dy];
                let lo = corners.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = corners.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                (px * dx + py * dy - lo) / (hi - lo).max(1e-6)
            }
            GradientKind::Radial { center } => {
                let cx = center.0.clamp(0.0, 1.0) * w;
                let cy = center.1.clamp(0.0, 1.0) * h;
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let fx = cx.max(w - cx);
                let fy = cy.max(h - cy);
                let far = (fx * fx + fy * fy).sqrt().max(1e-6);
                d / far
            }
        }
    };

    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let cell = *s.get(x, y).expect("clipped");
            if cell.is_continuation() {
                continue; // pair invariant mirrors the leader's bg
            }
            let t = proj(x, y);
            let (lo, hi, frac) = bracket(&sorted, t);
            let frac = if spec.dither {
                // Nudge the interpolation fraction by a sub-step ordered
                // threshold: bands dissolve into a stable 4x4 pattern.
                // The nudge is at most one lerp step wide.
                let cellsteps = (lo_hi_maxstep(lo, hi)).max(1) as f32;
                let bias = (BAYER4[(y & 3) as usize][(x & 3) as usize] as f32 + 0.5) / 16.0 - 0.5;
                (frac + bias / cellsteps).clamp(0.0, 1.0)
            } else {
                frac
            };
            s.set(x, y, cell.with_bg(lo.lerp(hi, frac)));
        }
    }
}

/// Largest channel distance between two stops — the number of discrete
/// lerp steps available; the dither bias is scaled to one such step.
fn lo_hi_maxstep(a: Rgba, b: Rgba) -> i32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs();
    d(a.r, b.r)
        .max(d(a.g, b.g))
        .max(d(a.b, b.b))
        .max(d(a.a, b.a))
}

/// Two-color axis convenience (the cycle-6 morning API, kept).
pub fn fill_gradient_axis(s: &mut Surface, rect: Rect, from: Rgba, to: Rgba, axis: Axis) {
    let angle = match axis {
        Axis::Horizontal => 0.0,
        Axis::Vertical => 90.0,
    };
    fill_gradient(
        s,
        rect,
        &GradientSpec::two(from, to, GradientKind::Linear { angle_deg: angle }).without_dither(),
    );
}

/// Builds a drop-shadow LAYER for a panel: place it in the compositor
/// BELOW the panel's layer (lower z). The shadow is a translucent ramp of
/// `color` (theme shadow token; alpha is the peak opacity) whose alpha
/// falls off over `feather` cells of Chebyshev distance outside the
/// panel's footprint — the compositor's Normal blend does the rest, and
/// the theme ground (if set) keeps it honest over default-bg cells.
///
/// The layer pattern (docs/design/render.md §2.2e): shadow layer at
/// `panel.origin + offset` and z-1, panel above; move/fade BOTH via their
/// handles — the shadow is content, not a post-effect, so it costs
/// nothing per frame once composed.
pub fn drop_shadow(panel: Rect, offset: Point, feather: i32, color: Rgba, z: i32) -> Layer {
    let feather = feather.max(0);
    let size = crate::base::Size::new(panel.w + 2 * feather, panel.h + 2 * feather);
    let mut surface = Surface::new(size, Cell::EMPTY);
    let peak = color.a as f32;
    for y in 0..size.h {
        for x in 0..size.w {
            // Chebyshev distance to the panel's footprint (which sits at
            // feather..feather+panel dims inside this surface).
            let dx = (feather - x).max(x - (feather + panel.w - 1)).max(0);
            let dy = (feather - y).max(y - (feather + panel.h - 1)).max(0);
            let d = dx.max(dy);
            if d > feather {
                continue;
            }
            let k = if feather == 0 {
                1.0
            } else {
                1.0 - d as f32 / (feather + 1) as f32
            };
            let a = (peak * k).round() as u8;
            if a == 0 {
                continue;
            }
            surface.set(x, y, Cell::EMPTY.with_bg(color.with_alpha(a)));
        }
    }
    Layer::new(
        surface,
        Point::new(panel.x - feather + offset.x, panel.y - feather + offset.y),
        z,
    )
}

#[cfg(test)]
#[path = "paint_tests.rs"]
mod tests;
