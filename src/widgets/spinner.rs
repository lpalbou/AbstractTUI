//! Spinner: an indeterminate activity glyph, pure over a frame index.
//!
//! ```ignore
//! use abstracttui::widgets::{Spinner, SpinnerKind};
//! let t = theme.tokens;
//! // `tick` is a signal owned by the caller (an anim tween or timer);
//! // the widget stays pure — same index, same pixels.
//! let view = Spinner::new()
//!     .kind(SpinnerKind::Braille)
//!     .frame(tick.get())
//!     .label("indexing")
//!     .element(&t)
//!     .build();
//! ```
//!
//! Driving it: wrap in a `dyn_view` reading a tick signal; each write
//! damages exactly the spinner's cells. The widget never owns time — that
//! is the reactive layer's job (and what makes this testable).
//!
//! Tokens: glyph in `accent`, label in `text_muted`.
//!
//! OWNER: DESIGN.

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::Element;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpinnerKind {
    /// Orbiting braille dot — subtle, single cell.
    Dots,
    /// Dense braille shimmer — busier, single cell.
    Braille,
    /// Pure-ASCII line — for the humblest terminals.
    Line,
}

impl SpinnerKind {
    pub fn frames(self) -> &'static [char] {
        match self {
            SpinnerKind::Dots => &['⠁', '⠂', '⠄', '⡀', '⢀', '⠠', '⠐', '⠈'],
            SpinnerKind::Braille => &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'],
            SpinnerKind::Line => &['|', '/', '-', '\\'],
        }
    }
}

pub struct Spinner {
    kind: SpinnerKind,
    frame: u64,
    label: Option<String>,
    layout: Option<LayoutStyle>,
}

impl Spinner {
    pub fn new() -> Spinner {
        Spinner {
            kind: SpinnerKind::Dots,
            frame: 0,
            label: None,
            layout: None,
        }
    }

    pub fn kind(mut self, kind: SpinnerKind) -> Spinner {
        self.kind = kind;
        self
    }

    /// Current animation frame (any monotonic counter; wraps by modulo).
    pub fn frame(mut self, frame: u64) -> Spinner {
        self.frame = frame;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Spinner {
        self.label = Some(label.into());
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Spinner {
        self.layout = Some(style);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let frames = self.kind.frames();
        let glyph = frames[(self.frame % frames.len() as u64) as usize];
        let glyph_fg = t.accent;
        let label_fg = t.text_muted;
        let label = self.label;
        let width = 1 + label.as_ref().map_or(0, |l| l.chars().count() as i32 + 1);

        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(1))
        });

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            let keep = crate::base::Rgba::TRANSPARENT;
            canvas.put(Point::new(rect.x, rect.y), glyph, glyph_fg, keep);
            if let Some(label) = &label {
                let avail = (rect.w - 2).max(0) as usize;
                let shown: String = label.chars().take(avail).collect();
                if !shown.is_empty() {
                    canvas.print(Point::new(rect.x + 2, rect.y), &shown, label_fg, keep);
                }
            }
        })
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Spinner::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    #[test]
    fn frame_index_selects_glyph_and_wraps() {
        let t = default_theme().tokens;
        let glyph_at = |frame: u64| {
            let c = draw_into(
                Spinner::new()
                    .kind(SpinnerKind::Line)
                    .frame(frame)
                    .element(&t),
                Size::new(1, 1),
            );
            c.cell(Point::new(0, 0)).unwrap().0
        };
        assert_eq!(glyph_at(0), '|');
        assert_eq!(glyph_at(1), '/');
        assert_eq!(glyph_at(4), '|', "wraps by modulo");
        assert_eq!(glyph_at(0), glyph_at(4), "pure: same index, same pixels");
    }

    #[test]
    fn label_renders_muted_next_to_the_glyph() {
        let t = default_theme().tokens;
        let view = Spinner::new()
            .kind(SpinnerKind::Line)
            .label("sync")
            .element(&t);
        let c = draw_into(view, Size::new(7, 1));
        assert_eq!(row(&c, 0), "| sync ");
        assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.accent);
        assert_eq!(c.cell(Point::new(2, 0)).unwrap().1, t.text_muted);
    }

    #[test]
    fn every_kind_has_nonempty_distinct_frames() {
        for kind in [SpinnerKind::Dots, SpinnerKind::Braille, SpinnerKind::Line] {
            let frames = kind.frames();
            assert!(frames.len() >= 4, "{kind:?}");
            let mut dedup = frames.to_vec();
            dedup.sort_unstable();
            dedup.dedup();
            assert_eq!(dedup.len(), frames.len(), "{kind:?} has duplicate frames");
        }
    }

    #[test]
    fn default_layout_hugs_content_and_zero_rect_is_safe() {
        let t = default_theme().tokens;
        let el = Spinner::new().label("busy").element(&t);
        assert_eq!(el.style.width, Dimension::Cells(6));
        let _ = draw_into(Spinner::new().element(&t), Size::new(0, 0));
    }
}
