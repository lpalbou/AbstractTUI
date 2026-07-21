//! Timeline: a storyboard of eased tracks on one clock.
//!
//! The boot identity storyboard (docs: `boot::identity` phase constants)
//! is the design consumer: named things start at offsets, run for
//! durations under easings, sometimes staggered, and the whole board can
//! play once, loop, or ping-pong.
//!
//! Deliberate shape: a track yields eased PROGRESS (0..=1), not a value —
//! storyboards mix types (a position, an opacity, a color per track), so
//! values stay with the consumer (`Tween`/`Lerp` compose:
//! `from.lerp(to, timeline.progress(track, t))`). This keeps the timeline
//! allocation-free at evaluation and trivially serializable.
//!
//! Evaluation is pure over `t` (wall-clock seconds may arrive repeated or
//! jumped-forward — frame drops — per DESIGN's frozen splash-source seam);
//! building the track list allocates, evaluating never does.

use std::time::Duration;

use super::Easing;

/// How the timeline's clock folds beyond its total duration.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LoopMode {
    /// Clamp at the end (enter animations, the boot splash).
    #[default]
    Once,
    /// Wrap around (spinners, shimmer drivers).
    Loop,
    /// Forward then backward (breathing effects).
    PingPong,
}

/// Handle to one track; returned by the builders, consumed by
/// [`Timeline::progress`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TrackId(usize);

#[derive(Copy, Clone, Debug)]
struct Track {
    start: Duration,
    duration: Duration,
    easing: Easing,
}

/// A storyboard of eased tracks on one clock (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    tracks: Vec<Track>,
    mode: LoopMode,
}

impl Timeline {
    /// An empty board with the given loop behavior.
    pub fn new(mode: LoopMode) -> Timeline {
        Timeline {
            tracks: Vec::new(),
            mode,
        }
    }

    /// Adds a track starting at `start` for `duration`. Tracks sharing a
    /// start run in parallel by construction.
    pub fn track(&mut self, start: Duration, duration: Duration, easing: Easing) -> TrackId {
        self.tracks.push(Track {
            start,
            duration,
            easing,
        });
        TrackId(self.tracks.len() - 1)
    }

    /// Adds a track beginning `gap` after `prev` ENDS — sequencing.
    pub fn track_after(
        &mut self,
        prev: TrackId,
        gap: Duration,
        duration: Duration,
        easing: Easing,
    ) -> TrackId {
        let p = self.tracks[prev.0];
        self.track(p.start + p.duration + gap, duration, easing)
    }

    /// Adds `count` identical tracks, each starting `step` after the one
    /// before (list reveals, particle bursts, the wordmark letters).
    /// Returns the first id; the rest are consecutive (`TrackId` is dense).
    pub fn stagger(
        &mut self,
        count: usize,
        start: Duration,
        duration: Duration,
        step: Duration,
        easing: Easing,
    ) -> TrackId {
        let first = TrackId(self.tracks.len());
        for i in 0..count {
            self.track(start + step * i as u32, duration, easing);
        }
        // A zero-count stagger still yields a valid (empty-range) handle;
        // progress() on it is the caller's own off-by-one to keep.
        first
    }

    /// The `i`-th track of a stagger created with [`Timeline::stagger`].
    pub fn nth(&self, first: TrackId, i: usize) -> TrackId {
        TrackId(first.0 + i)
    }

    /// True when no tracks were added.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// End of the last track — one loop period.
    pub fn duration(&self) -> Duration {
        self.tracks
            .iter()
            .map(|t| t.start + t.duration)
            .max()
            .unwrap_or(Duration::ZERO)
    }

    /// True once nothing can change anymore. Looping timelines never
    /// finish; the caller stops requesting frames when this returns true.
    pub fn is_finished(&self, t: Duration) -> bool {
        match self.mode {
            LoopMode::Once => t >= self.duration(),
            LoopMode::Loop | LoopMode::PingPong => false,
        }
    }

    /// Eased progress of `track` at timeline clock `t`: 0 before its
    /// start, 1 after its end, eased in between. The loop mode folds `t`
    /// over the WHOLE board first, so loops restart every track together.
    pub fn progress(&self, track: TrackId, t: Duration) -> f32 {
        let Some(tr) = self.tracks.get(track.0) else {
            return 0.0; // stale id: inert, not panicking (storyboards rebuild)
        };
        let t = self.fold(t);
        if t < tr.start {
            return tr.easing.eval(0.0);
        }
        if tr.duration.is_zero() {
            return tr.easing.eval(1.0);
        }
        let raw = (t - tr.start).as_secs_f32() / tr.duration.as_secs_f32();
        tr.easing.eval(raw.clamp(0.0, 1.0))
    }

    /// Scrub cursor: binds one clock position so a scrubber (the boot
    /// player's test rig, an effects demo slider) can sample many tracks
    /// at one instant without threading `t` through every call. Pure —
    /// the timeline stays stateless; a `Seek` is a view, not a playhead.
    pub fn seek(&self, t: Duration) -> Seek<'_> {
        Seek { timeline: self, t }
    }

    /// The same instant viewed in reverse playback: board position
    /// `duration − t` (clamped at zero past the end). One pass mirrored —
    /// loop folding still applies through `progress` as usual.
    pub fn seek_reversed(&self, t: Duration) -> Seek<'_> {
        Seek {
            timeline: self,
            t: self.duration().saturating_sub(t),
        }
    }

    /// Folds the clock per the loop mode.
    fn fold(&self, t: Duration) -> Duration {
        let total = self.duration();
        if total.is_zero() {
            return Duration::ZERO;
        }
        match self.mode {
            LoopMode::Once => t.min(total),
            LoopMode::Loop => {
                // Exact end maps to end (a completed pass reads finished,
                // the next nanosecond restarts).
                if t == total {
                    total
                } else {
                    nanos_mod(t, total)
                }
            }
            LoopMode::PingPong => {
                let period = total * 2;
                let folded = if t == period {
                    period
                } else {
                    nanos_mod(t, period)
                };
                if folded <= total {
                    folded
                } else {
                    period - folded
                }
            }
        }
    }
}

fn nanos_mod(t: Duration, m: Duration) -> Duration {
    Duration::from_nanos((t.as_nanos() % m.as_nanos()) as u64)
}

/// A timeline bound to one clock position (see [`Timeline::seek`]).
#[derive(Copy, Clone, Debug)]
pub struct Seek<'a> {
    timeline: &'a Timeline,
    t: Duration,
}

impl Seek<'_> {
    /// The bound clock (after any reverse mirroring, before loop folds).
    pub fn clock(&self) -> Duration {
        self.t
    }

    /// [`Timeline::progress`] at the bound clock.
    pub fn progress(&self, track: TrackId) -> f32 {
        self.timeline.progress(track, self.t)
    }

    /// [`Timeline::is_finished`] at the bound clock.
    pub fn is_finished(&self) -> bool {
        self.timeline.is_finished(self.t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn parallel_and_sequenced_tracks() {
        let mut tl = Timeline::new(LoopMode::Once);
        let a = tl.track(MS(0), MS(100), Easing::Linear);
        let b = tl.track(MS(0), MS(200), Easing::Linear); // parallel with a
        let c = tl.track_after(a, MS(50), MS(100), Easing::Linear); // 150..250
        assert_eq!(tl.duration(), MS(250));

        assert!(close(tl.progress(a, MS(50)), 0.5));
        assert!(
            close(tl.progress(b, MS(50)), 0.25),
            "parallel track on its own pace"
        );
        assert!(close(tl.progress(c, MS(50)), 0.0), "sequenced track waits");
        assert!(close(tl.progress(c, MS(200)), 0.5));
        assert!(tl.is_finished(MS(250)));
        assert!(!tl.is_finished(MS(249)));
    }

    #[test]
    fn stagger_offsets_each_member() {
        let mut tl = Timeline::new(LoopMode::Once);
        let first = tl.stagger(4, MS(100), MS(100), MS(30), Easing::Linear);
        // Member i starts at 100 + 30i.
        assert!(close(tl.progress(first, MS(150)), 0.5));
        assert!(close(tl.progress(tl.nth(first, 1), MS(150)), 0.2));
        assert!(close(tl.progress(tl.nth(first, 2), MS(150)), 0.0));
        assert!(close(tl.progress(tl.nth(first, 3), MS(190)), 0.0));
        assert!(close(tl.progress(tl.nth(first, 3), MS(290)), 1.0));
        assert_eq!(tl.duration(), MS(290));
    }

    #[test]
    fn easing_applies_per_track_and_before_start_is_eased_zero() {
        let mut tl = Timeline::new(LoopMode::Once);
        let t = tl.track(MS(100), MS(100), Easing::EaseIn);
        assert!(close(tl.progress(t, MS(150)), 0.125), "t^3 at midpoint");
        // Eased zero (all our curves pass through 0, but the contract is
        // "eval(0)", not literal 0 — overshoot anticipation curves dip).
        assert!(close(tl.progress(t, MS(0)), 0.0));
    }

    #[test]
    fn loop_mode_wraps_the_whole_board() {
        let mut tl = Timeline::new(LoopMode::Loop);
        let a = tl.track(MS(0), MS(100), Easing::Linear);
        let b = tl.track(MS(100), MS(100), Easing::Linear);
        assert!(close(tl.progress(a, MS(250)), 0.5), "t=250 folds to 50");
        assert!(
            close(tl.progress(b, MS(250)), 0.0),
            "second track restarts too"
        );
        assert!(!tl.is_finished(MS(10_000)));
    }

    #[test]
    fn pingpong_mirrors() {
        let mut tl = Timeline::new(LoopMode::PingPong);
        let a = tl.track(MS(0), MS(100), Easing::Linear);
        assert!(close(tl.progress(a, MS(60)), 0.6));
        assert!(
            close(tl.progress(a, MS(140)), 0.6),
            "mirrored on the way back"
        );
        assert!(close(tl.progress(a, MS(260)), 0.6), "period 2*total");
    }

    #[test]
    fn degenerate_shapes_are_inert() {
        let tl = Timeline::new(LoopMode::Once);
        assert_eq!(tl.duration(), Duration::ZERO);
        assert!(tl.is_finished(Duration::ZERO));
        assert_eq!(tl.progress(TrackId(9), MS(50)), 0.0, "stale id is inert");

        let mut tl = Timeline::new(LoopMode::Once);
        let z = tl.track(MS(10), MS(0), Easing::Linear);
        assert!(close(tl.progress(z, MS(5)), 0.0));
        assert!(
            close(tl.progress(z, MS(10)), 1.0),
            "zero-duration track is a step"
        );
    }

    #[test]
    fn seek_binds_one_clock_and_reverse_mirrors_the_pass() {
        let mut tl = Timeline::new(LoopMode::Once);
        let a = tl.track(MS(0), MS(100), Easing::Linear);
        let b = tl.track(MS(100), MS(100), Easing::Linear);

        // A scrub position samples every track at the same instant.
        let s = tl.seek(MS(150));
        assert!(close(s.progress(a), 1.0));
        assert!(close(s.progress(b), 0.5));
        assert_eq!(s.clock(), MS(150));
        assert!(!s.is_finished());
        assert!(tl.seek(MS(200)).is_finished());
        // A Seek is a pure view: sampling twice agrees, and the timeline
        // itself carries no playhead to disturb.
        assert!(close(s.progress(b), tl.progress(b, MS(150))));

        // Reverse playback: t seconds INTO the reversed pass = board
        // position duration - t.
        let r = tl.seek_reversed(MS(50));
        assert_eq!(r.clock(), MS(150), "200ms board, 50ms into reverse");
        assert!(close(r.progress(b), 0.5));
        // Past the end of the reversed pass clamps to the board start.
        assert_eq!(tl.seek_reversed(MS(500)).clock(), Duration::ZERO);
    }

    #[test]
    fn identity_storyboard_shape_composes() {
        // The boot storyboard's constants (identity.rs) as a smoke shape:
        // arrival 0..900ms, align 900..1400, reveal 1400..1850 staggered.
        let mut tl = Timeline::new(LoopMode::Once);
        let arrival = tl.track(MS(0), MS(900), Easing::bezier(0.16, 1.0, 0.30, 1.0));
        let align = tl.track(MS(900), MS(500), Easing::bezier(0.83, 0.0, 0.17, 1.0));
        let letters = tl.stagger(
            11,
            MS(1400),
            MS(180),
            MS(30),
            Easing::bezier(0.33, 1.0, 0.68, 1.0),
        );
        assert_eq!(tl.duration(), MS(1880));
        assert!(tl.progress(arrival, MS(900)) >= 1.0 - 1e-4);
        assert!(tl.progress(align, MS(1150)) > 0.0);
        assert!(tl.progress(tl.nth(letters, 10), MS(1879)) < 1.0);
        assert!(tl.is_finished(MS(1880)));
    }
}
