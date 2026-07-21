//! Tween: interpolate a value over a duration through an easing curve.
//!
//! A `Tween` is pure data + pure sampling: it holds no clock and mutates
//! nothing. The caller (REACT's transition layer next cycle) samples it
//! with an elapsed time and decides when to stop requesting frames. Pure
//! sampling makes tweens trivially testable and lets one tween be sampled
//! from several places without shared state.

use std::time::Duration;

use crate::base::{Point, Rgba};

use super::Easing;

/// Linear interpolation between two values of a type. `t` is pre-eased and
/// may lie outside [0, 1] for overshooting curves; implementations must
/// extrapolate sanely (or clamp where the domain demands it, e.g. color
/// channels saturate).
pub trait Lerp: Copy {
    /// The value `t` of the way from `self` to `to`.
    fn lerp(self, to: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, to: Self, t: f32) -> Self {
        self + (to - self) * t
    }
}

impl Lerp for i32 {
    fn lerp(self, to: Self, t: f32) -> Self {
        (self as f32 + (to as f32 - self as f32) * t).round() as i32
    }
}

impl Lerp for Point {
    fn lerp(self, to: Self, t: f32) -> Self {
        Point::new(self.x.lerp(to.x, t), self.y.lerp(to.y, t))
    }
}

impl Lerp for Rgba {
    fn lerp(self, to: Self, t: f32) -> Self {
        // Rgba::lerp clamps t: color channels saturate rather than wrap on
        // overshooting easings.
        Rgba::lerp(self, to, t)
    }
}

/// A fixed A→B animation over a duration, sampled by elapsed time.
/// Stateless — the caller owns the clock (see the module docs example).
#[derive(Copy, Clone, Debug)]
pub struct Tween<T: Lerp> {
    /// Start value (at elapsed 0).
    pub from: T,
    /// End value (at `duration` and beyond).
    pub to: T,
    /// Animation length; zero = an instant jump.
    pub duration: Duration,
    /// The progress curve (default [`Easing::Linear`]).
    pub easing: Easing,
}

impl<T: Lerp> Tween<T> {
    /// A linear tween from `from` to `to` over `duration`.
    pub fn new(from: T, to: T, duration: Duration) -> Tween<T> {
        Tween {
            from,
            to,
            duration,
            easing: Easing::Linear,
        }
    }

    /// Replaces the easing curve.
    pub fn with_easing(mut self, easing: Easing) -> Tween<T> {
        self.easing = easing;
        self
    }

    /// Value at `elapsed` since the tween started. Zero-duration tweens are
    /// already at their target (an instant jump, not a division by zero).
    pub fn sample(&self, elapsed: Duration) -> T {
        if self.duration.is_zero() || elapsed >= self.duration {
            // Every easing evaluates to exactly 1.0 at t = 1, so the rest
            // value is `to` — returned directly to avoid float lerp residue
            // (a fade must end at the target, not within 1e-7 of it).
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from.lerp(self.to, self.easing.eval(t))
    }

    /// True once sampling can no longer change: the caller stops requesting
    /// frames for it.
    pub fn is_finished(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn f32_and_i32_lerp() {
        let t = Tween::new(0.0f32, 10.0, MS(100));
        assert_eq!(t.sample(MS(0)), 0.0);
        assert_eq!(t.sample(MS(50)), 5.0);
        assert_eq!(t.sample(MS(100)), 10.0);
        assert_eq!(t.sample(MS(999)), 10.0, "rests at target after the end");

        let t = Tween::new(0i32, 9, MS(90));
        assert_eq!(t.sample(MS(30)), 3);
    }

    #[test]
    fn point_and_color_lerp() {
        let t = Tween::new(Point::new(0, 0), Point::new(10, -10), MS(100));
        assert_eq!(t.sample(MS(50)), Point::new(5, -5));

        let c = Tween::new(Rgba::BLACK, Rgba::WHITE, MS(100));
        let mid = c.sample(MS(50));
        assert!(mid.r > 100 && mid.r < 155);
    }

    #[test]
    fn zero_duration_is_an_instant_jump() {
        let t = Tween::new(0.0f32, 5.0, MS(0));
        assert_eq!(t.sample(MS(0)), 5.0);
        assert!(t.is_finished(MS(0)));
    }

    #[test]
    fn easing_shapes_the_timeline() {
        let t = Tween::new(0.0f32, 1.0, MS(100)).with_easing(Easing::EaseIn);
        assert!(t.sample(MS(50)) < 0.5, "ease-in lags at the midpoint");
        assert_eq!(t.sample(MS(100)), 1.0);
    }

    #[test]
    fn overshooting_easing_extrapolates_scalars_and_saturates_color() {
        let settle = Easing::bezier(0.34, 1.56, 0.64, 1.0);
        let t = Tween::new(0.0f32, 100.0, MS(100)).with_easing(settle);
        let peak = (1..100).map(|ms| t.sample(MS(ms))).fold(0.0f32, f32::max);
        assert!(peak > 100.0, "f32 lerp must extrapolate past `to`: {peak}");
        assert_eq!(t.sample(MS(100)), 100.0, "rests exactly at target");

        // Color channels saturate instead of wrapping on overshoot.
        let c =
            Tween::new(Rgba::rgb(0, 0, 0), Rgba::rgb(250, 250, 250), MS(100)).with_easing(settle);
        for ms in 1..100 {
            let v = c.sample(MS(ms));
            assert!(v.r >= v.g.min(v.b), "channels move together");
        }
        assert_eq!(c.sample(MS(100)), Rgba::rgb(250, 250, 250));
    }

    #[test]
    fn finished_boundary() {
        let t = Tween::new(0.0f32, 1.0, MS(100));
        assert!(!t.is_finished(MS(99)));
        assert!(t.is_finished(MS(100)));
    }
}
