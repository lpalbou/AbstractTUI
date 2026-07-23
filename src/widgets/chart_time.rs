//! Time-axis chart support (backlog 0190): the `TimeSeries` history
//! ring, its reactive handle, and the relative time-axis tick math —
//! child module of `chart` (file-size discipline).
//!
//! ## The model
//!
//! A monitor pushes `(t, value)` samples on its own cadence; the ring
//! quantizes time into CADENCE SLOTS (slot = t / cadence) and retains
//! a bounded window of them. Missed slots (a sampling pause, a
//! suspend) are padded with `NAN` at the next push, so the chart's
//! existing cell contract ("non-finite samples are SKIPPED — gap, not
//! zero", chart.rs) draws a HOLE for the pause instead of silently
//! compressing the x-axis. Padding is capped at the window (at most
//! `capacity - 1` hole slots per push — a day-long suspend never
//! loops a day of padding), so the gap behavior is UNIFORM at every
//! pause length: a pause at least as long as the window lands as a
//! full window of hole with the fresh sample at the right edge, never
//! a restart to a lone zero-span dot (cycle-2 review C-4 — the old
//! `missed >= capacity` restart WAS an x-axis compression, exactly
//! what this module claims to avoid). Sample spacing therefore stays
//! time-linear by construction, which is what makes the relative axis
//! labels honest.
//!
//! Time is a value, not `Instant::now()` calls (the `anim::Clock`
//! rule): `push` takes a `Duration` since the app clock's origin, so
//! tests — and deterministic demos like the dashboard — drive a
//! virtual timeline.
//!
//! CADENCE CHOICE (cycle-2 review): a producer pushing at wall-clock
//! `interval == cadence` WITH JITTER straddles slot boundaries — two
//! samples coalesce into one slot (latest wins) and the skipped
//! neighbor pads NAN, drawing a hole for a pause that never happened.
//! Drive pushes from a jitter-free clock (the dashboard's `interval`
//! on the app timeline), or pick `cadence` comfortably above the push
//! interval's jitter. Pinned in `wave_c2_review.rs`
//! (`timeseries_jittered_pushes_at_cadence_produce_phantom_gaps`).
//!
//! Retention is by SLOT COUNT (time is slot-quantized): [`TimeSeries::new`]
//! derives the count from a window duration (drop-by-age),
//! [`TimeSeries::with_slots`] takes it directly (drop-by-count). Both
//! are bounded: steady pushes never grow the ring (test-pinned).
//!
//! OWNER: CONTENT (app-widgets wave).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use crate::base::{Point, Rect, Rgba};
use crate::reactive::{Scope, Signal};
use crate::ui::Canvas;

/// Bounded history ring of cadence-slotted samples. See module docs.
#[derive(Clone, Debug)]
pub struct TimeSeries {
    cadence: Duration,
    capacity: usize,
    /// Retained slot values, oldest -> newest. Missed slots hold NAN.
    slots: VecDeque<f32>,
    /// Slot index one past the newest retained slot (0 = empty).
    next_slot: u64,
}

impl TimeSeries {
    /// Drop-by-age retention: keeps `window / cadence` slots (rounded
    /// up, at least one) — samples older than `window` fall off as new
    /// ones arrive.
    pub fn new(cadence: Duration, window: Duration) -> TimeSeries {
        let cadence = cadence.max(Duration::from_nanos(1));
        let slots = (window.as_nanos().div_ceil(cadence.as_nanos()).max(1)) as usize;
        TimeSeries::with_slots(cadence, slots)
    }

    /// Drop-by-count retention: keeps exactly `slots` cadence slots.
    pub fn with_slots(cadence: Duration, slots: usize) -> TimeSeries {
        let capacity = slots.max(1);
        TimeSeries {
            cadence: cadence.max(Duration::from_nanos(1)),
            capacity,
            slots: VecDeque::with_capacity(capacity),
            next_slot: 0,
        }
    }

    /// Record `v` at time `t` (a duration on the app's clock).
    /// Multiple pushes into one cadence slot coalesce (latest wins);
    /// skipped slots pad with `NAN` (the gap contract), capped at
    /// `capacity - 1` per push — bounded work, and uniform at every
    /// pause length: a pause at least as long as the whole window
    /// shows as a full window of hole ending in the fresh sample
    /// (never a zero-span restart); a value older than the retained
    /// window is dropped.
    pub fn push(&mut self, t: Duration, v: f32) {
        let slot = (t.as_nanos() / self.cadence.as_nanos()) as u64;
        if slot >= self.next_slot {
            if !self.slots.is_empty() {
                // The cap applies in u64 space BEFORE the usize cast,
                // so a >2^32-slot pause on a 32-bit target cannot wrap
                // into a bogus small pad count (cycle-2 review C-6).
                let missed = (slot - self.next_slot).min(self.capacity as u64 - 1) as usize;
                for _ in 0..missed {
                    self.push_slot(f32::NAN);
                }
            }
            self.push_slot(v);
            self.next_slot = slot + 1;
        } else {
            // Out-of-order or same-slot write: land it in its retained
            // slot (latest wins), drop it when the slot already aged out.
            let back = (self.next_slot - 1 - slot) as usize;
            let len = self.slots.len();
            if back < len {
                self.slots[len - 1 - back] = v;
            }
        }
    }

    fn push_slot(&mut self, v: f32) {
        if self.slots.len() == self.capacity {
            self.slots.pop_front();
        }
        self.slots.push_back(v);
    }

    /// Retained samples, oldest -> newest, one per cadence slot —
    /// missed slots read `NAN` (rendered as gaps). Feed this to
    /// [`super::LineChart`] / [`super::Sparkline`].
    pub fn samples(&self) -> Vec<f32> {
        self.slots.iter().copied().collect()
    }

    /// Time distance between the oldest and newest retained slot —
    /// the span the sample array covers on screen, i.e. the honest
    /// [`super::LineChart::time_axis`] argument. Zero until two slots
    /// exist (warmup shows real span, never the full window).
    pub fn span(&self) -> Duration {
        match self.slots.len() {
            0 | 1 => Duration::ZERO,
            n => self.cadence * (n as u32 - 1),
        }
    }

    /// The slot quantum samples are recorded at.
    pub fn cadence(&self) -> Duration {
        self.cadence
    }

    /// Retained slot count (== capacity once warmed up).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Newest retained sample (`NAN` when the newest slot is padding).
    pub fn last(&self) -> Option<f32> {
        self.slots.back().copied()
    }
}

/// Cloneable reactive handle over a [`TimeSeries`] — the `push(t, v)`
/// state monitors bind charts to (the `FeedState` shape):
///
/// ```ignore
/// let rx = TimeSeriesState::new(cx, TICK, TICK * 72);
/// interval(cx, TICK, move || rx.push(clock.now(), sample_rx()));
/// // in a dyn_view:
/// LineChart::new(vec![rx.samples()]).time_axis(rx.span())
/// ```
///
/// Reads are TRACKED (a `dyn_view` reading `samples()` re-renders per
/// push); the display refreshes exactly when data does.
#[derive(Clone)]
pub struct TimeSeriesState {
    inner: Rc<RefCell<TimeSeries>>,
    /// Bumped per push — the re-render key.
    version: Signal<u64>,
}

impl TimeSeriesState {
    /// Drop-by-age handle (see [`TimeSeries::new`]).
    pub fn new(cx: Scope, cadence: Duration, window: Duration) -> TimeSeriesState {
        TimeSeriesState {
            inner: Rc::new(RefCell::new(TimeSeries::new(cadence, window))),
            version: cx.signal(0u64),
        }
    }

    /// Drop-by-count handle (see [`TimeSeries::with_slots`]).
    pub fn with_slots(cx: Scope, cadence: Duration, slots: usize) -> TimeSeriesState {
        TimeSeriesState {
            inner: Rc::new(RefCell::new(TimeSeries::with_slots(cadence, slots))),
            version: cx.signal(0u64),
        }
    }

    /// Record a sample (see [`TimeSeries::push`]) and notify readers.
    /// Safe after UI disposal (an app-held handle stays inert).
    pub fn push(&self, t: Duration, v: f32) {
        self.inner.borrow_mut().push(t, v);
        if self.version.try_get_untracked().is_some() {
            self.version.update(|v| *v += 1);
        }
    }

    /// Tracked read of the sample window (see [`TimeSeries::samples`]).
    pub fn samples(&self) -> Vec<f32> {
        self.track();
        self.inner.borrow().samples()
    }

    /// Tracked read of the displayed span (see [`TimeSeries::span`]).
    pub fn span(&self) -> Duration {
        self.track();
        self.inner.borrow().span()
    }

    /// Tracked read of the newest sample.
    pub fn last(&self) -> Option<f32> {
        self.track();
        self.inner.borrow().last()
    }

    /// Subscribe the running computation to pushes; inert when the
    /// owning scope is gone (disposal-safe reads for diagnostics).
    fn track(&self) {
        if self.version.try_get_untracked().is_some() {
            self.version.get();
        }
    }
}

// ---------------------------------------------------------------------------
// Time-axis ticks (shared by LineChart and Sparkline)
// ---------------------------------------------------------------------------

/// Nice tick steps, seconds. Every multiple formats to a clean label.
const STEPS: [u64; 18] = [
    1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400,
];

/// Relative offset label: `0 -> "now"`, else `-30s` / `-5m` / `-1m30s`
/// / `-2h` / `-1h30m`.
fn fmt_offset(secs: u64) -> String {
    if secs == 0 {
        return "now".to_string();
    }
    if secs < 60 {
        return format!("-{secs}s");
    }
    if secs < 3600 {
        let (m, s) = (secs / 60, secs % 60);
        return if s == 0 {
            format!("-{m}m")
        } else {
            format!("-{m}m{s}s")
        };
    }
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if m == 0 {
        format!("-{h}h")
    } else {
        format!("-{h}h{m}m")
    }
}

/// Tick positions for a time axis: `(cells_from_right_edge, label)`,
/// right to left, "now" first. Label density adapts to `width` (a
/// step is admitted only when its labels cannot collide); narrow
/// widths degrade to just "now", and a zero span labels only "now".
/// Pure over (span, width) — deterministic, test-pinned.
pub(super) fn time_ticks(span: Duration, width: i32) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    if width < 3 {
        return out;
    }
    out.push((0, "now".to_string()));
    let secs = span.as_secs_f64();
    if secs <= 0.0 || width < 8 {
        return out;
    }
    let cols_per_sec = (width - 1) as f64 / secs;
    let step = STEPS.iter().copied().find(|&s| {
        if s as f64 > secs {
            return false;
        }
        // Widest label among this step's multiples inside the span;
        // +4 keeps labels breathing (readable axis density, not the
        // bare collision minimum).
        let mut widest = 0usize;
        let mut k = 1u64;
        while (k * s) as f64 <= secs {
            widest = widest.max(fmt_offset(k * s).chars().count());
            k += 1;
        }
        s as f64 * cols_per_sec >= (widest + 4) as f64
    });
    let Some(step) = step else {
        return out;
    };
    let mut k = 1u64;
    while (k * step) as f64 <= secs {
        let off = ((k * step) as f64 * cols_per_sec).round() as i32;
        if off > width - 1 {
            break;
        }
        out.push((off, fmt_offset(k * step)));
        k += 1;
    }
    out
}

/// Paint tick labels into a one-row axis rect: "now" right-aligned at
/// the edge, earlier ticks centered on their column, right-to-left
/// with collision skipping (one blank cell between labels). Backgrounds
/// stay transparent so labels embed in an existing rule row.
pub(super) fn draw_time_labels(canvas: &mut dyn Canvas, rect: Rect, span: Duration, ink: Rgba) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let mut free_right = rect.right();
    for (off, label) in time_ticks(span, rect.w) {
        let w = label.chars().count() as i32;
        let tick_col = rect.right() - 1 - off;
        let start = if off == 0 {
            rect.right() - w
        } else {
            (tick_col - w / 2).max(rect.x)
        };
        if start < rect.x || start + w > free_right {
            continue; // collides with a righter label or the edge
        }
        canvas.print(Point::new(start, rect.y), &label, ink, Rgba::TRANSPARENT);
        free_right = start - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn by_count_retention_evicts_oldest() {
        let mut ts = TimeSeries::with_slots(MS(100), 4);
        for i in 0..6u64 {
            ts.push(MS(i * 100), i as f32);
        }
        assert_eq!(ts.samples(), vec![2.0, 3.0, 4.0, 5.0]);
        assert_eq!(ts.len(), 4);
        assert_eq!(ts.last(), Some(5.0));
    }

    #[test]
    fn by_age_retention_derives_slot_count_from_window() {
        // 1s window at 250ms cadence = 4 slots.
        let mut ts = TimeSeries::new(MS(250), MS(1000));
        for i in 0..8u64 {
            ts.push(MS(i * 250), i as f32);
        }
        assert_eq!(ts.samples(), vec![4.0, 5.0, 6.0, 7.0]);
        // Span between oldest and newest retained slot: 3 * 250ms.
        assert_eq!(ts.span(), MS(750));
    }

    #[test]
    fn missed_slots_pad_with_nan_and_never_compress_time() {
        let mut ts = TimeSeries::with_slots(MS(100), 8);
        ts.push(MS(0), 1.0);
        ts.push(MS(100), 2.0);
        // Sampling pauses for 3 slots, resumes at t=500ms.
        ts.push(MS(500), 3.0);
        let s = ts.samples();
        assert_eq!(s.len(), 6, "pause slots occupy width (no compression)");
        assert_eq!((s[0], s[1], s[5]), (1.0, 2.0, 3.0));
        assert!(
            s[2].is_nan() && s[3].is_nan() && s[4].is_nan(),
            "pause materializes as non-finite padding: {s:?}"
        );
        assert_eq!(ts.span(), MS(500));
    }

    #[test]
    fn pause_longer_than_the_window_pads_a_full_window_of_hole_bounded() {
        // Updated with the C-4 fix (was `..._restarts_bounded`): the
        // pad cap keeps the work bounded — a "day" of missed slots
        // lands as at most capacity-1 NANs — while the DISPLAY stays
        // gap-honest: a full window of hole ending in the fresh
        // sample, never a zero-span restart dot.
        let mut ts = TimeSeries::with_slots(MS(100), 4);
        ts.push(MS(0), 1.0);
        // A "day" later — must not loop a day of padding.
        ts.push(Duration::from_secs(86_400), 2.0);
        let s = ts.samples();
        assert_eq!(s.len(), 4, "bounded: exactly one window, {s:?}");
        assert!(
            s[0].is_nan() && s[1].is_nan() && s[2].is_nan(),
            "the pause shows as hole: {s:?}"
        );
        assert_eq!(s[3], 2.0);
        assert_eq!(ts.span(), MS(300), "span covers the shown window");
    }

    #[test]
    fn same_slot_coalesces_and_stale_slots_drop() {
        let mut ts = TimeSeries::with_slots(MS(100), 4);
        ts.push(MS(0), 1.0);
        ts.push(MS(50), 1.5); // same slot: latest wins
        ts.push(MS(100), 2.0);
        assert_eq!(ts.samples(), vec![1.5, 2.0]);
        // Out-of-order write into a retained slot lands.
        ts.push(MS(20), 9.0);
        assert_eq!(ts.samples(), vec![9.0, 2.0]);
        // A write older than the whole window is dropped.
        for i in 2..6u64 {
            ts.push(MS(i * 100), i as f32);
        }
        ts.push(MS(0), 7.0);
        assert_eq!(ts.samples(), vec![2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn soak_push_loop_never_grows_the_ring_allocation() {
        let mut ts = TimeSeries::with_slots(MS(100), 72);
        // Warm to capacity, snapshot the ring's allocation, then soak.
        for i in 0..72u64 {
            ts.push(MS(i * 100), i as f32);
        }
        let cap = ts.slots.capacity();
        for i in 72..10_072u64 {
            ts.push(MS(i * 100), (i % 97) as f32);
        }
        assert_eq!(ts.slots.capacity(), cap, "steady pushes must not realloc");
        assert_eq!(ts.len(), 72);
    }

    #[test]
    fn tick_labels_adapt_density_and_anchor_now_right() {
        // 18s span (the dashboard window class) at two widths.
        let wide = time_ticks(Duration::from_secs(18), 60);
        assert_eq!(wide[0], (0, "now".to_string()));
        let labels: Vec<&str> = wide.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(labels, vec!["now", "-5s", "-10s", "-15s"]);
        // Offsets are right-edge distances, increasing leftward.
        assert!(wide.windows(2).all(|w| w[0].0 < w[1].0));

        let narrow = time_ticks(Duration::from_secs(18), 24);
        let labels: Vec<&str> = narrow.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(labels, vec!["now", "-10s"], "density adapts to width");

        // Too narrow for anything but "now"; and zero span = just now.
        assert_eq!(time_ticks(Duration::from_secs(18), 7).len(), 1);
        assert_eq!(time_ticks(Duration::ZERO, 60).len(), 1);
        assert!(time_ticks(Duration::from_secs(18), 2).is_empty());
    }

    #[test]
    fn tick_math_is_deterministic_and_minute_labels_format() {
        let a = time_ticks(Duration::from_secs(300), 80);
        let b = time_ticks(Duration::from_secs(300), 80);
        assert_eq!(a, b);
        let labels: Vec<&str> = a.iter().map(|(_, l)| l.as_str()).collect();
        assert!(labels.contains(&"-1m"), "{labels:?}");
        assert_eq!(fmt_offset(90), "-1m30s");
        assert_eq!(fmt_offset(7200), "-2h");
        assert_eq!(fmt_offset(5400), "-1h30m");
    }

    #[test]
    fn reactive_handle_tracks_pushes() {
        use crate::reactive::{create_root, flush_effects};
        let seen = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let s = seen.clone();
        let (root, ts) = create_root(|cx| {
            let ts = TimeSeriesState::new(cx, MS(100), MS(1000));
            let t2 = ts.clone();
            cx.effect(move || {
                let _ = t2.samples();
                s.set(s.get() + 1);
            });
            ts
        });
        flush_effects();
        assert_eq!(seen.get(), 1);
        ts.push(MS(0), 1.0);
        flush_effects();
        assert_eq!(seen.get(), 2, "push re-runs tracked readers");
        assert_eq!(ts.last(), Some(1.0));
        root.dispose();
        ts.push(MS(100), 2.0); // disposal-safe: inert, no panic
    }
}
