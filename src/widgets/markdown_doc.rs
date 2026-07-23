//! Doc-vocabulary typesetting (0142): `DocBlock` → typeset [`Row`]s.
//! Tables solve their columns through the Table widget's OWN
//! `solve_columns` (one width policy, never a duplicate — the shared
//! 1-cell-gap contract included); task items reuse the list-item row
//! shape; core blocks delegate to `push_block` verbatim.
//!
//! Also home to the doc layout FOLD (`layout_doc`): parse + typeset +
//! heading row positions in one pass — `MarkdownView::element`,
//! `rows`, `outline_rows` and `find` all consume this fold, so scroll
//! clamps, TOC jumps and search rects can never drift from the pixels.
//!
//! OWNER: READER (app-widgets wave 3).

use crate::render::md::{self, CellAlign, DocBlock, TableBlock, TaskBlock};
use crate::render::rich::{RichLine, Span};
use crate::render::{Attrs, Style};
use crate::text;
use crate::theme::TokenSet;

use super::super::table::{solve_columns, ColWidth};
use super::{imageflow, wrap_line, BlockTypesetter, Row};

/// One outline entry resolved to a typeset row (0146): the heading and
/// the row its text starts at for a given layout width.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OutlineEntry {
    /// Level, text, anchor id (see [`md::outline`]).
    pub heading: md::Heading,
    /// The typeset row the heading TEXT occupies at the fold's width —
    /// scroll here for a TOC jump.
    pub row: usize,
}

/// The doc fold result: typeset rows plus the row index of every
/// heading, in reading order (zips 1:1 with [`md::outline`] — same
/// parser, same block walk).
pub(crate) struct DocLayout {
    pub(crate) rows: Vec<Row>,
    pub(crate) heading_rows: Vec<usize>,
}

/// Parse + typeset at `width`. Pure over (source, tokens, width).
pub(crate) fn layout_doc(source: &str, t: &TokenSet, width: i32) -> DocLayout {
    let ts = BlockTypesetter::new(t);
    let mut rows: Vec<Row> = Vec::new();
    let mut heading_rows = Vec::new();
    for block in md::parse_doc(source, ts.styles()) {
        let start = rows.len();
        // Mirror push_block's spacing policy to locate the heading's
        // text row: one blank separator before non-list blocks when
        // rows exist (the property test re-pins this against the
        // rendered rows across widths).
        let sep = usize::from(!rows.is_empty());
        ts.push_doc_block(&mut rows, &block, width, true);
        if matches!(block, DocBlock::Core(md::Block::Heading { .. })) {
            heading_rows.push(start + sep);
        }
    }
    DocLayout { rows, heading_rows }
}

/// The width-resolved outline (0146): [`md::outline`] zipped with the
/// fold's heading rows.
pub(crate) fn outline_rows(source: &str, t: &TokenSet, width: i32) -> Vec<OutlineEntry> {
    let fold = layout_doc(source, t, width);
    let outline = md::outline(source);
    debug_assert_eq!(
        outline.len(),
        fold.heading_rows.len(),
        "outline and fold walk the same parse"
    );
    outline
        .into_iter()
        .zip(fold.heading_rows)
        .map(|(heading, row)| OutlineEntry { heading, row })
        .collect()
}

impl BlockTypesetter {
    /// Append `block`'s typeset rows — the doc-vocabulary twin of
    /// `push_block` (which it delegates to for core blocks, so a feed
    /// item and a MarkdownView keep one recipe).
    pub(crate) fn push_doc_block(
        &self,
        out: &mut Vec<Row>,
        block: &DocBlock,
        width: i32,
        separate: bool,
    ) {
        match block {
            DocBlock::Core(core) => self.push_block(out, core, width, separate),
            DocBlock::Table(table) => self.push_table(out, table, width, separate),
            DocBlock::Task(task) => self.push_task(out, task, width),
            DocBlock::Image(image) => {
                if separate && !out.is_empty() {
                    out.push(Row::plain(RichLine::new()));
                }
                imageflow::push_image_rows(out, image, width, &self.t);
            }
            // Future doc blocks degrade to nothing rather than lie —
            // the enum is non_exhaustive by design.
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// Table recipe: natural column widths, the SHARED solver, per-cell
    /// ellipsis truncation, alignment, a bold header and a border-ink
    /// separator rule. Cells never wrap (0142 non-goal).
    fn push_table(&self, out: &mut Vec<Row>, table: &TableBlock, width: i32, separate: bool) {
        if separate && !out.is_empty() {
            out.push(Row::plain(RichLine::new()));
        }
        let t = &self.t;
        let n = table.columns();
        if n == 0 {
            return;
        }
        // Natural width per column: the widest cell (header included).
        let mut natural = vec![0i32; n];
        for (i, cell) in table.header.iter().enumerate() {
            natural[i] = natural[i].max(cell.width());
        }
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                natural[i] = natural[i].max(cell.width());
            }
        }
        // Shared solver: columns that fit keep their natural size
        // (`Cells`); when the table overflows, columns beyond the fair
        // share flex proportionally to their natural width and truncate.
        let gaps = (n as i32 - 1).max(0);
        let usable = (width - gaps).max(0);
        let total_natural: i32 = natural.iter().sum();
        let widths: Vec<ColWidth> = if total_natural <= usable {
            natural.iter().map(|w| ColWidth::Cells(*w)).collect()
        } else {
            let fair = usable / n as i32;
            natural
                .iter()
                .map(|w| {
                    if *w <= fair {
                        ColWidth::Cells(*w)
                    } else {
                        ColWidth::Flex(*w as f32)
                    }
                })
                .collect()
        };
        let cols = solve_columns(&widths, width.max(1));
        let table_w = cols.iter().sum::<i32>() + gaps;

        // Header: bold over the cell's own inline styles.
        let bold = Style::new().attrs(Attrs::BOLD);
        let mut header = RichLine::new();
        for (i, cell) in table.header.iter().enumerate() {
            if i > 0 {
                header.push(Span::new(" ", Style::EMPTY));
            }
            let mut emboldened = RichLine::new();
            for span in &cell.spans {
                emboldened.push(Span::new(span.text.clone(), span.style.merge(bold)));
            }
            push_cell(&mut header, &emboldened, cols[i], table.align[i]);
        }
        out.push(Row::plain(header));

        // Separator: a continuous border-ink rule across the table's
        // solved width (not the full rect — the table owns its box).
        let rule: String = "─".repeat(table_w.max(0) as usize);
        out.push(Row::plain(RichLine::from_spans(vec![Span::new(
            rule,
            Style::new().fg(t.border),
        )])));

        for row in &table.rows {
            let mut line = RichLine::new();
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    line.push(Span::new(" ", Style::EMPTY));
                }
                push_cell(&mut line, cell, cols[i], table.align[i]);
            }
            out.push(Row::plain(line));
        }
    }

    /// Task recipe: the list-item shape with a checkbox marker in the
    /// list-marker ink (`[x]` matches the Checkbox widget's glyphs).
    fn push_task(&self, out: &mut Vec<Row>, task: &TaskBlock, width: i32) {
        let t = &self.t;
        let indent = 2 + task.depth as i32 * 2;
        let mut line = RichLine::new();
        let mark = if task.checked { "[x] " } else { "[ ] " };
        line.push(Span::new(mark, Style::new().fg(t.accent_alt)));
        for span in &task.content.spans {
            line.push(span.clone());
        }
        for (i, wrapped) in wrap_line(line, width - indent).into_iter().enumerate() {
            out.push(Row {
                line: wrapped,
                indent: indent + if i > 0 { 2 } else { 0 },
                ground: None,
                quote: false,
                rule: false,
                image: None,
            });
        }
    }
}

/// Append one table cell to `line`: content truncated to `w` with an
/// ellipsis when overwide, padded to exactly `w` with the column's
/// alignment. Zero-width columns contribute nothing (crushed layouts
/// stay honest, never panic).
fn push_cell(line: &mut RichLine, cell: &RichLine, w: i32, align: CellAlign) {
    if w <= 0 {
        return;
    }
    let content = truncate_rich(cell, w);
    let pad = (w - content.width()).max(0);
    let (left, right) = match align {
        CellAlign::Left => (0, pad),
        CellAlign::Right => (pad, 0),
        CellAlign::Center => (pad / 2, pad - pad / 2),
    };
    if left > 0 {
        line.push(Span::new(" ".repeat(left as usize), Style::EMPTY));
    }
    for span in &content.spans {
        line.push(span.clone());
    }
    if right > 0 {
        line.push(Span::new(" ".repeat(right as usize), Style::EMPTY));
    }
}

/// Style-preserving ellipsis truncation: keep whole clusters up to
/// `max_width - 1` columns, then `…` in the last kept span's style
/// (mirrors the draw-time rule in `render::rich`, but produces a LINE —
/// table cells are typeset, not clipped at draw).
fn truncate_rich(line: &RichLine, max_width: i32) -> RichLine {
    if line.width() <= max_width {
        return line.clone();
    }
    let mut out = RichLine::new();
    if max_width <= 0 {
        return out;
    }
    let budget = max_width - 1;
    let mut used = 0i32;
    let mut ellipsis_style = Style::EMPTY;
    'spans: for span in &line.spans {
        ellipsis_style = span.style;
        for seg in text::segments(&span.text) {
            if seg.width <= 0 {
                continue;
            }
            if used + seg.width > budget {
                break 'spans;
            }
            let mut s = Span::new(seg.cluster, span.style);
            s.link = span.link.clone();
            out.push(s);
            used += seg.width;
        }
    }
    out.push(Span::new("\u{2026}", ellipsis_style));
    out
}

#[cfg(test)]
#[path = "markdown_doc_tests.rs"]
mod tests;
