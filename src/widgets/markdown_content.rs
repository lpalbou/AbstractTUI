//! Source-to-cells sidecars for the ordinary markdown fold. OWNER: CONTENT.
//! Uses the parser's explicit byte runs and the ordinary wrap's emitted pieces;
//! it never searches composed glyphs to guess their source.
use super::{BlockTypesetter, Row};
use crate::{
    render::{
        content::{RowSource, SourceBlock, SourceCell, SourcePart},
        md::{Block, DocBlock, Marker},
        rich::{wrap_mapped, RichLine, Span},
    },
    theme::TokenSet,
};
use std::rc::Rc;

pub(crate) struct SourceDocument<'a> {
    pub mapping: &'a SourceBlock,
    pub source: &'a Rc<str>,
    pub block: usize,
}

fn cell(
    part: &SourcePart,
    bytes: std::ops::Range<usize>,
    columns: std::ops::Range<i32>,
) -> Option<SourceCell> {
    let run = part
        .runs
        .iter()
        .find(|r| r.display.start <= bytes.start && r.display.end >= bytes.end)?;
    Some(SourceCell {
        context: part.context.clone(),
        columns,
        bytes: run.source.start + bytes.start - run.display.start
            ..run.source.start + bytes.end - run.display.start,
        scope: part.scope.clone(),
        kind: part.kind,
    })
}

fn line_map(
    row: &mut Row,
    line: &RichLine,
    parts: &[(usize, &SourcePart)],
    source: &Rc<str>,
    block: usize,
) {
    let mut cells = Vec::new();
    let plain = line.plain();
    for (_, fragments) in wrap_mapped(line, i32::MAX) {
        for (column, bytes) in fragments {
            let width = crate::text::width(&plain[bytes.clone()]);
            for (base, part) in parts {
                if bytes.start >= *base {
                    if let Some(cell) = cell(
                        part,
                        bytes.start - base..bytes.end - base,
                        row.indent + column..row.indent + column + width,
                    ) {
                        cells.push(cell);
                        break;
                    }
                }
            }
        }
    }
    if !cells.is_empty() {
        row.source = Some(Box::new(RowSource {
            source: source.clone(),
            block,
            cells,
        }));
    }
}

fn wrapped(
    rows: &mut [Row],
    line: &RichLine,
    width: i32,
    parts: &[(usize, &SourcePart)],
    source: &Rc<str>,
    block: usize,
) {
    let plain = line.plain();
    for (row, (line, fragments)) in rows.iter_mut().zip(wrap_mapped(line, width.max(4))) {
        debug_assert_eq!(
            row.line.plain(),
            line.plain(),
            "mapped wrap must be the drawn wrap"
        );
        let mut cells = Vec::new();
        for (column, bytes) in fragments {
            let width = crate::text::width(&plain[bytes.clone()]);
            for (base, part) in parts {
                if bytes.start >= *base {
                    if let Some(cell) = cell(
                        part,
                        bytes.start - base..bytes.end - base,
                        row.indent + column..row.indent + column + width,
                    ) {
                        cells.push(cell);
                        break;
                    }
                }
            }
        }
        if !cells.is_empty() {
            row.source = Some(Box::new(RowSource {
                source: source.clone(),
                block,
                cells,
            }));
        }
    }
}

fn prefixed(prefix: &str, line: &RichLine) -> RichLine {
    let mut out = RichLine::from_spans(vec![Span::plain(prefix)]);
    for span in &line.spans {
        out.push(span.clone());
    }
    out
}

impl BlockTypesetter {
    pub(crate) fn push_sourced_doc(
        &self,
        out: &mut Vec<Row>,
        value: &DocBlock,
        origin: SourceDocument<'_>,
        width: i32,
        separate: bool,
    ) {
        let SourceDocument {
            mapping: map,
            source,
            block,
        } = origin;
        let sep = match value {
            DocBlock::Core(Block::ListItem { .. }) | DocBlock::Task(_) => 0,
            _ => self.separator_rows(out, separate),
        };
        let base = out.len() + sep;
        self.push_doc_block(out, value, width, separate);
        if map.parts.is_empty() || base >= out.len() {
            return;
        }
        let part = &map.parts[0];
        match value {
            DocBlock::Core(Block::Paragraph(line)) => {
                wrapped(&mut out[base..], line, width, &[(0, part)], source, block)
            }
            DocBlock::Core(Block::Blockquote(line)) => wrapped(
                &mut out[base..],
                line,
                width - 2,
                &[(0, part)],
                source,
                block,
            ),
            DocBlock::Core(Block::Heading { level, content }) => {
                let prefix = if *level >= 3 {
                    format!("{} ", "#".repeat(*level as usize))
                } else {
                    String::new()
                };
                line_map(
                    &mut out[base],
                    &prefixed(&prefix, content),
                    &[(prefix.len(), part)],
                    source,
                    block,
                );
            }
            DocBlock::Core(Block::ListItem {
                depth,
                marker,
                content,
            }) => {
                let prefix = match marker {
                    Marker::Bullet => "• ".into(),
                    Marker::Number(n) => format!("{n}. "),
                };
                wrapped(
                    &mut out[base..],
                    &prefixed(&prefix, content),
                    width - 2 - *depth as i32 * 2,
                    &[(prefix.len(), part)],
                    source,
                    block,
                );
            }
            DocBlock::Core(Block::CodeFence { lines, .. }) => {
                for ((row, text), part) in out[base..].iter_mut().zip(lines).zip(&map.parts) {
                    if row.fence.is_none() {
                        line_map(
                            row,
                            &prefixed(" ", &RichLine::from_spans(vec![Span::plain(text)])),
                            &[(1, part)],
                            source,
                            block,
                        );
                    }
                }
            }
            DocBlock::Task(task) => wrapped(
                &mut out[base..],
                &prefixed(if task.checked { "[x] " } else { "[ ] " }, &task.content),
                width - 2 - task.depth as i32 * 2,
                &[(4, part)],
                source,
                block,
            ),
            DocBlock::Table(table) => {
                let n = table.columns();
                if let Some(cols) = super::doc::table_columns(table, width) {
                    let align = super::doc::effective_alignments(table);
                    for (r, cells) in std::iter::once(&table.header)
                        .chain(table.rows.iter())
                        .enumerate()
                    {
                        let row = &mut out[base + if r == 0 { 0 } else { r + 1 }];
                        let mut x = 0;
                        let mut mapped = Vec::new();
                        for (c, text) in cells.iter().enumerate() {
                            let clipped = super::doc::truncate_rich(text, cols[c]);
                            let pad = cols[c] - clipped.width();
                            let left = match align[c] {
                                crate::render::md::CellAlign::Left => 0,
                                crate::render::md::CellAlign::Right => pad,
                                crate::render::md::CellAlign::Center => pad / 2,
                            };
                            let mut temporary = Row::plain(clipped.clone());
                            temporary.indent = x + left;
                            line_map(
                                &mut temporary,
                                text,
                                &[(0, &map.parts[r * n + c])],
                                source,
                                block,
                            );
                            if let Some(mut m) = temporary.source {
                                let edge =
                                    x + left + clipped.width() - i32::from(text.width() > cols[c]);
                                m.cells.retain(|cell| cell.columns.end <= edge);
                                mapped.append(&mut m.cells);
                            }
                            x += cols[c] + 1;
                        }
                        if !mapped.is_empty() {
                            row.source = Some(Box::new(RowSource {
                                source: source.clone(),
                                block,
                                cells: mapped,
                            }));
                        }
                    }
                } else {
                    let mut row = base;
                    if table.rows.is_empty() {
                        for (c, text) in table.header.iter().enumerate() {
                            if text.width() == 0 {
                                continue;
                            }
                            let count = wrap_mapped(text, width.max(4)).len();
                            wrapped(
                                &mut out[row..row + count],
                                text,
                                width,
                                &[(0, &map.parts[c])],
                                source,
                                block,
                            );
                            row += count;
                        }
                    } else {
                        for (r, cells) in table.rows.iter().enumerate() {
                            if r > 0 {
                                row += 1;
                            }
                            for (c, text) in cells.iter().enumerate() {
                                let label = &table.header[c];
                                if label.width() == 0 && text.width() == 0 {
                                    continue;
                                }
                                let prefix = if label.width() > 0 {
                                    format!("{}: ", label.plain())
                                } else {
                                    String::new()
                                };
                                let line = prefixed(&prefix, text);
                                let count = wrap_mapped(&line, width.max(4)).len();
                                wrapped(
                                    &mut out[row..row + count],
                                    &line,
                                    width,
                                    &[
                                        (0, &map.parts[c]),
                                        (prefix.len(), &map.parts[(r + 1) * n + c]),
                                    ],
                                    source,
                                    block,
                                );
                                row += count;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn layout(
    source: Rc<str>,
    tokens: &TokenSet,
    width: i32,
    rule: super::MdRuleStyle,
) -> Vec<Row> {
    let ts = BlockTypesetter::new(tokens).with_rule_style(rule);
    let mut rows = Vec::new();
    for (block, map) in crate::render::md::parse_doc_sourced(&source, ts.styles()) {
        ts.push_sourced_doc(
            &mut rows,
            &block,
            SourceDocument {
                mapping: &map,
                source: &source,
                block: 0,
            },
            width,
            true,
        );
    }
    rows
}

/// Plain/rich application content uses its own logical lines, not a markdown interpretation.
pub(crate) fn map_plain(
    rows: &mut [Row],
    text: &crate::render::RichText,
    original: Option<&str>,
    block: usize,
    width: i32,
) {
    use crate::render::content::{ContentKind, SourcePart};
    let source: Rc<str> = original
        .map(str::to_owned)
        .unwrap_or_else(|| {
            text.lines
                .iter()
                .map(|l| l.plain())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .into();
    let mut cursor = 0;
    let mut row = 0;
    for line in &text.lines {
        let count = wrap_mapped(line, width.max(4)).len();
        let part = SourcePart::literal(cursor..cursor + line.plain().len(), ContentKind::Span);
        if row < rows.len() {
            let end = (row + count).min(rows.len());
            wrapped(
                &mut rows[row..end],
                line,
                width,
                &[(0, &part)],
                &source,
                block,
            );
        }
        cursor = source[cursor..]
            .find('\n')
            .map(|i| cursor + i + 1)
            .unwrap_or(source.len());
        row += count;
    }
}

pub(crate) fn highlight(
    canvas: &mut dyn crate::ui::StyledCanvas,
    rect: crate::base::Rect,
    tokens: &TokenSet,
    rows: &[Row],
    owner: &str,
    selected: Option<&crate::widgets::SelectedContent>,
) {
    let Some(selected) = selected.filter(|s| s.owner == owner) else {
        return;
    };
    let mut matches = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let Some(source) = row
            .source
            .as_ref()
            .filter(|s| s.block == selected.block && s.source == selected.source)
        else {
            continue;
        };
        for cell in &source.cells {
            if cell.bytes.start < selected.bytes.end && cell.bytes.end > selected.bytes.start {
                matches.push(super::search::MdSearchMatch {
                    row: index,
                    bytes: (
                        super::search::row_byte_at_col(row, cell.columns.start),
                        super::search::row_byte_at_col(row, cell.columns.end),
                    ),
                    cells: (cell.columns.start, cell.columns.end),
                });
            }
        }
    }
    super::search::draw_highlights(canvas, rect, tokens, rows, 0, &matches, None);
}
