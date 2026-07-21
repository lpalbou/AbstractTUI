//! Logo: the AbstractTUI wordmark as a reusable widget — about screens,
//! headers, empty states. One brand rendering, sourced from
//! `boot::identity` so the widget and the splash can never spell the
//! product differently.
//!
//! ```ignore
//! use abstracttui::widgets::Logo;
//! let t = theme.tokens;
//! let mark = Logo::new().tagline(true).element(&t).build();
//! ```
//!
//! Tokens: "Abstract" in `text`, "TUI" in `accent` (the house split; the
//! splash's animated reveal leads with accent instead — deliberate:
//! statically, the accent belongs on the product suffix, in motion it
//! belongs on the arrival). Tagline in `text_muted`.
//!
//! OWNER: DESIGN.

use crate::base::Point;
use crate::boot::identity;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

/// Where the wordmark splits ink: `Abstract` | `TUI`.
const SPLIT: usize = 8;

pub struct Logo {
    tagline: bool,
    layout: Option<LayoutStyle>,
}

impl Logo {
    pub fn new() -> Logo {
        Logo {
            tagline: false,
            layout: None,
        }
    }

    /// Render the identity tagline under the wordmark.
    pub fn tagline(mut self, on: bool) -> Logo {
        self.tagline = on;
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Logo {
        self.layout = Some(style);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let head_fg = t.text;
        let tail_fg = t.accent;
        let tag_fg = t.text_muted;
        let with_tagline = self.tagline;

        let word_w = identity::WORDMARK.chars().count() as i32;
        let tag_w = identity::TAGLINE.chars().count() as i32;
        let width = if with_tagline {
            word_w.max(tag_w)
        } else {
            word_w
        };
        let height = if with_tagline { 2 } else { 1 };

        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(height))
        });

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let keep = crate::base::Rgba::TRANSPARENT;
            let (head, tail) = identity::WORDMARK.split_at(SPLIT);
            let x = canvas_center(rect.x, rect.w, word_w);
            let advanced = canvas.print(Point::new(x, rect.y), head, head_fg, keep);
            canvas.print(Point::new(x + advanced, rect.y), tail, tail_fg, keep);
            if with_tagline && rect.h > 1 {
                let tx = canvas_center(rect.x, rect.w, tag_w);
                canvas.print(Point::new(tx, rect.y + 1), identity::TAGLINE, tag_fg, keep);
            }
        })
    }
}

impl Default for Logo {
    fn default() -> Self {
        Logo::new()
    }
}

/// Center a run of `content_w` cells inside `[x, x+w)`, clamped to start.
fn canvas_center(x: i32, w: i32, content_w: i32) -> i32 {
    x + (w - content_w).max(0) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    #[test]
    fn wordmark_splits_ink_at_the_product_suffix() {
        let t = default_theme().tokens;
        let c = draw_into(Logo::new().element(&t), Size::new(11, 1));
        assert_eq!(row(&c, 0), identity::WORDMARK);
        assert_eq!(
            c.cell(Point::new(0, 0)).unwrap().1,
            t.text,
            "Abstract in text ink"
        );
        assert_eq!(
            c.cell(Point::new(8, 0)).unwrap().1,
            t.accent,
            "TUI in accent"
        );
    }

    #[test]
    fn tagline_renders_muted_and_centered() {
        let t = default_theme().tokens;
        let c = draw_into(Logo::new().tagline(true).element(&t), Size::new(24, 2));
        assert!(row(&c, 0).contains(identity::WORDMARK));
        let tag = row(&c, 1);
        assert!(tag.contains(identity::TAGLINE), "{tag:?}");
        let first = tag.find(|c: char| !c.is_whitespace()).unwrap() as i32;
        assert_eq!(c.cell(Point::new(first, 1)).unwrap().1, t.text_muted);
    }

    #[test]
    fn default_layout_hugs_content() {
        let t = default_theme().tokens;
        let el = Logo::new().element(&t);
        assert_eq!(el.style.width, Dimension::Cells(11));
        assert_eq!(el.style.height, Dimension::Cells(1));
        let el = Logo::new().tagline(true).element(&t);
        assert_eq!(el.style.height, Dimension::Cells(2));
        // The tagline is wider than the wordmark; width follows it.
        assert_eq!(
            el.style.width,
            Dimension::Cells(identity::TAGLINE.chars().count() as i32)
        );
    }

    #[test]
    fn tiny_rects_never_panic() {
        let t = default_theme().tokens;
        for size in [Size::new(0, 0), Size::new(3, 1), Size::new(5, 2)] {
            let _ = draw_into(Logo::new().tagline(true).element(&t), size);
        }
    }
}
