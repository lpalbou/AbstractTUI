//! Markdown-lite: a small, HONEST subset parsed into [`RichText`]-ready
//! blocks. This is the text-model half only — widgets (DESIGN/REACT) own
//! layout, spacing and chrome.
//!
//! SUPPORTED, exactly (docs/design/render.md §2.8):
//! - Inline: `**bold**`, `*italic*`, `~~strikethrough~~`, `` `code` ``,
//!   `[text](url)`, and backslash escapes for `\* \~ \` \[ \] \( \) \\ \#`.
//! - Blocks: `#`..`######` headings; `-`/`*`/`+` unordered and `N.`
//!   ordered list items (2-space indent steps); `>` blockquotes (one
//!   level; nested `>` folds into the same level); fenced code
//!   (` ``` `lang ... ` ``` `, verbatim, no inline parsing); `---`/`***`
//!   horizontal rules; blank-line paragraph separation.
//!
//! NOT supported in the CORE vocabulary (deliberately, parsed as
//! literal text): nested emphasis, `__`/`_` emphasis, setext headings,
//! HTML, reference links, multi-paragraph list items. Unclosed markers
//! degrade to literal text — no input can fail to parse. GFM tables,
//! block images and task-list items live in the DOC vocabulary:
//! [`parse_doc`] / [`DocBlock`] (additive beside this exhaustive enum).
//!
//! Styling: emphasis/code/link/heading styles are PATCHES supplied by
//! [`MdStyles`]; the defaults are attribute-only (theme-free), and
//! DESIGN's theme maps override them. Inline styles COMPOSE with the
//! block style via `Style::merge` (bold inside a heading keeps the
//! heading color and gains BOLD). Links carry their URL on the span; ids
//! resolve at draw time.
//!
//! End to end — markdown to cells:
//!
//! ```
//! use abstracttui::base::{Rect, Size};
//! use abstracttui::render::md::{self, MdStyles};
//! use abstracttui::render::{snapshot, Cell, HAlign, Surface};
//!
//! let styles = MdStyles::default();
//! let blocks = md::parse("# Title\n\nSome **bold** text and `code`.", &styles);
//! let rich = md::to_rich_text(&blocks, &styles).wrap(30); // wrap is the caller's move
//!
//! let mut s = Surface::new(Size::new(30, 6), Cell::EMPTY);
//! rich.draw(&mut s, Rect::new(0, 0, 30, 6), HAlign::Left);
//! let dump = snapshot(&s);
//! assert!(dump.contains("Title"));
//! assert!(dump.contains("Some bold text and code."));
//! ```

use super::cell::Attrs;
use super::rich::{RichLine, RichText, Span};
use super::style::Style;

/// Streaming session (backlog 0110): append deltas, re-parse only the
/// open tail block. Lives beside the parser so block-closing rules
/// share one implementation.
#[path = "md_stream.rs"]
mod stream;
pub use stream::StreamSession;

/// Document vocabulary (0142/0144): the core blocks extended with GFM
/// tables, block images and task items — additive beside [`Block`]
/// (which shipped exhaustive and cannot grow variants).
#[path = "md_doc.rs"]
mod doc;
pub(crate) use doc::parse_doc_sourced;
pub use doc::{parse_doc, CellAlign, DocBlock, ImageBlock, TableBlock, TaskBlock};

/// Streaming session for the doc vocabulary (0142).
#[path = "md_doc_stream.rs"]
mod doc_stream;
pub use doc_stream::DocStreamSession;

/// Heading outline + GitHub-compatible anchor slugs (0146).
#[path = "md_outline.rs"]
mod outline_impl;
pub use outline_impl::{outline, slugify, Heading};

/// One parsed block. Paragraph/heading/list/quote content is a single
/// logical [`RichLine`] — wrapping is the renderer's move.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Soft-joined running text between blank lines.
    Paragraph(RichLine),
    /// `#`..`######` heading; `level` is the hash count (1..=6).
    Heading {
        /// Hash count, 1..=6.
        level: u8,
        /// Heading text with inline styles applied.
        content: RichLine,
    },
    /// One list item (`-`/`*`/`+` or `N.`).
    ListItem {
        /// Nesting depth from 2-space indent steps (0 = top level).
        depth: u8,
        /// Bullet or number, as written.
        marker: Marker,
        /// Item text with inline styles applied.
        content: RichLine,
    },
    /// `>` quote content (nested `>` folds into one level).
    Blockquote(RichLine),
    /// ``` fenced code: verbatim lines, no inline parsing.
    CodeFence {
        /// The info string after the opening fence (may be empty).
        lang: String,
        /// Code lines exactly as written.
        lines: Vec<String>,
    },
    /// `---`/`***` horizontal rule.
    Rule,
}

/// List-item marker, as written in the source.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Marker {
    /// `-`, `*` or `+`.
    Bullet,
    /// `N.` with the written number (renderers may renumber).
    Number(u32),
}

/// Style patches for the markdown vocabulary. Defaults are theme-free
/// attributes; a theme substitutes colored patches (see
/// [`MdStyles::with_ink`]).
///
/// `base` contract (learned from the first theme integration): `base` is
/// stamped onto EVERY plain span by the inline parser, so a `base`
/// carrying an explicit `fg` DEFEATS downstream block recoloring (a
/// blockquote that dims its content, a list that mutes its markers).
/// Keep `base` fg-less (`Style::EMPTY`) and let spans inherit ink at
/// draw time; put color into the ELEMENT patches instead.
#[derive(Clone, Debug)]
pub struct MdStyles {
    /// Stamped on every plain span. KEEP FG-LESS (see the type docs).
    pub base: Style,
    /// `**bold**` patch, merged onto the block style.
    pub bold: Style,
    /// `*italic*` patch.
    pub italic: Style,
    /// `` `code` `` patch (also the fence body ink via `to_rich_text`).
    pub code: Style,
    /// `[text](url)` patch; the URL rides the span, ids resolve at draw.
    pub link: Style,
    /// Heading patch, all levels (widgets differentiate levels).
    pub heading: Style,
}

impl Default for MdStyles {
    fn default() -> Self {
        MdStyles {
            base: Style::EMPTY,
            bold: Style::new().attrs(Attrs::BOLD),
            italic: Style::new().attrs(Attrs::ITALIC),
            code: Style::new().attrs(Attrs::DIM),
            link: Style::new().attrs(Attrs::UNDERLINE),
            heading: Style::new().attrs(Attrs::BOLD),
        }
    }
}

impl MdStyles {
    /// The canonical theme mapping, taking plain ink colors (render sits
    /// below `theme` in the layer map, so tokens arrive as `Rgba` — the
    /// markdown widget resolves `TokenSet` fields and calls this):
    /// inline code renders as a raised chip (`code_fg` on `code_bg`),
    /// links are `link_fg` + underline, emphasis/headings stay
    /// attribute-only so they inherit the surrounding block ink, and
    /// `base` is fg-less per the field contract above.
    pub fn with_ink(
        code_fg: crate::base::Rgba,
        code_bg: crate::base::Rgba,
        link_fg: crate::base::Rgba,
    ) -> MdStyles {
        MdStyles {
            base: Style::EMPTY,
            bold: Style::new().attrs(Attrs::BOLD),
            italic: Style::new().attrs(Attrs::ITALIC),
            code: Style::new().fg(code_fg).bg(code_bg),
            link: Style::new().fg(link_fg).attrs(Attrs::UNDERLINE),
            heading: Style::new().attrs(Attrs::BOLD),
        }
    }
}

/// Parses a document. Every input parses — degradation is always "treat
/// as literal text", never an error.
pub fn parse(src: &str, styles: &MdStyles) -> Vec<Block> {
    parse_core(src, styles, false)
        .into_iter()
        .map(|(block, _)| block)
        .collect()
}

pub(crate) fn inline_source(
    src: &str,
    styles: &MdStyles,
    style: Style,
    base: usize,
    enabled: bool,
) -> (RichLine, super::content::SourcePart) {
    use super::content::{ContentKind, SourcePart, SourceRun};
    let mut runs = Vec::new();
    let line = parse_inline_traced(src, styles, style, |range, display| {
        if enabled {
            runs.push(SourceRun {
                display: display..display + range.len(),
                source: base + range.start..base + range.end,
            });
        }
    });
    (
        line,
        SourcePart {
            context: None,
            scope: base..base + src.len(),
            kind: ContentKind::Paragraph,
            runs,
        },
    )
}

pub(crate) fn parse_core(
    src: &str,
    styles: &MdStyles,
    mapped: bool,
) -> Vec<(Block, super::content::SourceBlock)> {
    use super::content::{ContentKind, SourceBlock, SourcePart};
    let mut blocks = Vec::new();
    let mut lines = src.lines().peekable();
    let line_starts: Vec<usize> = if mapped {
        std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect()
    } else {
        Vec::new()
    };
    let mut para: Option<(RichLine, SourcePart, usize)> = None;
    let offset = |s: &str| s.as_ptr() as usize - src.as_ptr() as usize;
    let flush = |para: &mut Option<(RichLine, SourcePart, usize)>,
                 blocks: &mut Vec<(Block, SourceBlock)>| {
        if let Some((line, part, _)) = para.take() {
            blocks.push((
                Block::Paragraph(line),
                SourceBlock {
                    parts: if mapped { vec![part] } else { Vec::new() },
                },
            ));
        }
    };
    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            flush(&mut para, &mut blocks);
            let mut body = Vec::new();
            let mut parts = Vec::new();
            let mut scope = offset(raw) + raw.len()..offset(raw) + raw.len();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                let range = offset(l)..offset(l) + l.len();
                if parts.is_empty() {
                    scope.start = range.start;
                }
                scope.end = range.end;
                if mapped {
                    parts.push(SourcePart::literal(range, ContentKind::CodeBlock));
                }
                body.push(l.to_string());
            }
            if mapped {
                let context = std::rc::Rc::new(super::content::ContentContext::Code {
                    language: rest.trim().into(),
                    lines: line_starts.partition_point(|start| *start <= scope.start)
                        ..line_starts.partition_point(|start| *start <= scope.end) + 1,
                });
                for part in &mut parts {
                    part.scope = scope.clone();
                    part.context = Some(context.clone());
                }
            }
            blocks.push((
                Block::CodeFence {
                    lang: rest.trim().into(),
                    lines: body,
                },
                SourceBlock { parts },
            ));
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush(&mut para, &mut blocks);
            blocks.push((Block::Rule, SourceBlock { parts: vec![] }));
            continue;
        }
        if let Some(h) = heading_level(trimmed) {
            flush(&mut para, &mut blocks);
            let content = trimmed[h as usize + 1..].trim_start();
            let (content, part) = inline_source(
                content,
                styles,
                styles.base.merge(styles.heading),
                offset(content),
                mapped,
            );
            blocks.push((
                Block::Heading { level: h, content },
                SourceBlock {
                    parts: if mapped { vec![part] } else { Vec::new() },
                },
            ));
            continue;
        }
        if trimmed.starts_with('>') {
            flush(&mut para, &mut blocks);
            let mut rest = trimmed;
            while let Some(r) = rest.strip_prefix('>') {
                rest = r.trim_start();
            }
            let (content, part) = inline_source(rest, styles, styles.base, offset(rest), mapped);
            blocks.push((
                Block::Blockquote(content),
                SourceBlock {
                    parts: if mapped { vec![part] } else { Vec::new() },
                },
            ));
            continue;
        }
        if let Some((marker, rest)) = list_marker(trimmed) {
            flush(&mut para, &mut blocks);
            let (content, part) = inline_source(rest, styles, styles.base, offset(rest), mapped);
            blocks.push((
                Block::ListItem {
                    depth: ((line.len() - trimmed.len()) / 2).min(8) as u8,
                    marker,
                    content,
                },
                SourceBlock {
                    parts: if mapped { vec![part] } else { Vec::new() },
                },
            ));
            continue;
        }
        let (inline, part) = inline_source(trimmed, styles, styles.base, offset(trimmed), mapped);
        let count = inline.spans.iter().map(|s| s.text.len()).sum::<usize>();
        match &mut para {
            Some((acc, accumulated, display)) => {
                acc.push(Span::new(" ", styles.base));
                for s in inline.spans {
                    acc.push(s);
                }
                for mut run in part.runs {
                    run.display.start += *display + 1;
                    run.display.end += *display + 1;
                    accumulated.runs.push(run);
                }
                accumulated.scope.end = part.scope.end;
                *display += count + 1;
            }
            None => para = Some((inline, part, count)),
        }
    }
    flush(&mut para, &mut blocks);
    blocks
}

/// Convenience: paragraphs/headings/quotes/lists flattened into one
/// [`RichText`] (one line per block; code verbatim; rules as `───`).
/// Widgets wanting real spacing/prefixes consume [`parse`] directly.
pub fn to_rich_text(blocks: &[Block], styles: &MdStyles) -> RichText {
    let mut lines = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(l) | Block::Heading { content: l, .. } => lines.push(l.clone()),
            Block::Blockquote(l) => {
                let mut line = RichLine::new();
                line.push(Span::new("│ ", styles.base));
                for s in &l.spans {
                    line.push(s.clone());
                }
                lines.push(line);
            }
            Block::ListItem {
                depth,
                marker,
                content,
            } => {
                let mut line = RichLine::new();
                let mut prefix = "  ".repeat(*depth as usize);
                match marker {
                    Marker::Bullet => prefix.push_str("• "),
                    Marker::Number(n) => prefix.push_str(&format!("{n}. ")),
                }
                line.push(Span::new(prefix, styles.base));
                for s in &content.spans {
                    line.push(s.clone());
                }
                lines.push(line);
            }
            Block::CodeFence { lines: body, .. } => {
                for l in body {
                    lines.push(RichLine::from_spans(vec![Span::new(
                        l.clone(),
                        styles.base.merge(styles.code),
                    )]));
                }
            }
            Block::Rule => {
                lines.push(RichLine::from_spans(vec![Span::new("───", styles.base)]));
            }
        }
    }
    RichText::from_lines(lines)
}

fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

fn list_marker(line: &str) -> Option<(Marker, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some((Marker::Bullet, rest));
        }
    }
    let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && digits <= 9 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ") {
            let n: u32 = line[..digits].parse().unwrap_or(0);
            return Some((Marker::Number(n), rest));
        }
    }
    None
}

/// Inline parser: single pass, no nesting. `block_style` is the base every
/// span starts from; markers merge onto it. Unclosed markers emit
/// literally.
#[cfg(test)]
fn parse_inline(src: &str, styles: &MdStyles, block_style: Style) -> RichLine {
    parse_inline_traced(src, styles, block_style, |_, _| {})
}

type InlineEmitter<'a> = dyn FnMut(&mut RichLine, std::ops::Range<usize>, Style, Option<&str>) + 'a;

fn parse_inline_traced(
    src: &str,
    styles: &MdStyles,
    block_style: Style,
    mut trace: impl FnMut(std::ops::Range<usize>, usize),
) -> RichLine {
    let mut display = 0;
    let mut emit =
        |out: &mut RichLine, range: std::ops::Range<usize>, style: Style, link: Option<&str>| {
            trace(range.clone(), display);
            display += range.len();
            let mut span = Span::new(&src[range], style);
            if let Some(link) = link {
                span = span.with_link(link);
            }
            out.push(span);
        };
    let mut out = RichLine::new();
    let b = src.as_bytes();
    let mut plain_start = 0;
    let mut i = 0;

    let flush_plain =
        |out: &mut RichLine, upto: usize, from: usize, emit: &mut InlineEmitter<'_>| {
            if upto > from {
                emit(out, from..upto, block_style, None);
            }
        };

    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() && is_escapable(b[i + 1]) => {
                flush_plain(&mut out, i, plain_start, &mut emit);
                emit(&mut out, i + 1..i + 2, block_style, None);
                i += 2;
                plain_start = i;
            }
            b'*' => {
                let bold_marker = b.get(i + 1) == Some(&b'*');
                let (open_len, style) = if bold_marker {
                    (2, styles.bold)
                } else {
                    (1, styles.italic)
                };
                let close: &[u8] = if bold_marker { b"**" } else { b"*" };
                match find_close(b, i + open_len, close) {
                    // Empty emphasis ("**" alone) stays literal.
                    Some(end) if end > i + open_len => {
                        flush_plain(&mut out, i, plain_start, &mut emit);
                        emit(&mut out, i + open_len..end, block_style.merge(style), None);
                        i = end + open_len;
                        plain_start = i;
                    }
                    _ => i += open_len, // unclosed: literal
                }
            }
            // `~~strike~~`: attribute-only by design (STRIKE is
            // semantic, not themable ink), so `MdStyles` needs no new
            // field — its literal-struct construction stays valid.
            b'~' if b.get(i + 1) == Some(&b'~') => {
                match find_close(b, i + 2, b"~~") {
                    // Empty ("~~~~" alone) stays literal.
                    Some(end) if end > i + 2 => {
                        flush_plain(&mut out, i, plain_start, &mut emit);
                        emit(
                            &mut out,
                            i + 2..end,
                            block_style.merge(Style::new().attrs(Attrs::STRIKE)),
                            None,
                        );
                        i = end + 2;
                        plain_start = i;
                    }
                    _ => i += 2, // unclosed: literal
                }
            }
            b'`' => match find_close(b, i + 1, b"`") {
                Some(end) if end > i + 1 => {
                    flush_plain(&mut out, i, plain_start, &mut emit);
                    emit(&mut out, i + 1..end, block_style.merge(styles.code), None);
                    i = end + 1;
                    plain_start = i;
                }
                _ => i += 1,
            },
            b'[' => match parse_link(src, i) {
                Some((text, url, consumed)) => {
                    flush_plain(&mut out, i, plain_start, &mut emit);
                    emit(
                        &mut out,
                        i + 1..i + 1 + text.len(),
                        block_style.merge(styles.link),
                        Some(url),
                    );
                    i += consumed;
                    plain_start = i;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    flush_plain(&mut out, b.len(), plain_start, &mut emit);
    out
}

fn is_escapable(b: u8) -> bool {
    matches!(
        b,
        b'*' | b'~' | b'`' | b'[' | b']' | b'(' | b')' | b'\\' | b'#' | b'|'
    )
}

/// Next unescaped occurrence of `close` at/after `from`; byte-exact (all
/// markers are ASCII, so UTF-8 boundaries are never split).
fn find_close(b: &[u8], from: usize, close: &[u8]) -> Option<usize> {
    let mut i = from;
    while i + close.len() <= b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if &b[i..i + close.len()] == close {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `[text](url)` at `at`; returns (text, url, bytes consumed).
fn parse_link(src: &str, at: usize) -> Option<(&str, &str, usize)> {
    let b = src.as_bytes();
    let text_end = find_close(b, at + 1, b"]")?;
    if b.get(text_end + 1) != Some(&b'(') {
        return None;
    }
    let url_end = find_close(b, text_end + 2, b")")?;
    let text = &src[at + 1..text_end];
    let url = &src[text_end + 2..url_end];
    // Empty text or url: literal (an honest non-link).
    if text.is_empty() || url.is_empty() {
        return None;
    }
    Some((text, url, url_end + 1 - at))
}

#[cfg(test)]
#[path = "md_tests.rs"]
mod tests;
