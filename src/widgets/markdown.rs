//! MarkdownView: RENDER's markdown blocks, themed and typeset.
//!
//! ```ignore
//! use abstracttui::widgets::MarkdownView;
//! let view = MarkdownView::new(doc_source)
//!     .scroll_offset(top.get())
//!     .element(&t)
//!     .build();
//! ```
//!
//! Typesetting (all tokens, §3.3): headings step `accent`(+BOLD) ->
//! `accent` -> `text`+BOLD with a `border` underline rule beneath level 1;
//! list markers in `accent_alt`; blockquotes carry a `border` bar with
//! `text_muted` prose; code fences render through the highlighter on the
//! `surface_raised` code ground (the same inks as `CodeView` — one
//! mapping, `code_token_color`); inline code = `surface_raised` chip;
//! links = `link` ink + underline. Layout happens at draw width (wrap),
//! deterministically — `MarkdownView::rows(...)` exposes the same fold
//! for the app's scroll clamp.
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rgba};
use crate::layout::Style as LayoutStyle;
use crate::render::md::{self, Block, Marker, MdStyles};
use crate::render::rich::{RichLine, RichText, Span};
use crate::render::{Attrs, Style};
use crate::text::{CLikeLexer, DiffLexer};
use crate::theme::TokenSet;
use crate::ui::{Element, StyledCanvas};

use super::code::{code_token_color, diff_rich_line};

/// Doc-vocabulary typesetting (0142): tables (column solving shared
/// with the Table widget), task items, and the doc layout fold with
/// heading rows.
#[path = "markdown_doc.rs"]
pub(crate) mod doc;
pub use doc::OutlineEntry;

/// In-flow image rows (0144): probe at typeset, decode lazily at first
/// draw, mosaic-only rendering.
#[path = "markdown_image.rs"]
pub(crate) mod imageflow;

/// Find-in-typeset-text (0148) + the text↔cells mapping substrate
/// shared with content selection (0160).
#[path = "markdown_search.rs"]
pub(crate) mod search;
pub use search::MdSearchMatch;

/// One typeset row: a rich line plus its chrome. Crate-shared: the Feed
/// widget caches these per item/block (backlog 0100) — ONE row recipe,
/// so a feed item and a MarkdownView can never typeset differently.
pub(crate) struct Row {
    pub(crate) line: RichLine,
    pub(crate) indent: i32,
    /// Full-width ground override (code fences).
    pub(crate) ground: Option<Rgba>,
    /// Leading quote bar.
    pub(crate) quote: bool,
    /// Full-width rule row (`---` and the level-1 underline).
    pub(crate) rule: bool,
    /// One mosaic slice of an in-flow image (0144): when set, the row
    /// paints image cells instead of `line` (which stays empty).
    pub(crate) image: Option<imageflow::MdImageSlice>,
}

impl Row {
    pub(crate) fn plain(line: RichLine) -> Row {
        Row {
            line,
            indent: 0,
            ground: None,
            quote: false,
            rule: false,
            image: None,
        }
    }
}

pub struct MarkdownView {
    source: String,
    scroll_offset: i32,
    layout: Option<LayoutStyle>,
    /// Search-highlight overlay (0148): matches from [`MarkdownView::find`]
    /// at the SAME width the element renders at, painted non-destructively
    /// over the typeset rows. Empty = zero extra work at draw.
    highlights: Vec<MdSearchMatch>,
    current_match: Option<usize>,
}

impl MarkdownView {
    pub fn new(source: impl Into<String>) -> MarkdownView {
        MarkdownView {
            source: source.into(),
            scroll_offset: 0,
            layout: None,
            highlights: Vec::new(),
            current_match: None,
        }
    }

    /// First visible typeset row (app-managed scrolling).
    pub fn scroll_offset(mut self, rows: i32) -> MarkdownView {
        self.scroll_offset = rows.max(0);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> MarkdownView {
        self.layout = Some(layout);
        self
    }

    /// Overlay search matches (0148): `matches` from [`MarkdownView::find`]
    /// computed at the width this element will draw at; `current` indexes
    /// into `matches` for the distinct current-match treatment. Painted as
    /// a style patch AFTER the rows — glyphs stay, tones change
    /// (`selection_bg`/`selection_fg`; the current match adds BOLD +
    /// UNDERLINE — the token set has no dedicated search tone, documented
    /// in the 0148 report).
    pub fn highlights(mut self, matches: Vec<MdSearchMatch>, current: Option<usize>) -> Self {
        self.highlights = matches;
        self.current_match = current;
        self
    }

    /// Typeset row count at `width` — the scroll clamp (same fold as the
    /// renderer, so the clamp can never drift from the pixels).
    pub fn rows(source: &str, t: &TokenSet, width: i32) -> usize {
        doc::layout_doc(source, t, width).rows.len()
    }

    /// Heading outline `(level, text)` — table-of-contents material.
    /// See [`MarkdownView::outline_rows`] for anchor ids + typeset row
    /// positions (0146).
    pub fn outline(source: &str, t: &TokenSet) -> Vec<(u8, String)> {
        md::parse(source, &md_styles(t))
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level, content } => Some((*level, content.plain())),
                _ => None,
            })
            .collect()
    }

    /// The document outline with TYPESET ROW positions (0146): each
    /// heading paired with the row its text starts at when the document
    /// is laid out at `width` — the row to scroll to for a TOC jump.
    /// Anchor ids are GitHub-compatible and deduplicated
    /// ([`md::outline`]); rows come from the SAME fold the renderer
    /// draws, so a jump can never drift from the pixels.
    pub fn outline_rows(source: &str, t: &TokenSet, width: i32) -> Vec<OutlineEntry> {
        doc::outline_rows(source, t, width)
    }

    /// Resolve an intra-document anchor (`#getting-started`, leading
    /// `#` optional) to the typeset row of its heading at `width` —
    /// `[text](#anchor)` link targets against [`md::outline`] ids.
    pub fn resolve_anchor(source: &str, t: &TokenSet, width: i32, anchor: &str) -> Option<usize> {
        let want = anchor.strip_prefix('#').unwrap_or(anchor);
        doc::outline_rows(source, t, width)
            .into_iter()
            .find(|e| e.heading.anchor_id == want)
            .map(|e| e.row)
    }

    /// Find `query` in the TYPESET text at `width` (0148): literal
    /// match, whole-fragment scope (matches never span wrapped rows —
    /// they live in what the eye sees). `case_insensitive` folds via
    /// Unicode lowercasing. Empty query = no matches, no work. Feed the
    /// result to [`MarkdownView::highlights`] and scroll to
    /// `matches[i].row`.
    pub fn find(
        source: &str,
        t: &TokenSet,
        width: i32,
        query: &str,
        case_insensitive: bool,
    ) -> Vec<MdSearchMatch> {
        search::find_in_rows(
            &doc::layout_doc(source, t, width).rows,
            query,
            case_insensitive,
        )
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let tokens = *t;
        let offset = self.scroll_offset as usize;
        let source = self.source;
        let highlights = self.highlights;
        let current = self.current_match;
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));
        // Draw-time typesetting, cached per width (resize re-lays-out;
        // steady-state repaints reuse).
        let mut cache: Option<(i32, Vec<Row>)> = None;
        Element::new().style(layout).draw(move |canvas, rect| {
            if rect.w <= 1 || rect.h <= 0 {
                return;
            }
            let rows = match &mut cache {
                Some((w, rows)) if *w == rect.w => rows,
                slot => {
                    let rows = doc::layout_doc(&source, &tokens, rect.w).rows;
                    &mut slot.insert((rect.w, rows)).1
                }
            };
            let offset = offset.min(rows.len().saturating_sub(1));
            draw_rows(canvas, rect, &tokens, &rows[offset..]);
            if !highlights.is_empty() {
                search::draw_highlights(canvas, rect, &tokens, rows, offset, &highlights, current);
            }
        })
    }
}

/// The markdown span vocabulary in theme tokens. `base` deliberately
/// carries NO fg: parse_inline stamps `base` onto every plain span, and
/// an explicit fg there would defeat block-level recoloring (blockquotes
/// dim to `text_muted`); fg-less spans inherit at draw time instead.
/// Crate-shared with the Feed widget (one mapping, no drift).
pub(crate) fn md_styles(t: &TokenSet) -> MdStyles {
    MdStyles {
        base: Style::EMPTY,
        bold: Style::new().attrs(Attrs::BOLD),
        italic: Style::new().attrs(Attrs::ITALIC),
        // Inline code: a raised chip, body ink.
        code: Style::new().fg(t.text).bg(t.surface_raised),
        link: Style::new().fg(t.link).attrs(Attrs::UNDERLINE),
        heading: Style::new().attrs(Attrs::BOLD),
    }
}

/// The block -> typeset-rows recipe, crate-shared (backlog 0100): the
/// Feed widget caches rows per item/block through this same fold, so a
/// feed item and a `MarkdownView` can never typeset differently.
pub(crate) struct BlockTypesetter {
    styles: MdStyles,
    lexer: CLikeLexer,
    diff: DiffLexer,
    code_base: Style,
    t: TokenSet,
}

impl BlockTypesetter {
    pub(crate) fn new(t: &TokenSet) -> BlockTypesetter {
        BlockTypesetter {
            styles: md_styles(t),
            lexer: CLikeLexer::default(),
            diff: DiffLexer::new(),
            code_base: Style::new().fg(t.text),
            t: *t,
        }
    }

    /// The span styles matching this typesetter's tokens — parse
    /// sources with these so inline patches line up.
    pub(crate) fn styles(&self) -> &MdStyles {
        &self.styles
    }

    /// Append `block`'s typeset rows to `out` at `width`. `separate`
    /// applies the document spacing policy: one blank row before every
    /// non-list block when `out` is not empty (list items stack tight).
    pub(crate) fn push_block(&self, out: &mut Vec<Row>, block: &Block, width: i32, separate: bool) {
        let t = &self.t;
        let blank = |rows: &mut Vec<Row>| {
            if separate && !rows.is_empty() {
                rows.push(Row::plain(RichLine::new()));
            }
        };
        match block {
            Block::Heading { level, content } => {
                blank(out);
                let ink = match level {
                    1 => Style::new().fg(t.accent).attrs(Attrs::BOLD),
                    2 => Style::new().fg(t.accent),
                    _ => Style::new().fg(t.text).attrs(Attrs::BOLD),
                };
                let mut line = RichLine::new();
                for span in &content.spans {
                    line.push(Span::new(span.text.clone(), span.style.merge(ink)));
                }
                out.push(Row::plain(line));
                if *level == 1 {
                    out.push(Row {
                        line: RichLine::new(),
                        indent: 0,
                        ground: None,
                        quote: false,
                        rule: true,
                        image: None,
                    });
                }
            }
            Block::Paragraph(line) => {
                blank(out);
                for wrapped in wrap_line(line.clone(), width) {
                    out.push(Row::plain(wrapped));
                }
            }
            Block::ListItem {
                depth,
                marker,
                content,
            } => {
                let indent = 2 + *depth as i32 * 2;
                let mut line = RichLine::new();
                let marker_text = match marker {
                    Marker::Bullet => "• ".to_string(),
                    Marker::Number(n) => format!("{n}. "),
                };
                line.push(Span::new(marker_text, Style::new().fg(t.accent_alt)));
                for span in &content.spans {
                    line.push(span.clone());
                }
                for (i, wrapped) in wrap_line(line, width - indent).into_iter().enumerate() {
                    out.push(Row {
                        line: wrapped,
                        // Continuation rows hang past the marker.
                        indent: indent + if i > 0 { 2 } else { 0 },
                        ground: None,
                        quote: false,
                        rule: false,
                        image: None,
                    });
                }
            }
            Block::Blockquote(line) => {
                blank(out);
                let mut muted = RichLine::new();
                for span in &line.spans {
                    // Quote prose dims; spans with their OWN ink (links,
                    // inline code) keep it.
                    let style = if span.style.fg.is_none() {
                        span.style.fg(t.text_muted)
                    } else {
                        span.style
                    };
                    muted.push(Span::new(span.text.clone(), style));
                }
                for wrapped in wrap_line(muted, width - 2) {
                    out.push(Row {
                        line: wrapped,
                        indent: 2,
                        ground: None,
                        quote: true,
                        rule: false,
                        image: None,
                    });
                }
            }
            Block::CodeFence { lang, lines } => {
                blank(out);
                // Fence labels route the lexer (0140's diff slice):
                // ```diff / ```patch tint through the diff mapping; every
                // other label keeps the C-like lexer as before.
                let diff_fence = DiffLexer::matches_lang(lang);
                for code_line in lines {
                    let rich = if diff_fence {
                        diff_rich_line(code_line, &self.diff, self.code_base, t)
                    } else {
                        RichLine::from_highlighted(code_line, &self.lexer, self.code_base, |k| {
                            Style::new().fg(code_token_color(k, t))
                        })
                    };
                    let mut padded = RichLine::new();
                    padded.push(Span::new(" ", self.code_base));
                    for span in rich.spans {
                        padded.push(span);
                    }
                    out.push(Row {
                        line: padded,
                        indent: 1,
                        ground: Some(t.surface_raised),
                        quote: false,
                        rule: false,
                        image: None,
                    });
                }
            }
            Block::Rule => {
                blank(out);
                out.push(Row {
                    line: RichLine::new(),
                    indent: 0,
                    ground: None,
                    quote: false,
                    rule: true,
                    image: None,
                });
            }
        }
    }
}

fn wrap_line(line: RichLine, width: i32) -> Vec<RichLine> {
    RichText::from_lines(vec![line]).wrap(width.max(4)).lines
}

/// Paint typeset rows into `rect`, one row per line from `rect.y` down,
/// clipped at `rect.bottom()`. Crate-shared with the Feed widget.
pub(crate) fn draw_rows(
    canvas: &mut dyn StyledCanvas,
    rect: crate::base::Rect,
    t: &TokenSet,
    rows: &[Row],
) {
    for (i, row) in rows.iter().enumerate() {
        let y = rect.y + i as i32;
        if y >= rect.bottom() {
            break;
        }
        if row.rule {
            for x in rect.x..rect.right() {
                canvas.put(Point::new(x, y), '─', t.border, Rgba::TRANSPARENT);
            }
            continue;
        }
        // In-flow image slice (0144): decode-on-first-draw, mosaic
        // cells only. The row's `line` is empty by construction.
        if let Some(slice) = &row.image {
            imageflow::draw_image_row(canvas, rect, y, t, row.indent, slice);
            continue;
        }
        if let Some(ground) = row.ground {
            canvas.fill(
                crate::base::Rect::new(rect.x, y, rect.w, 1),
                ' ',
                t.text,
                ground,
            );
        }
        if row.quote {
            canvas.put(Point::new(rect.x, y), '▎', t.border, Rgba::TRANSPARENT);
        }
        let mut x = rect.x + row.indent;
        for span in &row.line.spans {
            let style = if span.style.fg.is_none() {
                span.style.fg(t.text)
            } else {
                span.style
            };
            x += crate::widgets::richtext::print_span_clipped(
                canvas,
                x,
                y,
                rect.right(),
                &span.text,
                &style,
            );
            if x >= rect.right() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    const DOC: &str = "# Title\n\nBody with `code` inline.\n\n- first\n- second\n\n> wisdom\n\n```\nfn main() {}\n```\n";

    fn cell_of(row: &str, needle: &str) -> i32 {
        let byte = row.find(needle).unwrap();
        row[..byte].chars().count() as i32
    }

    #[test]
    fn heading_list_quote_and_fence_chrome() {
        let t = default_theme().tokens;
        let c = draw_into(MarkdownView::new(DOC).element(&t), Size::new(28, 14));
        // Level-1 heading in accent + underline rule beneath.
        let title_y = 0;
        assert!(row(&c, title_y).starts_with("Title"));
        assert_eq!(c.cell(Point::new(0, title_y)).unwrap().1, t.accent);
        assert!(row(&c, title_y + 1).starts_with('─'));
        assert_eq!(c.cell(Point::new(0, title_y + 1)).unwrap().1, t.border);

        // Inline code chip ground.
        let body_y = (0..14).find(|y| row(&c, *y).contains("code")).unwrap();
        let cx = cell_of(&row(&c, body_y), "code");
        assert_eq!(c.cell(Point::new(cx, body_y)).unwrap().2, t.surface_raised);

        // List marker ink.
        let li_y = (0..14).find(|y| row(&c, *y).contains("• first")).unwrap();
        let mx = cell_of(&row(&c, li_y), "•");
        assert_eq!(c.cell(Point::new(mx, li_y)).unwrap().1, t.accent_alt);

        // Blockquote bar + muted prose.
        let q_y = (0..14).find(|y| row(&c, *y).contains("wisdom")).unwrap();
        assert_eq!(c.cell(Point::new(0, q_y)).unwrap().0, '▎');
        let wx = cell_of(&row(&c, q_y), "wisdom");
        assert_eq!(c.cell(Point::new(wx, q_y)).unwrap().1, t.text_muted);

        // Code fence: raised ground + keyword ink.
        let f_y = (0..14).find(|y| row(&c, *y).contains("fn main")).unwrap();
        let fx = cell_of(&row(&c, f_y), "fn");
        let (_, fg, bg) = c.cell(Point::new(fx, f_y)).unwrap();
        assert_eq!(fg, t.syntax_keyword);
        assert_eq!(bg, t.surface_raised);
    }

    #[test]
    fn outline_rows_and_scroll_share_the_fold() {
        let t = default_theme().tokens;
        assert_eq!(
            MarkdownView::outline(DOC, &t),
            vec![(1, "Title".to_string())]
        );
        let total = MarkdownView::rows(DOC, &t, 28);
        assert!(total >= 10, "typeset rows: {total}");
        // Scrolling by one hides the title row.
        let c = draw_into(
            MarkdownView::new(DOC).scroll_offset(1).element(&t),
            Size::new(28, 6),
        );
        assert!(!row(&c, 0).contains("Title"));
    }

    #[test]
    fn diff_fences_tint_added_removed_and_plain_fences_stay_clike() {
        let t = default_theme().tokens;
        let doc = "```diff\n-old line\n+new line\n```\n\n```\nfn main() {}\n```\n";
        let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(28, 10));
        // Diff fence: removed line in error ink, added in ok, on the
        // fence's raised ground.
        let minus_y = (0..10).find(|y| row(&c, *y).contains("-old")).unwrap();
        let mx = cell_of(&row(&c, minus_y), "-old");
        let (_, fg, bg) = c.cell(Point::new(mx, minus_y)).unwrap();
        assert_eq!(fg, t.error);
        assert_eq!(bg, t.surface_raised);
        let plus_y = (0..10).find(|y| row(&c, *y).contains("+new")).unwrap();
        let px = cell_of(&row(&c, plus_y), "+new");
        assert_eq!(c.cell(Point::new(px, plus_y)).unwrap().1, t.ok);
        // The unlabeled fence still renders the C-like keyword ink.
        let fn_y = (0..10).find(|y| row(&c, *y).contains("fn main")).unwrap();
        let fx = cell_of(&row(&c, fn_y), "fn");
        assert_eq!(c.cell(Point::new(fx, fn_y)).unwrap().1, t.syntax_keyword);
    }

    #[test]
    fn wrapping_and_tiny_rects_never_panic() {
        let t = default_theme().tokens;
        let long = "paragraph with quite a few words that must wrap around";
        let c = draw_into(MarkdownView::new(long).element(&t), Size::new(12, 8));
        assert!(row(&c, 0).trim_end().len() <= 12);
        for size in [Size::new(0, 0), Size::new(2, 1), Size::new(5, 2)] {
            let _ = draw_into(MarkdownView::new(DOC).element(&t), size);
        }
    }
}
