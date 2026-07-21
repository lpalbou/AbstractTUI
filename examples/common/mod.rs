//! Shared helpers for the example apps (not an example itself — cargo
//! only auto-targets .rs files directly under examples/).
//!
//! Each example compiles its own copy and uses a subset; unused-item
//! warnings would differ per example, hence the module-level allow.
//!
//! OWNER: DESIGN.
#![allow(dead_code)]

use abstracttui::prelude::*;
use abstracttui::ui::Canvas;

/// Minimum comfortable size for the demo apps.
pub const MIN_SIZE: Size = Size { w: 40, h: 10 };

/// Draw-time small-terminal guard: when `rect` is under `min`, paints a
/// centered, theme-correct notice and returns `true` (the caller skips its
/// real content). Draw-time — so it follows live resizes with zero extra
/// plumbing, and examples degrade gracefully instead of clipping into
/// garbage or panicking.
pub fn too_small(canvas: &mut dyn Canvas, rect: Rect, min: Size, t: &TokenSet) -> bool {
    if rect.w >= min.w && rect.h >= min.h {
        return false;
    }
    canvas.fill(rect, ' ', t.text, t.bg);
    let msg = format!("terminal too small — need {}x{}", min.w, min.h);
    let hint = format!("(currently {}x{})", rect.w, rect.h);
    let cy = rect.y + (rect.h / 2 - 1).max(0);
    print_centered(canvas, rect, cy, &msg, t.warn);
    print_centered(canvas, rect, cy + 1, &hint, t.text_muted);
    true
}

/// Print a line centered in `rect` at row `y`, clipped by the canvas.
pub fn print_centered(canvas: &mut dyn Canvas, rect: Rect, y: i32, s: &str, fg: Rgba) {
    let w = s.chars().count() as i32;
    let x = rect.x + (rect.w - w).max(0) / 2;
    canvas.print(Point::new(x, y), s, fg, Rgba::TRANSPARENT);
}

/// Full-viewport absolute layout — the standard slot for the too-small
/// overlay (painted last, no-op above the minimum size).
pub fn overlay_layout() -> LayoutStyle {
    LayoutStyle::default().absolute(abstracttui::layout::Inset {
        left: Some(0),
        right: Some(0),
        top: Some(0),
        bottom: Some(0),
    })
}

/// A one-line key legend rendered in the footer style every example
/// shares: muted text, accent-colored key names, bottom row of `rect`.
pub fn key_legend(canvas: &mut dyn Canvas, rect: Rect, t: &TokenSet, entries: &[(&str, &str)]) {
    let y = rect.bottom() - 1;
    let mut x = rect.x + 1;
    for (key, what) in entries {
        if x >= rect.right() {
            break;
        }
        x += canvas.print(Point::new(x, y), key, t.accent, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), " ", t.text_muted, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), what, t.text_muted, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), "  ", t.text_muted, Rgba::TRANSPARENT);
    }
}
