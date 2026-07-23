//! meter — live audio-level rendering with real ballistics
//! (backlog media-av/0620).
//!
//! Feeding raw RMS frames to a bar chart flickers illegibly; every
//! audio UI ever shipped solves this with ballistics, and every app
//! would re-derive the same few lines of state wrong
//! (frame-rate-dependent decay). The state machine belongs in a widget
//! so its decay is FRAME-clocked (a stalled stream shows a falling bar,
//! not a frozen one) and its saturation colors are theme tokens.
//!
//! ## Ballistics
//!
//! - **Instant attack**: a rising input jumps the display immediately.
//! - **Timed decay**: a falling display moves at a constant rate in
//!   display space — `decay_db_per_s` over the meter's span (the dB
//!   span when [`Meter::db_floor`] is set, a 60 dB-equivalent span in
//!   linear mode), advanced on frame time, frame-rate-independent.
//! - **Peak hold**: the peak marker holds for [`Meter::peak_hold`]
//!   (~1.5 s default), then falls at the decay rate until it rejoins
//!   the display level.
//!
//! ## The idle law (media-av/0620, pinned by tests)
//!
//! A silent/idle meter reaches its FIXPOINT (`display == target` and
//! `peak == display`) and its frame task returns false — no more frame
//! requests, no allocations, byte-for-byte idle turns. Unchanged input
//! costs literal zero; only real motion bills frames (damage-contract
//! §4: an animation is a sequence of frame requests).
//!
//! Rendering: eighth-block sub-cell fill, horizontal (default) or
//! vertical, one channel ([`Meter::new`]) or N bands
//! ([`Meter::bands`]); zone colors ride the `ok`/`warn`/`error` tokens,
//! never hardcoded green/red. Data only: producers are app-side
//! (`latest_source`/`bounded_source`/signals) — no audio, no threads,
//! no I/O in the widget.
//!
//! OWNER: INPUTAV (wave 3).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::base::{Point, Rgba};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::ui::{dyn_view, Element, View};

/// Horizontal eighth blocks, index = eighths filled from the left.
const H_EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
/// Vertical eighth blocks, index = eighths filled from the bottom.
const V_EIGHTHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

// ---------------------------------------------------------------------------
// Ballistics core (pure, virtual-clock tested)
// ---------------------------------------------------------------------------

/// One channel's ballistic state, in NORMALIZED display space (0..1).
#[derive(Copy, Clone, Debug)]
struct Chan {
    target: f32,
    display: f32,
    peak: f32,
    /// Stamped by the first advance after the peak (re)rose; `None`
    /// means "hold starts at the next frame's clock reading" (the
    /// animate.rs started-stamp pattern — no real-clock coupling).
    peak_since: Option<Instant>,
}

impl Chan {
    const ZERO: Chan = Chan {
        target: 0.0,
        display: 0.0,
        peak: 0.0,
        peak_since: None,
    };
}

/// The frame-clocked meter state machine (shared by every render mode).
struct Ballistics {
    channels: Vec<Chan>,
    /// Display-space fall rate, positions per second.
    fall_per_s: f32,
    peak_hold: Duration,
    /// Previous advance's clock reading; `None` re-anchors after the
    /// task parked (a re-arm must not integrate the parked gap as dt).
    last: Option<Instant>,
}

impl Ballistics {
    fn new(channels: usize, fall_per_s: f32, peak_hold: Duration) -> Ballistics {
        Ballistics {
            channels: vec![Chan::ZERO; channels.max(1)],
            fall_per_s: fall_per_s.max(f32::EPSILON),
            peak_hold,
            last: None,
        }
    }

    /// Fold new normalized targets in (instant attack). Non-finite
    /// samples are ignored (gap semantics — keep the previous target,
    /// never poison the state). Returns true when anything visible
    /// changed. Never shrinks the channel count mid-flight: a short
    /// frame leaves the missing channels decaying toward their old
    /// targets, a longer frame grows the vector.
    fn set_targets(&mut self, values: &[f32]) -> bool {
        if values.len() > self.channels.len() {
            self.channels.resize(values.len(), Chan::ZERO);
        }
        let mut changed = false;
        for (chan, v) in self.channels.iter_mut().zip(values.iter()) {
            if !v.is_finite() {
                continue;
            }
            let v = v.clamp(0.0, 1.0);
            if v != chan.target {
                chan.target = v;
                changed = true;
            }
            if v > chan.display {
                chan.display = v; // instant attack
                changed = true;
            }
            if chan.display > chan.peak {
                chan.peak = chan.display;
                chan.peak_since = None; // hold restarts at next advance
                changed = true;
            }
        }
        changed
    }

    /// Advance decay/peak-fall to `now`. Exact-arithmetic fixpoint: the
    /// `max()` clamps land display ON target and peak ON display, so
    /// settledness is plain equality — no epsilon drift.
    fn advance(&mut self, now: Instant) {
        let dt = match self.last.replace(now) {
            Some(last) => now.saturating_duration_since(last).as_secs_f32(),
            None => 0.0, // first frame after (re)arming: anchor only
        };
        let fall = self.fall_per_s * dt;
        for chan in self.channels.iter_mut() {
            if chan.display > chan.target {
                chan.display = (chan.display - fall).max(chan.target);
            }
            if chan.peak > chan.display {
                let since = *chan.peak_since.get_or_insert(now);
                if now.saturating_duration_since(since) >= self.peak_hold {
                    chan.peak = (chan.peak - fall).max(chan.display);
                }
            } else {
                chan.peak = chan.display;
            }
        }
    }

    /// The fixpoint: nothing left to animate. The frame task drops the
    /// moment this is true — THE zero-idle law.
    fn settled(&self) -> bool {
        self.channels
            .iter()
            .all(|c| c.display == c.target && c.peak == c.display)
    }

    /// Re-anchor the clock (called when the frame task re-arms after a
    /// parked stretch — a 10 s gap must not integrate as 10 s of fall).
    fn reanchor(&mut self) {
        self.last = None;
    }
}

/// Map a linear 0..1 amplitude into normalized display space under an
/// optional dB floor: `db_floor = Some(-60.0)` puts -60 dB at 0 and
/// 0 dB at 1. Non-positive amplitudes floor at 0; non-finite pass
/// through (the ballistics ignore them as gaps).
fn map_level(v: f32, db_floor: Option<f32>) -> f32 {
    let Some(floor) = db_floor else {
        return v;
    };
    if !v.is_finite() {
        return v;
    }
    if v <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * v.log10();
    ((db - floor) / -floor).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Meter widget
// ---------------------------------------------------------------------------

enum MeterInput {
    Mono(Signal<f32>),
    Bands(Signal<Vec<f32>>),
}

/// Meter orientation (single-channel mode; band mode always renders
/// vertical bars).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

/// Live level meter with broadcast-feeling ballistics. Input is DATA
/// (`Signal<f32>` levels 0..1, or `Signal<Vec<f32>>` band frames);
/// producers live app-side.
///
/// ```ignore
/// let level = cx.signal(0.0f32);                 // fed by the recorder lane
/// Meter::new(level).db_floor(-60.0).view(cx)     // one horizontal channel
/// Meter::bands(band_frames).view(cx)             // N vertical bars
/// ```
pub struct Meter {
    input: MeterInput,
    orientation: Orientation,
    db_floor: Option<f32>,
    decay_db_per_s: f32,
    peak_hold: Duration,
    /// Zone boundaries in normalized display space.
    warn_at: f32,
    danger_at: f32,
    bar_w: i32,
    gap: i32,
    layout: Option<LayoutStyle>,
}

impl Meter {
    /// One channel from a level signal (0..1 linear amplitude).
    pub fn new(level: Signal<f32>) -> Meter {
        Meter::build(MeterInput::Mono(level))
    }

    /// N bands from a frame signal (each frame = one `Vec<f32>` of
    /// 0..1 levels; the band count follows the frames).
    pub fn bands(frames: Signal<Vec<f32>>) -> Meter {
        Meter::build(MeterInput::Bands(frames))
    }

    fn build(input: MeterInput) -> Meter {
        Meter {
            input,
            orientation: Orientation::Horizontal,
            db_floor: None,
            decay_db_per_s: 20.0,
            peak_hold: Duration::from_millis(1500),
            warn_at: 0.70,
            danger_at: 0.90,
            bar_w: 2,
            gap: 1,
            layout: None,
        }
    }

    /// Log-map inputs: `floor` dB (e.g. -60.0) renders at 0, 0 dB at
    /// full scale. Values must be negative; non-negative floors are
    /// ignored (linear mapping stays).
    pub fn db_floor(mut self, floor: f32) -> Meter {
        if floor < 0.0 && floor.is_finite() {
            self.db_floor = Some(floor);
        }
        self
    }

    /// Decay rate in dB/s over the meter span (default 20.0 — a
    /// full-scale fall over a 60 dB span takes 3 s). Clamped positive.
    pub fn decay(mut self, db_per_s: f32) -> Meter {
        if db_per_s > 0.0 && db_per_s.is_finite() {
            self.decay_db_per_s = db_per_s;
        }
        self
    }

    /// Peak-hold duration before the marker falls (default 1.5 s).
    pub fn peak_hold(mut self, hold: Duration) -> Meter {
        self.peak_hold = hold;
        self
    }

    /// Zone boundaries in normalized display space (defaults 0.70 /
    /// 0.90): fill below `warn_at` renders in the `ok` token, then
    /// `warn`, then `error`.
    pub fn zones(mut self, warn_at: f32, danger_at: f32) -> Meter {
        self.warn_at = warn_at.clamp(0.0, 1.0);
        self.danger_at = danger_at.clamp(self.warn_at, 1.0);
        self
    }

    /// Render the single channel as a vertical column (band mode is
    /// always vertical bars).
    pub fn vertical(mut self) -> Meter {
        self.orientation = Orientation::Vertical;
        self
    }

    /// Band-bar width / gap in cells (band mode; clamped >= 1 / >= 0).
    pub fn bar(mut self, width: i32, gap: i32) -> Meter {
        self.bar_w = width.max(1);
        self.gap = gap.max(0);
        self
    }

    /// Layout override (defaults: 1 row grown wide for horizontal,
    /// grow-both otherwise).
    pub fn layout(mut self, layout: LayoutStyle) -> Meter {
        self.layout = Some(layout);
        self
    }

    /// Build the reactive view: an input effect (instant attack +
    /// frame-task arming) and a `dyn_view` painting the ballistic
    /// state. Both are owned by `cx` and die with the component.
    pub fn view(self, cx: Scope) -> View {
        let span_db = self.db_floor.map(|f| -f).unwrap_or(60.0);
        let fall_per_s = self.decay_db_per_s / span_db;
        let channels = match self.input {
            MeterInput::Mono(_) => 1,
            MeterInput::Bands(_) => 1, // grows with the first frame
        };
        let core = Rc::new(RefCell::new(Ballistics::new(
            channels,
            fall_per_s,
            self.peak_hold,
        )));
        // The repaint pulse: bumped by the input effect (attack) and by
        // the frame task (decay motion); the dyn_view subscribes.
        let repaint = cx.signal(0u64);
        // One live frame task per meter, max (the animate.rs pattern).
        let task_live = Rc::new(std::cell::Cell::new(false));

        let db_floor = self.db_floor;
        let input_core = core.clone();
        let input_task = task_live.clone();
        match self.input {
            MeterInput::Mono(level) => {
                cx.effect_labeled("meter-input", move || {
                    let v = map_level(level.get(), db_floor);
                    fold_input(&input_core, &input_task, repaint, &[v]);
                });
            }
            MeterInput::Bands(frames) => {
                cx.effect_labeled("meter-input", move || {
                    let mapped: Vec<f32> = frames
                        .with(|frame| frame.iter().map(|v| map_level(*v, db_floor)).collect());
                    fold_input(&input_core, &input_task, repaint, &mapped);
                });
            }
        }

        let orientation = self.orientation;
        let (warn_at, danger_at) = (self.warn_at, self.danger_at);
        let (bar_w, gap) = (self.bar_w, self.gap);
        let layout = self.layout.unwrap_or_else(|| match orientation {
            Orientation::Horizontal => LayoutStyle::default().height(Dimension::Cells(1)).grow(1.0),
            Orientation::Vertical => LayoutStyle::default().grow(1.0),
        });

        dyn_view(layout, move || {
            let _ = repaint.get(); // subscribe to motion
                                   // Tokens resolve TRACKED inside the dyn scope: a theme
                                   // switch re-renders the meter in the new inks.
            let t = super::theme_tokens(cx);
            let zone_inks = [t.ok, t.warn, t.error];
            let track_ink = t.text_faint;
            let snapshot: Vec<Chan> = core.borrow().channels.clone();
            // The inner element fills the dyn region (the outer layout
            // lives on the dyn node itself).
            let el = Element::new().style(LayoutStyle::default().grow(1.0));
            let el = match orientation {
                Orientation::Horizontal if snapshot.len() == 1 => {
                    let chan = snapshot[0];
                    el.draw(move |canvas, rect| {
                        draw_horizontal(
                            canvas, rect, chan, warn_at, danger_at, zone_inks, track_ink,
                        );
                    })
                }
                _ => el.draw(move |canvas, rect| {
                    draw_bars(
                        canvas, rect, &snapshot, bar_w, gap, warn_at, danger_at, zone_inks,
                    );
                }),
            };
            el.build()
        })
    }
}

/// Shared input-effect body: attack + repaint + frame-task arming.
fn fold_input(
    core: &Rc<RefCell<Ballistics>>,
    task_live: &Rc<std::cell::Cell<bool>>,
    repaint: Signal<u64>,
    values: &[f32],
) {
    let (changed, settled) = {
        let mut b = core.borrow_mut();
        let changed = b.set_targets(values);
        (changed, b.settled())
    };
    if changed {
        repaint.update(|g| *g = g.wrapping_add(1));
    }
    if !settled && !task_live.get() {
        task_live.set(true);
        core.borrow_mut().reanchor();
        let core = core.clone();
        let task_live = task_live.clone();
        crate::reactive::register_frame_task(move |now| {
            // A disposed component mid-flight: drop the task quietly.
            if !repaint.is_alive() {
                task_live.set(false);
                return false;
            }
            let settled = {
                let mut b = core.borrow_mut();
                b.advance(now);
                b.settled()
            };
            repaint.update(|g| *g = g.wrapping_add(1));
            if settled {
                task_live.set(false);
                false // FIXPOINT: no further frames — the idle law
            } else {
                crate::reactive::request_frame();
                true
            }
        });
        crate::reactive::request_frame();
    }
}

/// Zone ink for a normalized position.
fn zone_ink(pos: f32, warn_at: f32, danger_at: f32, inks: [Rgba; 3]) -> Rgba {
    if pos >= danger_at {
        inks[2]
    } else if pos >= warn_at {
        inks[1]
    } else {
        inks[0]
    }
}

/// One-row horizontal meter: eighth-block fill, per-cell zone inks, a
/// faint dotted track, and the peak-hold tick.
fn draw_horizontal(
    canvas: &mut dyn crate::ui::StyledCanvas,
    rect: crate::base::Rect,
    chan: Chan,
    warn_at: f32,
    danger_at: f32,
    inks: [Rgba; 3],
    track_ink: Rgba,
) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let y = rect.y + rect.h / 2;
    let eighths_total = rect.w * 8;
    let fill = (chan.display * eighths_total as f32).round() as i32;
    let (full, part) = (fill / 8, fill % 8);
    for cx in 0..rect.w {
        let pos = (cx as f32 + 0.5) / rect.w as f32;
        let ink = zone_ink(pos, warn_at, danger_at, inks);
        let p = Point::new(rect.x + cx, y);
        if cx < full {
            canvas.put(p, '█', ink, Rgba::TRANSPARENT);
        } else if cx == full && part > 0 {
            canvas.put(p, H_EIGHTHS[(part - 1) as usize], ink, Rgba::TRANSPARENT);
        } else {
            canvas.put(p, '·', track_ink, Rgba::TRANSPARENT);
        }
    }
    // Peak tick: only when it sits beyond the fill tip (equal peak IS
    // the tip — overdrawing the partial glyph would erase resolution).
    let peak_cell = ((chan.peak * eighths_total as f32).round() as i32 / 8).min(rect.w - 1);
    if chan.peak > chan.display && peak_cell > full {
        let pos = (peak_cell as f32 + 0.5) / rect.w as f32;
        let ink = zone_ink(pos, warn_at, danger_at, inks);
        canvas.put(
            Point::new(rect.x + peak_cell, y),
            '│',
            ink,
            Rgba::TRANSPARENT,
        );
    }
}

/// Vertical bars (single vertical channel or N bands): eighth-block
/// columns, zone ink per HEIGHT position, peak tick above the fill.
#[allow(clippy::too_many_arguments)]
fn draw_bars(
    canvas: &mut dyn crate::ui::StyledCanvas,
    rect: crate::base::Rect,
    channels: &[Chan],
    bar_w: i32,
    gap: i32,
    warn_at: f32,
    danger_at: f32,
    inks: [Rgba; 3],
) {
    if rect.w <= 0 || rect.h <= 0 || channels.is_empty() {
        return;
    }
    let mut x = rect.x;
    for chan in channels {
        if x >= rect.right() {
            break;
        }
        let eighths_total = rect.h * 8;
        let fill = (chan.display * eighths_total as f32).round() as i32;
        let (full, part) = (fill / 8, fill % 8);
        let w = bar_w.min(rect.right() - x);
        for row in 0..rect.h {
            // Row 0 of the fill is the BOTTOM row; zone by row position.
            let pos = (row as f32 + 0.5) / rect.h as f32;
            let ink = zone_ink(pos, warn_at, danger_at, inks);
            let py = rect.bottom() - 1 - row;
            for col in 0..w {
                let p = Point::new(x + col, py);
                if row < full {
                    canvas.put(p, '█', ink, Rgba::TRANSPARENT);
                } else if row == full && part > 0 {
                    canvas.put(p, V_EIGHTHS[(part - 1) as usize], ink, Rgba::TRANSPARENT);
                }
            }
        }
        let peak_row = ((chan.peak * eighths_total as f32).round() as i32 / 8).min(rect.h - 1);
        if chan.peak > chan.display && peak_row > full {
            let pos = (peak_row as f32 + 0.5) / rect.h as f32;
            let ink = zone_ink(pos, warn_at, danger_at, inks);
            let py = rect.bottom() - 1 - peak_row;
            for col in 0..w {
                canvas.put(Point::new(x + col, py), '─', ink, Rgba::TRANSPARENT);
            }
        }
        x += bar_w + gap;
    }
}

// Tests live in a sibling file to keep this one within the size budget
// (virtual-clock ballistics + render pins + the fixpoint law).
#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;
