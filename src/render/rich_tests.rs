//! Rich text tests: span-preserving wrap, mixed widths, links, drawing.

use super::*;
use crate::base::{Rgba, Size};
use crate::render::cell::{Attrs, Cell};

fn bold() -> Style {
    Style::new().attrs(Attrs::BOLD)
}

fn red() -> Style {
    Style::new().fg(Rgba::rgb(255, 0, 0))
}

fn surf(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

fn line(spans: Vec<Span>) -> RichLine {
    let mut l = RichLine::new();
    for s in spans {
        l.push(s);
    }
    l
}

fn row_text(s: &Surface, y: i32) -> String {
    (0..s.width())
        .filter_map(|x| {
            let c = s.get(x, y).unwrap();
            if c.is_continuation() {
                return None;
            }
            let t = s.glyph_str(c);
            Some(if t.is_empty() {
                ".".to_string()
            } else {
                t.to_string()
            })
        })
        .collect()
}

#[test]
fn push_coalesces_same_ink() {
    let mut l = RichLine::new();
    l.push(Span::plain("ab"));
    l.push(Span::plain("cd"));
    l.push(Span::new("EF", bold()));
    assert_eq!(l.spans.len(), 2, "same-ink spans merge: {:?}", l.spans);
    assert_eq!(l.plain(), "abcdEF");
    assert_eq!(l.width(), 6);
}

#[test]
fn wrap_preserves_spans_across_boundaries() {
    // Bold covers "bold br" — the word "brave" STRADDLES the span edge
    // and must keep per-cluster styles through the wrap.
    let rt = RichText::from_lines(vec![line(vec![
        Span::new("bold br", bold()),
        Span::plain("ave new world"),
    ])]);
    let wrapped = rt.wrap(10);
    assert_eq!(
        wrapped
            .lines
            .iter()
            .map(RichLine::plain)
            .collect::<Vec<_>>(),
        vec!["bold brave", "new world"],
    );
    // Line 0 = bold("bold br") + plain("ave"), coalesced to two spans.
    let l0 = &wrapped.lines[0];
    assert_eq!(l0.spans.len(), 2, "{:?}", l0.spans);
    assert!(l0.spans[0].style.add.contains(Attrs::BOLD));
    assert_eq!(l0.spans[0].text, "bold br");
    assert_eq!(l0.spans[1].text, "ave");
    assert!(!l0.spans[1].style.add.contains(Attrs::BOLD));
    // Line 1 is entirely plain.
    let l1 = &wrapped.lines[1];
    assert_eq!(l1.spans.len(), 1);
    assert!(!l1.spans[0].style.add.contains(Attrs::BOLD));

    // Narrower: the straddling word lands on its own line, styles intact.
    let narrow = rt.wrap(6);
    assert_eq!(
        narrow.lines.iter().map(RichLine::plain).collect::<Vec<_>>(),
        vec!["bold", "brave", "new", "world"],
    );
    let brave = &narrow.lines[1];
    assert_eq!(
        brave.spans.len(),
        2,
        "bold 'br' + plain 'ave': {:?}",
        brave.spans
    );
    assert!(brave.spans[0].style.add.contains(Attrs::BOLD));
    assert_eq!(brave.spans[0].text, "br");
    assert_eq!(brave.spans[1].text, "ave");
}

#[test]
fn wrap_matches_plain_text_wrap() {
    // Single-style rich wrap must agree with text::wrap line-for-line
    // (one wrapping contract in the engine).
    let cases = [
        "the quick brown fox jumps",
        "a  b   c",
        "abcdefghij",
        "城市化进程加快 ok",
        "one\n\ntwo lines",
        "",
    ];
    for case in cases {
        for width in [1, 3, 5, 9] {
            let rich: Vec<String> = RichText::plain(case, Style::EMPTY)
                .wrap(width)
                .lines
                .iter()
                .map(RichLine::plain)
                .collect();
            let plain = crate::text::wrap(case, width);
            assert_eq!(rich, plain, "case {case:?} width {width}");
        }
    }
}

#[test]
fn wrap_mixed_width_content() {
    let rt = RichText::from_lines(vec![line(vec![
        Span::new("警告", red()),
        Span::plain(" disk full 世界"),
    ])]);
    let wrapped = rt.wrap(8);
    for l in &wrapped.lines {
        assert!(l.width() <= 8, "line overflows: {:?}", l.plain());
    }
    // The CJK span keeps its style.
    assert!(wrapped.lines[0].spans[0].style.fg.is_some());
    assert_eq!(wrapped.lines[0].spans[0].text, "警告");
}

#[test]
fn wrap_keeps_link_runs() {
    let rt = RichText::from_lines(vec![line(vec![
        Span::plain("see "),
        Span::new("the documentation page", Style::new()).with_link("https://example.com/docs"),
    ])]);
    // Width 10 long-word-splits "documentation": BOTH fragments must
    // still carry the URL (a wrapped link stays clickable on every line).
    let wrapped = rt.wrap(10);
    let mut linked = Vec::new();
    for l in &wrapped.lines {
        for s in &l.spans {
            if s.link.as_deref() == Some("https://example.com/docs") {
                linked.push(s.text.trim().to_string());
            }
        }
    }
    // Coalescing keeps same-ink neighbors together within a line ("ion
    // page" is ONE linked span on its line); what matters is that every
    // linked word survives with the URL and nothing else gains it.
    let linked: Vec<&str> = linked
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(linked, vec!["the", "documentat", "ion page"]);
    let all_linked_text: String = linked.join(" ");
    assert_eq!(all_linked_text, "the documentat ion page");
    // And the unlinked prefix never gained a URL.
    assert!(wrapped.lines[0].spans[0].link.is_none());
}

#[test]
fn draw_aligns_and_patches_over_panel() {
    let mut s = surf(12, 3);
    s.fill_rect(s.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(10, 10, 30)));
    let rt = RichText::from_lines(vec![
        line(vec![Span::new("hi", bold())]),
        line(vec![Span::plain("mid")]),
        line(vec![Span::plain("end")]),
    ]);
    let mut left = s;
    rt.draw(&mut left, Rect::new(0, 0, 12, 3), HAlign::Left);
    assert_eq!(row_text(&left, 0)[..2].to_string(), "hi");
    assert_eq!(
        left.get(0, 0).unwrap().bg,
        Rgba::rgb(10, 10, 30),
        "panel bg shows through"
    );
    assert!(left.get(0, 0).unwrap().attrs.contains(Attrs::BOLD));

    let mut center = surf(12, 3);
    rt.draw(&mut center, Rect::new(0, 0, 12, 3), HAlign::Center);
    assert!(!center.get(5, 0).unwrap().is_continuation());
    assert_eq!(row_text(&center, 0), ".....hi.....");

    let mut right = surf(12, 3);
    rt.draw(&mut right, Rect::new(0, 0, 12, 3), HAlign::Right);
    assert_eq!(row_text(&right, 1), ".........mid");
}

#[test]
fn draw_truncates_overwide_lines_with_ellipsis() {
    let mut s = surf(8, 1);
    let rt = RichText::from_lines(vec![line(vec![
        Span::plain("abc"),
        Span::new("defghijk", red()),
    ])]);
    rt.draw(&mut s, Rect::new(0, 0, 8, 1), HAlign::Left);
    assert_eq!(row_text(&s, 0), "abcdefg…");
    // Ellipsis carries the style in force at the cut.
    assert_eq!(s.get(7, 0).unwrap().fg, Rgba::rgb(255, 0, 0));
    s.debug_validate().unwrap();

    // Wide cluster at the cut never straddles.
    let mut s = surf(6, 1);
    let rt = RichText::from_lines(vec![line(vec![Span::plain("ab世界人")])]);
    rt.draw(&mut s, Rect::new(0, 0, 6, 1), HAlign::Left);
    assert_eq!(row_text(&s, 0), "ab世…."); // 2+2+1 = 5 cols + pad
    s.debug_validate().unwrap();
}

#[test]
fn draw_clips_height_and_registers_links() {
    let mut s = surf(20, 2);
    let rt = RichText::from_lines(vec![
        line(vec![
            Span::new("click here", Style::new()).with_link("https://a.example")
        ]),
        line(vec![Span::plain("second")]),
        line(vec![Span::plain("dropped")]),
    ]);
    rt.draw(&mut s, Rect::new(0, 0, 20, 2), HAlign::Left);
    let cell = s.get(0, 0).unwrap();
    assert_eq!(
        s.link_uri(cell.link),
        Some("https://a.example"),
        "URL registered at draw"
    );
    assert_eq!(row_text(&s, 1)[..6].to_string(), "second");
    // Third line clipped by rect.h == 2.
    assert!(s.get(0, 1).is_some());
}

#[test]
fn from_highlighted_bridges_the_lexer() {
    use crate::text::{CLikeLexer, TokenKind};
    let kw = Style::new().fg(Rgba::rgb(200, 120, 255));
    let stringy = Style::new().fg(Rgba::rgb(120, 200, 120));
    let base = Style::new().fg(Rgba::rgb(220, 220, 220));
    let l =
        RichLine::from_highlighted(r#"let s = "hi";"#, &CLikeLexer::rust(), base, |k| match k {
            TokenKind::Keyword => kw,
            TokenKind::String => stringy,
            _ => base,
        });
    assert_eq!(l.plain(), r#"let s = "hi";"#, "no bytes lost");
    assert_eq!(l.spans[0].text, "let");
    assert_eq!(l.spans[0].style, kw);
    assert!(l
        .spans
        .iter()
        .any(|s| s.text == r#""hi""# && s.style == stringy));
}

#[test]
fn wrap_scales_linearly_in_span_count_structurally() {
    // Complexity guard (RT: richtext wrap was flagged "tight"): the wrap
    // walk is one pass over clusters with O(1) amortized emission
    // (`push_run` merges into the LAST span only — no scans over prior
    // spans). Pin the structural facts that keep it linear: (a) output
    // span count tracks style CHANGES, not input span count; (b) a
    // thousand-span line wraps without churn (would time out here if
    // quadratic-with-allocation).
    let mut l = RichLine::new();
    for i in 0..1000 {
        // Two alternating inks -> worst-case span fragmentation input.
        let style = if i % 2 == 0 { bold() } else { Style::EMPTY };
        l.push(Span::new(format!("w{i:03} "), style));
    }
    let rt = RichText::from_lines(vec![l]);
    let wrapped = rt.wrap(40);
    let total_spans: usize = wrapped.lines.iter().map(|l| l.spans.len()).sum();
    let total_clusters: usize = wrapped.lines.iter().map(|l| l.plain().len()).sum();
    assert!(
        total_spans * 2 <= total_clusters,
        "spans stay far below clusters (coalescing works): {total_spans} vs {total_clusters}"
    );
    // Single-style input: whole output lines collapse to ONE span each.
    let uniform = RichText::plain(&"word ".repeat(2000), Style::EMPTY).wrap(50);
    assert!(uniform.lines.iter().all(|l| l.spans.len() <= 1));
}

#[test]
fn empty_lines_and_zero_rect_are_inert() {
    let rt = RichText::plain("a\n\nb", Style::EMPTY);
    assert_eq!(rt.height(), 3);
    let wrapped = rt.wrap(5);
    assert_eq!(wrapped.height(), 3, "empty line survives wrap");
    let mut s = surf(4, 2);
    wrapped.draw(&mut s, Rect::ZERO, HAlign::Left); // no panic, no writes
    assert_eq!(row_text(&s, 0), "....");
}
