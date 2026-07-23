//! Charts: sparkline, line chart and bar chart on sub-cell grids.
//!
//! ```ignore
//! use abstracttui::widgets::{Sparkline, LineChart, BarChart};
//! let t = theme.tokens;
//! let spark = Sparkline::new(cpu_history).slot(1).element(&t).build();
//! let chart = LineChart::new(vec![rx_series, tx_series])
//!     .range(0.0, 100.0)
//!     .element(&t)
//!     .build();
//! let bars = BarChart::new(per_core_load).element(&t).build();
//! ```
//!
//! Resolution: braille cells pack 2x4 dots (sparkline: 1 row = 4 vertical
//! steps; line chart: a WxH panel is a 2Wx4H dot canvas); bar charts use
//! vertical eighth blocks (8 steps per cell). Series colors come from the
//! theme's `chart[0..8]` ramp — pass a slot index, never a color. Axes
//! and labels use `border`/`text_faint` per §3.
//!
//! Data contract: non-finite samples are SKIPPED (gap, not zero); an
//! empty/all-skipped series draws nothing; a flat series draws its line
//! mid-range rather than dividing by zero. Rendering is deterministic —
//! same data, same cells (test-pinned).
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rgba};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

// Time-axis support (backlog 0190): history ring + reactive handle +
// tick math — sibling module for the file-size discipline.
#[path = "chart_time.rs"]
mod time;
pub use time::{TimeSeries, TimeSeriesState};

/// Braille dot bit for (col in 0..2, row in 0..4) — Unicode braille
/// bit order (dots 1-8).
const fn braille_bit(col: i32, row: i32) -> u8 {
    match (col, row) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, 3) => 0x40,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        _ => 0x80, // (1, 3)
    }
}

/// A dot grid over a cell rect: 2 columns x 4 rows of dots per cell.
/// Accumulates bits per cell, then emits braille chars.
struct BrailleGrid {
    cells_w: i32,
    cells_h: i32,
    bits: Vec<u8>,
}

impl BrailleGrid {
    fn new(cells_w: i32, cells_h: i32) -> BrailleGrid {
        let n = (cells_w.max(0) * cells_h.max(0)) as usize;
        BrailleGrid {
            cells_w,
            cells_h,
            bits: vec![0; n],
        }
    }

    fn dots_w(&self) -> i32 {
        self.cells_w * 2
    }

    fn dots_h(&self) -> i32 {
        self.cells_h * 4
    }

    fn set(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.dots_w() || y >= self.dots_h() {
            return;
        }
        let idx = ((y / 4) * self.cells_w + x / 2) as usize;
        self.bits[idx] |= braille_bit(x % 2, y % 4);
    }

    /// Bresenham segment on the dot grid.
    fn line(&mut self, a: (i32, i32), b: (i32, i32)) {
        let (mut x0, mut y0) = a;
        let (x1, y1) = b;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x0, y0);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn cell_char(&self, cx: i32, cy: i32) -> Option<char> {
        let bits = self.bits[(cy * self.cells_w + cx) as usize];
        if bits == 0 {
            None
        } else {
            char::from_u32(0x2800 + bits as u32)
        }
    }
}

/// Min/max over finite samples; `None` when nothing is drawable.
fn finite_range(series: &[Vec<f32>]) -> Option<(f32, f32)> {
    let mut range: Option<(f32, f32)> = None;
    for s in series {
        for v in s.iter().copied().filter(|v| v.is_finite()) {
            range = Some(match range {
                None => (v, v),
                Some((lo, hi)) => (lo.min(v), hi.max(v)),
            });
        }
    }
    range
}

/// Value -> dot row (0 = top). A flat range centers instead of dividing
/// by zero.
fn value_to_row(v: f32, lo: f32, hi: f32, rows: i32) -> i32 {
    let norm = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
    let r = ((1.0 - norm.clamp(0.0, 1.0)) * (rows - 1) as f32).round() as i32;
    r.clamp(0, rows - 1)
}

// ---------------------------------------------------------------------------
// Sparkline
// ---------------------------------------------------------------------------

/// One-row braille trend line.
pub struct Sparkline {
    data: Vec<f32>,
    slot: usize,
    range: Option<(f32, f32)>,
    time_axis: Option<std::time::Duration>,
    layout: Option<LayoutStyle>,
}

impl Sparkline {
    pub fn new(data: Vec<f32>) -> Sparkline {
        Sparkline {
            data,
            slot: 0,
            range: None,
            time_axis: None,
            layout: None,
        }
    }

    /// Chart-ramp slot for the line color (clamped like `TokenSet::chart`).
    pub fn slot(mut self, slot: usize) -> Sparkline {
        self.slot = slot;
        self
    }

    /// Fixed value range (default: the data's own min/max).
    pub fn range(mut self, lo: f32, hi: f32) -> Sparkline {
        self.range = Some((lo, hi));
        self
    }

    /// Opt-in relative time axis (backlog 0190): one label row under
    /// the trend line — "now" at the right edge, nice ticks leftward
    /// (`-30s`, `-1m`), density adapting to width. `span` is the time
    /// the samples cover ([`TimeSeries::span`] when fed from a ring).
    /// The default layout grows to two rows; a one-row rect degrades
    /// to the bare trend line.
    pub fn time_axis(mut self, span: std::time::Duration) -> Sparkline {
        self.time_axis = Some(span);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Sparkline {
        self.layout = Some(layout);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let color = t.chart(self.slot);
        let label_fg = t.text_faint;
        let data = self.data;
        let fixed = self.range;
        let span = self.time_axis;
        let default_h = if span.is_some() { 2 } else { 1 };
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .height(Dimension::Cells(default_h))
                .grow(1.0)
        });
        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 || data.is_empty() {
                return;
            }
            let series = [data.clone()];
            let (lo, hi) = match fixed.or_else(|| finite_range(&series)) {
                Some(r) => r,
                None => return,
            };
            let mut grid = BrailleGrid::new(rect.w, 1);
            let cols = grid.dots_w();
            let n = data.len() as i32;
            let mut prev: Option<(i32, i32)> = None;
            for c in 0..cols {
                let i = (c as i64 * n as i64 / cols as i64) as usize;
                let v = data[i.min(data.len() - 1)];
                if !v.is_finite() {
                    prev = None; // gap
                    continue;
                }
                let y = value_to_row(v, lo, hi, 4);
                match prev {
                    Some(p) => grid.line(p, (c, y)),
                    None => grid.set(c, y),
                }
                prev = Some((c, y));
            }
            for cx in 0..rect.w {
                if let Some(ch) = grid.cell_char(cx, 0) {
                    canvas.put(
                        Point::new(rect.x + cx, rect.y),
                        ch,
                        color,
                        Rgba::TRANSPARENT,
                    );
                }
            }
            // Label row below the trend, when opted in and there is room.
            if let Some(span) = span {
                if rect.h >= 2 {
                    time::draw_time_labels(
                        canvas,
                        crate::base::Rect::new(rect.x, rect.y + 1, rect.w, 1),
                        span,
                        label_fg,
                    );
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Line chart
// ---------------------------------------------------------------------------

/// Multi-series braille line chart with optional axes and range labels.
pub struct LineChart {
    series: Vec<Vec<f32>>,
    range: Option<(f32, f32)>,
    axes: bool,
    time_axis: Option<std::time::Duration>,
    layout: Option<LayoutStyle>,
}

impl LineChart {
    pub fn new(series: Vec<Vec<f32>>) -> LineChart {
        LineChart {
            series,
            range: None,
            axes: true,
            time_axis: None,
            layout: None,
        }
    }

    pub fn range(mut self, lo: f32, hi: f32) -> LineChart {
        self.range = Some((lo, hi));
        self
    }

    /// Axes + min/max labels (default on). Off = pure plot area.
    pub fn axes(mut self, on: bool) -> LineChart {
        self.axes = on;
        self
    }

    /// Opt-in relative time labels on the x-axis rule (backlog 0190):
    /// "now" anchored at the plot's right edge, nice ticks leftward
    /// (`-15s`, `-1m`), density adapting to width, embedded in the
    /// existing axis row (no extra height). `span` is the time the
    /// samples cover — [`TimeSeries::span`] when fed from a ring, so
    /// warmup labels the REAL span, never the target window. Requires
    /// `axes(true)` (there is no rule row without axes).
    pub fn time_axis(mut self, span: std::time::Duration) -> LineChart {
        self.time_axis = Some(span);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> LineChart {
        self.layout = Some(layout);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        // Series colors resolve up front: slot i for series i.
        let colors: Vec<Rgba> = (0..self.series.len().max(1)).map(|i| t.chart(i)).collect();
        let axis = t.border;
        let label_fg = t.text_faint;
        let series = self.series;
        let fixed = self.range;
        let axes = self.axes;
        let time_span = self.time_axis;
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let (lo, hi) = match fixed.or_else(|| finite_range(&series)) {
                Some(r) => r,
                None => return,
            };
            // Reserve a label gutter + axis column when axes are on and
            // there is room; degrade to the bare plot otherwise.
            let label_w = if axes { max_label_width(lo, hi) + 1 } else { 0 };
            let plot = if axes && rect.w > label_w + 2 && rect.h >= 2 {
                crate::base::Rect::new(
                    rect.x + label_w + 1,
                    rect.y,
                    rect.w - label_w - 1,
                    rect.h - 1,
                )
            } else {
                rect
            };
            let drew_axes = plot != rect;

            if drew_axes {
                for y in rect.y..plot.bottom() {
                    canvas.put(Point::new(plot.x - 1, y), '│', axis, Rgba::TRANSPARENT);
                }
                for x in plot.x - 1..rect.right() {
                    canvas.put(Point::new(x, plot.bottom()), '─', axis, Rgba::TRANSPARENT);
                }
                canvas.put(
                    Point::new(plot.x - 1, plot.bottom()),
                    '└',
                    axis,
                    Rgba::TRANSPARENT,
                );
                canvas.print(
                    Point::new(rect.x, rect.y),
                    &fmt_label(hi),
                    label_fg,
                    Rgba::TRANSPARENT,
                );
                canvas.print(
                    Point::new(rect.x, plot.bottom() - 1),
                    &fmt_label(lo),
                    label_fg,
                    Rgba::TRANSPARENT,
                );
                // Relative time ticks embed in the rule row (0190).
                if let Some(span) = time_span {
                    time::draw_time_labels(
                        canvas,
                        crate::base::Rect::new(plot.x, plot.bottom(), plot.w, 1),
                        span,
                        label_fg,
                    );
                }
            }

            // One dot grid per series so colors never merge in a cell:
            // later series win overlapping cells (documented z-order).
            for (si, data) in series.iter().enumerate() {
                if data.is_empty() {
                    continue;
                }
                let mut grid = BrailleGrid::new(plot.w, plot.h);
                let cols = grid.dots_w();
                let rows = grid.dots_h();
                let n = data.len() as i32;
                let mut prev: Option<(i32, i32)> = None;
                for c in 0..cols {
                    let i = (c as i64 * n as i64 / cols as i64) as usize;
                    let v = data[i.min(data.len() - 1)];
                    if !v.is_finite() {
                        prev = None;
                        continue;
                    }
                    let y = value_to_row(v, lo, hi, rows);
                    match prev {
                        Some(p) => grid.line(p, (c, y)),
                        None => grid.set(c, y),
                    }
                    prev = Some((c, y));
                }
                let color = colors[si.min(colors.len() - 1)];
                for cy in 0..plot.h {
                    for cx in 0..plot.w {
                        if let Some(ch) = grid.cell_char(cx, cy) {
                            canvas.put(
                                Point::new(plot.x + cx, plot.y + cy),
                                ch,
                                color,
                                Rgba::TRANSPARENT,
                            );
                        }
                    }
                }
            }
        })
    }
}

fn fmt_label(v: f32) -> String {
    if v.abs() >= 100.0 || v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn max_label_width(lo: f32, hi: f32) -> i32 {
    fmt_label(lo)
        .chars()
        .count()
        .max(fmt_label(hi).chars().count()) as i32
}

// ---------------------------------------------------------------------------
// Bar chart
// ---------------------------------------------------------------------------

/// Vertical bars with eighth-block sub-cell precision.
pub struct BarChart {
    values: Vec<f32>,
    range: Option<(f32, f32)>,
    /// Single ramp slot for all bars, or per-bar cycling when `None`.
    slot: Option<usize>,
    bar_w: i32,
    gap: i32,
    layout: Option<LayoutStyle>,
}

/// Vertical eighth blocks, index = eighths filled (1..=8).
const V_EIGHTHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl BarChart {
    pub fn new(values: Vec<f32>) -> BarChart {
        BarChart {
            values,
            range: None,
            slot: None,
            bar_w: 2,
            gap: 1,
            layout: None,
        }
    }

    /// One ramp slot for every bar (default: bar i cycles chart[i % 8]).
    pub fn slot(mut self, slot: usize) -> BarChart {
        self.slot = Some(slot);
        self
    }

    /// Value range (default 0..data-max — bars measure from zero).
    pub fn range(mut self, lo: f32, hi: f32) -> BarChart {
        self.range = Some((lo, hi));
        self
    }

    /// Bar width / gap in cells (clamped to >= 1 / >= 0).
    pub fn bar(mut self, width: i32, gap: i32) -> BarChart {
        self.bar_w = width.max(1);
        self.gap = gap.max(0);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> BarChart {
        self.layout = Some(layout);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let colors: Vec<Rgba> = match self.slot {
            Some(s) => vec![t.chart(s)],
            None => (0..8).map(|i| t.chart(i)).collect(),
        };
        let values = self.values;
        let fixed = self.range;
        let (bar_w, gap) = (self.bar_w, self.gap);
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 || values.is_empty() {
                return;
            }
            let hi = match fixed {
                Some((_, hi)) => hi,
                None => {
                    let m = values
                        .iter()
                        .copied()
                        .filter(|v| v.is_finite())
                        .fold(f32::MIN, f32::max);
                    if m == f32::MIN {
                        return;
                    }
                    m.max(f32::EPSILON)
                }
            };
            let lo = fixed.map(|(lo, _)| lo).unwrap_or(0.0);
            let span = (hi - lo).max(f32::EPSILON);

            let mut x = rect.x;
            for (i, v) in values.iter().enumerate() {
                if x >= rect.right() {
                    break;
                }
                if !v.is_finite() {
                    x += bar_w + gap;
                    continue;
                }
                let norm = ((v - lo) / span).clamp(0.0, 1.0);
                let eighths = (norm * (rect.h * 8) as f32).round() as i32;
                let color = colors[i % colors.len()];
                let (full, part) = (eighths / 8, eighths % 8);
                for col in 0..bar_w.min(rect.right() - x) {
                    for row in 0..full {
                        canvas.put(
                            Point::new(x + col, rect.bottom() - 1 - row),
                            '█',
                            color,
                            Rgba::TRANSPARENT,
                        );
                    }
                    if part > 0 && full < rect.h {
                        canvas.put(
                            Point::new(x + col, rect.bottom() - 1 - full),
                            V_EIGHTHS[(part - 1) as usize],
                            color,
                            Rgba::TRANSPARENT,
                        );
                    }
                }
                x += bar_w + gap;
            }
        })
    }
}

// Tests live in a sibling file to keep this one within the size
// budget (deterministic fixed-series pins).
#[cfg(test)]
#[path = "chart_tests.rs"]
mod tests;
