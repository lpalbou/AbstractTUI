//! Curve strokes for [`DotCanvas`]: quadratic/cubic beziers via
//! adaptive flattening, ellipse arcs via parameter stepping.
//!
//! Determinism contract (test-pinned): same inputs, same dots, on
//! every platform. Flattening is pure IEEE-754 arithmetic in a fixed
//! order; arcs use an in-crate polynomial sin/cos instead of the
//! platform libm (whose last-ulp differences could flip a rounded dot
//! on one CI OS and not another). Non-finite inputs draw nothing —
//! the same skip contract the chart widgets keep for samples.
//!
//! Bounded flattening (documented): bezier subdivision stops at the
//! flatness tolerance OR at depth [`MAX_DEPTH`] (≤ 2^12 = 4096
//! segments per curve); arcs emit at most [`MAX_ARC_SEGMENTS`]
//! segments. Pathological control points therefore cost bounded work,
//! never unbounded recursion.
//!
//! OWNER: CANVAS (extensions wave).

use super::DotCanvas;

/// Hard subdivision bound: at most 2^12 segments per bezier.
const MAX_DEPTH: u32 = 12;

/// Hard segment bound per arc call.
const MAX_ARC_SEGMENTS: i32 = 2048;

/// Tolerances below this (dots) buy nothing on a binary dot grid and
/// only burn subdivision depth; clamp keeps degenerate inputs cheap.
const MIN_TOL: f32 = 0.01;

impl DotCanvas {
    /// Quadratic bezier from `p0` to `p1` with control point `c`,
    /// flattened to segments until the curve deviates from each chord
    /// by at most `tol` dots (clamped to ≥ 0.01; 0.25 is a good
    /// default — deviation below a quarter dot is invisible on a
    /// binary grid). Coordinates are dot-space and may be fractional;
    /// each emitted endpoint rounds to the nearest dot.
    pub fn bezier_quad(&mut self, p0: (f32, f32), c: (f32, f32), p1: (f32, f32), tol: f32) {
        if !finite2(p0) || !finite2(c) || !finite2(p1) || !tol.is_finite() {
            return;
        }
        let tol = tol.max(MIN_TOL);
        self.quad_rec(p0, c, p1, tol * tol, MAX_DEPTH);
    }

    fn quad_rec(&mut self, p0: P, c: P, p1: P, tol_sq: f32, depth: u32) {
        // Max deviation of a quadratic from its chord is
        // |c - (p0+p1)/2| / 2, attained at t = 1/2.
        let dx = c.0 - 0.5 * (p0.0 + p1.0);
        let dy = c.1 - 0.5 * (p0.1 + p1.1);
        if depth == 0 || 0.25 * (dx * dx + dy * dy) <= tol_sq {
            self.line(round_dot(p0), round_dot(p1));
            return;
        }
        let ab = mid(p0, c);
        let bc = mid(c, p1);
        let m = mid(ab, bc);
        self.quad_rec(p0, ab, m, tol_sq, depth - 1);
        self.quad_rec(m, bc, p1, tol_sq, depth - 1);
    }

    /// Cubic bezier from `p0` to `p1` with control points `c0`, `c1`
    /// (same tolerance/rounding contract as [`DotCanvas::bezier_quad`]).
    pub fn bezier_cubic(
        &mut self,
        p0: (f32, f32),
        c0: (f32, f32),
        c1: (f32, f32),
        p1: (f32, f32),
        tol: f32,
    ) {
        if !finite2(p0) || !finite2(c0) || !finite2(c1) || !finite2(p1) || !tol.is_finite() {
            return;
        }
        let tol = tol.max(MIN_TOL);
        self.cubic_rec(p0, c0, c1, p1, tol * tol, MAX_DEPTH);
    }

    fn cubic_rec(&mut self, p0: P, c0: P, c1: P, p1: P, tol_sq: f32, depth: u32) {
        // Conservative flatness: the curve lies in the control hull,
        // so both control points within `tol` of the chord line bound
        // the deviation by `tol` (stricter than the 3/4 factor —
        // cheap and safe).
        if depth == 0
            || (dist_sq_to_line(c0, p0, p1) <= tol_sq && dist_sq_to_line(c1, p0, p1) <= tol_sq)
        {
            self.line(round_dot(p0), round_dot(p1));
            return;
        }
        // de Casteljau split at t = 1/2.
        let ab = mid(p0, c0);
        let bc = mid(c0, c1);
        let cd = mid(c1, p1);
        let abc = mid(ab, bc);
        let bcd = mid(bc, cd);
        let m = mid(abc, bcd);
        self.cubic_rec(p0, ab, abc, m, tol_sq, depth - 1);
        self.cubic_rec(m, bcd, cd, p1, tol_sq, depth - 1);
    }

    /// Elliptic arc around `center` with radii `rx`/`ry` (dots), from
    /// angle `start` sweeping `sweep` radians (positive = clockwise in
    /// screen space, because y grows down). Parameter-stepped at ~1
    /// dot per segment, capped at 2048 segments per call (bounded,
    /// like bezier flattening). A full circle is
    /// `ellipse_arc(c, r, r, 0.0, core::f32::consts::TAU)`.
    pub fn ellipse_arc(&mut self, center: (f32, f32), rx: f32, ry: f32, start: f32, sweep: f32) {
        if !finite2(center) || !rx.is_finite() || !ry.is_finite() {
            return;
        }
        if !start.is_finite() || !sweep.is_finite() {
            return;
        }
        let rmax = f64::from(rx.abs().max(ry.abs()));
        if rmax == 0.0 {
            self.set(center.0.round() as i32, center.1.round() as i32);
            return;
        }
        let steps = (f64::from(sweep.abs()) * rmax).ceil() as i32;
        let steps = steps.clamp(8, MAX_ARC_SEGMENTS);
        let (cx, cy) = (f64::from(center.0), f64::from(center.1));
        let (rxf, ryf) = (f64::from(rx), f64::from(ry));
        let (s, w) = (f64::from(start), f64::from(sweep));
        let mut prev: Option<(i32, i32)> = None;
        for i in 0..=steps {
            let t = s + w * f64::from(i) / f64::from(steps);
            let (sin, cos) = det_sin_cos(t);
            let p = (
                (cx + rxf * cos).round() as i32,
                (cy + ryf * sin).round() as i32,
            );
            match prev {
                Some(q) => self.line(q, p),
                None => self.set(p.0, p.1),
            }
            prev = Some(p);
        }
    }
}

type P = (f32, f32);

fn finite2(p: P) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

fn mid(a: P, b: P) -> P {
    (0.5 * (a.0 + b.0), 0.5 * (a.1 + b.1))
}

fn round_dot(p: P) -> (i32, i32) {
    // Saturating float->int casts; the grid clips anyway.
    (p.0.round() as i32, p.1.round() as i32)
}

/// Squared distance from `p` to the infinite line through `a`, `b`
/// (to `a` when the chord is degenerate).
fn dist_sq_to_line(p: P, a: P, b: P) -> f32 {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let len_sq = ex * ex + ey * ey;
    let (px, py) = (p.0 - a.0, p.1 - a.1);
    if len_sq <= f32::EPSILON {
        return px * px + py * py;
    }
    let cross = px * ey - py * ex;
    (cross * cross) / len_sq
}

/// Deterministic (sin, cos): IEEE-754 f64 arithmetic in a fixed order
/// — bit-identical on every platform, unlike the libm-backed std
/// `sin`/`cos`. Range-reduce to |r| ≤ π/4 by quarter turns, then
/// Taylor kernels (degree 9/10: |err| < 1e-9 on the reduced range —
/// three orders of magnitude below what dot rounding can see at the
/// largest bounded arc radius).
fn det_sin_cos(theta: f64) -> (f64, f64) {
    let k = (theta / std::f64::consts::FRAC_PI_2).round();
    let r = theta - k * std::f64::consts::FRAC_PI_2;
    let x2 = r * r;
    // sin r = r − r³/3! + r⁵/5! − r⁷/7! + r⁹/9!  (Horner-nested)
    let s = r * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
    // cos r = 1 − r²/2! + r⁴/4! − r⁶/6! + r⁸/8! − r¹⁰/10!
    let c = 1.0
        - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0 * (1.0 - x2 / 90.0))));
    match (k as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}
