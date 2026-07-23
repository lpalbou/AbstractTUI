//! Find-in-typeset-text (0148) + THE TEXT↔CELLS MAPPING SUBSTRATE.
//!
//! ## The mapping contract (shared with 0160 content selection)
//!
//! The typeset result is a `Vec<Row>`; one Row = one LINE FRAGMENT
//! (post-wrap). Its logical text is the concatenation of its spans
//! (`RichLine::plain()`), addressed by BYTE OFFSET; its cells are
//! COLUMNS relative to the content rect's left edge, starting at
//! `row.indent` (exactly where `draw_rows` puts the first cluster —
//! same spans, same segmentation, same widths, so text→cells can never
//! drift from the pixels). The two directions:
//!
//! - [`row_col_at_byte`]: byte offset → column (search rects, caret
//!   placement, selection endpoints → draw).
//! - [`row_byte_at_col`]: column → byte offset (mouse hit → text;
//!   the selection direction, built here per the 0148/0160 pact:
//!   whichever lands first builds the mapping, the other consumes).
//!
//! Offsets snap to grapheme-cluster boundaries (a match landing inside
//! `é`/emoji covers the whole cluster — cells hold clusters, not
//! bytes). Matches never span rows: search happens over what the eye
//! sees, one fragment at a time.
//!
//! ## Highlight pass
//!
//! Non-destructive style patch at draw: matched slices are RE-PRINTED
//! over the already-drawn row in selection tones (`selection_fg` on
//! `selection_bg` — the token set carries no dedicated search tone, so
//! the documented selection pair serves; the CURRENT match adds
//! BOLD+UNDERLINE as its distinct treatment). Glyphs stay identical by
//! construction (same text, same columns). Empty query/matches = the
//! pass is never entered: zero idle cost.
//!
//! OWNER: READER (app-widgets wave 3).

use crate::base::Rect;
use crate::render::{Attrs, Style};
use crate::text;
use crate::theme::TokenSet;
use crate::ui::StyledCanvas;

use super::Row;

/// One search match in the typeset document (see [`super::MarkdownView::find`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MdSearchMatch {
    /// Typeset row index (absolute — scroll offsets subtract later).
    pub row: usize,
    /// Byte range in the row's logical text (`RichLine::plain()`),
    /// snapped OUT to grapheme-cluster boundaries.
    pub bytes: (usize, usize),
    /// Column range `[start, end)` relative to the content rect's left
    /// edge (indent included) — the highlight rect on that row.
    pub cells: (i32, i32),
}

/// Find `query` across typeset rows. Literal match; `fold_case` uses
/// full Unicode lowercasing with offset-true mapping. Image and rule
/// rows carry no text and never match. Non-overlapping, reading order.
pub(crate) fn find_in_rows(rows: &[Row], query: &str, fold_case: bool) -> Vec<MdSearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = if fold_case {
        fold(query)
    } else {
        query.to_string()
    };
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        if row.rule || row.image.is_some() || row.line.is_empty() {
            continue;
        }
        let plain = row.line.plain();
        if fold_case {
            let (folded, map) = fold_with_map(&plain);
            let mut from = 0;
            while let Some(rel) = folded[from..].find(needle.as_str()) {
                let fa = from + rel;
                let fb = fa + needle.len();
                let a = map[fa].0;
                let b = map[fb - 1].1;
                push_match(&mut out, row_idx, row, &plain, a, b);
                from = fb;
            }
        } else {
            let mut from = 0;
            while let Some(rel) = plain[from..].find(needle.as_str()) {
                let a = from + rel;
                let b = a + needle.len();
                push_match(&mut out, row_idx, row, &plain, a, b);
                from = b;
            }
        }
    }
    out
}

fn push_match(
    out: &mut Vec<MdSearchMatch>,
    row_idx: usize,
    row: &Row,
    plain: &str,
    a: usize,
    b: usize,
) {
    let (a, b) = snap_to_clusters(plain, a, b);
    out.push(MdSearchMatch {
        row: row_idx,
        bytes: (a, b),
        cells: (row_col_at_byte(row, a), row_col_at_byte(row, b)),
    });
}

/// The highlight pass: re-print matched slices in selection tones over
/// rows already drawn by `draw_rows` (same visible slice: `rows[offset..]`).
pub(crate) fn draw_highlights(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    t: &TokenSet,
    rows: &[Row],
    offset: usize,
    matches: &[MdSearchMatch],
    current: Option<usize>,
) {
    let base = Style::new().fg(t.selection_fg).bg(t.selection_bg);
    let strong = base.attrs(Attrs::BOLD | Attrs::UNDERLINE);
    let visible = rect.h.max(0) as usize;
    for (i, m) in matches.iter().enumerate() {
        if m.row < offset || m.row >= offset + visible {
            continue;
        }
        let Some(row) = rows.get(m.row) else { continue };
        let y = rect.y + (m.row - offset) as i32;
        let style = if current == Some(i) { strong } else { base };
        // Walk the row's spans; re-print every slice overlapping the
        // match, at its own column (allocation-free: &str slices).
        let mut span_start = 0usize;
        let mut x = rect.x + row.indent;
        for span in &row.line.spans {
            let span_end = span_start + span.text.len();
            if span_end > m.bytes.0 && span_start < m.bytes.1 {
                let lo = m.bytes.0.max(span_start);
                let hi = m.bytes.1.min(span_end);
                let before = &span.text[..lo - span_start];
                let slice = &span.text[lo - span_start..hi - span_start];
                let sx = x + text::width(before);
                super::super::richtext::print_span_clipped(
                    canvas,
                    sx,
                    y,
                    rect.right(),
                    slice,
                    &style,
                );
            }
            x += text::width(&span.text);
            span_start = span_end;
            if span_start >= m.bytes.1 {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The text↔cells substrate (0160 reuse surface)
// ---------------------------------------------------------------------------

/// Byte offset → column (content-relative, indent included). Offsets
/// inside a cluster resolve to the cluster's START column; offsets at
/// or past the text's end resolve to the row's end column.
pub(crate) fn row_col_at_byte(row: &Row, byte: usize) -> i32 {
    let mut col = row.indent;
    // `segments` offsets are span-local; `base` carries the bytes of
    // the spans already walked.
    let mut base = 0usize;
    for span in &row.line.spans {
        for seg in text::segments(&span.text) {
            if base + seg.offset + seg.cluster.len() > byte {
                return col;
            }
            col += seg.width;
        }
        base += span.text.len();
    }
    col
}

/// Column → byte offset: the byte of the cluster occupying `col`
/// (columns before the indent map to byte 0; past the row's end, to
/// the text length). The 0160 selection direction — mouse x → text.
#[allow(dead_code)] // consumed by 0160 selection; contract-tested here
pub(crate) fn row_byte_at_col(row: &Row, col: i32) -> usize {
    let mut cur = row.indent;
    let mut base = 0usize;
    for span in &row.line.spans {
        for seg in text::segments(&span.text) {
            if seg.width > 0 && col < cur + seg.width {
                return base + seg.offset;
            }
            cur += seg.width;
        }
        base += span.text.len();
    }
    base
}

/// Expand `[a, b)` to grapheme-cluster boundaries of `plain`.
fn snap_to_clusters(plain: &str, a: usize, b: usize) -> (usize, usize) {
    let mut start = 0usize;
    let mut end = plain.len();
    for seg in text::segments(plain) {
        let s = seg.offset;
        let e = seg.offset + seg.cluster.len();
        if s <= a {
            start = s;
        }
        if s < b && e >= b {
            end = e;
            break;
        }
    }
    if b >= plain.len() {
        end = plain.len();
    }
    (start.min(plain.len()), end.max(start))
}

/// Unicode-lowercase fold (no offset map — query side).
fn fold(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

/// Fold with a per-BYTE map back to the original char's `(start, end)`
/// byte range, so folded match offsets translate to original offsets
/// even through one-to-many lowercasing (İ → i + combining dot).
fn fold_with_map(s: &str) -> (String, Vec<(usize, usize)>) {
    let mut folded = String::with_capacity(s.len());
    let mut map: Vec<(usize, usize)> = Vec::with_capacity(s.len());
    for (o, c) in s.char_indices() {
        let end = o + c.len_utf8();
        for lc in c.to_lowercase() {
            let n = lc.len_utf8();
            folded.push(lc);
            for _ in 0..n {
                map.push((o, end));
            }
        }
    }
    (folded, map)
}

#[cfg(test)]
#[path = "markdown_search_tests.rs"]
mod tests;
