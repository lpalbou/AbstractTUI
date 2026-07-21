//! Rich text: styled span runs with grapheme-correct measurement,
//! span-preserving wrap, and surface drawing.
//!
//! The model markdown/log/chat/code widgets render through. It lives in
//! `render` (not `text`) because the span currency is [`Style`] — text
//! must stay import-free of render (render → text is the one arrow), and
//! a style-generic model in `text` would be abstraction for its own sake.
//! Measurement/segmentation all route through `crate::text`, so the ONE
//! width policy holds here by construction.
//!
//! Links carry URLS, not ids: link ids are surface-local
//! (`Surface::register_link`), so a surface-independent model must hold
//! the URI and resolve it at draw time.
//!
//! Allocation posture: parse/wrap allocate (owned spans — parsed once,
//! rendered many frames); DRAWING allocates nothing beyond what
//! `Surface::draw_text` already does. Wrap reuses one scratch pieces
//! buffer per call and coalesces output spans, so span counts stay
//! proportional to STYLE CHANGES, not to clusters.

use crate::base::Rect;
use crate::text;

use super::style::Style;
use super::surface::Surface;

/// A run of text under one style patch (plus an optional hyperlink URL).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    /// The run's text (no newlines; constructors enforce it upstream).
    pub text: String,
    /// The styling patch this run wears.
    pub style: Style,
    /// Hyperlink URL (resolved to a surface-local id at draw time).
    pub link: Option<String>,
}

impl Span {
    /// A styled run without a link.
    pub fn new(text: impl Into<String>, style: Style) -> Span {
        Span {
            text: text.into(),
            style,
            link: None,
        }
    }

    /// An unstyled run (inherits surrounding ink at draw).
    pub fn plain(text: impl Into<String>) -> Span {
        Span::new(text, Style::EMPTY)
    }

    /// Attaches a hyperlink URL.
    pub fn with_link(mut self, url: impl Into<String>) -> Span {
        self.link = Some(url.into());
        self
    }

    /// Display width in columns (grapheme-aware).
    pub fn width(&self) -> i32 {
        text::width(&self.text)
    }

    /// Style identity for coalescing: same patch, same link target.
    fn same_ink(&self, other: &Span) -> bool {
        self.style == other.style && self.link == other.link
    }
}

/// One visual line: spans in order. Contains no newlines by contract
/// (constructors strip/split them; `RichText` is the multi-line unit).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichLine {
    /// Ordered spans; adjacent same-ink spans are coalesced by [`RichLine::push`].
    pub spans: Vec<Span>,
}

impl RichLine {
    /// An empty line.
    pub fn new() -> RichLine {
        RichLine::default()
    }

    /// Wraps pre-built spans verbatim (no coalescing pass).
    pub fn from_spans(spans: Vec<Span>) -> RichLine {
        RichLine { spans }
    }

    /// Appends, merging into the previous span when the ink matches (keeps
    /// span counts proportional to style changes).
    pub fn push(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.same_ink(&span) {
                last.text.push_str(&span.text);
                return;
            }
        }
        self.spans.push(span);
    }

    /// Borrowed-text append: merges into the last span WITHOUT allocating
    /// when the ink matches (the wrap path calls this once per cluster —
    /// a temp `Span`/`String` per cluster was the wrap's dominant
    /// allocation churn). Link is cloned only when a new span is born.
    fn push_run(&mut self, text: &str, style: Style, link: &Option<String>) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style == style && last.link == *link {
                last.text.push_str(text);
                return;
            }
        }
        self.spans.push(Span {
            text: text.to_string(),
            style,
            link: link.clone(),
        });
    }

    /// Display width in columns (sum of span widths).
    pub fn width(&self) -> i32 {
        self.spans.iter().map(Span::width).sum()
    }

    /// True when the line carries no text at all.
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Concatenated text without styles (assertions, clipboard).
    pub fn plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Builds a line from a plain string tinted by a [`text::Highlighter`],
    /// with `map` turning token kinds into style patches (the theme hook —
    /// render stays theme-agnostic). Bytes outside every token get
    /// `base`. Newlines/controls are stripped by draw/wrap downstream.
    pub fn from_highlighted(
        line: &str,
        lexer: &dyn text::Highlighter,
        base: Style,
        map: impl Fn(text::TokenKind) -> Style,
    ) -> RichLine {
        let mut out = RichLine::new();
        let mut cursor = 0;
        for (range, kind) in lexer.spans(line) {
            if range.start > cursor {
                out.push(Span::new(&line[cursor..range.start], base));
            }
            out.push(Span::new(&line[range.clone()], map(kind)));
            cursor = range.end;
        }
        if cursor < line.len() {
            out.push(Span::new(&line[cursor..], base));
        }
        out
    }
}

/// Multi-line rich text: the block the draw/wrap helpers operate on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    /// Visual lines, one per row when drawn.
    pub lines: Vec<RichLine>,
}

/// Horizontal alignment for [`RichText::draw`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HAlign {
    /// Flush left (the default).
    #[default]
    Left,
    /// Centered per line (odd leftovers lean left).
    Center,
    /// Flush right.
    Right,
}

impl RichText {
    /// An empty block.
    pub fn new() -> RichText {
        RichText::default()
    }

    /// Wraps pre-built lines verbatim.
    pub fn from_lines(lines: Vec<RichLine>) -> RichText {
        RichText { lines }
    }

    /// A single-style block from plain text (`\n`-separated).
    pub fn plain(s: &str, style: Style) -> RichText {
        let lines = s
            .split('\n')
            .map(|l| {
                let l = l.strip_suffix('\r').unwrap_or(l);
                RichLine::from_spans(if l.is_empty() {
                    Vec::new()
                } else {
                    vec![Span::new(l, style)]
                })
            })
            .collect();
        RichText { lines }
    }

    /// Line count (rows when drawn unwrapped).
    pub fn height(&self) -> i32 {
        self.lines.len() as i32
    }

    /// Widest line's display width in columns.
    pub fn width(&self) -> i32 {
        self.lines.iter().map(RichLine::width).max().unwrap_or(0)
    }

    /// Word-wraps every line into `max_width` columns, PRESERVING span
    /// styles across wrap boundaries (a word split out of a bold span
    /// stays bold on the next line). Same wrapping contract as
    /// [`text::wrap`]: whitespace at a break is consumed, long words break
    /// at cluster boundaries, an oversized cluster overflows alone,
    /// empty logical lines survive as empty visual lines.
    pub fn wrap(&self, max_width: i32) -> RichText {
        let max_width = max_width.max(1);
        let mut out = Vec::new();
        let mut pieces: Vec<Piece<'_>> = Vec::new();
        for line in &self.lines {
            pieces.clear();
            collect_pieces(line, &mut pieces);
            wrap_pieces(&pieces, max_width, &mut out);
        }
        RichText { lines: out }
    }

    /// Draws into `rect` (clipped): one visual line per row, aligned
    /// horizontally, lines beyond `rect.h` dropped, overwide lines
    /// ellipsis-truncated. Span styles are patches over the surface's
    /// existing paint (panel backgrounds show through); span links
    /// register into the surface's URI table at draw time.
    ///
    /// Wrapping is the CALLER's move (`wrap` first if desired) — a draw
    /// that silently re-wraps would fight widgets that measured first.
    pub fn draw(&self, s: &mut Surface, rect: Rect, align: HAlign) {
        let rect = rect.intersect(s.bounds());
        if rect.is_empty() {
            return;
        }
        for (i, line) in self.lines.iter().enumerate().take(rect.h as usize) {
            let y = rect.y + i as i32;
            let line_w = line.width();
            let x0 = match align {
                HAlign::Left => rect.x,
                HAlign::Center => rect.x + (rect.w - line_w).max(0) / 2,
                HAlign::Right => rect.x + (rect.w - line_w).max(0),
            };
            if line_w <= rect.w {
                let mut pen = x0;
                for span in &line.spans {
                    pen += draw_span(s, pen, y, span);
                }
            } else {
                draw_truncated(s, rect, y, line);
            }
        }
    }
}

fn draw_span(s: &mut Surface, x: i32, y: i32, span: &Span) -> i32 {
    let style = resolve_link(s, span);
    s.draw_text(x, y, &span.text, style)
}

/// Left-aligned ellipsis truncation for a line wider than the rect
/// (alignment is meaningless once the line overflows).
fn draw_truncated(s: &mut Surface, rect: Rect, y: i32, line: &RichLine) {
    let budget = rect.w - 1; // reserve the ellipsis column
    let mut pen = rect.x;
    let mut used = 0i32;
    let mut ellipsis_style = Style::EMPTY;
    'spans: for span in &line.spans {
        let style = resolve_link(s, span);
        ellipsis_style = style;
        for seg in text::segments(&span.text) {
            if seg.width <= 0 {
                continue;
            }
            if used + seg.width > budget {
                break 'spans;
            }
            pen += s.draw_text(pen, y, seg.cluster, style);
            used += seg.width;
        }
    }
    s.draw_text(pen, y, "\u{2026}", ellipsis_style);
}

fn resolve_link(s: &mut Surface, span: &Span) -> Style {
    match &span.link {
        Some(url) => span.style.link(s.register_link(url)),
        None => span.style,
    }
}

// ---------------------------------------------------------------------------
// Span-preserving wrap internals
// ---------------------------------------------------------------------------

/// One measured cluster carrying its span's ink. `Style` is `Copy`; the
/// link is borrowed and cloned only when a piece lands in an output span.
struct Piece<'a> {
    cluster: &'a str,
    width: i32,
    style: Style,
    link: &'a Option<String>,
    is_space: bool,
}

impl Piece<'_> {
    fn emit(&self, line: &mut RichLine) {
        line.push_run(self.cluster, self.style, self.link);
    }
}

fn collect_pieces<'a>(line: &'a RichLine, out: &mut Vec<Piece<'a>>) {
    for span in &line.spans {
        for seg in text::segments(&span.text) {
            // Controls have no place in a visual line; strip here so the
            // wrapper never counts them (draw would strip them anyway).
            if seg.width == 0 && seg.cluster.chars().any(char::is_control) {
                continue;
            }
            out.push(Piece {
                cluster: seg.cluster,
                width: seg.width,
                style: span.style,
                link: &span.link,
                is_space: seg.cluster.chars().next().is_some_and(char::is_whitespace),
            });
        }
    }
}

/// Greedy word wrap over pieces; emits coalesced rich lines. Contract
/// parity with `text::wrap` is test-pinned (`wrap_matches_plain_text_wrap`).
fn wrap_pieces(pieces: &[Piece<'_>], max_width: i32, out: &mut Vec<RichLine>) {
    let rows_before = out.len();
    let mut current = RichLine::new();
    let mut current_w = 0i32;

    fn flush(current: &mut RichLine, current_w: &mut i32, out: &mut Vec<RichLine>) {
        // Trailing whitespace at a break is consumed.
        while let Some(last) = current.spans.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
            if last.text.is_empty() {
                current.spans.pop();
            } else {
                break;
            }
        }
        if !current.spans.is_empty() {
            out.push(std::mem::take(current));
        }
        *current_w = 0;
    }

    let mut i = 0;
    while i < pieces.len() {
        if pieces[i].is_space {
            // Whitespace run: kept when it fits mid-line, consumed when it
            // is the break point, dropped at line starts.
            let start = i;
            let mut w = 0;
            while i < pieces.len() && pieces[i].is_space {
                w += pieces[i].width;
                i += 1;
            }
            if current_w > 0 && current_w + w <= max_width {
                for p in &pieces[start..i] {
                    p.emit(&mut current);
                }
                current_w += w;
            } else if current_w > 0 {
                flush(&mut current, &mut current_w, out);
            }
            continue;
        }
        // Word: maximal non-space run (freely crossing span boundaries —
        // "**bo**ld" is one word).
        let start = i;
        let mut w = 0;
        while i < pieces.len() && !pieces[i].is_space {
            w += pieces[i].width;
            i += 1;
        }
        if current_w > 0 && current_w + w > max_width {
            flush(&mut current, &mut current_w, out);
        }
        if w <= max_width {
            for p in &pieces[start..i] {
                p.emit(&mut current);
            }
            current_w += w;
        } else {
            // Long word: split at cluster boundaries; an oversized single
            // cluster overflows alone (truth over comfort, as text::wrap).
            for p in &pieces[start..i] {
                if current_w > 0 && current_w + p.width > max_width {
                    flush(&mut current, &mut current_w, out);
                }
                p.emit(&mut current);
                current_w += p.width;
            }
        }
    }
    flush(&mut current, &mut current_w, out);
    if out.len() == rows_before {
        out.push(RichLine::new()); // empty logical line = one empty row
    }
}

#[cfg(test)]
#[path = "rich_tests.rs"]
mod tests;
