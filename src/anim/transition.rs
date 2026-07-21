//! Transition: animate a value toward a retargetable goal.
//!
//! The signal-binding primitive (REACT wires it next cycle): a widget
//! writes targets whenever it likes; the transition eases from wherever
//! the value CURRENTLY is — retargeting mid-flight never jumps. Engine-
//! only by design: no reactive imports, no clock ownership. The caller
//! ticks it with `Clock` time and keeps requesting frames while
//! [`Transition::is_settled`] is false (charter billing: an animation
//! asks for exactly the frames it needs).

use std::time::Duration;

use super::{Easing, Lerp};

/// A value easing toward a retargetable goal (see the module docs).
#[derive(Clone, Debug)]
pub struct Transition<T: Lerp> {
    /// Value as of the last `tick` (or construction/retarget time).
    current: T,
    /// Segment start value (what we eased away from).
    from: T,
    target: T,
    duration: Duration,
    easing: Easing,
    /// Clock timestamp when the active segment started; `None` = settled.
    started: Option<Duration>,
}

impl<T: Lerp> Transition<T> {
    /// A settled transition holding `initial`.
    pub fn new(initial: T, duration: Duration, easing: Easing) -> Transition<T> {
        Transition {
            current: initial,
            from: initial,
            target: initial,
            duration,
            easing,
            started: None,
        }
    }

    /// The value as of the last [`Transition::tick`] (or retarget).
    pub fn value(&self) -> T {
        self.current
    }

    /// The goal currently being eased toward.
    pub fn target(&self) -> T {
        self.target
    }

    /// True when at the target — stop requesting frames.
    pub fn is_settled(&self) -> bool {
        self.started.is_none()
    }

    /// Changes the goal, easing from the CURRENT value (evaluated at
    /// `now`, so a mid-flight retarget continues from the live position,
    /// not the stale last-tick sample). Setting the goal it already has
    /// is a no-op — redundant signal writes must not restart the ease
    /// (an ease restarted from rest has a visible velocity hiccup).
    pub fn set_target(&mut self, target: T, now: Duration) -> &mut Self
    where
        T: PartialEq,
    {
        if target == self.target {
            return self;
        }
        // Sample the in-flight value first; it becomes the new origin.
        self.tick(now);
        self.from = self.current;
        self.target = target;
        self.started = Some(now);
        self
    }

    /// Jumps to `value` instantly (no animation) — initialization and
    /// "reduce motion" paths.
    pub fn snap_to(&mut self, value: T) {
        self.current = value;
        self.from = value;
        self.target = value;
        self.started = None;
    }

    /// Advances to clock time `now` and returns the current value.
    /// Settles exactly on the target when the duration elapses (float
    /// residue never leaves a fade at 254/255).
    pub fn tick(&mut self, now: Duration) -> T {
        let Some(started) = self.started else {
            return self.current;
        };
        // A clock running backwards (or a retarget stamped later than the
        // caller's frame time) clamps to the segment start.
        let elapsed = now.saturating_sub(started);
        if self.duration.is_zero() || elapsed >= self.duration {
            self.current = self.target;
            self.started = None;
            return self.current;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.current = self.from.lerp(self.target, self.easing.eval(t));
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn eases_toward_target_and_settles_exactly() {
        let mut tr = Transition::new(0.0f32, MS(100), Easing::Linear);
        assert!(tr.is_settled());
        tr.set_target(10.0, MS(0));
        assert!(!tr.is_settled());
        assert_eq!(tr.tick(MS(50)), 5.0);
        assert_eq!(tr.tick(MS(100)), 10.0);
        assert!(tr.is_settled());
        assert_eq!(tr.tick(MS(500)), 10.0, "settled value is stable");
    }

    #[test]
    fn retarget_mid_flight_continues_from_live_value() {
        let mut tr = Transition::new(0.0f32, MS(100), Easing::Linear);
        tr.set_target(10.0, MS(0));
        tr.tick(MS(40)); // at 4.0
                         // Retarget at t=50 WITHOUT ticking first: set_target must sample
                         // the live value (5.0), not the stale 4.0.
        tr.set_target(0.0, MS(50));
        assert_eq!(tr.value(), 5.0, "origin resampled at retarget time");
        // New segment: 5.0 -> 0.0 over 100ms starting at 50.
        assert_eq!(tr.tick(MS(100)), 2.5);
        assert_eq!(tr.tick(MS(150)), 0.0);
        assert!(tr.is_settled());
    }

    #[test]
    fn redundant_retarget_is_a_noop() {
        let mut tr = Transition::new(0.0f32, MS(100), Easing::Linear);
        tr.set_target(10.0, MS(0));
        tr.tick(MS(50));
        // Same goal again mid-flight: the ease must NOT restart.
        tr.set_target(10.0, MS(50));
        assert_eq!(tr.tick(MS(100)), 10.0, "finished on the original schedule");
    }

    #[test]
    fn snap_and_zero_duration_are_instant() {
        let mut tr = Transition::new(0.0f32, MS(0), Easing::EaseInOut);
        tr.set_target(7.0, MS(3));
        assert_eq!(tr.tick(MS(3)), 7.0, "zero duration lands immediately");
        tr.snap_to(1.0);
        assert!(tr.is_settled());
        assert_eq!(tr.value(), 1.0);
    }

    #[test]
    fn backwards_clock_clamps_to_segment_start() {
        let mut tr = Transition::new(0.0f32, MS(100), Easing::Linear);
        tr.set_target(10.0, MS(50));
        assert_eq!(
            tr.tick(MS(10)),
            0.0,
            "before the segment start: origin value"
        );
        assert!(!tr.is_settled());
    }

    #[test]
    fn works_for_points_and_colors() {
        use crate::base::{Point, Rgba};
        let mut p = Transition::new(Point::ZERO, MS(100), Easing::Linear);
        p.set_target(Point::new(10, -4), MS(0));
        assert_eq!(p.tick(MS(50)), Point::new(5, -2));

        let mut c = Transition::new(Rgba::BLACK, MS(100), Easing::Linear);
        c.set_target(Rgba::WHITE, MS(0));
        let mid = c.tick(MS(50));
        assert!(mid.r > 100 && mid.r < 155);
        assert_eq!(c.tick(MS(100)), Rgba::WHITE);
    }
}
