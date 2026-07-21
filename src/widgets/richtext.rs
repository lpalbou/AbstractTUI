//! RichTextView: renders a `render::RichText` model through the widget
//! canvas — wrapping, alignment, row scrolling, optional themed fill.
//!
//! ```ignore
//! use abstracttui::render::rich::{RichText, Span, RichLine};
//! use abstracttui::widgets::RichTextView;
//! let doc = RichText::from_lines(vec![RichLine::from_spans(vec![
//!     Span::new("styled ", Style::new().fg(t.accent)),
//!     Span::plain("world"),
//! ])]);
//! let view = RichTextView::new(doc).wrap(true).element(&t).build();
//! ```
//!
//! The span walk lives here once (`draw_rich_lines`) and is shared by the
//! markdown and code widgets — one renderer, three faces. Span styles are
//! PATCHES: `fg: None` inherits the widget's base ink (`text`), `bg: None`
//! inherits the fill, so theme-agnostic RichText from RENDER's parsers
//! lands themed without rewriting spans.
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rect, Rgba};
use crate::layout::Style as LayoutStyle;
use crate::render::rich::{HAlign, RichLine, RichText};
use crate::theme::TokenSet;
use crate::ui::{Element, StyledCanvas};

pub struct RichTextView {
    text: RichText,
    align: HAlign,
    wrap: bool,
    scroll_offset: i32,
    fill: Option<Rgba>,
    layout: Option<LayoutStyle>,
}

impl RichTextView {
    pub fn new(text: RichText) -> RichTextView {
        RichTextView {
            text,
            align: HAlign::Left,
            wrap: true,
            scroll_offset: 0,
            fill: None,
            layout: None,
        }
    }

    pub fn align(mut self, align: HAlign) -> RichTextView {
        self.align = align;
        self
    }

    /// Wrap to the drawn width (default). Off = lines clip.
    pub fn wrap(mut self, wrap: bool) -> RichTextView {
        self.wrap = wrap;
        self
    }

    /// First visible row (app-managed scrolling; clamped to content).
    pub fn scroll_offset(mut self, rows: i32) -> RichTextView {
        self.scroll_offset = rows.max(0);
        self
    }

    /// Paint the region with this ground first (pass a surface token).
    pub fn fill(mut self, ground: Rgba) -> RichTextView {
        self.fill = Some(ground);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> RichTextView {
        self.layout = Some(layout);
        self
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let base_ink = t.text;
        let (text, align, wrap, offset, fill) = (
            self.text,
            self.align,
            self.wrap,
            self.scroll_offset,
            self.fill,
        );
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));
        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            if let Some(ground) = fill {
                canvas.fill(rect, ' ', base_ink, ground);
            }
            let shaped;
            let doc = if wrap {
                shaped = text.wrap(rect.w);
                &shaped
            } else {
                &text
            };
            let visible = doc.lines.iter().skip(offset as usize);
            draw_rich_lines(canvas, rect, visible, base_ink, align);
        })
    }
}

/// Shared span walk: draw lines top-down inside `rect`, aligning each
/// line, patching span inks over `base_ink`. Rows beyond the rect stop;
/// spans clip at the rect's right edge ([`print_span_clipped`] — draw
/// closures see the whole canvas, so rect discipline is ours to keep).
pub(crate) fn draw_rich_lines<'a>(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    lines: impl Iterator<Item = &'a RichLine>,
    base_ink: Rgba,
    align: HAlign,
) {
    for (row, line) in lines.enumerate() {
        let y = rect.y + row as i32;
        if y >= rect.bottom() {
            break;
        }
        let w = line.width();
        let mut x = match align {
            HAlign::Left => rect.x,
            HAlign::Center => rect.x + (rect.w - w).max(0) / 2,
            HAlign::Right => rect.x + (rect.w - w).max(0),
        };
        for span in &line.spans {
            // Patch semantics: a span without its own fg wears the base.
            let style = if span.style.fg.is_none() {
                span.style.fg(base_ink)
            } else {
                span.style
            };
            x += print_span_clipped(canvas, x, y, rect.right(), &span.text, &style);
            if x >= rect.right() {
                break;
            }
        }
    }
}

/// Print `text` at (x, y) clipped to `right` (exclusive). Widget draw
/// closures receive the WHOLE canvas — the compositor clips to damage,
/// never to the element rect — so a span crossing the panel edge would
/// otherwise overwrite the neighbor's cells (live finding: a long code
/// line ate its block's right border). Char-per-cell truncation matches
/// the ui `Canvas::print` v1 width model.
pub(crate) fn print_span_clipped(
    canvas: &mut dyn StyledCanvas,
    x: i32,
    y: i32,
    right: i32,
    text: &str,
    style: &crate::render::Style,
) -> i32 {
    let avail = right - x;
    if avail <= 0 {
        return 0;
    }
    let fits = text.chars().count() as i32 <= avail;
    if fits {
        return canvas.print_styled(Point::new(x, y), text, style);
    }
    let clipped: String = text.chars().take(avail as usize).collect();
    canvas.print_styled(Point::new(x, y), &clipped, style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::render::rich::Span;
    use crate::render::Attrs;
    use crate::render::Style;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    fn doc(t: &TokenSet) -> RichText {
        RichText::from_lines(vec![
            RichLine::from_spans(vec![
                Span::new("bold", Style::new().attrs(Attrs::BOLD)),
                Span::plain(" plain"),
            ]),
            RichLine::from_spans(vec![Span::new("accent", Style::new().fg(t.accent))]),
        ])
    }

    #[test]
    fn spans_render_with_patch_inheritance() {
        let t = default_theme().tokens;
        let c = draw_into(RichTextView::new(doc(&t)).element(&t), Size::new(12, 2));
        assert_eq!(row(&c, 0).trim_end(), "bold plain");
        // Base ink inherited where the span had no fg; BOLD recorded.
        assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.text);
        assert!(c.attrs_at(Point::new(0, 0)).contains(Attrs::BOLD));
        // Explicit span ink wins.
        assert_eq!(c.cell(Point::new(0, 1)).unwrap().1, t.accent);
    }

    #[test]
    fn wrap_scroll_and_alignment() {
        let t = default_theme().tokens;
        let long = RichText::plain("one two three four", Style::EMPTY);
        // Width 9: wraps to ["one two", "three", "four"]-ish rows.
        let c = draw_into(RichTextView::new(long.clone()).element(&t), Size::new(9, 4));
        assert!(c.row_text(0).contains("one"));
        assert!(!c.row_text(0).contains("three"), "{:?}", c.row_text(0));
        // Scrolled by one row: the first row disappears.
        let c = draw_into(
            RichTextView::new(long).scroll_offset(1).element(&t),
            Size::new(9, 4),
        );
        assert!(!c.row_text(0).contains("one"));

        let centered = RichText::plain("mid", Style::EMPTY);
        let c = draw_into(
            RichTextView::new(centered)
                .align(HAlign::Center)
                .element(&t),
            Size::new(9, 1),
        );
        assert_eq!(c.row_text(0), "   mid   ");
    }

    #[test]
    fn fill_paints_ground_and_zero_rect_is_safe() {
        let t = default_theme().tokens;
        let c = draw_into(
            RichTextView::new(RichText::plain("x", Style::EMPTY))
                .fill(t.surface_raised)
                .element(&t),
            Size::new(4, 2),
        );
        assert_eq!(c.cell(Point::new(3, 1)).unwrap().2, t.surface_raised);
        let _ = draw_into(
            RichTextView::new(RichText::new()).element(&t),
            Size::new(0, 0),
        );
    }
}
