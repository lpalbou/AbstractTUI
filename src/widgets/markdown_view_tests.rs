//! MarkdownView draw-path tests: typeset chrome (headings, lists,
//! quotes, fences), scroll-offset/fold agreement, fence lexer routing
//! (diff + the wave-13 json/yaml data slice), and tiny-rect totality.
//! Split out of `markdown.rs` for the file-size discipline.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::test_util::{draw_into, row};

const DOC: &str = "# Title\n\nBody with `code` inline.\n\n- first\n- second\n\n> wisdom\n\n```\nfn main() {}\n```\n";

fn cell_of(row: &str, needle: &str) -> i32 {
    let byte = row.find(needle).unwrap();
    row[..byte].chars().count() as i32
}

#[test]
fn heading_list_quote_and_fence_chrome() {
    let t = default_theme().tokens;
    let c = draw_into(MarkdownView::new(DOC).element(&t), Size::new(28, 14));
    // Level-1 heading in accent + underline rule beneath.
    let title_y = 0;
    assert!(row(&c, title_y).starts_with("Title"));
    assert_eq!(c.cell(Point::new(0, title_y)).unwrap().1, t.accent);
    assert!(row(&c, title_y + 1).starts_with('─'));
    assert_eq!(c.cell(Point::new(0, title_y + 1)).unwrap().1, t.border);

    // Inline code chip ground.
    let body_y = (0..14).find(|y| row(&c, *y).contains("code")).unwrap();
    let cx = cell_of(&row(&c, body_y), "code");
    assert_eq!(c.cell(Point::new(cx, body_y)).unwrap().2, t.surface_raised);

    // List marker ink.
    let li_y = (0..14).find(|y| row(&c, *y).contains("• first")).unwrap();
    let mx = cell_of(&row(&c, li_y), "•");
    assert_eq!(c.cell(Point::new(mx, li_y)).unwrap().1, t.accent_alt);

    // Blockquote bar + muted prose.
    let q_y = (0..14).find(|y| row(&c, *y).contains("wisdom")).unwrap();
    assert_eq!(c.cell(Point::new(0, q_y)).unwrap().0, '▎');
    let wx = cell_of(&row(&c, q_y), "wisdom");
    assert_eq!(c.cell(Point::new(wx, q_y)).unwrap().1, t.text_muted);

    // Code fence: raised ground + keyword ink.
    let f_y = (0..14).find(|y| row(&c, *y).contains("fn main")).unwrap();
    let fx = cell_of(&row(&c, f_y), "fn");
    let (_, fg, bg) = c.cell(Point::new(fx, f_y)).unwrap();
    assert_eq!(fg, t.syntax_keyword);
    assert_eq!(bg, t.surface_raised);
}

#[test]
fn outline_rows_and_scroll_share_the_fold() {
    let t = default_theme().tokens;
    assert_eq!(
        MarkdownView::outline(DOC, &t),
        vec![(1, "Title".to_string())]
    );
    let total = MarkdownView::rows(DOC, &t, 28);
    assert!(total >= 10, "typeset rows: {total}");
    // Scrolling by one hides the title row.
    let c = draw_into(
        MarkdownView::new(DOC).scroll_offset(1).element(&t),
        Size::new(28, 6),
    );
    assert!(!row(&c, 0).contains("Title"));
}

#[test]
fn diff_fences_tint_added_removed_and_plain_fences_stay_clike() {
    let t = default_theme().tokens;
    let doc = "```diff\n-old line\n+new line\n```\n\n```\nfn main() {}\n```\n";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(28, 10));
    // Diff fence: removed line in error ink, added in ok, on the
    // fence's raised ground.
    let minus_y = (0..10).find(|y| row(&c, *y).contains("-old")).unwrap();
    let mx = cell_of(&row(&c, minus_y), "-old");
    let (_, fg, bg) = c.cell(Point::new(mx, minus_y)).unwrap();
    assert_eq!(fg, t.error);
    assert_eq!(bg, t.surface_raised);
    let plus_y = (0..10).find(|y| row(&c, *y).contains("+new")).unwrap();
    let px = cell_of(&row(&c, plus_y), "+new");
    assert_eq!(c.cell(Point::new(px, plus_y)).unwrap().1, t.ok);
    // The unlabeled fence still renders the C-like keyword ink.
    let fn_y = (0..10).find(|y| row(&c, *y).contains("fn main")).unwrap();
    let fx = cell_of(&row(&c, fn_y), "fn");
    assert_eq!(c.cell(Point::new(fx, fn_y)).unwrap().1, t.syntax_keyword);
}

/// Wave 13: ```json and ```yaml fences tint through the data mapping —
/// keys in `syntax_func`, string values in `syntax_string`, literals
/// in `syntax_keyword`, comments in `syntax_comment` — on the fence's
/// raised ground, through the REAL MarkdownView draw path.
#[test]
fn json_and_yaml_fences_tint_through_the_data_mapping() {
    let t = default_theme().tokens;
    let doc = "```json\n{\"name\": \"Ada\", \"ok\": true}\n```\n\n```yaml\nregion: eu-west-1 # main\n```\n";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(40, 8));
    let j_y = (0..8).find(|y| row(&c, *y).contains("name")).unwrap();
    let jrow = row(&c, j_y);
    let kx = cell_of(&jrow, "\"name\"");
    let (_, fg, bg) = c.cell(Point::new(kx, j_y)).unwrap();
    assert_eq!(fg, t.syntax_func, "json key ink");
    assert_eq!(bg, t.surface_raised, "fence ground survives");
    let vx = cell_of(&jrow, "\"Ada\"");
    assert_eq!(c.cell(Point::new(vx, j_y)).unwrap().1, t.syntax_string);
    let lx = cell_of(&jrow, "true");
    assert_eq!(c.cell(Point::new(lx, j_y)).unwrap().1, t.syntax_keyword);

    let y_y = (0..8).find(|y| row(&c, *y).contains("region")).unwrap();
    let yrow = row(&c, y_y);
    let yx = cell_of(&yrow, "region");
    assert_eq!(
        c.cell(Point::new(yx, y_y)).unwrap().1,
        t.syntax_func,
        "yaml key ink"
    );
    let cx = cell_of(&yrow, "# main");
    assert_eq!(c.cell(Point::new(cx, y_y)).unwrap().1, t.syntax_comment);
    // The bare scalar value stays body ink (prose is prose).
    let sx = cell_of(&yrow, "eu-west-1");
    assert_eq!(c.cell(Point::new(sx, y_y)).unwrap().1, t.text);
}

/// Wave 13 (mdpad port): H3+ headings carry a faint hash prefix so
/// depth stays readable where the heading inks stop differentiating
/// (L3..L6 all render body+BOLD). L1/L2 stay clean.
#[test]
fn h3_and_deeper_carry_a_faint_level_prefix() {
    let t = default_theme().tokens;
    let doc = "## Two\n\n### Three\n\n#### Four";
    let c = draw_into(MarkdownView::new(doc).element(&t), Size::new(24, 8));
    let two_y = (0..8).find(|y| row(&c, *y).contains("Two")).unwrap();
    assert!(row(&c, two_y).starts_with("Two"), "H2 has no prefix");
    let three_y = (0..8).find(|y| row(&c, *y).contains("Three")).unwrap();
    assert!(
        row(&c, three_y).starts_with("### Three"),
        "{:?}",
        row(&c, three_y)
    );
    assert_eq!(
        c.cell(Point::new(0, three_y)).unwrap().1,
        t.text_faint,
        "prefix rides the faint ink"
    );
    let tx = cell_of(&row(&c, three_y), "Three");
    assert_eq!(
        c.cell(Point::new(tx, three_y)).unwrap().1,
        t.text,
        "heading text keeps body ink + bold"
    );
    let four_y = (0..8).find(|y| row(&c, *y).contains("Four")).unwrap();
    assert!(row(&c, four_y).starts_with("#### Four"));
}

#[test]
fn wrapping_and_tiny_rects_never_panic() {
    let t = default_theme().tokens;
    let long = "paragraph with quite a few words that must wrap around";
    let c = draw_into(MarkdownView::new(long).element(&t), Size::new(12, 8));
    assert!(row(&c, 0).trim_end().len() <= 12);
    for size in [Size::new(0, 0), Size::new(2, 1), Size::new(5, 2)] {
        let _ = draw_into(MarkdownView::new(DOC).element(&t), size);
    }
}

// ---------------------------------------------------------------------------
// FenceBlock: a fenced block claimed by a sibling crate renders INSIDE
// the document — one scroll surface, one outline, one search index.
// ---------------------------------------------------------------------------

/// A stand-in for a diagram renderer: claims ```box and draws a frame
/// with the source's first line inside it.
struct BoxFence;

impl crate::widgets::FenceBlock for BoxFence {
    fn measure(&self, lang: &str, _source: &str, _width: i32) -> Option<i32> {
        (lang == "box").then_some(3)
    }

    fn draw_row(
        &self,
        _lang: &str,
        source: &str,
        row: i32,
        at: crate::base::Point,
        width: i32,
        t: &crate::theme::TokenSet,
        canvas: &mut dyn crate::ui::StyledCanvas,
    ) {
        let w = width.max(2) as usize;
        let text = match row {
            0 => format!("┌{}┐", "─".repeat(w - 2)),
            1 => {
                let body: String = source
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(w - 2)
                    .collect();
                format!("│{body:<0$}│", w - 2)
            }
            _ => format!("└{}┘", "─".repeat(w - 2)),
        };
        canvas.print(at, &text, t.text, Rgba::TRANSPARENT);
    }
}

#[test]
fn a_claimed_fence_renders_in_place_of_its_code() {
    let src = "# Title\n\n```box\ndiagram here\n```\n\nafter";
    let canvas = draw_into(
        MarkdownView::new(src)
            .fence_block(std::rc::Rc::new(BoxFence))
            .element(&default_theme().tokens),
        Size::new(24, 12),
    );
    let screen: Vec<String> = (0..12).map(|y| canvas.row_text(y)).collect();
    let joined = screen.join("\n");
    assert!(joined.contains("┌"), "the block drew its frame:\n{joined}");
    assert!(
        joined.contains("diagram here"),
        "the block drew its content:\n{joined}"
    );
    assert!(
        !joined.contains("```"),
        "the fence markers never reach the screen:\n{joined}"
    );
    // The document continues after it, in the same surface.
    assert!(joined.contains("after"), "{joined}");
    assert!(joined.contains("Title"), "{joined}");
}

#[test]
fn a_declined_fence_still_renders_as_code() {
    let src = "```rust\nlet x = 1;\n```";
    let canvas = draw_into(
        MarkdownView::new(src)
            .fence_block(std::rc::Rc::new(BoxFence))
            .element(&default_theme().tokens),
        Size::new(30, 6),
    );
    let joined: String = (0..6)
        .map(|y| canvas.row_text(y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("let x = 1;"),
        "code is untouched:\n{joined}"
    );
    assert!(!joined.contains("┌"), "the claimant declined:\n{joined}");
}

/// Claimed rows are ROWS: they scroll, and the document's row count
/// includes them, so `Scroll` and the outline stay correct.
#[test]
fn claimed_rows_participate_in_scrolling_and_measurement() {
    let src = "```box\nd\n```\n\ntail";
    let t = crate::theme::default_theme().tokens;
    let plain = MarkdownView::rows(src, &t, 24);
    // 3 rows for the block instead of the fence's own code rows.
    let with_block = super::doc::layout_doc_with(src, &t, 24, Some(std::rc::Rc::new(BoxFence)))
        .rows
        .len();
    assert_ne!(plain, with_block, "the block changes the row count");
    let canvas = draw_into(
        MarkdownView::new(src)
            .fence_block(std::rc::Rc::new(BoxFence))
            .scroll_offset(1)
            .element(&default_theme().tokens),
        Size::new(24, 3),
    );
    assert!(
        canvas.row_text(0).contains("│"),
        "scrolled INTO the block: {:?}",
        canvas.row_text(0)
    );
}
