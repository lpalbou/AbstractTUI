//! Separator: a horizontal or vertical rule, optionally labeled.
//!
//! ```ignore
//! use abstracttui::widgets::Separator;
//! let t = theme.tokens;
//! let rule = Separator::horizontal().label("history").element(&t).build();
//! let wall = Separator::vertical().element(&t).build();
//! ```
//!
//! Tokens: the stroke is `border`; the label is `text_muted` with one
//! space of breathing room each side, centered on the run. Horizontal
//! separators default to height 1 (vertical: width 1) via layout style —
//! callers can still override with `.layout(..)`.
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rgba};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

pub struct Separator {
    orientation: Orientation,
    label: Option<String>,
    layout: Option<LayoutStyle>,
}

impl Separator {
    pub fn horizontal() -> Separator {
        Separator {
            orientation: Orientation::Horizontal,
            label: None,
            layout: None,
        }
    }

    pub fn vertical() -> Separator {
        Separator {
            orientation: Orientation::Vertical,
            label: None,
            layout: None,
        }
    }

    /// Label rendered mid-rule (horizontal only; a vertical label would
    /// need per-cell rotation the cell grid cannot express — ignored with
    /// the stroke kept, never a panic).
    pub fn label(mut self, label: impl Into<String>) -> Separator {
        self.label = Some(label.into());
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Separator {
        self.layout = Some(style);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let stroke = t.border;
        let label_fg = t.text_muted;
        let orientation = self.orientation;
        let label = self.label;

        let layout = self.layout.unwrap_or_else(|| {
            // shrink 0: the rule's single row/column never vanishes
            // under overflow pressure (0240 #2).
            let mut s = LayoutStyle::default().shrink(0.0);
            match orientation {
                Orientation::Horizontal => {
                    s.height = Dimension::Cells(1);
                    s.grow = 1.0;
                }
                Orientation::Vertical => {
                    s.width = Dimension::Cells(1);
                    s.grow = 1.0;
                }
            }
            s
        });

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let keep = Rgba::TRANSPARENT;
            match orientation {
                Orientation::Horizontal => {
                    let y = rect.y;
                    for x in rect.x..rect.right() {
                        canvas.put(Point::new(x, y), '─', stroke, keep);
                    }
                    if let Some(label) = &label {
                        let shown: String =
                            label.chars().take((rect.w - 4).max(0) as usize).collect();
                        if !shown.is_empty() {
                            let w = shown.chars().count() as i32 + 2;
                            let x = rect.x + (rect.w - w).max(0) / 2;
                            canvas.print(Point::new(x, y), &format!(" {shown} "), label_fg, keep);
                        }
                    }
                }
                Orientation::Vertical => {
                    let x = rect.x;
                    for y in rect.y..rect.bottom() {
                        canvas.put(Point::new(x, y), '│', stroke, keep);
                    }
                }
            }
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
    fn horizontal_rule_with_centered_label() {
        let t = default_theme().tokens;
        let view = Separator::horizontal().label("io").element(&t);
        let c = draw_into(view, Size::new(12, 1));
        assert_eq!(row(&c, 0), "──── io ────");
        assert_eq!(c.cell(crate::base::Point::new(0, 0)).unwrap().1, t.border);
        assert_eq!(
            c.cell(crate::base::Point::new(5, 0)).unwrap().1,
            t.text_muted
        );
    }

    #[test]
    fn vertical_rule_fills_the_column() {
        let t = default_theme().tokens;
        let view = Separator::vertical().element(&t);
        let c = draw_into(view, Size::new(1, 4));
        for y in 0..4 {
            assert_eq!(c.cell(crate::base::Point::new(0, y)).unwrap().0, '│');
        }
    }

    #[test]
    fn long_labels_truncate_and_zero_rects_are_safe() {
        let t = default_theme().tokens;
        let view = Separator::horizontal()
            .label("much too long for this rule")
            .element(&t);
        let c = draw_into(view, Size::new(8, 1));
        assert!(row(&c, 0).contains("much"));
        let view = Separator::horizontal().element(&t);
        let _ = draw_into(view, Size::new(0, 0));
    }

    #[test]
    fn default_layout_reserves_one_cell() {
        let t = default_theme().tokens;
        let el = Separator::horizontal().element(&t);
        assert_eq!(el.style.height, Dimension::Cells(1));
        let el = Separator::vertical().element(&t);
        assert_eq!(el.style.width, Dimension::Cells(1));
    }
}
