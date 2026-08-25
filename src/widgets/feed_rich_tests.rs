//! Rich-block (backlog 0102) + selection-by-key (0100 item 6) tests —
//! sibling of `feed_tests.rs` for the file-size discipline. Reuses the
//! parent test module's mount/settle helpers.

use std::cell::RefCell;
use std::rc::Rc;

use super::tests::{mount_feed, settle};
use super::*;
use crate::base::{Point, Size};
use crate::widgets::itest_util::mount_widget;

// ---------------------------------------------------------------------------
// Rich blocks (backlog 0102) + selection by key (0100 item 6)
// ---------------------------------------------------------------------------

use crate::render::rich::{RichLine, RichText, Span};
use crate::render::{Attrs, Style};
use crate::theme::default_theme;

/// Three-ink fixture: explicit accent + bold-only (fg-less) + plain,
/// long enough to wrap at narrow widths.
fn three_ink_doc(t: &crate::theme::TokenSet) -> RichText {
    RichText::from_lines(vec![
        RichLine::from_spans(vec![
            Span::new("badge", Style::new().fg(t.accent)),
            Span::new(" strong", Style::new().attrs(Attrs::BOLD)),
            Span::plain(" and a plain tail that wraps"),
        ]),
        RichLine::from_spans(vec![Span::new("second line", Style::new().fg(t.error))]),
    ])
}

/// The 0102 parity pin: a Rich feed block renders the EXACT cells
/// `RichTextView` renders for the same `RichText` at the same width —
/// one span walk, four faces (chars, inks, grounds, attrs).
#[test]
fn rich_block_matches_richtextview_pixels() {
    let t = default_theme().tokens;
    let size = Size::new(18, 6);
    let reference = crate::widgets::test_util::draw_into(
        crate::widgets::RichTextView::new(three_ink_doc(&t)).element(&t),
        size,
    );
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("r", FeedItem::rich(three_ink_doc(&t)));
    let canvas = settle(&mut tree, size);
    for y in 0..size.h {
        for x in 0..size.w {
            let p = Point::new(x, y);
            assert_eq!(
                canvas.cell(p),
                reference.cell(p),
                "cell divergence at ({x},{y}):\nfeed: {:?}\nview: {:?}",
                canvas.row_text(y),
                reference.row_text(y)
            );
            assert_eq!(
                canvas.attrs_at(p),
                reference.attrs_at(p),
                "attrs at ({x},{y})"
            );
        }
    }
    root.dispose();
}

/// The consumer shapes the item was filed for: a severity-tinted log
/// line (badge ink + muted timestamp + body) and a chat header (name
/// ink + muted time) over a markdown body — no `FeedBlock::Custom`.
#[test]
fn rich_item_consumers_severity_log_and_chat_header() {
    let t = default_theme().tokens;
    let size = Size::new(40, 12);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push(
        "log",
        FeedItem::rich_lines(vec![RichLine::from_spans(vec![
            Span::new("ERROR ", Style::new().fg(t.error)),
            Span::new("12:04 ", Style::new().fg(t.text_muted)),
            Span::plain("disk full on shard 2"),
        ])]),
    );
    feed.push(
        "msg",
        FeedItem::rich_lines(vec![RichLine::from_spans(vec![
            Span::new("ariadne", Style::new().fg(t.accent)),
            Span::new(" · 12:05", Style::new().fg(t.text_muted)),
        ])])
        .block(FeedBlock::Markdown("hello **there**".into())),
    );
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    let log_y = dump.iter().position(|r| r.contains("ERROR")).unwrap() as i32;
    assert_eq!(canvas.cell(Point::new(0, log_y)).unwrap().1, t.error);
    assert_eq!(canvas.cell(Point::new(6, log_y)).unwrap().1, t.text_muted);
    let body_x = dump[log_y as usize].find("disk").unwrap() as i32;
    assert_eq!(
        canvas.cell(Point::new(body_x, log_y)).unwrap().1,
        t.text,
        "fg-less body span inherits the item ink"
    );
    let hdr_y = dump.iter().position(|r| r.contains("ariadne")).unwrap() as i32;
    assert_eq!(canvas.cell(Point::new(0, hdr_y)).unwrap().1, t.accent);
    let time_x = dump[hdr_y as usize].find('·').unwrap() as i32;
    assert_eq!(
        canvas.cell(Point::new(time_x, hdr_y)).unwrap().1,
        t.text_muted
    );
    // The markdown body rides the SAME item, one row below the header.
    let body_y = dump.iter().position(|r| r.contains("there")).unwrap() as i32;
    assert!(body_y > hdr_y);
    root.dispose();
}

/// Wrap honesty under resize: rich rows re-typeset at the new width
/// and the reactive extent follows (the windowing trusts heights).
#[test]
fn rich_blocks_rewrap_and_resync_extent_on_resize() {
    let size = Size::new(36, 8);
    let t = default_theme().tokens;
    let (root, mut tree, feed) = mount_feed(size);
    feed.push(
        "r",
        FeedItem::rich_lines(vec![RichLine::from_spans(vec![
            Span::new("head ", Style::new().fg(t.accent)),
            Span::plain("a long plain tail that will wrap differently at narrow widths"),
        ])]),
    );
    let _ = settle(&mut tree, size);
    let wide = feed.total_rows().get_untracked();
    assert!(wide >= 2, "wrapped at 36: {wide}");
    tree.set_viewport(Size::new(18, 8));
    let canvas = settle(&mut tree, Size::new(18, 8));
    let narrow = feed.total_rows().get_untracked();
    assert!(
        narrow > wide,
        "narrower pane wraps more ({wide} -> {narrow})"
    );
    assert!(canvas.row_text(0).contains("head"));
    root.dispose();
}

/// The theme patch rule across token sets: fg-less spans land in each
/// theme's `text` ink; explicit inks (resolved Rgba) render verbatim.
#[test]
fn rich_span_patch_rule_binds_item_ink_per_theme() {
    let themes = crate::theme::themes();
    let dark = default_theme().tokens;
    let light = themes
        .iter()
        .find(|th| !th.is_dark())
        .expect("a light theme exists")
        .tokens;
    let explicit = dark.error; // resolved Rgba, deliberately from theme A
    for t in [dark, light] {
        let size = Size::new(24, 4);
        let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
        let h = holder.clone();
        let (root, mut tree) = mount_widget(size, move |cx| {
            let feed = FeedState::new(cx);
            *h.borrow_mut() = Some(feed.clone());
            crate::ui::Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .child(Feed::new(&feed).element(cx, &t).build())
                .build()
        });
        let feed = holder.borrow().clone().unwrap();
        feed.push(
            "r",
            FeedItem::rich_lines(vec![RichLine::from_spans(vec![
                Span::new("E ", Style::new().fg(explicit)),
                Span::plain("plain"),
            ])]),
        );
        let canvas = settle(&mut tree, size);
        assert_eq!(canvas.cell(Point::new(0, 0)).unwrap().1, explicit);
        let px = canvas.row_text(0).find("plain").unwrap() as i32;
        assert_eq!(
            canvas.cell(Point::new(px, 0)).unwrap().1,
            t.text,
            "fg-less span wears THIS theme's item ink"
        );
        root.dispose();
    }
}

/// Rich blocks in fixed-box mode: the feed clips at the box and shows
/// its head — same windowing as every other block kind.
#[test]
fn rich_blocks_render_in_fixed_box_mode() {
    let size = Size::new(20, 3);
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        *h.borrow_mut() = Some(feed.clone());
        Feed::new(&feed)
            .layout(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .view(cx)
    });
    let feed = holder.borrow().clone().unwrap();
    for i in 0..6 {
        feed.push(
            format!("r{i}"),
            FeedItem::rich_lines(vec![RichLine::from_spans(vec![Span::plain(format!(
                "rich row {i}"
            ))])]),
        );
    }
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("rich row 0"), "head visible");
    assert!(!canvas.row_text(2).contains("rich row 5"), "tail clipped");
    root.dispose();
}

/// Measured (report evidence, perf-suite convention): typeset cost of
/// 1k three-ink rich items pushed at a known width. Debug builds print
/// without asserting; release asserts a generous ceiling.
///
/// ```sh
/// cargo test --release --lib perf_rich_block_typeset -- --ignored --nocapture
/// ```
#[test]
#[ignore = "perf: measured evidence, run explicitly in release (--ignored)"]
fn perf_rich_block_typeset_1k_items() {
    let t = default_theme().tokens;
    let size = Size::new(40, 12);
    let m = crate::testing::time_median("rich typeset 1k items", 1, 5, 1, |_| {
        let (root, mut tree, feed) = mount_feed(size);
        let _ = settle(&mut tree, size); // width known: pushes typeset inline
        for i in 0..1_000 {
            feed.push(
                format!("r{i}"),
                FeedItem::rich_lines(vec![RichLine::from_spans(vec![
                    Span::new("ERROR ", Style::new().fg(t.error)),
                    Span::new("12:04 ", Style::new().fg(t.text_muted)),
                    Span::plain(format!("severity-tinted body line number {i}")),
                ])]),
            );
        }
        root.dispose();
    });
    eprintln!("{}", m.report());
    if !cfg!(debug_assertions) {
        m.assert_under(std::time::Duration::from_millis(250));
    }
}

/// Selection by key (0100 item 6): the selected item's band grounds in
/// `selection_bg`, item inks stay, `row_of` answers the scroll target,
/// and clearing/unknown keys highlight nothing.
#[test]
fn selection_by_key_highlights_band_and_row_of_targets() {
    type SelHolder = Rc<RefCell<Option<(FeedState, crate::reactive::Signal<Option<String>>)>>>;
    let t = default_theme().tokens;
    let size = Size::new(24, 10);
    let holder: SelHolder = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        let sel = cx.signal(None::<String>);
        *h.borrow_mut() = Some((feed.clone(), sel));
        crate::ui::Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).selected_key(sel).view(cx))
            .build()
    });
    let (feed, sel) = holder.borrow().clone().unwrap();
    feed.push("a", FeedItem::text("alpha"));
    feed.push("b", FeedItem::text("beta"));
    feed.push("c", FeedItem::text("gamma"));
    let canvas = settle(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, 0)).unwrap().2,
        crate::base::Rgba::TRANSPARENT
    );

    sel.set(Some("b".to_string()));
    let canvas = settle(&mut tree, size);
    let b_row = feed.row_of("b").expect("known key");
    assert_eq!(b_row, 2, "one row + one gap before item b");
    // The whole band grounds in selection_bg; text ink survives.
    let (ch, fg, bg) = canvas.cell(Point::new(0, b_row)).unwrap();
    assert_eq!(bg, t.selection_bg, "selected band grounded");
    assert_eq!((ch, fg), ('b', t.text), "item ink kept over the tint");
    assert_eq!(
        canvas.cell(Point::new(12, b_row)).unwrap().2,
        t.selection_bg
    );
    // Neighbors stay untinted.
    assert_eq!(
        canvas.cell(Point::new(0, 0)).unwrap().2,
        crate::base::Rgba::TRANSPARENT
    );
    assert_eq!(
        canvas
            .cell(Point::new(0, feed.row_of("c").unwrap()))
            .unwrap()
            .2,
        crate::base::Rgba::TRANSPARENT
    );

    // Unknown key and cleared selection: no highlight, no panic.
    sel.set(Some("zz".to_string()));
    let canvas = settle(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, b_row)).unwrap().2,
        crate::base::Rgba::TRANSPARENT
    );
    sel.set(None);
    let canvas = settle(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, b_row)).unwrap().2,
        crate::base::Rgba::TRANSPARENT
    );
    assert_eq!(feed.row_of("zz"), None);
    root.dispose();
}

// ---------------------------------------------------------------------------
// Capped preview blocks (first-app/0283): width-aware max_rows +
// honest overflow marker on Text/Rich blocks.
// ---------------------------------------------------------------------------

/// Six logical lines, 18 columns each: one wrapped row per line at
/// width 40, two at width 12 — so the SAME cap hides a different K.
fn six_line_body() -> String {
    (0..6)
        .map(|_| "one two three four")
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn max_rows_caps_post_wrap_and_k_changes_with_width() {
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("body", FeedItem::text(six_line_body()).max_rows(3));
    let canvas = settle(&mut tree, size);
    // Width 40: 6 wrapped rows -> 2 shown + marker; 4 hidden.
    assert!(canvas.row_text(0).contains("one two three four"));
    assert!(canvas.row_text(1).contains("one two three four"));
    assert!(
        canvas.row_text(2).contains("(+4 more lines)"),
        "marker names the hidden count: {:?}",
        canvas.row_text(2)
    );
    assert_eq!(
        feed.total_rows().get_untracked(),
        3,
        "capped extent counts the marker row"
    );

    // Width 12: every line wraps to 2 rows (12 total) -> K = 10. The
    // cap must recompute where the wrap happens.
    let narrow = Size::new(12, 8);
    tree.set_viewport(narrow);
    let canvas = settle(&mut tree, narrow);
    assert!(
        canvas.row_text(2).contains("(+10 more"),
        "K recomputed post-wrap at the new width: {:?}",
        canvas.row_text(2)
    );
    assert_eq!(feed.total_rows().get_untracked(), 3);
    root.dispose();
}

#[test]
fn overflow_marker_wears_muted_ink_and_custom_wording() {
    let t = default_theme().tokens;
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("plain", FeedItem::text(six_line_body()).max_rows(2));
    feed.push(
        "worded",
        FeedItem::text(six_line_body())
            .max_rows(2)
            .overflow_marker(|k| format!("+{k} hidden — see the ledger")),
    );
    let canvas = settle(&mut tree, size);
    // Default wording, theme-muted ink on the marker row.
    assert!(canvas.row_text(1).contains("… (+5 more lines)"));
    let (ch, fg, _) = canvas.cell(Point::new(0, 1)).unwrap();
    assert_eq!(ch, '…');
    assert_eq!(fg, t.text_muted, "marker ink is text_muted");
    // Custom wording renders verbatim (item 2 starts after the gap).
    let worded_row = feed.row_of("worded").unwrap() + 1;
    assert!(
        canvas
            .row_text(worded_row)
            .contains("+5 hidden — see the ledger"),
        "custom marker wording: {:?}",
        canvas.row_text(worded_row)
    );
    root.dispose();
}

#[test]
fn fitting_content_and_uncapped_blocks_render_unchanged() {
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    // Fits exactly: 2 wrapped rows under a 2-row cap — no marker.
    feed.push("fits", FeedItem::text("alpha\nbeta").max_rows(2));
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("alpha"));
    assert!(canvas.row_text(1).contains("beta"));
    assert!(
        !(0..size.h).any(|y| canvas.row_text(y).contains("more lines")),
        "no marker when content fits"
    );
    assert_eq!(feed.total_rows().get_untracked(), 2);
    root.dispose();
}

#[test]
fn rich_blocks_cap_with_marker_and_keep_shown_inks() {
    let t = default_theme().tokens;
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    let lines: Vec<RichLine> = (0..5)
        .map(|i| {
            RichLine::from_spans(vec![
                Span::new("ERR ", Style::new().fg(t.error)),
                Span::plain(format!("rich body line {i}")),
            ])
        })
        .collect();
    feed.push("rich", FeedItem::rich_lines(lines).max_rows(3));
    let canvas = settle(&mut tree, size);
    // Shown rows keep their span inks; the marker follows.
    assert_eq!(canvas.cell(Point::new(0, 0)).unwrap().1, t.error);
    assert!(canvas.row_text(1).contains("rich body line 1"));
    assert!(
        canvas.row_text(2).contains("(+3 more lines)"),
        "{:?}",
        canvas.row_text(2)
    );
    assert_eq!(feed.total_rows().get_untracked(), 3);
    root.dispose();
}

// ---------------------------------------------------------------------------
// The cap on the DEFAULT path (field report, commons#329): a chat body
// is `FeedBlock::Markdown`, where `max_rows` used to be a debug_assert
// in debug and a SILENT no-op in release — the call looked applied and
// drew every row. Markdown and Code now cap the rows they typeset to.
// ---------------------------------------------------------------------------

/// Six markdown paragraphs of one short line each: at width 40 the
/// document typesets to 11 rows (six paragraphs, five blank separator
/// rows between them), so the cap counts RENDERED rows and not the six
/// source lines — the distinction the whole feature turns on.
fn six_paragraph_markdown() -> String {
    (0..6)
        .map(|i| format!("para {i} body"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn markdown_blocks_cap_rendered_rows_not_source_lines() {
    let size = Size::new(40, 16);
    let (root, mut tree, feed) = mount_feed(size);
    // Uncapped first, to establish what the cap is capping — 11 rows,
    // not 6. A test that assumed 6 would pass for the wrong reason.
    feed.push("full", FeedItem::markdown(six_paragraph_markdown()));
    settle(&mut tree, size);
    assert_eq!(
        feed.total_rows().get_untracked(),
        11,
        "six paragraphs render as 11 rows (five separators)"
    );

    feed.clear();
    feed.push(
        "capped",
        FeedItem::markdown(six_paragraph_markdown()).max_rows(4),
    );
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("para 0"));
    assert!(canvas.row_text(2).contains("para 1"));
    assert!(
        canvas.row_text(3).contains("(+8 more lines)"),
        "marker names the ROWS dropped (11 - 3 shown): {:?}",
        canvas.row_text(3)
    );
    assert!(
        !(0..size.h).any(|y| canvas.row_text(y).contains("para 5")),
        "the tail is gone, and the marker says so"
    );
    assert_eq!(
        feed.total_rows().get_untracked(),
        4,
        "capped extent counts the marker row — windowing arithmetic \
         stays exact"
    );
    root.dispose();
}

#[test]
fn a_markdown_cap_that_fits_changes_nothing_at_all() {
    // The guard that stops the cap from being a truncation that always
    // fires: same document, a cap above its height, byte-identical
    // screen to the uncapped item.
    let size = Size::new(40, 16);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("full", FeedItem::markdown(six_paragraph_markdown()));
    let uncapped = settle(&mut tree, size);
    let uncapped: Vec<String> = (0..size.h).map(|y| uncapped.row_text(y)).collect();

    feed.clear();
    feed.push(
        "roomy",
        FeedItem::markdown(six_paragraph_markdown()).max_rows(11),
    );
    let canvas = settle(&mut tree, size);
    let capped: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert_eq!(capped, uncapped, "a cap at exactly the height is inert");
    assert_eq!(feed.total_rows().get_untracked(), 11);
    root.dispose();
}

#[test]
fn code_blocks_cap_with_the_same_marker() {
    let size = Size::new(40, 16);
    let (root, mut tree, feed) = mount_feed(size);
    let source = (0..8)
        .map(|i| format!("let x{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    feed.push("code", FeedItem::code("rust", source).max_rows(3));
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("let x0"));
    assert!(
        canvas.row_text(2).contains("more lines"),
        "{:?}",
        canvas.row_text(2)
    );
    assert_eq!(feed.total_rows().get_untracked(), 3);
    root.dispose();
}

#[test]
fn a_capped_markdown_body_costs_the_same_rows_however_long_the_source_is() {
    // The reason the field wanted this: a 400-line paste must not own
    // the pane. The cap is what makes the row count CONSTANT in the
    // body length — assert that directly rather than trusting the
    // single-length case above.
    let size = Size::new(40, 16);
    let (root, mut tree, feed) = mount_feed(size);
    for lines in [20usize, 400] {
        feed.clear();
        let src = (0..lines)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        feed.push("body", FeedItem::markdown(src).max_rows(6));
        settle(&mut tree, size);
        assert_eq!(
            feed.total_rows().get_untracked(),
            6,
            "{lines} source lines still cost 6 rows"
        );
    }
    root.dispose();
}

#[test]
fn streaming_items_are_unaffected_by_row_caps() {
    // Caps live on static Text/Rich blocks only: a streamed item of
    // ten lines renders all ten, and its extent counts them all.
    let size = Size::new(24, 14);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push_stream("s");
    for i in 0..10 {
        feed.stream_append("s", &format!("line {i}\n\n"));
    }
    feed.stream_finish("s");
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("line 0"));
    assert!(
        (0..size.h).any(|y| canvas.row_text(y).contains("line 6")),
        "streamed rows render uncapped"
    );
    assert!(
        !(0..size.h).any(|y| canvas.row_text(y).contains("more lines")),
        "no marker ever appears on a stream"
    );
    assert!(feed.total_rows().get_untracked() >= 19, "full extent kept");
    root.dispose();
}
