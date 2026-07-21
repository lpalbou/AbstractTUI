//! Animation engine skeleton: clock, easing, tweens, frame requests.
//!
//! The engine is event-driven — idle apps burn zero CPU — so animations
//! never poll. An active animation asks the scheduler for one more frame
//! through [`FrameRequester`]; REACT's scheduler (cycle 2) wires that to
//! its frame loop, and a finished animation simply stops asking.
//!
//! Time is a [`Clock`] value, not `Instant::now()` calls scattered around:
//! tests drive a virtual clock deterministically, and the app runtime owns
//! exactly one real clock so every animation in a frame samples the same
//! timestamp (no intra-frame time skew between two tweens).
//!
//! The three motion primitives in one glance — fixed A→B ([`Tween`]),
//! retargetable goal ([`Transition`]), storyboard ([`Timeline`]):
//!
//! ```
//! use std::time::Duration;
//! use abstracttui::anim::{Easing, LoopMode, Timeline, Transition, Tween};
//!
//! let ms = Duration::from_millis;
//!
//! // Tween: a fixed A -> B over a duration, sampled by elapsed time.
//! let slide = Tween::new(0.0f32, 100.0, ms(200)).with_easing(Easing::EaseOut);
//! assert_eq!(slide.sample(ms(200)), 100.0);
//! assert!(slide.sample(ms(100)) > 50.0); // ease-out front-loads motion
//!
//! // Transition: a value chasing a RETARGETABLE goal (signal bindings).
//! let mut opacity = Transition::new(0.0f32, ms(100), Easing::Linear);
//! opacity.set_target(1.0, ms(0));
//! assert_eq!(opacity.tick(ms(50)), 0.5);
//! opacity.set_target(0.0, ms(50)); // mid-flight retarget: no jump
//! assert_eq!(opacity.value(), 0.5);
//!
//! // Timeline: named tracks on one clock; scrub any instant via seek.
//! let mut board = Timeline::new(LoopMode::Once);
//! let fade = board.track(ms(0), ms(100), Easing::Linear);
//! let rise = board.track_after(fade, ms(0), ms(100), Easing::Linear);
//! let at = board.seek(ms(150));
//! assert_eq!(at.progress(fade), 1.0); // finished tracks clamp
//! assert!((at.progress(rise) - 0.5).abs() < 1e-4);
//! ```

mod easing;
pub mod particles;
pub mod shaders;
mod timeline;
mod transition;
mod tween;

pub use easing::Easing;
pub use particles::{Burst, Particle, ParticleField};
pub use timeline::{LoopMode, Seek, Timeline};
pub use transition::Transition;
pub use tween::{Lerp, Tween};

/// Re-exported from `base` (damage contract §7): the ONE frame-request
/// trait, shared with the reactive scheduler. The cycle-1 local duplicate
/// is gone; `anim::FrameRequester` remains a valid path for consumers.
pub use crate::base::FrameRequester;

use std::time::{Duration, Instant};

/// Monotonic time source. `Clock::real()` for the app runtime,
/// `Clock::fixed()` for tests (starts at zero, advanced manually).
#[derive(Debug, Clone)]
pub struct Clock {
    kind: ClockKind,
}

#[derive(Debug, Clone)]
enum ClockKind {
    Real { start: Instant },
    Virtual { now: Duration },
}

impl Clock {
    /// A wall clock anchored at construction (monotonic).
    pub fn real() -> Clock {
        Clock {
            kind: ClockKind::Real {
                start: Instant::now(),
            },
        }
    }

    /// A virtual clock frozen at zero. Time moves only via [`Clock::advance`].
    pub fn fixed() -> Clock {
        Clock {
            kind: ClockKind::Virtual {
                now: Duration::ZERO,
            },
        }
    }

    /// Time elapsed since the clock's origin. Monotonic by construction in
    /// both modes.
    pub fn now(&self) -> Duration {
        match &self.kind {
            ClockKind::Real { start } => start.elapsed(),
            ClockKind::Virtual { now } => *now,
        }
    }

    /// Advances a virtual clock. Calling this on a real clock is a test
    /// harness bug; failing loudly beats silently diverging timelines.
    pub fn advance(&mut self, by: Duration) {
        match &mut self.kind {
            ClockKind::Virtual { now } => *now += by,
            ClockKind::Real { .. } => {
                panic!("Clock::advance on a real clock — use Clock::fixed() in tests")
            }
        }
    }

    /// True for test clocks driven by `advance`.
    pub fn is_virtual(&self) -> bool {
        matches!(self.kind, ClockKind::Virtual { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_clock_is_deterministic() {
        let mut c = Clock::fixed();
        assert_eq!(c.now(), Duration::ZERO);
        c.advance(Duration::from_millis(16));
        c.advance(Duration::from_millis(16));
        assert_eq!(c.now(), Duration::from_millis(32));
    }

    #[test]
    fn real_clock_moves_forward() {
        let c = Clock::real();
        let a = c.now();
        let b = c.now();
        assert!(b >= a);
    }

    #[test]
    #[should_panic(expected = "real clock")]
    fn advancing_a_real_clock_is_a_harness_bug() {
        Clock::real().advance(Duration::from_millis(1));
    }
}
