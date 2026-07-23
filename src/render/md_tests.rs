//! Markdown-lite tests: the supported subset, clean degradation of
//! everything else.

use super::*;

fn parse_default(src: &str) -> Vec<Block> {
    parse(src, &MdStyles::default())
}

fn styles() -> MdStyles {
    MdStyles::default()
}

#[test]
fn representative_document() {
    let doc = "\
# Title

Intro paragraph with **bold**, *italic*, `code` and [a link](https://x.example).
It continues on a second source line.

## Section

- first item
- second **strong** item
  - nested item
1. numbered

> quoted wisdom

```rust
let x = 1; // verbatim, **not** parsed
```

---
tail";
    let blocks = parse_default(doc);
    // Block shape, in order.
    assert!(matches!(blocks[0], Block::Heading { level: 1, .. }));
    assert!(matches!(blocks[1], Block::Paragraph(_)));
    assert!(matches!(blocks[2], Block::Heading { level: 2, .. }));
    assert!(matches!(
        blocks[3],
        Block::ListItem {
            depth: 0,
            marker: Marker::Bullet,
            ..
        }
    ));
    assert!(matches!(blocks[4], Block::ListItem { .. }));
    assert!(matches!(
        blocks[5],
        Block::ListItem {
            depth: 1,
            marker: Marker::Bullet,
            ..
        }
    ));
    assert!(matches!(
        blocks[6],
        Block::ListItem {
            marker: Marker::Number(1),
            ..
        }
    ));
    assert!(matches!(blocks[7], Block::Blockquote(_)));
    let Block::CodeFence { lang, lines } = &blocks[8] else {
        panic!("expected fence, got {:?}", blocks[8]);
    };
    assert_eq!(lang, "rust");
    assert_eq!(
        lines,
        &vec!["let x = 1; // verbatim, **not** parsed".to_string()]
    );
    assert!(matches!(blocks[9], Block::Rule));
    assert!(matches!(blocks[10], Block::Paragraph(_)));

    // Paragraph soft-join: two source lines, one logical line.
    let Block::Paragraph(p) = &blocks[1] else {
        unreachable!()
    };
    let plain = p.plain();
    assert!(plain.contains("code and a link. It continues"), "{plain}");
}

#[test]
fn inline_styles_compose_with_block_style() {
    use crate::base::Rgba;
    use crate::render::cell::Attrs;
    use crate::render::Style;
    // Distinct patches so composition is observable (with the DEFAULT
    // styles, heading and bold are the same BOLD patch and the spans
    // legitimately coalesce into one — that case is pinned separately).
    let mut s = styles();
    s.heading = Style::new().fg(Rgba::rgb(255, 200, 0)).attrs(Attrs::BOLD);
    s.bold = Style::new().attrs(Attrs::REVERSE);
    let blocks = parse("# head **deep**", &s);
    let Block::Heading { content, .. } = &blocks[0] else {
        unreachable!()
    };
    let bold_span = content.spans.iter().find(|sp| sp.text == "deep").unwrap();
    // Inline patch composed ONTO the heading style: keeps the heading
    // color+bold, gains the inline attrs.
    assert!(bold_span.style.add.contains(Attrs::BOLD | Attrs::REVERSE));
    assert_eq!(bold_span.style.fg, Some(Rgba::rgb(255, 200, 0)));

    // Default styles: identical ink coalesces (documented behavior).
    let blocks = parse("# head **deep**", &styles());
    let Block::Heading { content, .. } = &blocks[0] else {
        unreachable!()
    };
    assert_eq!(content.plain(), "head deep");
    assert_eq!(
        content.spans.len(),
        1,
        "same ink merges: {:?}",
        content.spans
    );
}

#[test]
fn inline_markers_parse_and_degrade() {
    let s = styles();
    let line = |src: &str| parse_inline(src, &s, s.base);

    let l = line("a **b** *c* `d` [e](f) tail");
    assert_eq!(l.plain(), "a b c d e tail");
    assert!(l.spans.iter().any(|sp| sp.link.as_deref() == Some("f")));

    // Unclosed markers are literal — nothing eats the text.
    assert_eq!(line("un **closed").plain(), "un **closed");
    assert_eq!(line("un *closed").plain(), "un *closed");
    assert_eq!(line("un `closed").plain(), "un `closed");
    assert_eq!(line("not [a link](").plain(), "not [a link](");
    assert_eq!(line("empty ****").plain(), "empty ****");

    // Escapes yield the literal character, unstyled.
    assert_eq!(line(r"\*not italic\*").plain(), "*not italic*");
    assert_eq!(line(r"back\\slash").plain(), r"back\slash");

    // Deliberate non-features stay literal.
    assert_eq!(line("_under_").plain(), "_under_");
    assert_eq!(line("**outer *inner* rest**").plain(), "outer *inner* rest");
}

#[test]
fn code_spans_are_verbatim() {
    let s = styles();
    let l = parse_inline("`**not bold** [x](y)`", &s, s.base);
    assert_eq!(l.plain(), "**not bold** [x](y)");
    assert_eq!(l.spans.len(), 1, "one code span: {:?}", l.spans);
}

#[test]
fn fence_without_close_recovers_at_eof() {
    let blocks = parse_default("```\nline1\nline2");
    let Block::CodeFence { lines, .. } = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(lines.len(), 2);
}

#[test]
fn blockquote_nesting_folds_and_hash7_is_text() {
    let blocks = parse_default(">> deep quote");
    assert!(matches!(&blocks[0], Block::Blockquote(l) if l.plain() == "deep quote"));
    // Seven hashes: not a heading (level cap 6) -> paragraph literal.
    let blocks = parse_default("####### seven");
    assert!(matches!(&blocks[0], Block::Paragraph(l) if l.plain() == "####### seven"));
    // "#heading" without the space is also literal (documented rule).
    let blocks = parse_default("#nospace");
    assert!(matches!(&blocks[0], Block::Paragraph(_)));
}

#[test]
fn to_rich_text_flattens_with_prefixes() {
    let s = styles();
    let blocks = parse("- item\n1. one\n> q\n---", &s);
    let rt = to_rich_text(&blocks, &s);
    let lines: Vec<String> = rt.lines.iter().map(|l| l.plain()).collect();
    assert_eq!(lines, vec!["• item", "1. one", "│ q", "───"]);
}

#[test]
fn with_ink_maps_theme_colors_and_keeps_base_fgless() {
    use crate::base::Rgba;
    let s = MdStyles::with_ink(
        Rgba::rgb(220, 220, 230), // code fg
        Rgba::rgb(40, 42, 60),    // code chip bg
        Rgba::rgb(90, 170, 255),  // link
    );
    assert_eq!(
        s.base.fg, None,
        "base stays fg-less: block recoloring works"
    );
    assert_eq!(s.code.fg, Some(Rgba::rgb(220, 220, 230)));
    assert_eq!(s.code.bg, Some(Rgba::rgb(40, 42, 60)));
    assert_eq!(s.link.fg, Some(Rgba::rgb(90, 170, 255)));
    // Emphasis inherits surrounding ink (attribute-only patches).
    assert_eq!(s.bold.fg, None);
    assert_eq!(s.heading.fg, None);
    // And the mapping actually flows through a parse.
    let blocks = parse("try `x` and [docs](u)", &s);
    let Block::Paragraph(l) = &blocks[0] else {
        unreachable!()
    };
    assert!(l
        .spans
        .iter()
        .any(|sp| sp.style.bg == Some(Rgba::rgb(40, 42, 60))));
    assert!(l
        .spans
        .iter()
        .any(|sp| sp.link.is_some() && sp.style.fg == Some(Rgba::rgb(90, 170, 255))));
}

#[test]
fn strikethrough_parses_and_degrades_like_the_other_markers() {
    use crate::render::Attrs;
    // Closed marker: STRIKE attribute merged onto the block style.
    let blocks = parse_default("keep ~~gone~~ end");
    let Block::Paragraph(l) = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(l.plain(), "keep gone end");
    let struck = l.spans.iter().find(|s| s.text == "gone").unwrap();
    assert!(struck.style.add.contains(Attrs::STRIKE), "{struck:?}");
    // Unclosed / empty / escaped: literal, exactly like * and `.
    for (src, want) in [
        ("a ~~open", "a ~~open"),
        ("~~~~", "~~~~"),
        ("\\~~not struck\\~~", "~~not struck~~"),
        ("lone ~ tilde", "lone ~ tilde"),
    ] {
        let blocks = parse_default(src);
        let Block::Paragraph(l) = &blocks[0] else {
            panic!("{src:?} -> {blocks:?}");
        };
        assert_eq!(l.plain(), want, "source {src:?}");
        assert!(
            l.spans.iter().all(|s| !s.style.add.contains(Attrs::STRIKE)),
            "no STRIKE on degraded forms: {src:?}"
        );
    }
}

#[test]
fn everything_parses_something() {
    // Hostile-ish inputs: no panics, no empty output for non-empty input.
    for src in [
        "***",
        "***a",
        "``",
        "[",
        "[]()",
        "\\",
        "> ",
        "-",
        "1.",
        "```",
        "＃全角 hash",
        "**世界`mix[ed](",
        "   \n\t\n",
    ] {
        let _ = parse_default(src); // must not panic
    }
    let blocks = parse_default("just text");
    assert_eq!(blocks.len(), 1);
}
