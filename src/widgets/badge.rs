//! Badge: a small tinted label — status chips, counts, tags.
//!
//! ```ignore
//! use abstracttui::widgets::{Badge, Tone};
//! let t = theme.tokens;
//! let chip = Badge::new("ready").tone(Tone::Ok).element(&t).build();
//! ```
//!
//! Tokens: the label renders in the tone's semantic color over
//! `surface_raised` (the raised chip ground), padded one cell each side.
//! Every tone/ground pair inherits the audited `semantic/bg` floors —
//! badges never invent tints (RT1-9b).
//!
//! OWNER: DESIGN.

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

/// Which semantic ink the badge wears.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Ok,
    Warn,
    Error,
    Info,
    Muted,
}

pub struct Badge {
    label: String,
    tone: Tone,
    layout: Option<LayoutStyle>,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Badge {
        Badge {
            label: label.into(),
            tone: Tone::Muted,
            layout: None,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Badge {
        self.tone = tone;
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Badge {
        self.layout = Some(style);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let fg = match self.tone {
            Tone::Accent => t.accent,
            Tone::Ok => t.ok,
            Tone::Warn => t.warn,
            Tone::Error => t.error,
            Tone::Info => t.info,
            Tone::Muted => t.text_muted,
        };
        let ground = t.surface_raised;
        let label = self.label;
        let width = label.chars().count() as i32 + 2;

        let layout = self.layout.unwrap_or_else(|| {
            // shrink 0: a status chip never vanishes under overflow
            // pressure (0240 #2).
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(1))
                .shrink(0.0)
        });

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            canvas.fill(rect, ' ', fg, ground);
            let shown: String = label.chars().take((rect.w - 2).max(0) as usize).collect();
            canvas.print(Point::new(rect.x + 1, rect.y), &shown, fg, ground);
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
    fn badge_renders_padded_label_on_raised_ground() {
        let t = default_theme().tokens;
        let view = Badge::new("ok").tone(Tone::Ok).element(&t);
        let c = draw_into(view, Size::new(4, 1));
        assert_eq!(row(&c, 0), " ok ");
        let (_, fg, bg) = c.cell(Point::new(1, 0)).unwrap();
        assert_eq!(fg, t.ok);
        assert_eq!(bg, t.surface_raised);
    }

    #[test]
    fn tones_map_to_their_tokens() {
        let t = default_theme().tokens;
        for (tone, expect) in [
            (Tone::Accent, t.accent),
            (Tone::Warn, t.warn),
            (Tone::Error, t.error),
            (Tone::Info, t.info),
            (Tone::Muted, t.text_muted),
        ] {
            let view = Badge::new("x").tone(tone).element(&t);
            let c = draw_into(view, Size::new(3, 1));
            assert_eq!(c.cell(Point::new(1, 0)).unwrap().1, expect, "{tone:?}");
        }
    }

    #[test]
    fn default_layout_hugs_the_label_and_tight_rects_truncate() {
        let t = default_theme().tokens;
        let el = Badge::new("build").element(&t);
        assert_eq!(el.style.width, Dimension::Cells(7));
        let view = Badge::new("build").element(&t);
        let c = draw_into(view, Size::new(4, 1));
        assert_eq!(row(&c, 0), " bu ");
        let _ = draw_into(Badge::new("x").element(&t), Size::new(0, 0));
    }
}
