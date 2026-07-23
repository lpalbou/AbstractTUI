//! Sub-cell vector canvas: dots, strokes, curves and partial fills
//! (backlog 0420 — the substrate charts already used privately,
//! promoted to public API for diagram-class extensions and app-side
//! custom traces).
//!
//! ## The dot-space model
//!
//! A [`DotCanvas`] covers a cell rectangle with a finer dot grid: in
//! [`DotMode::Braille`] every cell holds 2x4 dots (a `WxH`-cell panel
//! is a `2Wx4H` dot canvas), in [`DotMode::Quadrant`] 2x2 (universal
//! glyph coverage — braille glyphs can be missing or ugly in some
//! fonts; quadrant is the degradation, the same rationale as the
//! mosaic auto-pick). Dot (0, 0) is the top-left dot; x grows right,
//! y grows down, matching cell coordinates. All stroke primitives
//! clip at the grid edge (out-of-range dots are dropped, never
//! panic), and non-finite curve inputs draw nothing — the same data
//! contract the chart widgets keep for samples.
//!
//! ## The cell-color rule (documented z-order)
//!
//! A terminal cell carries ONE foreground color and ONE glyph, so a
//! grid blits with a single stroke color: [`DotCanvas::blit`] takes
//! one `Rgba` for every lit cell and skips empty cells (they stay
//! transparent). Multi-color pictures are therefore multiple grids
//! blitted in back-to-front order: later blits win overlapping cells
//! — glyph and color both — dots from different grids never merge
//! into one glyph. This is the contract the line chart has always
//! shipped ("one dot grid per series so colors never merge in a
//! cell"); the canvas layer carries it explicitly instead of working
//! around it silently.
//!
//! Blitting goes through [`crate::ui::Canvas`]/[`crate::ui::StyledCanvas`],
//! so clipping ([`crate::ui::ClippedCanvas`]) and damage tracking
//! compose for free: a blit into a damaged region repaints only that
//! region.
//!
//! Colors are caller-resolved `Rgba` (the widget token rule): resolve
//! from the theme's tokens — e.g. `TokenSet::chart(i)` — and pass the
//! resolved value; this module invents no colors.
//!
//! ## A custom trace in ten lines
//!
//! ```
//! use abstracttui::base::{Point, Size};
//! use abstracttui::canvas::DotCanvas;
//! use abstracttui::theme::default_theme;
//! use abstracttui::ui::BufferCanvas;
//!
//! let ink = default_theme().tokens.chart(0); // caller-resolved color
//! let mut dots = DotCanvas::braille(10, 3);  // 10x3 cells = 20x12 dots
//! dots.line((0, 11), (6, 2));
//! dots.bezier_quad((6.0, 2.0), (12.0, 14.0), (19.0, 3.0), 0.25);
//! let mut out = BufferCanvas::new(Size::new(10, 3));
//! dots.blit(&mut out, Point::new(0, 0), ink);
//! assert!(!out.row_text(1).trim().is_empty(), "the trace drew cells");
//! ```
//!
//! Inside a widget or app view the same calls run in an
//! `Element::draw` closure against the frame's canvas; build the
//! `DotCanvas` from the solved `rect` (`rect.w`, `rect.h` cells) and
//! blit at `rect` origin.
//!
//! OWNER: CANVAS (extensions wave).

mod curves;
mod fill;
mod glyphs;

pub use fill::{fill_h, fill_v};
pub use glyphs::{braille_bit, H_EIGHTHS, QUADRANT_CHARS, V_EIGHTHS};

use crate::base::{Point, Rgba};
use crate::ui::{Canvas, StyledCanvas};

/// Dot resolution per cell. The engine may grow this vocabulary
/// (sextant mode is a known candidate, opt-in once a consumer proves
/// the font risk), hence `#[non_exhaustive]` per ADR-0003 §3.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DotMode {
    /// 2x4 dots per cell via U+2800.. braille patterns — the highest
    /// sub-cell resolution with per-dot addressing.
    Braille,
    /// 2x2 dots per cell via quadrant blocks — universal glyph
    /// coverage where braille rendering is unreliable.
    Quadrant,
}

impl DotMode {
    /// Dot rows per cell (columns are 2 in every mode).
    const fn dot_rows(self) -> i32 {
        match self {
            DotMode::Braille => 4,
            DotMode::Quadrant => 2,
        }
    }
}

/// A dot grid over a cell rectangle. Accumulates lit dots per cell,
/// then emits one glyph per non-empty cell (see the module docs for
/// the dot-space model and the cell-color rule).
pub struct DotCanvas {
    mode: DotMode,
    cells_w: i32,
    cells_h: i32,
    bits: Vec<u8>,
}

impl DotCanvas {
    /// A grid over `cells_w x cells_h` cells (negative sizes clamp to
    /// zero — an empty grid accepts every call and draws nothing).
    pub fn new(mode: DotMode, cells_w: i32, cells_h: i32) -> DotCanvas {
        let cells_w = cells_w.max(0);
        let cells_h = cells_h.max(0);
        DotCanvas {
            mode,
            cells_w,
            cells_h,
            bits: vec![0; (cells_w * cells_h) as usize],
        }
    }

    /// Braille-mode convenience (`2 x cells_w` by `4 x cells_h` dots).
    pub fn braille(cells_w: i32, cells_h: i32) -> DotCanvas {
        DotCanvas::new(DotMode::Braille, cells_w, cells_h)
    }

    /// Quadrant-mode convenience (`2 x cells_w` by `2 x cells_h` dots).
    pub fn quadrant(cells_w: i32, cells_h: i32) -> DotCanvas {
        DotCanvas::new(DotMode::Quadrant, cells_w, cells_h)
    }

    pub fn mode(&self) -> DotMode {
        self.mode
    }

    pub fn cells_w(&self) -> i32 {
        self.cells_w
    }

    pub fn cells_h(&self) -> i32 {
        self.cells_h
    }

    /// Grid width in dots.
    pub fn dots_w(&self) -> i32 {
        self.cells_w * 2
    }

    /// Grid height in dots.
    pub fn dots_h(&self) -> i32 {
        self.cells_h * self.mode.dot_rows()
    }

    fn dot_bit(&self, col: i32, row: i32) -> u8 {
        match self.mode {
            DotMode::Braille => glyphs::braille_bit(col, row),
            DotMode::Quadrant => 1u8 << (row * 2 + col),
        }
    }

    /// Cell index + bit for an in-range dot.
    fn slot(&self, x: i32, y: i32) -> Option<(usize, u8)> {
        if x < 0 || y < 0 || x >= self.dots_w() || y >= self.dots_h() {
            return None;
        }
        let rows = self.mode.dot_rows();
        let idx = ((y / rows) * self.cells_w + x / 2) as usize;
        Some((idx, self.dot_bit(x % 2, y % rows)))
    }

    /// Light a dot. Out-of-range dots are clipped (no panic).
    pub fn set(&mut self, x: i32, y: i32) {
        if let Some((idx, bit)) = self.slot(x, y) {
            self.bits[idx] |= bit;
        }
    }

    /// Unlight a dot. Out-of-range dots are clipped (no panic).
    pub fn clear(&mut self, x: i32, y: i32) {
        if let Some((idx, bit)) = self.slot(x, y) {
            self.bits[idx] &= !bit;
        }
    }

    /// Whether a dot is lit (`false` out of range).
    pub fn get(&self, x: i32, y: i32) -> bool {
        match self.slot(x, y) {
            Some((idx, bit)) => self.bits[idx] & bit != 0,
            None => false,
        }
    }

    /// Unlight every dot, keeping the allocation — the reuse path for
    /// per-frame redraws (the stroke + blit steady state allocates
    /// nothing, pinned in `tests/alloc_budget.rs`).
    pub fn clear_all(&mut self) {
        self.bits.fill(0);
    }

    /// Bresenham segment on the dot grid (endpoints included).
    ///
    /// Off-grid geometry is handled robustly: the segment is first
    /// clipped parametrically to the grid box (inflated by one dot),
    /// so a segment with far-away endpoints costs O(grid), never
    /// O(segment length) — and coordinate deltas can never overflow.
    /// Segments whose endpoints are already in the box walk EXACTLY
    /// as the unclipped algorithm (the chart byte-identity gate);
    /// for clipped segments the visible dots approximate the ideal
    /// line and may differ from an unclipped walk by one dot at the
    /// boundary (deterministic either way).
    pub fn line(&mut self, a: (i32, i32), b: (i32, i32)) {
        let Some((a, b)) = self.clip_segment(a, b) else {
            return;
        };
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

    /// Liang-Barsky clip of `a -> b` to the grid box inflated by one
    /// dot. `None` = no visible portion. In-box segments pass through
    /// untouched (the exact-raster fast path).
    fn clip_segment(&self, a: (i32, i32), b: (i32, i32)) -> Option<((i32, i32), (i32, i32))> {
        let (hi_x, hi_y) = (self.dots_w(), self.dots_h());
        let in_box = |p: (i32, i32)| p.0 >= -1 && p.1 >= -1 && p.0 <= hi_x && p.1 <= hi_y;
        if in_box(a) && in_box(b) {
            return Some((a, b));
        }
        let (x0, y0) = (f64::from(a.0), f64::from(a.1));
        let (dx, dy) = (f64::from(b.0) - x0, f64::from(b.1) - y0);
        let mut t0 = 0.0f64;
        let mut t1 = 1.0f64;
        for (p, q) in [
            (-dx, x0 + 1.0),            // x >= -1
            (dx, f64::from(hi_x) - x0), // x <= dots_w
            (-dy, y0 + 1.0),            // y >= -1
            (dy, f64::from(hi_y) - y0), // y <= dots_h
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None; // parallel to and outside this edge
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    if r > t1 {
                        return None;
                    }
                    if r > t0 {
                        t0 = r;
                    }
                } else {
                    if r < t0 {
                        return None;
                    }
                    if r < t1 {
                        t1 = r;
                    }
                }
            }
        }
        let at = |t: f64| ((x0 + t * dx).round() as i32, (y0 + t * dy).round() as i32);
        Some((at(t0), at(t1)))
    }

    /// Connected segments through every point in order. A single
    /// point lights one dot; an empty slice draws nothing.
    pub fn polyline(&mut self, points: &[(i32, i32)]) {
        match points {
            [] => {}
            [p] => self.set(p.0, p.1),
            _ => {
                for w in points.windows(2) {
                    self.line(w[0], w[1]);
                }
            }
        }
    }

    /// The glyph for a cell: `None` when the cell is empty or out of
    /// range (empty cells stay transparent under blit — the z-order
    /// contract in the module docs).
    pub fn cell_char(&self, cx: i32, cy: i32) -> Option<char> {
        if cx < 0 || cy < 0 || cx >= self.cells_w || cy >= self.cells_h {
            return None;
        }
        let bits = self.bits[(cy * self.cells_w + cx) as usize];
        if bits == 0 {
            return None;
        }
        match self.mode {
            DotMode::Braille => char::from_u32(0x2800 + bits as u32),
            DotMode::Quadrant => Some(glyphs::QUADRANT_CHARS[bits as usize]),
        }
    }

    /// Blit every non-empty cell at `origin` with ONE stroke color
    /// (foreground only; the cell background is left as-is via the
    /// alpha-0 convention). Empty cells are skipped, so overlapping
    /// grids compose at cell granularity: later blits win (see the
    /// cell-color rule in the module docs).
    pub fn blit<C: Canvas + ?Sized>(&self, canvas: &mut C, origin: Point, color: Rgba) {
        for cy in 0..self.cells_h {
            for cx in 0..self.cells_w {
                if let Some(ch) = self.cell_char(cx, cy) {
                    canvas.put(
                        Point::new(origin.x + cx, origin.y + cy),
                        ch,
                        color,
                        Rgba::TRANSPARENT,
                    );
                }
            }
        }
    }

    /// [`DotCanvas::blit`] with a full [`crate::render::Style`] patch
    /// instead of a bare color, so a stroke can carry attributes and
    /// a link id (`Style::link`) — the path diagram extensions use to
    /// make edges activatable.
    pub fn blit_styled<C: StyledCanvas + ?Sized>(
        &self,
        canvas: &mut C,
        origin: Point,
        style: &crate::render::Style,
    ) {
        let mut buf = [0u8; 4];
        for cy in 0..self.cells_h {
            for cx in 0..self.cells_w {
                if let Some(ch) = self.cell_char(cx, cy) {
                    let s: &str = ch.encode_utf8(&mut buf);
                    canvas.print_styled(Point::new(origin.x + cx, origin.y + cy), s, style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
