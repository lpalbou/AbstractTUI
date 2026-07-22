//! CodeView: a read-only, syntax-highlighted code pane — and the ONE
//! place lexer token kinds become theme colors.
//!
//! ```ignore
//! use abstracttui::widgets::CodeView;
//! let view = CodeView::new(source_text)
//!     .lang("rust")
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
//! [`diff_token_color`] is the diff twin (backlog 0140's additive
//! slice): `text::DiffKind` onto the SEMANTIC inks — added `ok`,
//! removed `error`, hunk headers `info`, chrome `text_muted` — so a
//! patch scans red/green in every theme without any theme knowing diff.
//! `CodeView::lang("diff")` and diff-labeled markdown fences both route
//! through it (one mapping, no drift).
//!
//! Chrome: code sits on `surface_raised` (the audited code ground), line
//! numbers right-aligned in `text_faint` with a `border` separator.
//!
//! OWNER: DESIGN.

use std::rc::Rc;

use crate::base::{Point, Rgba};
use crate::layout::Style as LayoutStyle;
use crate::render::rich::{RichLine, Span};
use crate::render::{Attrs, Style};
use crate::text::{CLikeLexer, DiffKind, DiffLexer, Highlighter, TokenKind};
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

/// The theming of diffs: diff line kind -> theme ink, beside
/// [`code_token_color`] so kinds become colors in exactly one place.
/// Semantic inks, not syntax inks: added/removed are *state* ("this
/// changed"), so they ride `ok`/`error` — the tokens every theme already
/// audits — never a hue a diff invents.
pub fn diff_token_color(kind: DiffKind, t: &TokenSet) -> Rgba {
    match kind {
        DiffKind::Added => t.ok,
        DiffKind::Removed => t.error,
        DiffKind::HunkHeader => t.info,
        DiffKind::FileHeader => t.text,
        DiffKind::Meta => t.text_muted,
        DiffKind::Context => t.text,
        // DiffKind is #[non_exhaustive]; in-crate this match stays
        // exhaustive so the compiler walks every new kind through here,
        // but FOREIGN matches need a `_` arm — document theirs as body
        // text (never invisible).
    }
}

/// One diff line -> themed spans: the crate-shared recipe both
/// `CodeView` (lang "diff") and diff-labeled markdown fences render
/// through, mirroring `RichLine::from_highlighted`'s gap-filling
/// contract. File
/// headers additionally carry BOLD (the anchor a reader scans for);
/// everything else is ink-only.
pub(crate) fn diff_rich_line(line: &str, lexer: &DiffLexer, base: Style, t: &TokenSet) -> RichLine {
    let mut out = RichLine::new();
    let mut cursor = 0;
    for (range, kind) in lexer.spans(line) {
        if range.start > cursor {
            out.push(Span::new(&line[cursor..range.start], base));
        }
        let mut style = Style::new().fg(diff_token_color(kind, t));
        if kind == DiffKind::FileHeader {
            style = style.attrs(Attrs::BOLD);
        }
        out.push(Span::new(&line[range.clone()], style));
        cursor = range.end;
    }
    if cursor < line.len() {
        out.push(Span::new(&line[cursor..], base));
    }
    out
}

/// How a `CodeView` tints lines: a token lexer behind the
/// [`Highlighter`] seam, or the line-oriented diff lexer (a different
/// span vocabulary, so a different arm — see `text::diff`).
enum Tinter {
    Syntax(Rc<dyn Highlighter>),
    Diff(DiffLexer),
}

pub struct CodeView {
    source: String,
    tinter: Tinter,
    line_numbers: bool,
    scroll_offset: i32,
    layout: Option<LayoutStyle>,
}

impl CodeView {
    pub fn new(source: impl Into<String>) -> CodeView {
        CodeView {
            source: source.into(),
            tinter: Tinter::Syntax(Rc::new(CLikeLexer::default())),
            line_numbers: true,
            scroll_offset: 0,
            layout: None,
        }
    }

    /// Swap the lexer (language modes; the default is the C-like demo
    /// lexer).
    pub fn lexer(mut self, lexer: impl Highlighter + 'static) -> CodeView {
        self.tinter = Tinter::Syntax(Rc::new(lexer));
        self
    }

    /// Pick the built-in lexer by language label (best effort, the same
    /// labels markdown fences carry): `"diff"`/`"patch"`/`"udiff"` route
    /// to the diff lexer ([`diff_token_color`] inks), `"rust"`/`"rs"`
    /// and `"c"` pick the matching [`CLikeLexer`] preset, anything else
    /// keeps the default C-like lexer — unknown labels never change
    /// today's rendering.
    pub fn lang(mut self, label: &str) -> CodeView {
        let first = label.split_whitespace().next().unwrap_or("");
        self.tinter = if DiffLexer::matches_lang(first) {
            Tinter::Diff(DiffLexer::new())
        } else if first.eq_ignore_ascii_case("c") {
            Tinter::Syntax(Rc::new(CLikeLexer::c()))
        } else {
            // "rust"/"rs" and unknown labels: the rust-preset default.
            Tinter::Syntax(Rc::new(CLikeLexer::default()))
        };
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
        let tinter = self.tinter;
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
                let rich = match &tinter {
                    Tinter::Syntax(lexer) => {
                        RichLine::from_highlighted(line, &**lexer, base, |kind| {
                            Style::new().fg(code_token_color(kind, &tokens))
                        })
                    }
                    Tinter::Diff(lexer) => diff_rich_line(line, lexer, base, &tokens),
                };
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

    const DIFF: &str = "diff --git a/x.rs b/x.rs\n@@ -1,2 +1,2 @@ fn main() {\n-let a = 0;\n+let a = 1;\n context\n\\ No newline at end of file";

    #[test]
    fn diff_kinds_map_to_semantic_inks() {
        let t = default_theme().tokens;
        assert_eq!(diff_token_color(DiffKind::Added, &t), t.ok);
        assert_eq!(diff_token_color(DiffKind::Removed, &t), t.error);
        assert_eq!(diff_token_color(DiffKind::HunkHeader, &t), t.info);
        assert_eq!(diff_token_color(DiffKind::FileHeader, &t), t.text);
        assert_eq!(diff_token_color(DiffKind::Meta, &t), t.text_muted);
        assert_eq!(diff_token_color(DiffKind::Context, &t), t.text);
    }

    #[test]
    fn lang_diff_renders_added_removed_inks_through_the_real_draw_path() {
        let t = default_theme().tokens;
        let c = draw_into(
            CodeView::new(DIFF).lang("diff").element(&t),
            Size::new(36, 6),
        );
        // Meta chrome (diff --git) in muted ink on the code ground.
        let (ch, fg, bg) = c.cell(Point::new(4, 0)).unwrap();
        assert_eq!(ch, 'd');
        assert_eq!(fg, t.text_muted);
        assert_eq!(bg, t.surface_raised);
        // Hunk header in info; its trailing function context in body ink.
        let hunk_x = cell_of(&row(&c, 1), "@@");
        assert_eq!(c.cell(Point::new(hunk_x, 1)).unwrap().1, t.info);
        let fn_x = cell_of(&row(&c, 1), "fn main");
        assert_eq!(c.cell(Point::new(fn_x, 1)).unwrap().1, t.text);
        // Removed line in error ink, added line in ok ink — whole line.
        let minus_x = cell_of(&row(&c, 2), "-let");
        assert_eq!(c.cell(Point::new(minus_x, 2)).unwrap().1, t.error);
        assert_eq!(c.cell(Point::new(minus_x + 6, 2)).unwrap().1, t.error);
        let plus_x = cell_of(&row(&c, 3), "+let");
        assert_eq!(c.cell(Point::new(plus_x, 3)).unwrap().1, t.ok);
        // Context body ink; no-newline marker muted.
        let ctx_x = cell_of(&row(&c, 4), "context");
        assert_eq!(c.cell(Point::new(ctx_x, 4)).unwrap().1, t.text);
        let nn_x = cell_of(&row(&c, 5), "\\ No newline");
        assert_eq!(c.cell(Point::new(nn_x, 5)).unwrap().1, t.text_muted);
    }

    #[test]
    fn lang_labels_route_and_unknown_labels_keep_todays_rendering() {
        let t = default_theme().tokens;
        // "rust" and an unknown label both render the keyword ink (the
        // default C-like path — unknown labels change nothing).
        for label in ["rust", "elixir"] {
            let c = draw_into(CodeView::new(SRC).lang(label).element(&t), Size::new(28, 4));
            assert_eq!(c.cell(Point::new(4, 0)).unwrap().1, t.syntax_keyword);
        }
        // A diff label with fence-style extra words still routes.
        let c = draw_into(
            CodeView::new("+x")
                .lang("diff filename=a.patch")
                .line_numbers(false)
                .element(&t),
            Size::new(8, 1),
        );
        assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.ok);
    }

    /// Theme honesty for the diff mapping: the inks it borrows (`ok`,
    /// `error`, `info`, `text_muted`) must stay readable on
    /// `surface_raised`, the declared code ground — measured across
    /// every built-in theme, not assumed from the `bg`-anchored audit.
    /// The floor matches the semantic-state class (3.0:1); the failure
    /// message names the theme and measured value.
    #[test]
    fn diff_inks_clear_contrast_on_the_code_ground_in_every_theme() {
        use crate::theme::{contrast_ratio, themes};
        for theme in themes() {
            let t = &theme.tokens;
            for (name, ink) in [
                ("ok", t.ok),
                ("error", t.error),
                ("info", t.info),
                ("text_muted", t.text_muted),
            ] {
                let ratio = contrast_ratio(ink, t.surface_raised);
                assert!(
                    ratio >= 3.0,
                    "{}: diff ink `{name}` measures {ratio:.2}:1 on surface_raised",
                    theme.id
                );
            }
        }
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
