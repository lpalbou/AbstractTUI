//! Progress: a bar with sub-cell precision via eighth-block glyphs.
//!
//! ```ignore
//! use abstracttui::widgets::Progress;
//! let t = theme.tokens;
//! let bar = Progress::new(0.62).element(&t).build();          // accent fill
//! let disk = Progress::new(0.91).ramp(true).element(&t).build(); // ok->warn->error
//! ```
//!
//! The filled run uses `█` plus one partial eighth-block cell
//! (`▏▎▍▌▋▊▉`), so a 40-cell bar resolves 320 steps. The track is the
//! raised surface ground. With `.ramp(true)` the fill color maps the
//! fraction through `ok -> warn -> error` at the threshold props
//! (usage-meter semantics: low is calm, full is alarming); default fill is
//! `accent`.
//!
//! OWNER: DESIGN.

use crate::base::Point;
use crate::canvas::fill_h;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

pub struct Progress {
    fraction: f32,
    ramp: bool,
    warn_at: f32,
    error_at: f32,
    layout: Option<LayoutStyle>,
}

impl Progress {
    pub fn new(fraction: f32) -> Progress {
        Progress {
            fraction: fraction.clamp(0.0, 1.0),
            ramp: false,
            warn_at: 0.65,
            error_at: 0.85,
            layout: None,
        }
    }

    /// Color the fill by fraction: `ok` below `warn_at`, `warn` below
    /// `error_at`, `error` at or above.
    pub fn ramp(mut self, on: bool) -> Progress {
        self.ramp = on;
        self
    }

    /// Ramp thresholds (clamped into order at build).
    pub fn thresholds(mut self, warn_at: f32, error_at: f32) -> Progress {
        self.warn_at = warn_at.clamp(0.0, 1.0);
        self.error_at = error_at.clamp(0.0, 1.0).max(self.warn_at);
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Progress {
        self.layout = Some(style);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let fill = if self.ramp {
            if self.fraction >= self.error_at {
                t.error
            } else if self.fraction >= self.warn_at {
                t.warn
            } else {
                t.ok
            }
        } else {
            t.accent
        };
        let track = t.surface_raised;
        let fraction = self.fraction;

        // shrink 0: the bar's one row never vanishes under column
        // overflow (0240 #2); width stays flexible through grow.
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .height(Dimension::Cells(1))
                .grow(1.0)
                .shrink(0.0)
        });

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let y = rect.y;
            // Track first (also the ground under the partial cell).
            for x in rect.x..rect.right() {
                canvas.put(Point::new(x, y), ' ', track, track);
            }
            // 320 steps on a 40-cell bar: whole cells + one eighth
            // cell, via the shared canvas layer's horizontal fill
            // (same rounding: w*8 steps; backlog 0420 refactor).
            fill_h(
                canvas,
                crate::base::Rect::new(rect.x, y, rect.w, 1),
                fraction,
                fill,
                track,
            );
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    #[test]
    fn sub_cell_precision_renders_partial_block() {
        let t = default_theme().tokens;
        // 0.5 of 10 cells = 5 full cells exactly.
        let c = draw_into(Progress::new(0.5).element(&t), Size::new(10, 1));
        assert_eq!(row(&c, 0), "█████     ");
        // 0.56 of 10 cells = 44.8 eighths -> 5 full + a 5/8 block.
        let c = draw_into(Progress::new(0.56).element(&t), Size::new(10, 1));
        assert_eq!(row(&c, 0), "█████▋    ");
    }

    #[test]
    fn empty_and_full_are_exact() {
        let t = default_theme().tokens;
        let c = draw_into(Progress::new(0.0).element(&t), Size::new(8, 1));
        assert_eq!(row(&c, 0), "        ");
        let c = draw_into(Progress::new(1.0).element(&t), Size::new(8, 1));
        assert_eq!(row(&c, 0), "████████");
        // Out-of-range input clamps instead of overflowing the rect.
        let c = draw_into(Progress::new(7.0).element(&t), Size::new(8, 1));
        assert_eq!(row(&c, 0), "████████");
    }

    #[test]
    fn ramp_colors_by_threshold() {
        let t = default_theme().tokens;
        let fill_color = |frac: f32| {
            let c = draw_into(Progress::new(frac).ramp(true).element(&t), Size::new(8, 1));
            c.cell(Point::new(0, 0)).unwrap().1
        };
        assert_eq!(fill_color(0.3), t.ok);
        assert_eq!(fill_color(0.7), t.warn);
        assert_eq!(fill_color(0.9), t.error);
        // Default (no ramp) is the brand accent.
        let c = draw_into(Progress::new(0.5).element(&t), Size::new(8, 1));
        assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.accent);
    }

    #[test]
    fn track_uses_raised_surface_and_zero_rect_is_safe() {
        let t = default_theme().tokens;
        let c = draw_into(Progress::new(0.2).element(&t), Size::new(8, 1));
        assert_eq!(c.cell(Point::new(7, 0)).unwrap().2, t.surface_raised);
        let _ = draw_into(Progress::new(0.2).element(&t), Size::new(0, 0));
    }
}
