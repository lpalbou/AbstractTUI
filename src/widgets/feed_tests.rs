//! Feed tests: rendering through the real tree, keyed identity,
//! streaming freeze/parity, windowing cost at 10k items, and the
//! width-fixup settle.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::base::{Point, Size};
use crate::reactive::{flush_effects, run_due_timers};
use crate::ui::{BufferCanvas, Canvas, UiTree};
use crate::widgets::itest_util::{mount_widget, render};

/// One full settle: effects -> layout -> draw (width discovery) ->
/// due timers (geometry sync) -> effects -> layout. Mirrors what one
/// `Driver::turn` + the next does in a real app. Shared with the
/// sibling `feed_rich_tests.rs` (pub(super)).
pub(super) fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let _ = render(tree, size);
    run_due_timers(std::time::Instant::now());
    flush_effects();
    tree.layout();
    render(tree, size)
}

pub(super) fn mount_feed(size: Size) -> (crate::reactive::RootScope, UiTree, FeedState) {
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    let (root, tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        *h.borrow_mut() = Some(feed.clone());
        crate::ui::Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Feed::new(&feed).view(cx))
            .build()
    });
    let feed = holder.borrow().clone().expect("state captured");
    (root, tree, feed)
}

#[test]
fn markdown_text_and_code_items_render_with_gap_rows() {
    let size = Size::new(30, 12);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("m1", FeedItem::markdown("# Hello\n\nfirst message"));
    feed.push("m2", FeedItem::text("plain line"));
    feed.push("m3", FeedItem::code("rust", "fn main() {}"));
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    let row_of = |needle: &str| {
        dump.iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not rendered:\n{dump:#?}"))
    };
    let hello = row_of("Hello");
    let first = row_of("first message");
    let plain = row_of("plain line");
    let code = row_of("fn main");
    assert!(hello < first && first < plain && plain < code);
    // One blank gap row between items.
    assert_eq!(dump[plain - 1].trim(), "", "gap row before item 2");
    assert!(
        feed.total_rows().get_untracked() > 0,
        "extent synced after settle"
    );
    root.dispose();
}

#[test]
fn duplicate_key_replaces_and_update_reflows_later_items() {
    let size = Size::new(24, 10);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("a", FeedItem::text("alpha"));
    feed.push("b", FeedItem::text("beta"));
    let _ = settle(&mut tree, size);
    let before = feed.total_rows().get_untracked();

    // Same key: replace, not append.
    feed.push("a", FeedItem::text("alpha two\nlines now"));
    assert_eq!(feed.len(), 2, "duplicate key must replace");
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    assert!(dump.iter().any(|r| r.contains("alpha two")));
    assert!(
        dump.iter().any(|r| r.contains("beta")),
        "later item still renders after the earlier one grew:\n{dump:#?}"
    );
    let after = feed.total_rows().get_untracked();
    assert_eq!(after, before + 1, "one extra wrapped row shifts the total");

    // update() by key works and unknown keys refuse.
    assert!(feed.update("b", FeedItem::text("BETA")));
    assert!(!feed.update("zz", FeedItem::text("nope")));
    let canvas = settle(&mut tree, size);
    assert!((0..size.h).any(|y| canvas.row_text(y).contains("BETA")));
    root.dispose();
}

/// Streaming parity: a token-streamed item must typeset EXACTLY like a
/// static markdown item of the same source (same pixels) — across the
/// DOC vocabulary: the source carries a table, a task item and
/// strikethrough beside the core blocks, and the 3-byte chunking cuts
/// hostile places (mid-cell, between header and delimiter).
#[test]
fn streamed_item_matches_static_item_pixels() {
    let source = "# Answer\n\nSome **bold** prose that wraps around the pane.\n\n```rust\nlet x = 1;\n```\n\n| tool | calls |\n|:-----|------:|\n| grep | 12 |\n\n- point one\n- [x] shipped ~~draft~~\n\ntail paragraph";
    let size = Size::new(28, 22);

    let (root_a, mut tree_a, feed_a) = mount_feed(size);
    feed_a.push_stream("s");
    // Stream in hostile little chunks (3 bytes at a time, char-safe).
    let chars: Vec<char> = source.chars().collect();
    for chunk in chars.chunks(3) {
        let s: String = chunk.iter().collect();
        feed_a.stream_append("s", &s);
    }
    feed_a.stream_finish("s");
    let canvas_a = settle(&mut tree_a, size);

    let (root_b, mut tree_b, feed_b) = mount_feed(size);
    feed_b.push("s", FeedItem::markdown(source));
    let canvas_b = settle(&mut tree_b, size);

    for y in 0..size.h {
        for x in 0..size.w {
            assert_eq!(
                canvas_a.cell(Point::new(x, y)),
                canvas_b.cell(Point::new(x, y)),
                "pixel divergence at ({x},{y}):\nstreamed: {:?}\nstatic:   {:?}",
                canvas_a.row_text(y),
                canvas_b.row_text(y)
            );
            assert_eq!(
                canvas_a.attrs_at(Point::new(x, y)),
                canvas_b.attrs_at(Point::new(x, y)),
                "attr divergence at ({x},{y})"
            );
        }
    }
    // The doc vocabulary actually rendered (not literal pipe text):
    // the table's separator rule and the task checkbox are visible.
    let dump: Vec<String> = (0..size.h).map(|y| canvas_b.row_text(y)).collect();
    assert!(
        dump.iter().any(|r| r.trim_start().starts_with('─')),
        "table separator rule rendered:\n{dump:#?}"
    );
    assert!(
        dump.iter().any(|r| r.contains("[x] shipped")),
        "task item rendered:\n{dump:#?}"
    );
    root_a.dispose();
    root_b.dispose();
}

/// The DOC vocabulary in static markdown items (the chat/transcript
/// case): a table renders as a TABLE — bold header, border-ink
/// separator rule, right-aligned numeric cells — a task item wears the
/// checkbox marker in the list-marker ink, and `~~strike~~` carries
/// the STRIKE attribute. One recipe with `MarkdownView` (layout_doc).
#[test]
fn markdown_items_render_tables_tasks_and_strikethrough() {
    let t = crate::theme::default_theme().tokens;
    let size = Size::new(30, 12);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push(
        "m",
        FeedItem::markdown(
            "| tool | calls |\n|:-----|------:|\n| grep | 12 |\n\n- [x] done\n\n~~old~~ new",
        ),
    );
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    let row_of = |needle: &str| {
        dump.iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not rendered:\n{dump:#?}"))
    };
    // Header row: bold cells.
    let hdr = row_of("tool");
    let hx = dump[hdr].find("tool").unwrap() as i32;
    assert!(canvas
        .attrs_at(Point::new(hx, hdr as i32))
        .contains(crate::render::Attrs::BOLD));
    // Separator rule right below, in border ink.
    let sep = hdr + 1;
    assert!(dump[sep].trim_start().starts_with('─'), "{:?}", dump[sep]);
    let sx = dump[sep].find('─').unwrap() as i32;
    assert_eq!(canvas.cell(Point::new(sx, sep as i32)).unwrap().1, t.border);
    // Body row: "12" right-aligned under the "calls" column (both end
    // at the same column).
    let body = row_of("grep");
    let calls_end = dump[hdr].find("calls").unwrap() + "calls".len();
    let num_end = dump[body].find("12").unwrap() + "12".len();
    assert_eq!(num_end, calls_end, "right-aligned numeric cell");
    // Task item: checkbox marker in the list-marker ink.
    let task = row_of("[x] done");
    let tx = dump[task].find("[x]").unwrap() as i32;
    assert_eq!(
        canvas.cell(Point::new(tx, task as i32)).unwrap().1,
        t.accent_alt
    );
    // Strikethrough: attribute on the struck span only.
    let strike = row_of("old");
    let ox = dump[strike].find("old").unwrap() as i32;
    let nx = dump[strike].find("new").unwrap() as i32;
    assert!(canvas
        .attrs_at(Point::new(ox, strike as i32))
        .contains(crate::render::Attrs::STRIKE));
    assert!(!canvas
        .attrs_at(Point::new(nx, strike as i32))
        .contains(crate::render::Attrs::STRIKE));
    root.dispose();
}

/// The 0144 laziness contract THROUGH the feed: an item with an image
/// measures its extent from the header PROBE at typeset — no decode.
/// Off-screen image rows never decode (windowed draw); scrolling them
/// into view pays the one decode.
#[test]
fn feed_item_with_image_measures_without_decoding() {
    use crate::widgets::markdown::imageflow::decode_count;
    // A real 8x8 PNG on disk (probe reads its header only).
    let bmp = crate::gfx::Bitmap::from_fn(8, 8, |x, _| {
        if x < 4 {
            crate::base::Rgba::WHITE
        } else {
            crate::base::Rgba::BLACK
        }
    });
    let path =
        std::env::temp_dir().join(format!("abstracttui_feed_img_{}.png", std::process::id()));
    std::fs::write(&path, crate::gfx::png_encode::encode(&bmp)).unwrap();

    // Fixed 2-row box: the image item sits BELOW the clip — its rows
    // never draw, so nothing may decode; the extent still counts them.
    let size = Size::new(20, 2);
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
    let feed = holder.borrow().clone().expect("state");
    let before = decode_count();
    feed.push("head", FeedItem::text("above the fold"));
    feed.push(
        "img",
        FeedItem::markdown(format!("![pic]({})", path.display())),
    );
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains("above the fold"));
    // Extent: head(1) + gap(1) + image rows(8px/2=4) + caption(1) = 7.
    assert_eq!(feed.total_rows().get_untracked(), 7, "probe-sized extent");
    assert_eq!(decode_count(), before, "measure + clipped draw: no decode");

    // A window tall enough to show the image rows decodes exactly once.
    tree.set_viewport(Size::new(20, 8));
    let canvas = settle(&mut tree, Size::new(20, 8));
    assert_eq!(
        decode_count(),
        before + 1,
        "first visible draw decodes once"
    );
    // Mosaic pixels landed (left half bright, right dark).
    let (_, _, lbg) = canvas.cell(Point::new(1, 3)).unwrap();
    let (_, _, rbg) = canvas.cell(Point::new(6, 3)).unwrap();
    assert!(lbg.r > 200, "left half bright: {lbg:?}");
    assert!(rbg.r < 60, "right half dark: {rbg:?}");
    let _ = std::fs::remove_file(&path);
    root.dispose();
}

/// The freeze contract: closed blocks typeset once; a token append
/// re-typesets ONLY the open tail block.
#[test]
fn stream_appends_typeset_only_the_open_block() {
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push_stream("s");
    let _ = settle(&mut tree, size); // width known, styles bound
                                     // Close 40 blocks (paragraph + blank each).
    for i in 0..40 {
        feed.stream_append("s", &format!("closed paragraph {i}\n\n"));
    }
    let baseline = feed.blocks_typeset_total();
    // 60 tokens into the open tail: each append may re-typeset the one
    // open block, never the 40 closed ones.
    for _ in 0..60 {
        feed.stream_append("s", "token ");
    }
    let cost = feed.blocks_typeset_total() - baseline;
    assert!(
        cost <= 60,
        "60 tail tokens re-typeset {cost} blocks — closed blocks are being revisited"
    );
    root.dispose();
}

/// The freeze contract over a streamed TABLE (doc vocabulary): while
/// the table is the OPEN region, each row append re-typesets exactly
/// that one open block — never the closed prefix; the table's closing
/// line seals it into the frozen segment, and the tail typesets like
/// any open paragraph after it.
#[test]
fn streamed_table_retypesets_only_the_open_region() {
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push_stream("s");
    let _ = settle(&mut tree, size); // width known, styles bound
    for i in 0..20 {
        feed.stream_append("s", &format!("closed paragraph {i}\n\n"));
    }
    // Open a table (header + delimiter complete = OPEN block).
    feed.stream_append("s", "| tool | calls |\n|---|---:|\n");
    let baseline = feed.blocks_typeset_total();
    // 30 body-row appends: each re-typesets the ONE open table block.
    for i in 0..30 {
        feed.stream_append("s", &format!("| step {i} | {i} |\n"));
    }
    let grow_cost = feed.blocks_typeset_total() - baseline;
    assert!(
        grow_cost <= 30,
        "30 table rows re-typeset {grow_cost} blocks — the closed prefix thawed"
    );
    // A blank line closes the table: it freezes (typesets once more
    // into segment 0); the closed count never grows again.
    feed.stream_append("s", "\n");
    let sealed = feed.blocks_typeset_total();
    for _ in 0..20 {
        feed.stream_append("s", "tail token ");
    }
    let tail_cost = feed.blocks_typeset_total() - sealed;
    assert!(
        tail_cost <= 20,
        "20 tail tokens re-typeset {tail_cost} blocks — the sealed table is being revisited"
    );
    feed.stream_finish("s");
    // And the pixels are a table: separator rule + right-aligned cell.
    let tall = Size::new(40, 60);
    tree.set_viewport(tall);
    let canvas = settle(&mut tree, tall);
    let dump: Vec<String> = (0..tall.h).map(|y| canvas.row_text(y)).collect();
    assert!(
        dump.iter()
            .any(|r| r.contains("tool") && r.contains("calls")),
        "table header rendered:\n{dump:#?}"
    );
    assert!(
        dump.iter().any(|r| r.trim_start().starts_with('─')),
        "table separator rule rendered"
    );
    root.dispose();
}

/// Windowing: drawing a 10k-item feed inside a small box costs only the
/// window (draw-call counting canvas), wherever the window sits.
struct CountingCanvas {
    inner: BufferCanvas,
    puts: Rc<RefCell<usize>>,
}

impl Canvas for CountingCanvas {
    fn size(&self) -> Size {
        self.inner.size()
    }
    fn put(&mut self, p: Point, ch: char, fg: crate::base::Rgba, bg: crate::base::Rgba) {
        *self.puts.borrow_mut() += 1;
        self.inner.put(p, ch, fg, bg);
    }
}
impl crate::ui::StyledCanvas for CountingCanvas {}

#[test]
fn feed_10k_items_draws_only_the_window() {
    let size = Size::new(30, 10);
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();
    // Fixed-box mode: the feed clips at the box (windowing must bound
    // the draw cost by the box, not the content).
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
    let feed = holder.borrow().clone().expect("state");
    for i in 0..10_000 {
        feed.push(
            format!("m{i}"),
            FeedItem::text(format!("message number {i}")),
        );
    }
    let _ = settle(&mut tree, size);

    let puts = Rc::new(RefCell::new(0usize));
    let mut canvas = CountingCanvas {
        inner: BufferCanvas::new(size),
        puts: puts.clone(),
    };
    tree.draw(&mut canvas);
    let cost = *puts.borrow();
    let budget = (size.w * size.h) as usize * 3;
    assert!(
        cost <= budget,
        "drawing 10k items cost {cost} puts (budget {budget}) — the feed is not windowing"
    );
    assert!(
        canvas.inner.row_text(0).contains("message number 0"),
        "head visible in fixed-box mode: {:?}",
        canvas.inner.row_text(0)
    );
    root.dispose();
}

/// Width discovery settles the reactive extent; a later resize
/// re-typesets at the new width and re-syncs.
#[test]
fn width_change_retypesets_and_resyncs_the_extent() {
    let size = Size::new(40, 8);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push(
        "a",
        FeedItem::markdown("a paragraph long enough to wrap differently at different widths"),
    );
    let _ = settle(&mut tree, size);
    let wide_rows = feed.total_rows().get_untracked();
    assert!(wide_rows >= 2, "wrapped at 40 cols: {wide_rows}");

    tree.set_viewport(Size::new(20, 8));
    let narrow = Size::new(20, 8);
    let canvas = settle(&mut tree, narrow);
    let narrow_rows = feed.total_rows().get_untracked();
    assert!(
        narrow_rows > wide_rows,
        "narrower pane must wrap into more rows ({wide_rows} -> {narrow_rows})"
    );
    assert!(canvas.row_text(0).contains("a paragraph"));
    root.dispose();
}

/// Custom blocks: honest height, drawn at their sub-rect, after the
/// state borrow releases (mutating the feed from a custom draw is a
/// contract violation, not tested).
#[test]
fn custom_blocks_occupy_their_height_and_draw() {
    let size = Size::new(24, 8);
    let (root, mut tree, feed) = mount_feed(size);
    let drawn: Rc<RefCell<Vec<crate::base::Rect>>> = Rc::new(RefCell::new(Vec::new()));
    let d = drawn.clone();
    feed.push(
        "c",
        FeedItem::new()
            .block(FeedBlock::Text("above".into()))
            .block(FeedBlock::Custom(CustomBlock::new(
                |_w| 2,
                move |canvas, rect| {
                    d.borrow_mut().push(rect);
                    canvas.fill(
                        rect,
                        '#',
                        crate::base::Rgba::WHITE,
                        crate::base::Rgba::BLACK,
                    );
                },
            )))
            .block(FeedBlock::Text("below".into())),
    );
    let canvas = settle(&mut tree, size);
    let dump: Vec<String> = (0..size.h).map(|y| canvas.row_text(y)).collect();
    let above = dump.iter().position(|r| r.contains("above")).unwrap();
    let below = dump.iter().position(|r| r.contains("below")).unwrap();
    let hashes = dump.iter().position(|r| r.starts_with("##")).unwrap();
    assert!(above < hashes && hashes < below);
    let rect = drawn.borrow().last().copied().expect("custom drew");
    assert_eq!(rect.h, 2, "honest height honored");
    assert_eq!(rect.w, size.w);
    root.dispose();
}

/// Appends at a known width publish the extent synchronously (no timer
/// round needed) — the single-frame pin path for follow-tail.
#[test]
fn appends_at_known_width_sync_the_extent_immediately() {
    let size = Size::new(24, 6);
    let (root, mut tree, feed) = mount_feed(size);
    feed.push("a", FeedItem::text("one"));
    let _ = settle(&mut tree, size);
    let before = feed.total_rows().get_untracked();
    feed.push("b", FeedItem::text("two"));
    // No settle: the signal must already carry the new extent.
    let after = feed.total_rows().get_untracked();
    assert_eq!(after, before + 2, "gap + one row, synchronously");
    root.dispose();
}
