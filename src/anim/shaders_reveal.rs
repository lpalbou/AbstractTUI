//! Reveal-family shaders (split from `shaders.rs`, file-size budget):
//! effects whose job is SHOWING/HIDING content — a not-yet-revealed cell
//! contributes nothing (fully transparent) and the glyph pops at half
//! coverage. Public paths are unchanged (`anim::shaders::ScanlineFade`
//! etc. re-export from the parent). Same determinism contract as every
//! built-in: pure in `(x, y, t, cell)`, no libm.

use crate::base::{Rect, Rgba};
use crate::render::cell::Cell;
use crate::render::layer::CellShader;

use super::{cell_hash, STABLE, TRANSPARENT_CELL};

// ---------------------------------------------------------------------------
// ScanlineFade
// ---------------------------------------------------------------------------

/// Top-down reveal: a sweep line moves down `rows` over `duration`
/// seconds. Rows above the line show normally; rows below contribute
/// nothing (fully transparent — lower layers show); the row under the
/// line blends in (background alpha scales with the fractional coverage,
/// the glyph pops once the row is half-covered). Drive `t` backwards for
/// a bottom-up hide.
#[derive(Copy, Clone, Debug)]
pub struct ScanlineFade {
    /// Seconds for the sweep line to cross `rows`.
    pub duration: f32,
    /// The layer height being revealed (the shader has no layer handle;
    /// the caller states the sweep range).
    pub rows: i32,
}

impl CellShader for ScanlineFade {
    fn shade(&self, _x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        if self.duration <= 0.0 {
            return cell; // degenerate config: instant reveal, never divide by 0
        }
        let progress = (t / self.duration).clamp(0.0, 1.0);
        let line = progress * self.rows as f32;
        let coverage = (line - y as f32).clamp(0.0, 1.0);
        if coverage >= 1.0 {
            return cell;
        }
        if coverage <= 0.0 {
            return TRANSPARENT_CELL;
        }
        let mut c = cell;
        c.bg = c.bg.with_alpha((c.bg.a as f32 * coverage).round() as u8);
        if coverage < 0.5 {
            // Ground fades in first; the glyph pops at half coverage.
            c.glyph = crate::render::cell::Glyph::EMPTY;
            c.fg = Rgba::TRANSPARENT;
            c.ul = Rgba::TRANSPARENT;
            c.link = 0;
        }
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, bounds: Rect) -> Option<Rect> {
        if self.duration <= 0.0 {
            return STABLE; // degenerate: shade is identity at every t
        }
        let line = |t: f32| (t / self.duration).clamp(0.0, 1.0) * self.rows as f32;
        let (l0, l1) = (line(t0), line(t1));
        if l0 == l1 {
            return STABLE; // settled (both clamped) or same clock
        }
        // Rows fully shown at BOTH clocks (coverage pinned 1: y ≤ lo−1)
        // and rows fully hidden at both (coverage pinned 0: y ≥ hi) are
        // stable; the moving band between them is the change. `y` is
        // frame-space in `shade`, so the band is absolute rows.
        let (lo, hi) = (l0.min(l1), l0.max(l1));
        let y0 = (lo - 1.0).floor() as i32 + 1; // first y with y > lo−1
        let y1 = hi.ceil() as i32 - 1; //          last y with y < hi
        Some(Rect::new(bounds.x, y0, bounds.w, (y1 - y0 + 1).max(0)))
    }
}

// ---------------------------------------------------------------------------
// GradientReveal
// ---------------------------------------------------------------------------

/// Directional wipe with a soft edge: cells behind the moving front show,
/// cells ahead contribute nothing, and a `softness`-cell band in between
/// fades (ground alpha ramps; the glyph pops at half coverage — same
/// pop rule as [`ScanlineFade`], which this generalizes). Drive `t`
/// 0 -> duration to reveal; backwards to hide.
#[derive(Copy, Clone, Debug)]
pub struct GradientReveal {
    /// Seconds for the front to travel the full range.
    pub duration: f32,
    /// Wipe direction (cells; aspect-uncorrected on purpose — wipes read
    /// as row/column motions): e.g. (1,0) left-to-right, (0,-1) bottom-up,
    /// (1,1) diagonal.
    pub dir: (f32, f32),
    /// Total travel in projected cells (set to the layer's extent along
    /// `dir`).
    pub travel: f32,
    /// Soft edge width in projected cells.
    pub softness: f32,
}

impl CellShader for GradientReveal {
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        if self.duration <= 0.0 {
            return cell;
        }
        let len = (self.dir.0 * self.dir.0 + self.dir.1 * self.dir.1).sqrt();
        if len < 1e-6 {
            return cell;
        }
        let (dx, dy) = (self.dir.0 / len, self.dir.1 / len);
        let progress = (t / self.duration).clamp(0.0, 1.0);
        let soft = self.softness.max(0.0);
        // Front sweeps from -soft (nothing shown) to travel+soft (all).
        let front = -soft + progress * (self.travel + 2.0 * soft);
        let p = x as f32 * dx + y as f32 * dy;
        let coverage = ((front - p) / soft.max(1e-6)).clamp(0.0, 1.0);
        if coverage >= 1.0 {
            return cell;
        }
        if coverage <= 0.0 {
            return TRANSPARENT_CELL;
        }
        let mut c = cell;
        c.bg = c.bg.with_alpha((c.bg.a as f32 * coverage).round() as u8);
        if coverage < 0.5 {
            c.glyph = crate::render::cell::Glyph::EMPTY;
            c.fg = Rgba::TRANSPARENT;
            c.ul = Rgba::TRANSPARENT;
            c.link = 0;
        }
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, bounds: Rect) -> Option<Rect> {
        if self.duration <= 0.0 {
            return STABLE; // degenerate: identity at every t
        }
        let len = (self.dir.0 * self.dir.0 + self.dir.1 * self.dir.1).sqrt();
        if len < 1e-6 {
            return STABLE;
        }
        let soft = self.softness.max(0.0);
        let front = |t: f32| {
            let progress = (t / self.duration).clamp(0.0, 1.0);
            -soft + progress * (self.travel + 2.0 * soft)
        };
        let (f0, f1) = (front(t0), front(t1));
        if f0 == f1 {
            return STABLE; // settled — a completed wipe ticks for free
        }
        // Changed cells project into (min front − soft edge, max front)
        // along dir. Axis wipes (the common case) bound to a clean slab;
        // diagonal wipes fall back to "whole layer" honestly rather than
        // approximating a rotated slab with a giant rect.
        let (dx, dy) = (self.dir.0 / len, self.dir.1 / len);
        let lo = f0.min(f1) - soft.max(1e-6);
        let hi = f0.max(f1);
        if dy == 0.0 {
            // p = x * dx̂ with |dx̂| = 1: solve the slab in x.
            let (a, b) = if dx > 0.0 { (lo, hi) } else { (-hi, -lo) };
            let x0 = a.floor() as i32;
            let x1 = b.ceil() as i32;
            return Some(Rect::new(x0, bounds.y, (x1 - x0 + 1).max(0), bounds.h));
        }
        if dx == 0.0 {
            let (a, b) = if dy > 0.0 { (lo, hi) } else { (-hi, -lo) };
            let y0 = a.floor() as i32;
            let y1 = b.ceil() as i32;
            return Some(Rect::new(bounds.x, y0, bounds.w, (y1 - y0 + 1).max(0)));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Dissolve
// ---------------------------------------------------------------------------

/// Per-cell hash-threshold dissolve for enter/exit transitions: each cell
/// has a fixed random threshold; cells whose threshold is below the
/// progress `t / duration` are shown, the rest contribute nothing. Drive
/// `t` 0 -> duration to materialize, duration -> 0 to dissolve away.
#[derive(Copy, Clone, Debug)]
pub struct Dissolve {
    /// Seconds from fully hidden to fully shown.
    pub duration: f32,
    /// Seed for the mask — two dissolving layers with different seeds
    /// never share a pattern.
    pub seed: u32,
}

impl CellShader for Dissolve {
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        if self.duration <= 0.0 {
            return cell; // degenerate config: fully materialized
        }
        let progress = (t / self.duration).clamp(0.0, 1.0);
        if cell_hash(x, y, self.seed) < progress {
            cell
        } else {
            TRANSPARENT_CELL
        }
    }

    fn changed_region(&self, t0: f32, t1: f32, _bounds: Rect) -> Option<Rect> {
        if self.duration <= 0.0 {
            return STABLE;
        }
        let p = |t: f32| (t / self.duration).clamp(0.0, 1.0);
        // Equal progress (incl. both clamped past an end) = same mask.
        // Mid-flight the flipped cells are hash-scattered — no rect can
        // honestly bound them below the whole layer.
        if p(t0) == p(t1) {
            STABLE
        } else {
            None
        }
    }
}
