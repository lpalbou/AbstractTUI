//! Document-vocabulary markdown: the core [`Block`] set extended with
//! GFM tables (0142), block images (0144) and task-list items — parsed
//! by [`parse_doc`] into [`DocBlock`].
//!
//! WHY A SECOND ENUM: `Block` shipped in 0.2.x as an exhaustive public
//! enum; adding variants would break every downstream exhaustive match
//! (semver). `DocBlock` is `#[non_exhaustive]` from birth, wraps the
//! core vocabulary verbatim in [`DocBlock::Core`], and is where new
//! block kinds land from now on. For sources containing none of the
//! extended constructs, `parse_doc` is exactly `parse` wrapped in
//! `Core` (test-pinned).
//!
//! ## The honest subset (exactly)
//!
//! - TABLE: a plain-text line containing at least one unescaped `|`
//!   (the header), immediately followed by a delimiter row (cells of
//!   `:?-+:?` separated by `|`, at least one unescaped `|`, same cell
//!   count as the header). Body rows are the following plain-text lines
//!   containing an unescaped `|`; the first line without one (blank,
//!   block start, or plain prose) CLOSES the table. `\|` inside a cell
//!   is a literal pipe. Extra body cells are dropped, missing ones pad
//!   empty (GFM behavior). Deviations from full GFM, deliberately:
//!   body rows REQUIRE a pipe (GFM would absorb pipe-less lines as
//!   one-cell rows — hostile to streaming cut-safety), and tables
//!   inside lists/quotes are not recognized.
//! - IMAGE: a line that is exactly `![alt](src)` (whole line, after
//!   trim) becomes [`ImageBlock`]. Inline images inside paragraphs stay
//!   literal text, as in the core parser. Empty `src` stays literal.
//! - TASK: `- [ ] text` / `- [x] text` (any core bullet, `x` or `X`)
//!   becomes [`TaskBlock`]. Numbered task items are not recognized
//!   (they stay ordered list items).
//!
//! ## Streaming open/close semantics (the 0142 contract)
//!
//! A table OPENS when its header line and delimiter line are both
//! complete, accumulates a row per complete pipe line, and CLOSES at
//! the first non-pipe line (or end of input — EOF closes an open
//! table with the rows received, mirroring the fence recovery rule).
//! [`DocStreamSession`](super::DocStreamSession) seals nothing from
//! the header line onward while the table is open or while a header
//! CANDIDATE (a complete pipe line whose successor has not arrived)
//! is unresolved — a cut between header and delimiter, or between two
//! body rows, would split one table into text + a smaller table and
//! break streamed-vs-batch equivalence. Images and tasks are
//! single-line blocks: complete line = closed block.

use crate::render::rich::RichLine;

use super::{find_close, list_marker, parse, parse_inline, Block, Marker, MdStyles};

/// Per-column alignment from the delimiter row (`:--` left, `:-:`
/// center, `--:` right; bare `---` defaults left). Exhaustive on
/// purpose: the alignment vocabulary is complete.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    /// `---` or `:--`.
    #[default]
    Left,
    /// `:-:`.
    Center,
    /// `--:`.
    Right,
}

/// A parsed GFM table. Invariant (enforced by [`TableBlock::new`]):
/// `header.len() == align.len()`, and every row in `rows` has exactly
/// `align.len()` cells. Cell content carries inline styles
/// (bold/code/links) like any other line.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TableBlock {
    /// Per-column alignment, from the delimiter row.
    pub align: Vec<CellAlign>,
    /// Header cells (one per column).
    pub header: Vec<RichLine>,
    /// Body rows, each padded/truncated to the column count.
    pub rows: Vec<Vec<RichLine>>,
}

impl TableBlock {
    /// Builds a table, normalizing `header` and every row to
    /// `align.len()` cells (missing cells pad empty, extra cells drop —
    /// the GFM row rule, applied uniformly so consumers never index
    /// out of bounds).
    pub fn new(align: Vec<CellAlign>, header: Vec<RichLine>, rows: Vec<Vec<RichLine>>) -> Self {
        let n = align.len();
        let fit = |mut cells: Vec<RichLine>| -> Vec<RichLine> {
            cells.truncate(n);
            while cells.len() < n {
                cells.push(RichLine::new());
            }
            cells
        };
        TableBlock {
            align,
            header: fit(header),
            rows: rows.into_iter().map(fit).collect(),
        }
    }

    /// Column count.
    pub fn columns(&self) -> usize {
        self.align.len()
    }
}

/// A block-level image reference (`![alt](src)` alone on its line).
/// The parser carries the reference only — decoding is the consumer's
/// move (widgets decode lazily on first draw, 0144).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ImageBlock {
    /// Alt text (may be empty) — the caption and the decode-failure
    /// fallback, verbatim from the source.
    pub alt: String,
    /// The image source as written (a path for file-backed readers).
    pub src: String,
}

impl ImageBlock {
    pub fn new(alt: impl Into<String>, src: impl Into<String>) -> Self {
        ImageBlock {
            alt: alt.into(),
            src: src.into(),
        }
    }
}

/// A task-list item (`- [ ]` / `- [x]`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TaskBlock {
    /// `[x]` / `[X]` checked, `[ ]` not.
    pub checked: bool,
    /// Nesting depth from 2-space indent steps (0 = top level), same
    /// rule as [`Block::ListItem`].
    pub depth: u8,
    /// Item text with inline styles applied.
    pub content: RichLine,
}

impl TaskBlock {
    pub fn new(checked: bool, depth: u8, content: RichLine) -> Self {
        TaskBlock {
            checked,
            depth,
            content,
        }
    }
}

/// One parsed document block: the core vocabulary plus the extended
/// kinds. `#[non_exhaustive]` — future block kinds are additive here;
/// always keep a wildcard arm.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DocBlock {
    /// The core vocabulary (headings, paragraphs, lists, quotes,
    /// fences, rules), unchanged.
    Core(Block),
    /// A GFM table (0142).
    Table(TableBlock),
    /// A block image (0144).
    Image(ImageBlock),
    /// A task-list item.
    Task(TaskBlock),
}

/// Parses a document into the extended vocabulary. Every input parses —
/// degradation is always "treat as core markdown", never an error.
/// Sources without tables/images/tasks yield exactly
/// `parse(src).map(DocBlock::Core)` (test-pinned).
///
/// ```
/// use abstracttui::render::md::{self, DocBlock, MdStyles};
///
/// let styles = MdStyles::default();
/// let blocks = md::parse_doc("| a | b |\n|---|--:|\n| 1 | 2 |", &styles);
/// assert!(matches!(blocks[0], DocBlock::Table(_)));
/// ```
pub fn parse_doc(src: &str, styles: &MdStyles) -> Vec<DocBlock> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<DocBlock> = Vec::new();
    // Pending core-source lines, flushed through `parse` at extended
    // block boundaries: ONE core parser, never a re-implementation.
    let mut core: Vec<&str> = Vec::new();
    let mut in_fence = false;
    let mut i = 0;

    let flush = |core: &mut Vec<&str>, out: &mut Vec<DocBlock>, styles: &MdStyles| {
        if core.is_empty() {
            return;
        }
        let seg = core.join("\n");
        out.extend(parse(&seg, styles).into_iter().map(DocBlock::Core));
        core.clear();
    };

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim_end().trim_start();
        if in_fence {
            core.push(raw);
            if trimmed.starts_with("```") {
                in_fence = false;
            }
            i += 1;
            continue;
        }
        match doc_line_class(raw) {
            DocLineClass::FenceOpen => {
                in_fence = true;
                core.push(raw);
                i += 1;
            }
            DocLineClass::Image => {
                // Class guarantees the shape; parse is infallible here.
                let (alt, src_ref) = image_line(trimmed).expect("classified image line");
                flush(&mut core, &mut out, styles);
                out.push(DocBlock::Image(ImageBlock::new(alt, src_ref)));
                i += 1;
            }
            DocLineClass::Task => {
                let line = raw.trim_end();
                let indent = line.len() - line.trim_start().len();
                let (checked, rest) = task_item(trimmed).expect("classified task line");
                flush(&mut core, &mut out, styles);
                out.push(DocBlock::Task(TaskBlock::new(
                    checked,
                    (indent / 2).min(8) as u8,
                    parse_inline(rest, styles, styles.base),
                )));
                i += 1;
            }
            DocLineClass::PipeText
                if lines
                    .get(i + 1)
                    .is_some_and(|next| table_opens(trimmed, next)) =>
            {
                flush(&mut core, &mut out, styles);
                let align = delimiter_alignments(lines[i + 1].trim_end().trim_start())
                    .expect("table_opens verified the delimiter");
                let header = split_row_cells(trimmed)
                    .into_iter()
                    .map(|c| parse_inline(&c, styles, styles.base))
                    .collect();
                i += 2; // header + delimiter consumed
                let mut rows = Vec::new();
                while i < lines.len() && doc_line_class(lines[i]) == DocLineClass::PipeText {
                    rows.push(
                        split_row_cells(lines[i].trim_end().trim_start())
                            .into_iter()
                            .map(|c| parse_inline(&c, styles, styles.base))
                            .collect(),
                    );
                    i += 1;
                }
                out.push(DocBlock::Table(TableBlock::new(align, header, rows)));
            }
            // Pipe line without a delimiter next, or any core line:
            // core markdown.
            _ => {
                core.push(raw);
                i += 1;
            }
        }
    }
    flush(&mut core, &mut out, styles);
    out
}

/// Line classification for the DOC dispatch — shared verbatim between
/// [`parse_doc`] and the streaming seal (`DocStreamSession`), so batch
/// and stream can never disagree on what a line is. Order matters and
/// mirrors the dispatch: fence, image, task, core boundary, pipe text,
/// plain text.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum DocLineClass {
    /// Opens a fenced code block.
    FenceOpen,
    /// A whole-line `![alt](src)` image block.
    Image,
    /// A `- [ ]`/`- [x]` task item.
    Task,
    /// Blank / heading / rule / quote / list: closes paragraphs and
    /// tables, never joins either.
    Boundary,
    /// Plain text carrying at least one unescaped `|` — a table header
    /// candidate, delimiter, or body row (context decides).
    PipeText,
    /// Plain text: opens or continues a paragraph.
    ParaText,
}

pub(super) fn doc_line_class(raw: &str) -> DocLineClass {
    let trimmed = raw.trim_end().trim_start();
    // Refine the CORE classifier (one boundary rule set, no drift):
    // task items are core list lines; block images and pipe lines are
    // core paragraph text.
    match super::stream::line_class(raw) {
        super::stream::LineClass::FenceOpen => DocLineClass::FenceOpen,
        super::stream::LineClass::Boundary => {
            if task_item(trimmed).is_some() {
                DocLineClass::Task
            } else {
                DocLineClass::Boundary
            }
        }
        super::stream::LineClass::ParaText => {
            if image_line(trimmed).is_some() {
                DocLineClass::Image
            } else if has_unescaped_pipe(trimmed) {
                DocLineClass::PipeText
            } else {
                DocLineClass::ParaText
            }
        }
    }
}

/// Does `header` + `next` open a table? (`next` must be a delimiter row
/// with the same cell count.) Both arguments are raw lines.
pub(super) fn table_opens(header_trimmed: &str, next_raw: &str) -> bool {
    match delimiter_alignments(next_raw.trim_end().trim_start()) {
        Some(align) => split_row_cells(header_trimmed).len() == align.len(),
        None => false,
    }
}

fn has_unescaped_pipe(line: &str) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'|' => return true,
            _ => i += 1,
        }
    }
    false
}

/// Splits a trimmed row line into cell texts: one optional boundary
/// pipe stripped per side, unescaped `|` separates, cells trim their
/// whitespace, `\|` unescapes to a literal pipe (other escapes are the
/// inline parser's job).
pub(super) fn split_row_cells(trimmed: &str) -> Vec<String> {
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let b = inner.as_bytes();
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    // True only when the very last consumed byte was an UNESCAPED `|`
    // (escape parity is a walk fact — `a\\|` ends on a boundary pipe,
    // `a\|` does not; no suffix test can tell them apart).
    let mut ended_on_boundary = false;
    while i < b.len() {
        match b[i] {
            b'\\' if b.get(i + 1) == Some(&b'|') => {
                cur.push('|');
                i += 2;
                ended_on_boundary = false;
            }
            b'\\' if i + 1 < b.len() => {
                // Keep the escape pair verbatim for the inline parser
                // (multi-byte safe: one whole char follows).
                cur.push('\\');
                let start = i + 1;
                i = next_char_boundary(inner, start);
                cur.push_str(&inner[start..i]);
                ended_on_boundary = false;
            }
            b'|' => {
                cells.push(std::mem::take(&mut cur));
                i += 1;
                ended_on_boundary = true;
            }
            _ => {
                // Copy one whole char (UTF-8 safe).
                let start = i;
                i = next_char_boundary(inner, start);
                cur.push_str(&inner[start..i]);
                ended_on_boundary = false;
            }
        }
    }
    // A trailing boundary pipe leaves an empty final accumulator —
    // drop it; anything else (including interior emptiness) is a cell.
    if !(cur.is_empty() && ended_on_boundary) {
        cells.push(cur);
    }
    for c in &mut cells {
        let t = c.trim();
        if t.len() != c.len() {
            *c = t.to_string();
        }
    }
    cells
}

/// Byte offset one whole char after `start` (or the string's end).
fn next_char_boundary(s: &str, start: usize) -> usize {
    s[start..]
        .char_indices()
        .nth(1)
        .map(|(o, _)| start + o)
        .unwrap_or(s.len())
}

/// Parses a delimiter row (`| :-- | :-: | --: |` shapes). Returns the
/// per-column alignments, or `None` when the line is not a delimiter.
/// Requires at least one unescaped `|` (so `---` stays a rule) and
/// every cell to be `:?-+:?`.
pub(super) fn delimiter_alignments(trimmed: &str) -> Option<Vec<CellAlign>> {
    if !has_unescaped_pipe(trimmed) {
        return None;
    }
    let cells = split_row_cells(trimmed);
    if cells.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(cells.len());
    for cell in &cells {
        let c = cell.as_str();
        let left = c.starts_with(':');
        let right = c.ends_with(':') && c.len() > 1;
        let dashes = &c[usize::from(left)..c.len() - usize::from(right)];
        if dashes.is_empty() || !dashes.bytes().all(|b| b == b'-') {
            return None;
        }
        out.push(match (left, right) {
            (true, true) => CellAlign::Center,
            (false, true) => CellAlign::Right,
            _ => CellAlign::Left,
        });
    }
    Some(out)
}

/// `![alt](src)` covering the WHOLE trimmed line; `src` must be
/// non-empty (an empty target stays literal text, matching the core
/// link rule). Alt text is verbatim (it is caption material).
pub(super) fn image_line(trimmed: &str) -> Option<(String, String)> {
    if !trimmed.starts_with("![") {
        return None;
    }
    let b = trimmed.as_bytes();
    let alt_end = find_close(b, 2, b"]")?;
    if b.get(alt_end + 1) != Some(&b'(') {
        return None;
    }
    let src_end = find_close(b, alt_end + 2, b")")?;
    if src_end + 1 != b.len() {
        return None; // trailing content: not a block image
    }
    let src = &trimmed[alt_end + 2..src_end];
    if src.is_empty() {
        return None;
    }
    Some((trimmed[2..alt_end].to_string(), src.to_string()))
}

/// `- [ ] rest` / `- [x] rest` (any bullet). Returns (checked, rest).
pub(super) fn task_item(trimmed: &str) -> Option<(bool, &str)> {
    let (marker, rest) = list_marker(trimmed)?;
    if marker != Marker::Bullet {
        return None;
    }
    let checked = if rest.starts_with("[ ]") {
        false
    } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
        true
    } else {
        return None;
    };
    match rest.as_bytes().get(3) {
        None => Some((checked, "")),
        Some(b' ') => Some((checked, rest[4..].trim_start())),
        Some(_) => None, // "[ ]x" is ordinary list text
    }
}

#[cfg(test)]
#[path = "md_doc_tests.rs"]
mod tests;
