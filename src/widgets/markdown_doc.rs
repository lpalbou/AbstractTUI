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
/// Test-only since the rule policy landed: every production fold goes
/// through `layout_doc_all` so the caller's `---` style cannot be lost
/// on the way in.
#[cfg(test)]
pub(crate) fn layout_doc(source: &str, t: &TokenSet, width: i32) -> DocLayout {
    layout_doc_with(source, t, width, None)
}

/// [`layout_doc`] under a `---` policy — the fold a styled view
/// renders, so a row position taken from here cannot drift from the
/// pixels the way one taken from the default fold would.
pub(crate) fn layout_doc_ruled(
    source: &str,
    t: &TokenSet,
    width: i32,
    rule: crate::widgets::MdRuleStyle,
) -> DocLayout {
    layout_doc_all(source, t, width, None, rule)
}

/// [`layout_doc`] with a fenced-block claimant installed (test-only,
/// as above).
#[cfg(test)]
pub(crate) fn layout_doc_with(
    source: &str,
    t: &TokenSet,
    width: i32,
    fence: Option<std::rc::Rc<dyn crate::widgets::FenceBlock>>,
) -> DocLayout {
    layout_doc_all(
        source,
        t,
        width,
        fence,
        crate::widgets::MdRuleStyle::default(),
    )
}

/// The one fold: claimant and rule policy both installed.
pub(crate) fn layout_doc_all(
    source: &str,
    t: &TokenSet,
    width: i32,
    fence: Option<std::rc::Rc<dyn crate::widgets::FenceBlock>>,
    rule: crate::widgets::MdRuleStyle,
) -> DocLayout {
    let ts = BlockTypesetter::new(t)
        .with_fence_block(fence)
        .with_rule_style(rule);
    let mut rows: Vec<Row> = Vec::new();
    let mut heading_rows = Vec::new();
    for block in md::parse_doc(source, ts.styles()) {
        let start = rows.len();
        // ASK the typesetter what its separator costs rather than
        // mirroring it here: under a `---` policy the gap after a rule
        // is that rule's `space_after`, so a mirrored `1` would put
        // every heading following a rule on the wrong row (silently —
        // a TOC jump landing one row off looks like taste).
        let sep = ts.separator_rows(&rows, true);
        ts.push_doc_block(&mut rows, &block, width, true);
        if matches!(block, DocBlock::Core(md::Block::Heading { .. })) {
            heading_rows.push(start + sep);
        }
    }
    DocLayout { rows, heading_rows }
}

/// The width-resolved outline (0146): [`md::outline`] zipped with the
/// fold's heading rows, under a `---` policy.
pub(crate) fn outline_rows_ruled(
    source: &str,
    t: &TokenSet,
    width: i32,
    rule: crate::widgets::MdRuleStyle,
) -> Vec<OutlineEntry> {
    let fold = layout_doc_ruled(source, t, width, rule);
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

    /// Table recipe: natural column widths, the SHARED solver when the
    /// table fits, per-cell ellipsis truncation, alignment (numeric
    /// columns auto-right-align when nothing was declared — the mdpad
    /// rule), a bold header and a border-ink separator rule. Grid cells
    /// never wrap (0142 non-goal); the honest-degradation ladder at
    /// crush widths (wave 13) is: floor every column at
    /// `min(natural, 3)` and grow toward natural proportionally to
    /// need, and when even the floors overflow, degrade to the RECORD
    /// layout ([`push_records`](Self::push_records)) — a table that
    /// cannot fit shows honest words, never silently vanished columns.
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
        let align = effective_alignments(table);
        let gaps = (n as i32 - 1).max(0);
        let usable = (width - gaps).max(0);
        let total_natural: i32 = natural.iter().sum();
        let cols: Vec<i32> = if total_natural <= usable {
            // Fits: natural sizes through the shared solver (the
            // 1-cell-gap contract with the Table widget).
            let widths: Vec<ColWidth> = natural.iter().map(|w| ColWidth::Cells(*w)).collect();
            solve_columns(&widths, width.max(1))
        } else {
            // Overflow: floor first, then grow toward natural
            // proportionally to remaining need (largest-remainder
            // tiling — the browser algorithm in miniature). The old
            // fair-share/Flex mix let first-come fixed columns starve
            // the rightmost ones to ZERO width, which rendered as
            // silently missing cells — the operator's "tables don't
            // render in panels" crush shape.
            let floors: Vec<i32> = natural.iter().map(|w| (*w).min(MIN_COL_FLOOR)).collect();
            let floor_sum: i32 = floors.iter().sum();
            if floor_sum > usable {
                self.push_records(out, table, width);
                return;
            }
            let need: Vec<f64> = natural
                .iter()
                .zip(&floors)
                .map(|(nat, fl)| (nat - fl) as f64)
                .collect();
            let grow = crate::layout::distribute(usable - floor_sum, &need);
            floors.iter().zip(&grow).map(|(f, g)| f + g).collect()
        };
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
            push_cell(&mut header, &emboldened, cols[i], align[i]);
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
                push_cell(&mut line, cell, cols[i], align[i]);
            }
            out.push(Row::plain(line));
        }
    }

    /// The record fallback (wave 13, ported from mdpad): when even
    /// per-column floors overflow the width, any grid would be vertical
    /// confetti — render each body row as a `Header: value` list
    /// (labels bold, records separated by a blank row) instead. Nothing
    /// is dropped; long values wrap at the panel width. A header-only
    /// table lists its labels.
    fn push_records(&self, out: &mut Vec<Row>, table: &TableBlock, width: i32) {
        let bold = Style::new().attrs(Attrs::BOLD);
        if table.rows.is_empty() {
            for cell in &table.header {
                if cell.width() == 0 {
                    continue;
                }
                let mut line = RichLine::new();
                for span in &cell.spans {
                    line.push(Span::new(span.text.clone(), span.style.merge(bold)));
                }
                for wrapped in wrap_line(line, width) {
                    out.push(Row::plain(wrapped));
                }
            }
            return;
        }
        for (r, row) in table.rows.iter().enumerate() {
            if r > 0 {
                out.push(Row::plain(RichLine::new()));
            }
            for (i, cell) in row.iter().enumerate() {
                let label = &table.header[i];
                if label.width() == 0 && cell.width() == 0 {
                    continue; // nothing to say for this field
                }
                let mut line = RichLine::new();
                if label.width() > 0 {
                    for span in &label.spans {
                        line.push(Span::new(span.text.clone(), span.style.merge(bold)));
                    }
                    line.push(Span::new(": ", Style::EMPTY));
                }
                for span in &cell.spans {
                    line.push(span.clone());
                }
                for wrapped in wrap_line(line, width) {
                    out.push(Row::plain(wrapped));
                }
            }
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
                rule: None,
                image: None,
                fence: None,
            });
        }
    }
}

/// The per-column width floor under overflow: two clusters + the
/// ellipsis — the narrowest cell a reader can still identify. Below
/// this the recipe abandons the grid for the record layout.
const MIN_COL_FLOOR: i32 = 3;

/// Per-column EFFECTIVE alignment (wave 13, the mdpad rule): a column
/// with no written alignment marker (`declared[i] == false`, bare
/// `---`) whose body cells are ALL numeric right-aligns — the way a
/// human lays out a numbers column. Explicit `:--`/`:-:`/`--:` is
/// always honored verbatim; header cells are labels and never vote;
/// empty cells abstain; an all-empty column stays left.
fn effective_alignments(table: &TableBlock) -> Vec<CellAlign> {
    (0..table.columns())
        .map(|i| {
            let declared = table.declared.get(i).copied().unwrap_or(false);
            if declared || table.align[i] != CellAlign::Left {
                return table.align[i];
            }
            let mut any = false;
            for row in &table.rows {
                let text = row[i].plain();
                let cell = text.trim();
                if cell.is_empty() {
                    continue;
                }
                if !looks_numeric(cell) {
                    return CellAlign::Left;
                }
                any = true;
            }
            if any {
                CellAlign::Right
            } else {
                CellAlign::Left
            }
        })
        .collect()
}

/// Conservatively numeric: at least one ASCII digit and nothing but
/// digits, sign/decimal/grouping punctuation, percent and common
/// currency marks. Anything word-shaped disqualifies the cell (a
/// false "left" costs nothing; a false "right" would look deranged).
fn looks_numeric(cell: &str) -> bool {
    let mut digit = false;
    for c in cell.chars() {
        match c {
            '0'..='9' => digit = true,
            '+' | '-' | '.' | ',' | '_' | '%' | '$' | '€' | '£' | '¥' | ' ' | '(' | ')' => {}
            _ => return false,
        }
    }
    digit
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
