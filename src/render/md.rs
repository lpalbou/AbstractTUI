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
    let mut blocks = Vec::new();
    let mut lines = src.lines().peekable();
    // Paragraph accumulation: consecutive non-blank plain lines join with
    // a space (soft wrap), emitted on blank/block boundary.
    let mut para: Option<RichLine> = None;

    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();

        // Fence open?
        if let Some(rest) = trimmed.strip_prefix("```") {
            flush_para(&mut para, &mut blocks);
            let lang = rest.trim().to_string();
            let mut body = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break; // fence close (EOF also closes: honest recovery)
                }
                body.push(l.to_string());
            }
            blocks.push(Block::CodeFence { lang, lines: body });
            continue;
        }
        if trimmed.is_empty() {
            flush_para(&mut para, &mut blocks);
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Rule);
            continue;
        }
        // Heading: 1-6 '#' then a space.
        if let Some(h) = heading_level(trimmed) {
            flush_para(&mut para, &mut blocks);
            let content = &trimmed[h as usize + 1..];
            let block_style = styles.base.merge(styles.heading);
            blocks.push(Block::Heading {
                level: h,
                content: parse_inline(content.trim_start(), styles, block_style),
            });
            continue;
        }
        // Blockquote: strip one or more '>' (nesting folds — documented).
        if trimmed.starts_with('>') {
            flush_para(&mut para, &mut blocks);
            let mut rest = trimmed;
            while let Some(r) = rest.strip_prefix('>') {
                rest = r.trim_start();
            }
            blocks.push(Block::Blockquote(parse_inline(rest, styles, styles.base)));
            continue;
        }
        // List item: bullet or "N." followed by a space.
        if let Some((marker, rest)) = list_marker(trimmed) {
            flush_para(&mut para, &mut blocks);
            let indent = line.len() - trimmed.len();
            blocks.push(Block::ListItem {
                depth: (indent / 2).min(8) as u8,
                marker,
                content: parse_inline(rest, styles, styles.base),
            });
            continue;
        }
        // Paragraph text: soft-join with the accumulator.
        let inline = parse_inline(trimmed, styles, styles.base);
        match &mut para {
            Some(acc) => {
                acc.push(Span::new(" ", styles.base));
                for s in inline.spans {
                    acc.push(s);
                }
            }
            None => para = Some(inline),
        }
    }
    flush_para(&mut para, &mut blocks);
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

fn flush_para(para: &mut Option<RichLine>, blocks: &mut Vec<Block>) {
    if let Some(l) = para.take() {
        blocks.push(Block::Paragraph(l));
    }
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
fn parse_inline(src: &str, styles: &MdStyles, block_style: Style) -> RichLine {
    let mut out = RichLine::new();
    let b = src.as_bytes();
    let mut plain_start = 0;
    let mut i = 0;

    let flush_plain = |out: &mut RichLine, upto: usize, from: usize| {
        if upto > from {
            out.push(Span::new(&src[from..upto], block_style));
        }
    };

    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() && is_escapable(b[i + 1]) => {
                flush_plain(&mut out, i, plain_start);
                out.push(Span::new(&src[i + 1..i + 2], block_style));
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
                        flush_plain(&mut out, i, plain_start);
                        out.push(Span::new(&src[i + open_len..end], block_style.merge(style)));
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
                        flush_plain(&mut out, i, plain_start);
                        out.push(Span::new(
                            &src[i + 2..end],
                            block_style.merge(Style::new().attrs(Attrs::STRIKE)),
                        ));
                        i = end + 2;
                        plain_start = i;
                    }
                    _ => i += 2, // unclosed: literal
                }
            }
            b'`' => match find_close(b, i + 1, b"`") {
                Some(end) if end > i + 1 => {
                    flush_plain(&mut out, i, plain_start);
                    out.push(Span::new(&src[i + 1..end], block_style.merge(styles.code)));
                    i = end + 1;
                    plain_start = i;
                }
                _ => i += 1,
            },
            b'[' => match parse_link(src, i) {
                Some((text, url, consumed)) => {
                    flush_plain(&mut out, i, plain_start);
                    out.push(Span::new(text, block_style.merge(styles.link)).with_link(url));
                    i += consumed;
                    plain_start = i;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    flush_plain(&mut out, b.len(), plain_start);
    out
}

fn is_escapable(b: u8) -> bool {
    matches!(
        b,
        b'*' | b'~' | b'`' | b'[' | b']' | b'(' | b')' | b'\\' | b'#'
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
