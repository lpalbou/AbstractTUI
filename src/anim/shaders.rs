//! Built-in cell shaders: the effects library over `render::CellShader`.
//!
//! Lives in `anim` (not `render`): shaders are time-driven effects — the
//! render layer defines the hook, the animation layer ships the motion.
//! `anim` sits above `render` in the layer map, so the import arrow is
//! legal and one-way.
//!
//! Determinism contract (REDTEAM golden-tests these): every shader is a
//! pure function of `(x, y, t, cell)` and its construction parameters.
//! No transcendentals — the wave is a triangle (float add/mul/floor are
//! IEEE-exact, `sin` varies by libm), randomness is a seeded integer
//! hash of the cell position. Identical inputs produce identical bytes on
//! every platform.
//!
//! Color-model note shared by all built-ins: `Rgba` alpha 0 means
//! "terminal default color" (unknown RGB), so shaders never invent a
//! grade for it — default-colored slots pass through untouched, exactly
//! like `render::ColorTransform`.

use crate::base::{Rect, Rgba};
use crate::render::cell::Cell;
use crate::render::layer::CellShader;

/// The "this tick changes nothing" hint value (see
/// [`CellShader::changed_region`]): a settled or `t`-independent shader
/// returns this and the clock advance costs zero repaint.
pub(super) const STABLE: Option<Rect> = Some(Rect::ZERO);

/// Triangle wave, period 1, range [-1, 1], peak at phase 0.
/// (0 -> 1, 0.25 -> 0, 0.5 -> -1, 0.75 -> 0.)
fn wave(phase: f32) -> f32 {
    let frac = phase - phase.floor();
    (frac - 0.5).abs() * 4.0 - 1.0
}

/// Deterministic per-cell hash in [0, 1): an integer avalanche over the
/// position and seed — uniform enough for dissolve masks, exact on every
/// platform.
pub(super) fn cell_hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9)
        ^ (y as u32).wrapping_mul(0x85EB_CA6B)
        ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    // 24 mantissa-exact bits.
    (h >> 8) as f32 / 16_777_216.0
}

/// Scale a color's channels by `f` (≥ 0; values > 1 brighten, saturating).
/// Alpha untouched; default-colored (alpha 0) slots pass through.
fn scale_channels(c: Rgba, f: f32) -> Rgba {
    if c.is_transparent() {
        return c;
    }
    let s = |v: u8| ((v as f32 * f).round() as i64).clamp(0, 255) as u8;
    Rgba::new(s(c.r), s(c.g), s(c.b), c.a)
}

/// The fully see-through cell: contributes nothing during compositing
/// (lower layers show through) — the "not here yet / gone" state used by
/// reveal and dissolve shaders.
pub(super) const TRANSPARENT_CELL: Cell = Cell::EMPTY;

// ---------------------------------------------------------------------------
// Shimmer
// ---------------------------------------------------------------------------

/// Luminance ripple: a diagonal brightness wave rolls across the layer's
/// INK (fg + underline color; backgrounds stay still so panels don't
/// strobe). `speed` in wave cycles per second, `amplitude` as a fraction
/// of brightness (0.15 = ±15%).
#[derive(Copy, Clone, Debug)]
pub struct Shimmer {
    /// Wave cycles per second.
    pub speed: f32,
    /// Brightness swing as a fraction (0.18 = ±18%).
    pub amplitude: f32,
    /// Cells per wave period along the x+y diagonal.
    pub wavelength: f32,
}

impl Default for Shimmer {
    fn default() -> Self {
        Shimmer {
            speed: 0.8,
            amplitude: 0.18,
            wavelength: 14.0,
        }
    }
}

impl CellShader for Shimmer {
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        let wavelength = if self.wavelength.abs() < 1e-3 {
            1.0
        } else {
            self.wavelength
        };
        let phase = t * self.speed - (x + y) as f32 / wavelength;
        let f = 1.0 + self.amplitude * wave(phase);
        let mut c = cell;
        c.fg = scale_channels(c.fg, f);
        c.ul = scale_channels(c.ul, f);
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, _bounds: Rect) -> Option<Rect> {
        // The wave translates with t across the whole layer: any phase
        // movement changes (almost) every ink cell — the honest hint is
        // "everything" unless the field is provably t-independent. Only
        // exact phase equality is bit-safe (integral phase deltas round
        // differently per cell).
        if self.amplitude == 0.0 || t0 * self.speed == t1 * self.speed {
            STABLE
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// HueDrift
// ---------------------------------------------------------------------------

/// Subtle accent pulse (focus rings, active borders): the ink drifts
/// toward its channel-rotated companion color and back. Channel rotation
/// `(r,g,b) -> (g,b,r)` shifts hue without HSV math — deterministic and
/// cheap; at low `strength` it reads as a living tint, not a color flip.
#[derive(Copy, Clone, Debug)]
pub struct HueDrift {
    /// Pulse cycles per second.
    pub speed: f32,
    /// 0..=1: how far toward the rotated color at the pulse peak.
    pub strength: f32,
}

impl Default for HueDrift {
    fn default() -> Self {
        HueDrift {
            speed: 0.5,
            strength: 0.35,
        }
    }
}

impl CellShader for HueDrift {
    fn shade(&self, _x: i32, _y: i32, t: f32, cell: Cell) -> Cell {
        let pulse = (wave(t * self.speed) + 1.0) * 0.5; // [0, 1]
        let k = (self.strength.clamp(0.0, 1.0)) * pulse;
        let mut c = cell;
        c.fg = drift(c.fg, k);
        c.ul = drift(c.ul, k);
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, _bounds: Rect) -> Option<Rect> {
        // Spatially uniform: output depends on t only through the pulse.
        // Equal pulses (same wave sample, or strength 0 — lerp at k=0 is
        // bit-identity) mean an identical field at both clocks.
        if self.strength <= 0.0 || wave(t0 * self.speed) == wave(t1 * self.speed) {
            STABLE
        } else {
            None
        }
    }
}

fn drift(c: Rgba, k: f32) -> Rgba {
    if c.is_transparent() {
        return c;
    }
    let rotated = Rgba::new(c.g, c.b, c.r, c.a);
    c.lerp(rotated, k)
}

// ---------------------------------------------------------------------------
// Pulse
// ---------------------------------------------------------------------------

/// Attention pulse: the whole layer's INK breathes brighter and back,
/// spatially uniform (unlike [`Shimmer`]'s traveling wave). For "look
/// here" moments — validation errors, an armed destructive button.
#[derive(Copy, Clone, Debug)]
pub struct Pulse {
    /// Pulses per second.
    pub speed: f32,
    /// Peak brightness gain (0.3 = +30% at the top of the pulse).
    pub amplitude: f32,
}

impl Default for Pulse {
    fn default() -> Self {
        Pulse {
            speed: 1.2,
            amplitude: 0.3,
        }
    }
}

impl CellShader for Pulse {
    fn shade(&self, _x: i32, _y: i32, t: f32, cell: Cell) -> Cell {
        // Triangle rise/fall in [0, 1]: calm at phase 0 (a layer that
        // stops advancing t rests at its normal look).
        let k = (wave(t * self.speed + 0.5) + 1.0) * 0.5;
        let f = 1.0 + self.amplitude * k;
        let mut c = cell;
        c.fg = scale_channels(c.fg, f);
        c.ul = scale_channels(c.ul, f);
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, _bounds: Rect) -> Option<Rect> {
        // Uniform brightness pulse: equal wave samples = identical field.
        if self.amplitude == 0.0 || wave(t0 * self.speed + 0.5) == wave(t1 * self.speed + 0.5) {
            STABLE
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Sweep
// ---------------------------------------------------------------------------

/// A diagonal highlight band sweeping across the layer once per `period`
/// seconds — the "shine" pass over a selection or a freshly-arrived row.
/// Cells within the band brighten by up to `boost`, feathered linearly to
/// the band edges.
#[derive(Copy, Clone, Debug)]
pub struct Sweep {
    /// Seconds per full crossing (the band re-enters each period).
    pub period: f32,
    /// Band width in columns.
    pub band: f32,
    /// Peak brightness gain at the band center.
    pub boost: f32,
    /// Columns the sweep travels per period — set to layer width + slack;
    /// the shader has no layer handle, so the caller states the range.
    pub travel: f32,
}

impl Default for Sweep {
    fn default() -> Self {
        Sweep {
            period: 1.6,
            band: 6.0,
            boost: 0.5,
            travel: 90.0,
        }
    }
}

impl CellShader for Sweep {
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        if self.period <= 0.0 || self.band <= 0.0 {
            return cell;
        }
        let phase = t / self.period;
        let center = (phase - phase.floor()) * self.travel;
        // Diagonal: y leans the band like a light source upper-left.
        let d = ((x + y) as f32 - center).abs();
        let half = self.band * 0.5;
        if d >= half {
            return cell;
        }
        let k = 1.0 - d / half; // 1 at center, 0 at edge
        let f = 1.0 + self.boost * k;
        let mut c = cell;
        c.fg = scale_channels(c.fg, f);
        c.bg = scale_channels(c.bg, f);
        c.ul = scale_channels(c.ul, f);
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, bounds: Rect) -> Option<Rect> {
        if self.period <= 0.0 || self.band <= 0.0 {
            return STABLE; // degenerate: shade is identity at every t
        }
        let center = |t: f32| {
            let phase = t / self.period;
            (phase - phase.floor()) * self.travel
        };
        let (c0, c1) = (center(t0), center(t1));
        if c0 == c1 {
            return STABLE;
        }
        // Cells outside BOTH band positions are identity at both clocks.
        // The union of the two diagonal slabs (x+y in the swept interval)
        // is not a rect; its bounding rect over the layer's rows is the
        // conservative hint. A period wrap makes the interval span most
        // of the travel — still a correct superset, just a weaker win.
        let half = self.band * 0.5;
        let lo = c0.min(c1) - half;
        let hi = c0.max(c1) + half;
        let y1 = bounds.bottom() - 1;
        let x_lo = (lo - y1 as f32).floor() as i32;
        let x_hi = (hi - bounds.y as f32).ceil() as i32;
        Some(Rect::new(
            x_lo,
            bounds.y,
            (x_hi - x_lo + 1).max(0),
            bounds.h,
        ))
    }
}

// ---------------------------------------------------------------------------
// Rainbow
// ---------------------------------------------------------------------------

/// Debug/fun ink cycling: position + time walk a 6-segment hue wheel
/// (R→Y→G→C→B→M→R, piecewise-linear — no HSV/trig, bit-stable). The
/// original ink's luminance is preserved by scaling the wheel color to
/// the ink's channel mean, so dark text stays dark while cycling.
#[derive(Copy, Clone, Debug)]
pub struct Rainbow {
    /// Hue cycles per second.
    pub speed: f32,
    /// Cells per full hue cycle along x+y.
    pub wavelength: f32,
    /// 0..=1: how far ink moves toward the wheel color (1 = full replace).
    pub strength: f32,
}

impl Default for Rainbow {
    fn default() -> Self {
        Rainbow {
            speed: 0.4,
            wavelength: 24.0,
            strength: 1.0,
        }
    }
}

impl CellShader for Rainbow {
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell {
        let wavelength = if self.wavelength.abs() < 1e-3 {
            1.0
        } else {
            self.wavelength
        };
        let phase = t * self.speed + (x + y) as f32 / wavelength;
        let hue = phase - phase.floor();
        let mut c = cell;
        c.fg = toward_wheel(c.fg, hue, self.strength);
        c.ul = toward_wheel(c.ul, hue, self.strength);
        c
    }

    fn changed_region(&self, t0: f32, t1: f32, _bounds: Rect) -> Option<Rect> {
        // Position + time walk one phase: exact time-term equality is the
        // only bit-safe stability (strength 0 = lerp identity).
        if self.strength <= 0.0 || t0 * self.speed == t1 * self.speed {
            STABLE
        } else {
            None
        }
    }
}

/// Piecewise-linear hue wheel at full saturation/value.
fn hue_wheel(h: f32) -> (f32, f32, f32) {
    let h6 = (h - h.floor()) * 6.0;
    let seg = h6 as i32 % 6;
    let f = h6 - h6.floor();
    match seg {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    }
}

fn toward_wheel(c: Rgba, hue: f32, strength: f32) -> Rgba {
    if c.is_transparent() {
        return c;
    }
    // Scale the wheel color to the ink's brightness (channel mean) so the
    // cycle changes HUE, not legibility.
    let level = (c.r as f32 + c.g as f32 + c.b as f32) / 3.0;
    let (r, g, b) = hue_wheel(hue);
    let wheel = Rgba::new(
        (r * level).round() as u8,
        (g * level).round() as u8,
        (b * level).round() as u8,
        c.a,
    );
    c.lerp(wheel, strength.clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// Vignette
// ---------------------------------------------------------------------------

/// Radial dim toward the edges — focus framing for modals/splash. Static
/// unless `t` is driven (breathing vignette = animate strength via a
/// wrapping shader or just re-set params). Distance is aspect-corrected
/// (cells ~1:2) so the vignette LOOKS circular.
#[derive(Copy, Clone, Debug)]
pub struct Vignette {
    /// Layer size in cells (the shader has no layer handle; the caller
    /// states its frame, like `ScanlineFade::rows`).
    pub size: (i32, i32),
    /// 0..=1 fraction of the radius that stays fully lit.
    pub inner: f32,
    /// Peak dim at the far corner (0.6 = down to 40% brightness).
    pub strength: f32,
}

impl CellShader for Vignette {
    // `shade` ignores `t` entirely: a vignette layer's clock can tick
    // forever for free (the flagship changed_region win — a breathing
    // UI that also carries a vignette must not repaint the vignette).
    fn changed_region(&self, _t0: f32, _t1: f32, _bounds: Rect) -> Option<Rect> {
        STABLE
    }

    fn shade(&self, x: i32, y: i32, _t: f32, cell: Cell) -> Cell {
        let (w, h) = (self.size.0.max(1) as f32, self.size.1.max(1) as f32 * 2.0);
        let px = x as f32 + 0.5;
        let py = (y as f32 + 0.5) * 2.0;
        let (cx, cy) = (w * 0.5, h * 0.5);
        let far = (cx * cx + cy * cy).sqrt().max(1e-6);
        let d = (((px - cx).powi(2) + (py - cy).powi(2)).sqrt() / far).clamp(0.0, 1.0);
        let inner = self.inner.clamp(0.0, 0.99);
        if d <= inner {
            return cell;
        }
        // Smoothstep from the inner radius to the corner.
        let k = ((d - inner) / (1.0 - inner)).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let f = 1.0 - self.strength.clamp(0.0, 1.0) * k;
        let mut c = cell;
        c.fg = scale_channels(c.fg, f);
        c.bg = scale_channels(c.bg, f);
        c.ul = scale_channels(c.ul, f);
        c
    }
}

#[path = "shaders_reveal.rs"]
mod shaders_reveal;
pub use shaders_reveal::{Dissolve, GradientReveal, ScanlineFade};

#[cfg(test)]
#[path = "shaders_tests.rs"]
mod tests;
