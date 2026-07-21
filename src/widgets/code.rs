//! CodeView: a read-only, syntax-highlighted code pane — and the ONE
//! place lexer token kinds become theme colors.
//!
//! ```ignore
//! use abstracttui::widgets::CodeView;
//! let view = CodeView::new(source_text)
//!     .lang_label("rust")
//!     .scroll_offset(top_line.get())
//!     .element(&t)
//!     .build();
//! ```
//!
//! [`code_token_color`] maps `text::TokenKind` onto the theme's derived
//! `syntax_*` inks (doc §1.4a): lexers never mint colors, themes never
//! know lexers. `Ident` deliberately renders as body `text` — the
//! built-in lexer cannot split types from functions; the `syntax_type` /
//! `syntax_func` inks stand ready for richer lexers (mapping documented
//! there, not invented per widget).
//!
//! Chrome: code sits on `surface_raised` (the audited code ground), line
//! numbers right-aligned in `text_faint` with a `border` separator.
//!
//! OWNER: DESIGN.

use std::rc::Rc;

use crate::base::{Point, Rgba};
use crate::layout::Style as LayoutStyle;
use crate::render::rich::RichLine;
use crate::render::Style;
use crate::text::{CLikeLexer, Highlighter, TokenKind};
use crate::theme::TokenSet;
use crate::ui::Element;

/// The theming of syntax: token kind -> derived theme ink (§1.4a).
pub fn code_token_color(kind: TokenKind, t: &TokenSet) -> Rgba {
    match kind {
        TokenKind::Keyword => t.syntax_keyword,
        TokenKind::String => t.syntax_string,
        TokenKind::Number => t.syntax_number,
        TokenKind::Comment => t.syntax_comment,
        TokenKind::Punct => t.syntax_punct,
        // Identifiers are body ink: the C-like lexer cannot tell types
        // from functions; syntax_type/syntax_func await a lexer that can.
        TokenKind::Ident => t.text,
    }
}

pub struct CodeView {
    source: String,
    lexer: Rc<dyn Highlighter>,
    line_numbers: bool,
    scroll_offset: i32,
    layout: Option<LayoutStyle>,
}

impl CodeView {
    pub fn new(source: impl Into<String>) -> CodeView {
        CodeView {
            source: source.into(),
            lexer: Rc::new(CLikeLexer::default()),
            line_numbers: true,
            scroll_offset: 0,
            layout: None,
        }
    }

    /// Swap the lexer (language modes; the default is the C-like demo
    /// lexer).
    pub fn lexer(mut self, lexer: impl Highlighter + 'static) -> CodeView {
        self.lexer = Rc::new(lexer);
        self
    }

    pub fn line_numbers(mut self, on: bool) -> CodeView {
        self.line_numbers = on;
        self
    }

    /// First visible source line (app-managed scrolling, clamped).
    pub fn scroll_offset(mut self, lines: i32) -> CodeView {
        self.scroll_offset = lines.max(0);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> CodeView {
        self.layout = Some(layout);
        self
    }

    /// Total source lines — the app's scroll clamp.
    pub fn line_count(source: &str) -> usize {
        source.lines().count()
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let tokens = *t;
        let ground = t.surface_raised;
        let gutter_fg = t.text_faint;
        let rule_fg = t.border;
        let base = Style::new().fg(t.text);
        let lexer = self.lexer;
        let line_numbers = self.line_numbers;
        let offset = self.scroll_offset as usize;
        let lines: Vec<String> = self.source.lines().map(|s| s.to_string()).collect();
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 0 || rect.h <= 0 {
                return;
            }
            canvas.fill(rect, ' ', tokens.text, ground);
            let total = lines.len();
            let gutter_w = if line_numbers {
                (digits(total) + 1).min(rect.w - 2)
            } else {
                0
            };
            let offset = offset.min(total.saturating_sub(1));
            for (row, line) in lines.iter().skip(offset).enumerate() {
                let y = rect.y + row as i32;
                if y >= rect.bottom() {
                    break;
                }
                let mut x = rect.x;
                if gutter_w > 0 {
                    let num = format!("{:>w$} ", offset + row + 1, w = (gutter_w - 1) as usize);
                    x += canvas.print(Point::new(x, y), &num, gutter_fg, ground);
                    canvas.put(Point::new(x, y), '│', rule_fg, ground);
                    x += 2;
                }
                let rich = RichLine::from_highlighted(line, &*lexer, base, |kind| {
                    Style::new().fg(code_token_color(kind, &tokens))
                });
                for span in &rich.spans {
                    x += crate::widgets::richtext::print_span_clipped(
                        canvas,
                        x,
                        y,
                        rect.right(),
                        &span.text,
                        &span.style,
                    );
                    if x >= rect.right() {
                        break;
                    }
                }
            }
        })
    }
}

fn digits(n: usize) -> i32 {
    n.max(1).ilog10() as i32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    const SRC: &str = "fn main() {\n    // greet\n    print(\"hi\", 42);\n}";

    /// Cell column of `needle` in a row string — `str::find` returns
    /// BYTE offsets, and gutter glyphs (`│`) are multi-byte: cells are
    /// chars (the bug this helper exists to prevent).
    fn cell_of(row: &str, needle: &str) -> i32 {
        let byte = row.find(needle).unwrap();
        row[..byte].chars().count() as i32
    }

    #[test]
    fn token_kinds_map_to_syntax_inks() {
        let t = default_theme().tokens;
        assert_eq!(code_token_color(TokenKind::Keyword, &t), t.syntax_keyword);
        assert_eq!(code_token_color(TokenKind::String, &t), t.syntax_string);
        assert_eq!(code_token_color(TokenKind::Number, &t), t.syntax_number);
        assert_eq!(code_token_color(TokenKind::Comment, &t), t.syntax_comment);
        assert_eq!(code_token_color(TokenKind::Punct, &t), t.syntax_punct);
        assert_eq!(code_token_color(TokenKind::Ident, &t), t.text);
    }

    #[test]
    fn renders_highlighted_cells_on_the_code_ground() {
        let t = default_theme().tokens;
        let c = draw_into(CodeView::new(SRC).element(&t), Size::new(28, 4));
        // Gutter: right-aligned number in faint + rule.
        assert!(row(&c, 0).starts_with("1 │ fn main"), "{:?}", row(&c, 0));
        assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.text_faint);
        assert_eq!(c.cell(Point::new(2, 0)).unwrap().1, t.border);
        // `fn` keyword ink at its cell; ground is surface_raised.
        let (ch, fg, bg) = c.cell(Point::new(4, 0)).unwrap();
        assert_eq!(ch, 'f');
        assert_eq!(fg, t.syntax_keyword);
        assert_eq!(bg, t.surface_raised);
        // Comment line ink.
        let comment_x = cell_of(&row(&c, 1), "//");
        assert_eq!(
            c.cell(Point::new(comment_x, 1)).unwrap().1,
            t.syntax_comment
        );
        // String + number inks on line 3.
        let quote_x = cell_of(&row(&c, 2), "\"");
        assert_eq!(c.cell(Point::new(quote_x, 2)).unwrap().1, t.syntax_string);
        let num_x = cell_of(&row(&c, 2), "42");
        assert_eq!(c.cell(Point::new(num_x, 2)).unwrap().1, t.syntax_number);
    }

    #[test]
    fn scroll_clamps_and_numbers_track() {
        let t = default_theme().tokens;
        let c = draw_into(
            CodeView::new(SRC).scroll_offset(2).element(&t),
            Size::new(24, 2),
        );
        assert!(row(&c, 0).starts_with("3 │"), "{:?}", row(&c, 0));
        // Absurd offset clamps to the last line instead of blanking.
        let c = draw_into(
            CodeView::new(SRC).scroll_offset(999).element(&t),
            Size::new(24, 2),
        );
        assert!(row(&c, 0).starts_with("4 │"), "{:?}", row(&c, 0));
        assert_eq!(CodeView::line_count(SRC), 4);
    }

    #[test]
    fn no_gutter_mode_and_tiny_rects() {
        let t = default_theme().tokens;
        let c = draw_into(
            CodeView::new(SRC).line_numbers(false).element(&t),
            Size::new(16, 2),
        );
        assert!(row(&c, 0).starts_with("fn main"), "{:?}", row(&c, 0));
        for size in [Size::new(0, 0), Size::new(2, 1), Size::new(5, 1)] {
            let _ = draw_into(CodeView::new(SRC).element(&t), size);
        }
    }
}
