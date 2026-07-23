//! Search + mapping tests: rect goldens across wrapping/styled spans,
//! case folding with offset truth, cluster snapping, the mapping
//! round-trip (the 0160 contract), highlight painting, and zero-cost
//! empties.

use super::*;
use crate::base::{Point, Size};
use crate::theme::default_theme;
use crate::widgets::test_util::draw_into;
use crate::widgets::MarkdownView;

fn t() -> crate::theme::TokenSet {
    default_theme().tokens
}

#[test]
fn finds_matches_across_wrapped_rows_with_correct_cells() {
    let doc = "alpha beta gamma beta delta";
    // Width 12 wraps: "alpha beta" / "gamma beta" / "delta".
    let m = MarkdownView::find(doc, &t(), 12, "beta", false);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].row, 0);
    assert_eq!(m[0].cells, (6, 10), "second word of row 0");
    assert_eq!(m[1].row, 1);
    assert_eq!(m[1].cells, (6, 10));
    // Matches never span rows: "gammabeta" split by wrap ≠ a match.
    assert!(MarkdownView::find(doc, &t(), 12, "beta gamma", false).is_empty());
}

#[test]
fn cells_account_for_indent_and_styled_spans() {
    // List item: indent 2 + "• " marker; bold spans must not shift
    // columns (width comes from text, not styling).
    let doc = "- has **bold** word";
    let m = MarkdownView::find(doc, &t(), 40, "word", false);
    assert_eq!(m.len(), 1);
    // Row text: "• has bold word" at indent 2 -> "word" starts at
    // plain col 11 + indent 2.
    assert_eq!(m[0].cells, (13, 17), "{m:?}");
}

#[test]
fn case_insensitive_folds_unicode_with_true_offsets() {
    let doc = "Grüße from Berlin. GRÜSSE again.";
    let m = MarkdownView::find(doc, &t(), 80, "grüße", true);
    assert_eq!(m.len(), 1, "ß folds to ss, GRÜSSE does not equal grüße");
    let m = MarkdownView::find(doc, &t(), 80, "grüsse", true);
    assert_eq!(m.len(), 1, "the folded needle matches GRÜSSE");
    assert_eq!(&doc[m[0].bytes.0..m[0].bytes.1], "GRÜSSE");
    // Wide CJK: columns are display columns, not chars.
    let m = MarkdownView::find("字 word 字", &t(), 80, "word", true);
    assert_eq!(m[0].cells, (3, 7), "wide cluster before the match");
}

#[test]
fn match_bytes_snap_to_cluster_boundaries() {
    // Decomposed é: "e" + U+0301. A search for "e" must cover the
    // whole cluster (re-printing half a cluster would tear the glyph).
    let doc = "caf\u{65}\u{301} time";
    let m = MarkdownView::find(doc, &t(), 80, "e", true);
    assert_eq!(m.len(), 2, "cafe's e and time's e: {m:?}");
    let first = &m[0];
    assert_eq!(
        &doc[first.bytes.0..first.bytes.1],
        "\u{65}\u{301}",
        "snapped out to the full cluster"
    );
    assert_eq!(first.cells.1 - first.cells.0, 1, "one column, one cluster");
}

#[test]
fn empty_query_is_free_and_table_text_is_searchable() {
    assert!(MarkdownView::find("anything", &t(), 40, "", false).is_empty());
    assert!(MarkdownView::find("anything", &t(), 40, "", true).is_empty());
    // Typeset table text (padded, aligned) is what search sees.
    let doc = "| Name |\n|---|\n| alpha |";
    let m = MarkdownView::find(doc, &t(), 40, "alpha", false);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].row, 2, "body row below header + separator");
}

/// The 0160 mapping round-trip: byte→col→byte is identity at cluster
/// starts; col→byte→col lands on the cluster's start column.
#[test]
fn mapping_round_trips_at_cluster_boundaries() {
    let line = {
        let mut l = crate::render::rich::RichLine::new();
        l.push(crate::render::rich::Span::new(
            "ab 字 c",
            crate::render::Style::EMPTY,
        ));
        l.push(crate::render::rich::Span::new(
            "def 👍🏽 g",
            crate::render::Style::new().attrs(crate::render::Attrs::BOLD),
        ));
        l
    };
    let row = Row {
        line,
        indent: 3,
        ground: None,
        quote: false,
        rule: false,
        image: None,
    };
    let plain = row.line.plain();
    let mut offsets: Vec<usize> = Vec::new();
    for seg in crate::text::segments(&plain) {
        offsets.push(seg.offset);
    }
    offsets.push(plain.len());
    for &byte in &offsets {
        let col = row_col_at_byte(&row, byte);
        assert!(col >= row.indent);
        if byte < plain.len() {
            assert_eq!(
                row_byte_at_col(&row, col),
                byte,
                "byte {byte} -> col {col} -> byte"
            );
        }
    }
    // Columns before the indent map to byte 0; past the end, to len.
    assert_eq!(row_byte_at_col(&row, 0), 0);
    assert_eq!(row_byte_at_col(&row, 9_999), plain.len());
    // The SECOND cell of a wide cluster maps back to the cluster start.
    let wide_byte = plain.find('字').unwrap();
    let wide_col = row_col_at_byte(&row, wide_byte);
    assert_eq!(row_byte_at_col(&row, wide_col + 1), wide_byte);
}

#[test]
fn highlights_paint_selection_tones_and_current_gets_bold() {
    let tokens = t();
    let doc = "find the needle here and the needle there";
    let width = 50;
    let matches = MarkdownView::find(doc, &tokens, width, "needle", false);
    assert_eq!(matches.len(), 2);
    let c = draw_into(
        MarkdownView::new(doc)
            .highlights(matches.clone(), Some(1))
            .element(&tokens),
        Size::new(width, 3),
    );
    let (x0, _) = matches[0].cells;
    let cell = c.cell(Point::new(x0, 0)).unwrap();
    assert_eq!(cell.0, 'n', "glyphs survive the patch");
    assert_eq!(cell.2, tokens.selection_bg, "match wears selection ground");
    assert_eq!(cell.1, tokens.selection_fg);
    // Current match: same tones + BOLD/UNDERLINE distinct treatment.
    let (x1, _) = matches[1].cells;
    let attrs = c.attrs_at(Point::new(x1, 0));
    assert!(attrs.contains(crate::render::Attrs::BOLD), "{attrs:?}");
    assert!(attrs.contains(crate::render::Attrs::UNDERLINE));
    let other_attrs = c.attrs_at(Point::new(x0, 0));
    assert!(!other_attrs.contains(crate::render::Attrs::UNDERLINE));
    // Outside the matches: untouched ground.
    let outside = c.cell(Point::new(0, 0)).unwrap();
    assert_ne!(outside.2, tokens.selection_bg);
}

#[test]
fn highlights_respect_scroll_offset_and_clip() {
    let tokens = t();
    // Match in the FIRST row; plenty of rows below so scrolling past
    // it stays within the clamp.
    let doc = "needle first\n\ntwo\n\nthree\n\nfour\n\nfive";
    let width = 30;
    let matches = MarkdownView::find(doc, &tokens, width, "needle", false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].row, 0);
    // Match visible at offset 0: painted.
    let c = draw_into(
        MarkdownView::new(doc)
            .highlights(matches.clone(), Some(0))
            .element(&tokens),
        Size::new(width, 2),
    );
    let (x0, _) = matches[0].cells;
    assert_eq!(c.cell(Point::new(x0, 0)).unwrap().2, tokens.selection_bg);
    // Scrolled PAST the match: nothing highlighted, nothing panics.
    let c = draw_into(
        MarkdownView::new(doc)
            .scroll_offset(1)
            .highlights(matches, Some(0))
            .element(&tokens),
        Size::new(width, 2),
    );
    for x in 0..width {
        assert_ne!(
            c.cell(Point::new(x, 0)).unwrap().2,
            tokens.selection_bg,
            "match above the viewport must not paint"
        );
    }
}

#[test]
fn find_survives_hostile_sources() {
    let tokens = t();
    for chunk in crate::testing::hostile_corpus(0x0148, 150) {
        let s = String::from_utf8_lossy(&chunk);
        for q in ["a", "\u{1b}", "世", "  "] {
            let _ = MarkdownView::find(&s, &tokens, 24, q, true);
            let _ = MarkdownView::find(&s, &tokens, 24, q, false);
        }
    }
}
