//! Doc-vocabulary parser tests: table/image/task recognition, the
//! core-equivalence pin, cell lexing edge cases, and hostile-input
//! fuzz (never panics, and always parses to SOMETHING).

use super::*;
use crate::render::md::{parse, parse_doc, CellAlign, DocBlock, MdStyles};
use crate::testing::{hostile_corpus, Rng};

fn styles() -> MdStyles {
    MdStyles::default()
}

fn plain_cells(cells: &[RichLine]) -> Vec<String> {
    cells.iter().map(|c| c.plain()).collect()
}

#[test]
fn table_parses_header_alignment_and_rows() {
    let doc = "| Name | Count | Notes |\n|:-----|:-----:|------:|\n| a | 1 | x |\n| b | 2 | y |";
    let blocks = parse_doc(doc, &styles());
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    let DocBlock::Table(t) = &blocks[0] else {
        panic!("expected a table, got {blocks:?}");
    };
    assert_eq!(
        t.align,
        vec![CellAlign::Left, CellAlign::Center, CellAlign::Right]
    );
    assert_eq!(plain_cells(&t.header), vec!["Name", "Count", "Notes"]);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(plain_cells(&t.rows[0]), vec!["a", "1", "x"]);
    assert_eq!(plain_cells(&t.rows[1]), vec!["b", "2", "y"]);
}

#[test]
fn table_rows_pad_and_truncate_to_header_count() {
    let doc = "| a | b |\n|---|---|\n| only |\n| 1 | 2 | 3 extra |";
    let blocks = parse_doc(doc, &styles());
    let DocBlock::Table(t) = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(plain_cells(&t.rows[0]), vec!["only", ""]);
    assert_eq!(plain_cells(&t.rows[1]), vec!["1", "2"], "extras drop");
}

#[test]
fn table_needs_matching_delimiter_and_closes_on_non_pipe_lines() {
    // Count mismatch: no table, plain paragraphs.
    let doc = "| a | b |\n|---|\nplain";
    assert!(
        parse_doc(doc, &styles())
            .iter()
            .all(|b| matches!(b, DocBlock::Core(_))),
        "count mismatch must stay core"
    );
    // Blank closes the table; the following paragraph is core.
    let doc = "| a |\n|---|\n| 1 |\n\nafter";
    let blocks = parse_doc(doc, &styles());
    assert!(matches!(blocks[0], DocBlock::Table(_)));
    assert!(
        matches!(&blocks[1], DocBlock::Core(Block::Paragraph(l)) if l.plain() == "after"),
        "{blocks:?}"
    );
    // A pipe-less prose line closes the table too (documented
    // deviation from GFM's one-cell-row absorption).
    let doc = "| a |\n|---|\nprose without pipes";
    let blocks = parse_doc(doc, &styles());
    assert!(matches!(blocks[0], DocBlock::Table(_)));
    assert!(matches!(&blocks[1], DocBlock::Core(Block::Paragraph(_))));
}

#[test]
fn table_interrupts_a_paragraph_and_rule_is_not_a_delimiter() {
    let doc = "intro text\n| a | b |\n|---|---|\n| 1 | 2 |";
    let blocks = parse_doc(doc, &styles());
    assert!(matches!(&blocks[0], DocBlock::Core(Block::Paragraph(l)) if l.plain() == "intro text"));
    assert!(matches!(blocks[1], DocBlock::Table(_)));
    // `---` (no pipe) is a RULE, never a one-column delimiter.
    let doc = "header-ish\n---\nbody";
    let blocks = parse_doc(doc, &styles());
    assert!(
        blocks.iter().all(|b| matches!(b, DocBlock::Core(_))),
        "{blocks:?}"
    );
}

#[test]
fn escaped_pipes_are_literal_cell_content() {
    let doc = "| a \\| b | c |\n|---|---|\n| \\|start | end\\| |";
    let blocks = parse_doc(doc, &styles());
    let DocBlock::Table(t) = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(plain_cells(&t.header), vec!["a | b", "c"]);
    assert_eq!(plain_cells(&t.rows[0]), vec!["|start", "end|"]);
}

#[test]
fn inline_spans_parse_inside_cells() {
    let doc = "| head |\n|---|\n| has **bold** and `code` |";
    let blocks = parse_doc(doc, &styles());
    let DocBlock::Table(t) = &blocks[0] else {
        panic!("{blocks:?}");
    };
    let cell = &t.rows[0][0];
    assert_eq!(cell.plain(), "has bold and code");
    assert!(
        cell.spans
            .iter()
            .any(|s| s.text == "bold" && s.style.add.contains(crate::render::Attrs::BOLD)),
        "{cell:?}"
    );
}

#[test]
fn pipes_inside_fences_are_code_not_tables() {
    let doc = "```\n| a | b |\n|---|---|\n```";
    let blocks = parse_doc(doc, &styles());
    assert_eq!(blocks.len(), 1);
    assert!(
        matches!(&blocks[0], DocBlock::Core(Block::CodeFence { lines, .. }) if lines.len() == 2),
        "{blocks:?}"
    );
}

#[test]
fn image_lines_become_image_blocks_inline_images_stay_text() {
    let doc = "![logo](img/logo.png)\n\ntext with ![inline](x.png) stays text\n\n![](no-alt.png)";
    let blocks = parse_doc(doc, &styles());
    assert!(
        matches!(&blocks[0], DocBlock::Image(i) if i.alt == "logo" && i.src == "img/logo.png"),
        "{blocks:?}"
    );
    assert!(matches!(&blocks[1], DocBlock::Core(Block::Paragraph(_))));
    assert!(
        matches!(&blocks[2], DocBlock::Image(i) if i.alt.is_empty() && i.src == "no-alt.png"),
        "empty alt is legal: {blocks:?}"
    );
    // Empty src stays literal; trailing content disqualifies the line.
    for doc in ["![alt]()", "![a](b.png) trailing"] {
        let blocks = parse_doc(doc, &styles());
        assert!(
            blocks.iter().all(|b| matches!(b, DocBlock::Core(_))),
            "{doc:?} -> {blocks:?}"
        );
    }
}

#[test]
fn task_items_parse_and_numbered_tasks_stay_lists() {
    let doc = "- [ ] open item\n- [x] done **bold**\n  - [X] nested\n- [ ]\n1. [ ] numbered";
    let blocks = parse_doc(doc, &styles());
    assert!(
        matches!(&blocks[0], DocBlock::Task(t) if !t.checked && t.content.plain() == "open item")
    );
    assert!(
        matches!(&blocks[1], DocBlock::Task(t) if t.checked && t.content.plain() == "done bold")
    );
    assert!(matches!(&blocks[2], DocBlock::Task(t) if t.checked && t.depth == 1));
    assert!(
        matches!(&blocks[3], DocBlock::Task(t) if !t.checked && t.content.plain().is_empty()),
        "bare checkbox: {blocks:?}"
    );
    assert!(
        matches!(&blocks[4], DocBlock::Core(Block::ListItem { .. })),
        "numbered stays a list item: {blocks:?}"
    );
    // "[ ]x" without the separating space is ordinary list text.
    let blocks = parse_doc("- [ ]x not a task", &styles());
    assert!(matches!(&blocks[0], DocBlock::Core(Block::ListItem { .. })));
}

/// THE additive pin: for sources with none of the extended constructs,
/// `parse_doc` is exactly `parse` wrapped in `Core`.
#[test]
fn doc_parse_equals_core_parse_on_core_sources() {
    let styles = styles();
    let corpus = [
        "",
        "plain paragraph\njoined",
        "# Title\n\nIntro **bold** `code` [l](u).\n\n- a\n- b\n  - c\n1. n\n\n> q\n\n```rust\nlet x;\n```\n\n---\ntail",
        "```\nunclosed fence\nstill code",
        "> quote\n\npara\n\n###### deep",
        "text\n---x\n\n---",
        "héllo — 世界 👍🏽",
    ];
    for doc in corpus {
        let expected: Vec<DocBlock> = parse(doc, &styles)
            .into_iter()
            .map(DocBlock::Core)
            .collect();
        assert_eq!(parse_doc(doc, &styles), expected, "source: {doc:?}");
    }
}

#[test]
fn cell_lexer_edges() {
    assert_eq!(split_row_cells("a|b"), vec!["a", "b"]);
    assert_eq!(split_row_cells("| a | b |"), vec!["a", "b"]);
    assert_eq!(split_row_cells("|a|"), vec!["a"]);
    assert_eq!(split_row_cells("||"), vec![""]);
    assert_eq!(split_row_cells("|"), vec![""], "one empty cell");
    assert_eq!(split_row_cells("a\\|b"), vec!["a|b"]);
    // An ESCAPED backslash then a REAL boundary pipe at the end: the
    // trailing empty accumulator drops (escape parity is walk state).
    assert_eq!(split_row_cells("a\\\\|"), vec!["a\\\\"]);
    assert_eq!(split_row_cells("世|界"), vec!["世", "界"]);
    assert_eq!(
        delimiter_alignments("|:--|:-:|--:|---|"),
        Some(vec![
            CellAlign::Left,
            CellAlign::Center,
            CellAlign::Right,
            CellAlign::Left
        ])
    );
    assert_eq!(delimiter_alignments("---"), None, "no pipe, no delimiter");
    assert_eq!(delimiter_alignments("|::|"), None, "dashes required");
    assert_eq!(delimiter_alignments("| - x |"), None);
}

/// Hostile-input fuzz: `parse_doc` (and the classifiers) never panic on
/// any byte soup, and parse SOMETHING for every input (total function).
#[test]
fn parse_doc_survives_hostile_corpus_and_markdown_soup() {
    let styles = styles();
    for chunk in hostile_corpus(0x0142, 300) {
        let s = String::from_utf8_lossy(&chunk);
        let _ = parse_doc(&s, &styles);
        let _ = doc_line_class(&s);
    }
    // Markdown-shaped soup biased toward table/image/task fragments.
    let fragments = [
        "|", "|-", "---|", ":-:", "\\|", "| a ", "![", "![a](", "](x)", "- [ ] ", "- [x]", "```",
        "\n", "# ", "> ", "**b**", "世|界", " ", "x",
    ];
    let mut rng = Rng::new(0x0142_0142);
    for _ in 0..400 {
        let mut doc = String::new();
        for _ in 0..rng.below(30) {
            doc.push_str(fragments[rng.below(fragments.len())]);
        }
        let _ = parse_doc(&doc, &styles);
    }
}
