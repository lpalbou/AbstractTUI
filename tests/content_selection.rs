//! Logical source selection through Driver, terminal parser and real compositor.
use abstracttui::{
    app::{selection::selection, Driver},
    prelude::*,
    testing::CaptureTerm,
    widgets::{Button, ContentKind, ContentSelection, Feed, FeedItem, FeedState, Scroll},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Harness {
    app: App,
    driver: Driver,
    term: CaptureTerm,
    feed: FeedState,
    selected: ContentSelection,
    offset: Signal<i32>,
    follow: Signal<bool>,
    clicks: Rc<Cell<usize>>,
    clock: Rc<Cell<std::time::Instant>>,
}
fn harness(source: &str, width: i32) -> Harness {
    let size = Size::new(width, 9);
    let mut app = App::new(size);
    let out = Rc::new(RefCell::new(None));
    let slot = out.clone();
    let clicks = Rc::new(Cell::new(0));
    let clicked = clicks.clone();
    let source = source.to_owned();
    app.mount(move |cx| {
        let feed = FeedState::new(cx);
        feed.push("message", FeedItem::markdown(source));
        let selected = ContentSelection::new(cx);
        let offset = cx.signal(0);
        let follow = cx.signal(false);
        *slot.borrow_mut() = Some((feed.clone(), selected.clone(), offset, follow));
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Scroll::new(Feed::new(&feed).gap(0).content_selection(selected).view(cx))
                    .offset_y(offset)
                    .follow_tail(follow)
                    .layout(LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0)))
                    .view(cx),
            )
            .child(
                Button::new("Keep working")
                    .layout(LayoutStyle::line(1))
                    .on_click(move || clicked.set(clicked.get() + 1))
                    .view(cx),
            )
            .build()
    })
    .unwrap();
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(
        &mut app,
        &mut term,
        RunConfig {
            probe: false,
            platform_clipboard: false,
            caps: Some(abstracttui::term::Capabilities::with(|c| {
                c.truecolor = true;
                c.unicode_ok = true;
                c.osc52_copy = true;
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let clock = Rc::new(Cell::new(std::time::Instant::now()));
    let now = clock.clone();
    driver.set_clock(move || now.get());
    let (feed, selected, offset, follow) = out.borrow().clone().unwrap();
    selection().set_enabled(true);
    let mut h = Harness {
        app,
        driver,
        term,
        feed,
        selected,
        offset,
        follow,
        clicks,
        clock,
    };
    h.turn();
    h.turn();
    h
}
impl Harness {
    fn turn(&mut self) -> String {
        self.clock
            .set(self.clock.get() + std::time::Duration::from_millis(25));
        self.driver.turn(&mut self.app, &mut self.term).unwrap();
        self.term.screen().to_text()
    }
    fn mouse(&mut self, button: u8, x: usize, y: usize, released: bool) {
        self.term.push_input(
            format!(
                "\x1b[<{button};{};{}{}",
                x + 1,
                y + 1,
                if released { 'm' } else { 'M' }
            )
            .as_bytes(),
        );
        self.turn();
    }
    fn click(&mut self, x: usize, y: usize) {
        self.clock
            .set(self.clock.get() + std::time::Duration::from_millis(600));
        self.mouse(0, x, y, false);
        self.mouse(0, x, y, true);
    }
}
#[test]
fn clicks_and_wrapped_drag_have_real_highlights_and_never_copy_on_release() {
    let mut h = harness("alpha café 👩‍💻 世界. Another paragraph sentence.", 20);
    h.click(1, 0);
    assert_eq!(h.selected.value().get_untracked().unwrap().text, "alpha");
    let bg = current_theme().tokens.selection_bg;
    assert_eq!(h.term.screen().cell(1, 0).unwrap().paint.bg, Some(bg));
    assert!(h.term.screen().to_text().starts_with("alpha café"));
    assert!(
        h.term.screen().clipboard().is_none(),
        "logical release must not invoke OSC52"
    );
    h.mouse(0, 1, 0, false);
    h.mouse(32, 2, 1, false);
    h.mouse(0, 2, 1, true);
    let s = h.selected.value().get_untracked().unwrap();
    assert_eq!(s.kind, ContentKind::Span);
    assert!(
        !s.text.contains('\n'),
        "visual wraps are not source newlines"
    );
    assert!(h.term.screen().clipboard().is_none());
    h.term.push_input(b"\x1b[27u");
    h.turn();
    assert!(h.selected.value().get_untracked().is_none());
}
#[test]
fn reflow_and_scrolling_resolve_current_visible_source_without_selecting_offscreen_rows() {
    let source = (0..15)
        .map(|i| format!("Paragraph {i}: café 世界 has several words to wrap.\n\n"))
        .collect::<String>();
    let mut h = harness(&source, 24);
    h.offset.set(8);
    h.turn();
    h.turn();
    h.click(2, 0);
    let selected = h.selected.value().get_untracked().unwrap();
    assert!(selected.source.is_char_boundary(selected.bytes.start));
    assert_eq!(&selected.source[selected.bytes.clone()], selected.text);
    let before = selected.bytes.clone();
    h.term.push_resize(Size::new(32, 9));
    h.turn();
    h.turn();
    assert_eq!(h.selected.value().get_untracked().unwrap().bytes, before);
    // A captured drag below the viewport clamps to visible content; it cannot
    // treat the control row as an invisible transcript row.
    // CaptureTerm deliberately keeps a fixed VT grid on resize; resolve a
    // painted-width source cell instead of mistaking its old screen for truth.
    let offset = h.offset.get_untracked();
    let (x, y) = (0..8)
        .flat_map(|y| (0..30).map(move |x| (x, y)))
        .find(|(x, y)| h.feed.content_at(*x, *y + offset).is_some())
        .expect("visible content after reflow");
    let (x, y) = (x as usize, y as usize);
    h.mouse(0, x, y, false);
    assert!(
        h.selected.value().get_untracked().is_some(),
        "new anchor at {x},{y} after reflow:\n{}",
        h.term.screen().to_text()
    );
    h.mouse(32, x, 8, false);
    assert!(
        h.selected.value().get_untracked().is_some(),
        "drag must retain anchor"
    );
    h.mouse(0, x, 8, true);
    assert!(!h
        .selected
        .value()
        .get_untracked()
        .unwrap()
        .text
        .contains("Paragraph 14"));
}
#[test]
fn buttons_and_scrollbar_keep_their_normal_gestures() {
    let source = (0..35).map(|i| format!("line {i}\n\n")).collect::<String>();
    let mut h = harness(&source, 24);
    h.click(2, 8);
    assert_eq!(h.clicks.get(), 1);
    assert!(h.selected.value().get_untracked().is_none());
    // The Scroll's strip is outside Feed content and retains drag capture.
    h.mouse(0, 23, 0, false);
    h.mouse(32, 23, 6, false);
    h.mouse(0, 23, 6, true);
    assert!(h.offset.get_untracked() > 0);
    assert!(h.selected.value().get_untracked().is_none());
}
#[test]
fn changing_source_invalidates_selection_and_new_click_reads_current_source() {
    let mut h = harness("old content", 24);
    h.click(1, 0);
    h.feed.update("message", FeedItem::markdown("new content"));
    h.turn();
    assert!(h.selected.value().get_untracked().is_none());
    h.click(1, 0);
    assert_eq!(h.selected.value().get_untracked().unwrap().text, "new");
}

#[test]
fn clearing_or_invalidating_a_drag_releases_only_its_pointer_capture() {
    let mut h = harness("alpha beta gamma", 24);
    h.mouse(0, 1, 0, false);
    assert!(h.app.tree().pointer_capture().is_some());
    h.selected.clear();
    assert!(
        h.app.tree().pointer_capture().is_none(),
        "programmatic cancel releases retained capture"
    );
    h.mouse(0, 1, 0, false);
    h.feed.update("message", FeedItem::markdown("replacement"));
    h.turn();
    assert!(
        h.app.tree().pointer_capture().is_none(),
        "source invalidation releases capture"
    );
    h.click(2, 8);
    assert_eq!(h.clicks.get(), 1, "the next normal button must work");
}
#[test]
fn logical_and_screen_freezes_compose_and_pinned_tail_stays_still() {
    let source = (0..24)
        .map(|i| format!("paragraph {i} text\n\n"))
        .collect::<String>();
    let mut h = harness(&source, 24);
    h.follow.set(true);
    h.turn();
    h.turn();
    let offset = h.offset.get_untracked();
    let (x, y) = (0..7)
        .flat_map(|y| (0..22).map(move |x| (x, y)))
        .find(|(x, y)| h.feed.content_at(*x, *y + offset).is_some())
        .unwrap();
    let old_row = h
        .term
        .screen()
        .to_text()
        .lines()
        .nth(y as usize)
        .unwrap()
        .to_owned();
    h.click(x as usize, y as usize);
    assert!(abstracttui::widgets::scroll::follow_tail_frozen());
    h.feed.push(
        "later",
        FeedItem::markdown("new streamed content\n\nmore activity"),
    );
    h.turn();
    h.turn();
    assert_eq!(
        h.offset.get_untracked(),
        offset,
        "appends cannot move selected source"
    );
    assert!(
        h.follow.get_untracked(),
        "freeze never permanently disengages following"
    );
    assert_eq!(
        h.term.screen().to_text().lines().nth(y as usize).unwrap(),
        old_row
    );
    abstracttui::widgets::scroll::freeze_follow_tail(true);
    h.selected.clear();
    assert!(
        abstracttui::widgets::scroll::follow_tail_frozen(),
        "logical cancel must not release a screen/manual owner"
    );
    abstracttui::widgets::scroll::freeze_follow_tail(false);
    h.turn();
    h.turn();
    assert!(!abstracttui::widgets::scroll::follow_tail_frozen());
    assert!(
        h.offset.get_untracked() > offset,
        "clearing the final hold resumes normal tail pinning"
    );
}
#[test]
fn streamed_closed_and_open_blocks_keep_current_provenance() {
    let mut h = harness("placeholder", 24);
    h.feed.clear();
    h.feed.push_stream("stream");
    h.feed
        .stream_append("stream", "closed paragraph.\n\nopen tail");
    h.turn();
    h.turn();
    h.click(1, 0);
    assert_eq!(h.selected.value().get_untracked().unwrap().text, "closed");
    h.feed.stream_append("stream", " extended");
    h.turn();
    assert!(h.selected.value().get_untracked().is_none());
    h.click(1, 0);
    let selected = h.selected.value().get_untracked().unwrap();
    assert!(
        selected.source.ends_with("open tail extended"),
        "a closed-row click must not reuse stale full-document provenance"
    );
}
#[test]
fn custom_unmapped_content_retains_legacy_screen_copy_fallback() {
    let mut h = harness("placeholder", 24);
    h.feed.clear();
    h.feed.push(
        "custom",
        FeedItem::new().block(abstracttui::widgets::FeedBlock::Custom(
            abstracttui::widgets::CustomBlock::new(
                |_| 1,
                |canvas, rect| {
                    canvas.print_styled(
                        rect.origin(),
                        "custom content",
                        &abstracttui::render::Style::EMPTY,
                    );
                },
            ),
        )),
    );
    h.turn();
    h.turn();
    h.mouse(0, 0, 0, false);
    h.mouse(32, 5, 0, false);
    h.mouse(0, 5, 0, true);
    assert!(h.selected.value().get_untracked().is_none());
    assert!(
        h.term.screen().clipboard().is_some(),
        "unsupported source rows must retain screen-copy fallback"
    );
}
