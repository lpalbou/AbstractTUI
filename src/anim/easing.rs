//! Easing curves. All curves map progress `t ∈ [0, 1]` to eased progress;
//! inputs outside the range clamp (animations sampled after their end must
//! rest exactly at the target).

/// The cubic presets are the CSS named curves expressed directly as
/// polynomials (cheaper and exact); `CubicBezier` matches CSS
/// `cubic-bezier(x1, y1, x2, y2)` semantics for designer-tuned curves.
///
/// Physical-feel curves (`Bounce`, `Elastic`, `Spring`) are built from
/// polynomials and a polynomial sine (Bhaskara I approximation, max
/// error ~0.0016 of amplitude — documented, deterministic, no libm), so
/// identical inputs yield identical bits on every platform. They are
/// FEEL approximations, not physics: a designer picks them by look.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Easing {
    /// Identity: progress = t.
    Linear,
    /// t³ — starts slow.
    EaseIn,
    /// 1-(1-t)³ — ends slow.
    EaseOut,
    /// Piecewise cubic — slow at both ends.
    EaseInOut,
    /// Control points (x1, y1, x2, y2); x components are clamped to
    /// [0, 1] at evaluation (CSS rule: x outside the unit range would make
    /// the curve non-invertible as a function of time).
    CubicBezier(f32, f32, f32, f32),
    /// Penner ease-out bounce: three diminishing parabolic bounces after
    /// the fall. Exact polynomials; ends exactly at 1.
    Bounce,
    /// Ease-out elastic: overshoots and rings `period`-fast before
    /// settling. `period` ≈ fraction of the duration per oscillation
    /// (CSS-elastic-like; 0.3 = snappy, 0.5 = loose). Output exceeds 1
    /// mid-flight by design.
    Elastic(f32),
    /// Underdamped spring settle: one parameter, `bounciness` 0..=1 —
    /// 0 ≈ critically damped (no overshoot), 1 = springy (~3 visible
    /// oscillations). The toast/overlay arrival feel.
    Spring(f32),
}

impl Easing {
    /// Named constructor for designer-tuned curves (DESIGN request 4):
    /// `const` so identity curves can live as constants
    /// (`boot::identity::EASE_SETTLE` etc.). Only the X control points are
    /// time-clamped at evaluation; Y may leave [0, 1] — overshoot/settle
    /// curves depend on it, and `eval` never clamps its OUTPUT for
    /// intermediate `t`.
    pub const fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Easing {
        Easing::CubicBezier(x1, y1, x2, y2)
    }

    /// Eased progress for `t` in 0..=1 (input clamps; OUTPUT may leave
    /// 0..=1 for overshooting curves — consumers must not re-clamp).
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => {
                let x1 = x1.clamp(0.0, 1.0);
                let x2 = x2.clamp(0.0, 1.0);
                // Endpoint exactness: solving is unnecessary and float
                // error there is visible (a fade that ends at 0.9997).
                if t <= 0.0 {
                    return 0.0;
                }
                if t >= 1.0 {
                    return 1.0;
                }
                let u = solve_bezier_x(x1, x2, t);
                bezier(y1, y2, u)
            }
            Easing::Bounce => bounce_out(t),
            Easing::Elastic(period) => {
                if t <= 0.0 {
                    return 0.0;
                }
                if t >= 1.0 {
                    return 1.0;
                }
                let p = period.clamp(0.05, 2.0);
                // Ring: decaying oscillation around 1. Decay is the exact
                // polynomial (1-t)^4 (visually close to exp for one unit
                // interval); phase via the polynomial sine.
                let decay = (1.0 - t).powi(4);
                let cycles = 1.0 / p; // oscillations across the interval
                1.0 + decay * poly_sin_turns(t * cycles + 0.75) * 1.0
            }
            Easing::Spring(bounciness) => {
                if t <= 0.0 {
                    return 0.0;
                }
                if t >= 1.0 {
                    return 1.0;
                }
                let b = bounciness.clamp(0.0, 1.0);
                // Damped approach: value = 1 - u³·env(t) with the
                // envelope a raised cosine, env(0) = 1 (so eval(0) = 0
                // exactly). k sets how far env dips negative: at b = 0,
                // env ∈ [0, 1] — overshoot IMPOSSIBLE by construction; at
                // b = 1, env dips to -0.5 and the peak overshoot is
                // 0.5·u³ ≈ 21% at the first trough. cycles = 1 + b gives
                // one settle wave at 0 and ~2 visible oscillations at 1.
                let u = 1.0 - t;
                let decay = u * u * u;
                let k = 0.5 + 0.25 * b;
                let cycles = 1.0 + b;
                let env = (1.0 - k) + k * poly_cos_turns(t * cycles);
                1.0 - decay * env
            }
        }
    }
}

/// cos(2π·x) via the polynomial sine (quarter-turn shift).
fn poly_cos_turns(x: f32) -> f32 {
    poly_sin_turns(x + 0.25)
}

/// sin(2π·x) via Bhaskara I's half-turn approximation, mirrored for the
/// negative half: pure polynomial, |error| ≤ ~0.0017. Bit-stable across
/// platforms (no libm), which is why the physical-feel curves use it.
/// Crate-visible: `anim::particles` reuses it for direction vectors.
pub(super) fn poly_sin_turns(x: f32) -> f32 {
    let frac = x - x.floor(); // one turn
    let (half, sign) = if frac < 0.5 {
        (frac * 2.0, 1.0)
    } else {
        ((frac - 0.5) * 2.0, -1.0)
    };
    // Bhaskara over the half turn h ∈ [0,1]: sin(πh) ≈ 16h(1-h)/(5-4h(1-h)).
    let hh = half * (1.0 - half);
    sign * (16.0 * hh) / (5.0 - 4.0 * hh)
}

/// Penner ease-out bounce (exact piecewise parabolas).
fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984_375
    }
}

/// One-dimensional cubic Bezier with implicit endpoints 0 and 1:
/// `B(u) = 3(1-u)²u·p1 + 3(1-u)u²·p2 + u³`.
fn bezier(p1: f32, p2: f32, u: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u
}

fn bezier_dx(p1: f32, p2: f32, u: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * p1 + 6.0 * v * u * (p2 - p1) + 3.0 * u * u * (1.0 - p2)
}

/// Inverts `x(u) = t`: Newton–Raphson from the WebKit playbook, falling
/// back to bisection when the derivative degenerates (control points can
/// flatten the curve and stall Newton).
fn solve_bezier_x(x1: f32, x2: f32, t: f32) -> f32 {
    let mut u = t; // good initial guess: x(u) ≈ u for gentle curves
    for _ in 0..8 {
        let err = bezier(x1, x2, u) - t;
        if err.abs() < 1e-6 {
            return u;
        }
        let d = bezier_dx(x1, x2, u);
        if d.abs() < 1e-6 {
            break;
        }
        u = (u - err / d).clamp(0.0, 1.0);
    }
    // Bisection: x(u) is monotone in u for clamped control points.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        u = (lo + hi) / 2.0;
        if bezier(x1, x2, u) < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }

    #[test]
    fn endpoints_are_exact_for_every_curve() {
        let curves = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
            Easing::CubicBezier(0.0, 1.5, 1.0, -0.5), // y overshoot allowed
            Easing::Bounce,
            Easing::Elastic(0.3),
            Easing::Spring(0.0),
            Easing::Spring(1.0),
        ];
        for c in curves {
            assert_eq!(c.eval(0.0), 0.0, "{c:?} at 0");
            assert_eq!(c.eval(1.0), 1.0, "{c:?} at 1");
            assert_eq!(c.eval(-1.0), 0.0, "{c:?} clamps below");
            assert_eq!(c.eval(2.0), 1.0, "{c:?} clamps above");
        }
    }

    #[test]
    fn poly_sine_stays_within_error_bound() {
        // Landmarks of the documented approximation.
        assert!((poly_sin_turns(0.25) - 1.0).abs() < 2e-3, "peak");
        assert!(poly_sin_turns(0.0).abs() < 1e-6, "zero at 0");
        assert!(poly_sin_turns(0.5).abs() < 1e-6, "zero at half turn");
        assert!((poly_sin_turns(0.75) + 1.0).abs() < 2e-3, "trough");
        assert_eq!(poly_sin_turns(1.25), poly_sin_turns(0.25), "periodic");
    }

    #[test]
    fn bounce_shape() {
        // Monotone segments with the classic touch points at 1.
        for touch in [1.0 / 2.75, 2.0 / 2.75, 2.5 / 2.75] {
            assert!(
                (Easing::Bounce.eval(touch) - 1.0).abs() < 1e-3,
                "touch at {touch}"
            );
        }
        // Dips between touches (it bounces).
        let dip = Easing::Bounce.eval(1.35 / 2.75);
        assert!(dip < 0.95, "bounce dips: {dip}");
        // Never leaves [0, 1]: bounce is an undershoot family.
        for i in 0..=100 {
            let v = Easing::Bounce.eval(i as f32 / 100.0);
            assert!((0.0..=1.0 + 1e-6).contains(&v), "{v}");
        }
    }

    #[test]
    fn elastic_rings_and_spring_bounciness_scales_overshoot() {
        let e = Easing::Elastic(0.3);
        let peak = (1..100)
            .map(|i| e.eval(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        let trough = (50..100)
            .map(|i| e.eval(i as f32 / 100.0))
            .fold(2.0f32, f32::min);
        assert!(peak > 1.02, "elastic overshoots: {peak}");
        assert!(trough < 1.0, "and rings back under: {trough}");

        let calm = Easing::Spring(0.0);
        for i in 0..=100 {
            let v = calm.eval(i as f32 / 100.0);
            assert!(v <= 1.0 + 1e-6, "bounciness 0 never overshoots: {v}");
        }
        let springy = Easing::Spring(1.0);
        let peak1 = (1..100)
            .map(|i| springy.eval(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(peak1 > 1.05, "bounciness 1 visibly overshoots: {peak1}");
        let mild = Easing::Spring(0.4);
        let peak_mild = (1..100)
            .map(|i| mild.eval(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(
            peak_mild > 1.0 && peak_mild < peak1,
            "overshoot scales: {peak_mild} < {peak1}"
        );
    }

    #[test]
    fn cubic_shapes() {
        assert_close(Easing::EaseIn.eval(0.5), 0.125);
        assert_close(Easing::EaseOut.eval(0.5), 0.875);
        assert_close(Easing::EaseInOut.eval(0.5), 0.5);
        assert!(Easing::EaseInOut.eval(0.25) < 0.25);
        assert!(Easing::EaseInOut.eval(0.75) > 0.75);
    }

    #[test]
    fn bezier_reproduces_css_ease() {
        // cubic-bezier(0.25, 0.1, 0.25, 1.0) is CSS `ease`; reference
        // values from evaluating the curve definition directly.
        let ease = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
        assert_close(ease.eval(0.25), 0.4085);
        assert_close(ease.eval(0.5), 0.8024);
        assert_close(ease.eval(0.75), 0.9606);
    }

    #[test]
    fn linear_bezier_matches_linear() {
        let b = Easing::CubicBezier(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert_close(b.eval(t), t);
        }
    }

    #[test]
    fn overshoot_settle_curve_exceeds_one_mid_flight() {
        // DESIGN's settle class: y control points outside [0,1] must
        // produce intermediate outputs beyond 1.0 (the evaluator clamps
        // TIME, never OUTPUT), and still land exactly at 1.
        let settle = Easing::bezier(0.34, 1.56, 0.64, 1.0);
        let peak = (1..40)
            .map(|i| settle.eval(i as f32 / 40.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.05, "settle curve must overshoot: peak {peak}");
        assert_eq!(settle.eval(1.0), 1.0);

        // Anticipation curves likewise dip below 0 on the way in.
        let anticipate = Easing::bezier(0.36, -0.32, 0.66, 1.0);
        let dip = (1..40)
            .map(|i| anticipate.eval(i as f32 / 40.0))
            .fold(1.0f32, f32::min);
        assert!(dip < -0.02, "anticipation must dip: {dip}");
    }

    #[test]
    fn degenerate_control_points_still_converge() {
        // Extreme flat-then-cliff curve: Newton stalls, bisection finishes.
        let b = Easing::CubicBezier(1.0, 0.0, 1.0, 0.0);
        for i in 1..10 {
            let t = i as f32 / 10.0;
            let y = b.eval(t);
            assert!((0.0..=1.0).contains(&y), "t={t} y={y}");
        }
        // Monotone in t (x-solve must not fold back).
        let mut prev = 0.0;
        for i in 0..=20 {
            let y = b.eval(i as f32 / 20.0);
            assert!(y >= prev - 1e-4, "non-monotone at {i}: {y} < {prev}");
            prev = y;
        }
    }
}
