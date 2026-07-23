//! Doc typesetting tests: table goldens (alignment, truncation, inline
//! spans, shared-solver tiling), task rows, and the outline-rows
//! property (re-wrap then re-outline stays consistent).

use super::*;
use crate::base::{Point, Size};
use crate::render::md::Heading;
use crate::theme::default_theme;
use crate::widgets::test_util::{draw_into, row};
use crate::widgets::MarkdownView;

const TABLE_DOC: &str = "| Name | N |\n|:-----|--:|\n| alpha | 1 |\n| beta | 22 |";

fn rows_plain(source: &str, width: i32) -> Vec<String> {
    let t = default_theme().tokens;
    layout_doc(source, &t, width)
        .rows
        .iter()
        .map(|r| {
            if r.rule {
                "<rule>".to_string()
            } else {
                r.line.plain()
            }
        })
        .collect()
}

#[test]
fn table_typesets_alignment_padding_and_separator() {
    let rows = rows_plain(TABLE_DOC, 28);
    // Natural widths: col0 = 5 ("alpha"), col1 = 2 ("22"); 1-cell gap.
    assert_eq!(rows[0], "Name   N", "left header + right-aligned header");
    assert_eq!(rows[1], "─".repeat(8), "separator spans the table width");
    assert_eq!(rows[2], "alpha  1", "right-aligned numeric cell");
    assert_eq!(rows[3], "beta  22");
}

#[test]
fn table_center_alignment_and_cell_truncation() {
    let doc = "| head |\n|:---:|\n| ab |\n| much longer content |";
    // Fits: natural = width of the longest cell (19); centering leans
    // left on odd leftovers.
    let rows = rows_plain(doc, 40);
    assert_eq!(rows[0], "       head        ");
    assert_eq!(rows[2], "        ab         ");
    // Crushed: per-column ellipsis truncation, never a wrap or a panic.
    let rows = rows_plain(doc, 9);
    assert_eq!(rows[2], "   ab    ", "{rows:?}");
    assert!(rows[3].ends_with('…'), "{rows:?}");
    assert!(rows[3].starts_with("much"), "{rows:?}");
}

#[test]
fn table_draws_bold_header_and_border_separator() {
    let t = default_theme().tokens;
    let c = draw_into(MarkdownView::new(TABLE_DOC).element(&t), Size::new(28, 6));
    assert!(row(&c, 0).starts_with("Name"));
    assert!(
        c.attrs_at(Point::new(0, 0))
            .contains(crate::render::Attrs::BOLD),
        "header renders bold"
    );
    assert_eq!(c.cell(Point::new(0, 1)).unwrap().0, '─');
    assert_eq!(c.cell(Point::new(0, 1)).unwrap().1, t.border);
    assert!(row(&c, 2).starts_with("alpha"));
}

#[test]
fn table_inline_spans_keep_their_styles_in_cells() {
    let t = default_theme().tokens;
    let doc = "| c |\n|---|\n| `code` x |";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(20, 4));
    let body = row(&c, 2);
    let cx = body.find("code").unwrap() as i32;
    assert_eq!(
        c.cell(Point::new(cx, 2)).unwrap().2,
        t.surface_raised,
        "inline code chip ground survives the cell recipe"
    );
}

#[test]
fn zero_and_tiny_widths_never_panic() {
    for w in [0, 1, 2, 3, 5] {
        let _ = rows_plain(TABLE_DOC, w);
    }
    let t = default_theme().tokens;
    for size in [Size::new(0, 0), Size::new(2, 1), Size::new(6, 2)] {
        let _ = draw_into(MarkdownView::new(TABLE_DOC).element(&t), size);
    }
}

#[test]
fn task_rows_render_checkbox_markers_in_marker_ink() {
    let t = default_theme().tokens;
    let doc = "- [ ] open thing\n- [x] done thing\n  - [X] nested";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(30, 4));
    assert!(
        row(&c, 0).starts_with("  [ ] open thing"),
        "{:?}",
        row(&c, 0)
    );
    assert!(row(&c, 1).starts_with("  [x] done thing"));
    assert!(row(&c, 2).starts_with("    [x] nested"), "depth indents");
    let mx = row(&c, 0).find('[').unwrap() as i32;
    assert_eq!(c.cell(Point::new(mx, 0)).unwrap().1, t.accent_alt);
}

#[test]
fn task_content_wraps_with_hanging_indent() {
    let doc = "- [x] a rather long task item that wraps";
    let rows = rows_plain(doc, 18);
    assert!(rows.len() >= 2, "{rows:?}");
    assert!(rows[0].starts_with("[x] a rather"), "{rows:?}");
}

/// 0146: outline rows point at the heading's typeset row — across
/// widths (re-wrap then re-outline stays consistent).
#[test]
fn outline_rows_match_the_typeset_fold_across_widths() {
    let t = default_theme().tokens;
    let doc = "\
# Intro

Some paragraph text that will wrap at narrow widths for sure.

## Setup steps

| a | b |
|---|---|
| 1 | 2 |

### Deep dive

- [ ] task
- item

## Setup steps

tail paragraph
";
    let expected: Vec<Heading> = crate::render::md::outline(doc);
    assert_eq!(expected.len(), 4);
    for width in [10, 16, 24, 40, 80] {
        let entries = outline_rows(doc, &t, width);
        assert_eq!(entries.len(), expected.len(), "width {width}");
        let fold = layout_doc(doc, &t, width);
        let mut last = 0usize;
        for (entry, want) in entries.iter().zip(&expected) {
            assert_eq!(&entry.heading, want, "width {width}");
            assert!(
                entry.row >= last,
                "rows monotonic at width {width}: {entries:?}"
            );
            last = entry.row;
            // The pointed row IS the heading's text row (headings
            // typeset as one unwrapped row; draw truncates overwide).
            assert_eq!(
                fold.rows[entry.row].line.plain(),
                entry.heading.text,
                "width {width}, anchor {}",
                entry.heading.anchor_id
            );
        }
    }
}

#[test]
fn resolve_anchor_finds_rows_and_scroll_reaches_them() {
    let t = default_theme().tokens;
    let doc = "# Top\n\nfiller one\n\nfiller two\n\n## Target\n\nbody";
    let width = 30;
    let row_idx = MarkdownView::resolve_anchor(doc, &t, width, "#target").unwrap();
    assert!(row_idx > 0);
    assert_eq!(
        MarkdownView::resolve_anchor(doc, &t, width, "target"),
        Some(row_idx),
        "leading # optional"
    );
    assert_eq!(
        MarkdownView::resolve_anchor(doc, &t, width, "#missing"),
        None
    );
    // Scrolling to the anchor row puts the heading on the first line.
    let c = draw_into(
        MarkdownView::new(doc)
            .scroll_offset(row_idx as i32)
            .element(&t),
        Size::new(width, 4),
    );
    assert!(row(&c, 0).starts_with("Target"), "{:?}", row(&c, 0));
}

#[test]
fn core_sources_typeset_identically_through_the_doc_fold() {
    // The doc fold must not change core rendering: same rows as the
    // core recipe for a table-free document.
    let t = default_theme().tokens;
    let doc = "# T\n\npara **bold**\n\n- li\n\n> q\n\n```\ncode\n```\n\n---";
    let ts = BlockTypesetter::new(&t);
    let mut core_rows: Vec<Row> = Vec::new();
    for block in crate::render::md::parse(doc, ts.styles()) {
        ts.push_block(&mut core_rows, &block, 28, true);
    }
    let doc_rows = layout_doc(doc, &t, 28).rows;
    assert_eq!(core_rows.len(), doc_rows.len());
    for (a, b) in core_rows.iter().zip(&doc_rows) {
        assert_eq!(a.line.plain(), b.line.plain());
        assert_eq!(a.indent, b.indent);
        assert_eq!(a.rule, b.rule);
    }
}
