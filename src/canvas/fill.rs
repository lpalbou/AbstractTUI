//! Eighth-block partial fills: the gauge/bar cell vocabulary
//! ([`V_EIGHTHS`]/[`H_EIGHTHS`]) as primitives. `BarChart` and
//! `Progress` draw through these; extensions and app draw closures
//! get the same 8-steps-per-cell resolution in one call.
//!
//! Cells beyond the filled run are left untouched (transparent
//! composition, like [`DotCanvas::blit`](super::DotCanvas::blit) —
//! draw the track/ground first when one is wanted). Colors are
//! caller-resolved `Rgba` per the widget token rule.
//!
//! OWNER: CANVAS (extensions wave).

use super::glyphs::{H_EIGHTHS, V_EIGHTHS};
use crate::base::{Point, Rect, Rgba};
use crate::ui::Canvas;

/// Vertical fill rising from the bottom edge of `rect`: full-block
/// rows plus one eighth-block boundary cell per column. `fraction`
/// clamps to `0..=1`; resolution is `rect.h * 8` steps (rounded to
/// the nearest eighth). Non-finite fractions draw nothing.
pub fn fill_v<C: Canvas + ?Sized>(canvas: &mut C, rect: Rect, fraction: f32, fg: Rgba, bg: Rgba) {
    if rect.w <= 0 || rect.h <= 0 || !fraction.is_finite() {
        return;
    }
    let eighths = (fraction.clamp(0.0, 1.0) * (rect.h * 8) as f32).round() as i32;
    let (full, part) = (eighths / 8, eighths % 8);
    for x in rect.x..rect.right() {
        for row in 0..full {
            canvas.put(Point::new(x, rect.bottom() - 1 - row), '█', fg, bg);
        }
        if part > 0 && full < rect.h {
            canvas.put(
                Point::new(x, rect.bottom() - 1 - full),
                V_EIGHTHS[(part - 1) as usize],
                fg,
                bg,
            );
        }
    }
}

/// Horizontal fill growing from the left edge of `rect`: full-block
/// columns plus one eighth-block boundary cell per row. Same
/// contract as [`fill_v`] with `rect.w * 8` steps.
pub fn fill_h<C: Canvas + ?Sized>(canvas: &mut C, rect: Rect, fraction: f32, fg: Rgba, bg: Rgba) {
    if rect.w <= 0 || rect.h <= 0 || !fraction.is_finite() {
        return;
    }
    let eighths = (fraction.clamp(0.0, 1.0) * (rect.w * 8) as f32).round() as i32;
    let (full, part) = (eighths / 8, eighths % 8);
    for y in rect.y..rect.bottom() {
        for col in 0..full {
            canvas.put(Point::new(rect.x + col, y), '█', fg, bg);
        }
        if part > 0 && full < rect.w {
            canvas.put(
                Point::new(rect.x + full, y),
                H_EIGHTHS[(part - 1) as usize],
                fg,
                bg,
            );
        }
    }
}
