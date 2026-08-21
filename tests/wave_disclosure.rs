//! Disclosure wave (first-app 0260 + field-agora 0850): the fold/
//! unfold card and the Feed item-press enabler, through the REAL
//! `Driver` — wire bytes in via `CaptureTerm`, modeled VT screen out
//! (the wave_choice_0271.rs harness posture; helper duplication across
//! integration files is the house style).
//!
//! Covers the commissioned integration surface: click-on-title
//! toggles, Enter toggles while focused, the capped body's visible
//! scrollbar + wheel scrolling, zero idle bytes with cards parked
//! (folded AND unfolded), toggle damage contained to the card's band,
//! and SGR item presses reporting `(key, row_within_item)`.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::theme::default_theme;
use abstracttui::ui::text;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..128 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 128 turns");
}

fn boot(app: &mut App, term: &mut CaptureTerm) -> Driver {
    let mut driver = Driver::new(app, term, config()).expect("driver");
    settle(&mut driver, app, term);
    driver
}

fn screen_lines(term: &CaptureTerm) -> Vec<String> {
    term.screen()
        .to_text()
        .lines()
        .map(str::to_string)
        .collect()
}

/// SGR left click (press + release) at 1-BASED terminal coordinates.
fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

/// SGR wheel-down at 1-based coordinates.
fn sgr_wheel_down(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<65;{col};{row}M").into_bytes()
}

/// 1-based rows addressed by absolute CUP (`ESC [ r ; c H` / `ESC [ H`)
/// in an emitted byte stream — the damage-containment probe (the
/// presenter re-anchors each damaged row absolutely; the frame trailer
/// parks bottom-LEFT, so the park row is the screen's last row).
fn cup_rows(bytes: &[u8]) -> Vec<i32> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b';') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'H' {
                let params = &bytes[i + 2..j];
                let row: i32 = params
                    .split(|&b| b == b';')
                    .next()
                    .filter(|p| !p.is_empty())
                    .map(|p| String::from_utf8_lossy(p).parse().unwrap_or(1))
                    .unwrap_or(1);
                rows.push(row);
            }
            i = j;
        }
        i += 1;
    }
    rows
}

const W: i32 = 32;
const H: i32 = 12;

fn twelve_lines() -> String {
    (0..12)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ===========================================================================
// Toggle surfaces through the wire: click on the title row, Enter while
// focused (the click focused the header — the tree's click-to-focus rule).
// ===========================================================================

#[test]
fn click_on_the_title_unfolds_and_enter_folds_back() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("alpha card", "alpha body text").view(cx))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let lines = screen_lines(&term);
    assert!(
        lines[0].contains('▸') && lines[0].contains("alpha card"),
        "folded header renders: {lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("alpha body")),
        "folded: no body on screen"
    );

    // Click the title row (screen row 0 = SGR row 1).
    term.push_input(&sgr_click(4, 1));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(lines[0].contains('▾'), "glyph flips open: {lines:#?}");
    assert!(
        lines.iter().any(|l| l.contains("alpha body")),
        "unfolded: body renders: {lines:#?}"
    );

    // The click focused the header: Enter folds it back.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(lines[0].contains('▸'), "{lines:#?}");
    assert!(
        !lines.iter().any(|l| l.contains("alpha body")),
        "Enter while focused re-folds: {lines:#?}"
    );
}

// ===========================================================================
// The capped body: visible scrollbar on overflow, wheel scrolls the body.
// ===========================================================================

#[test]
fn capped_body_shows_a_scrollbar_and_the_wheel_scrolls_it() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("log", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(4)
                    .view(cx),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let lines = screen_lines(&term);
    assert!(lines[1].contains("line 0"), "{lines:#?}");
    assert!(lines[4].contains("line 3"), "4 capped rows: {lines:#?}");
    assert!(
        lines[5].contains("BELOW"),
        "card ends at the cap: {lines:#?}"
    );
    // The thumb lives in the body's right column (host right pad = 1).
    let bar_col = W - 2;
    let bar: String = (1..5)
        .filter_map(|y| term.screen().cell(bar_col, y).map(|c| c.ch()))
        .collect();
    assert!(
        bar.contains('█'),
        "scrollbar thumb visible on overflow: {bar:?}\n{lines:#?}"
    );

    // Wheel-down over the body (row 3 on screen = SGR row 3+1): +3 rows.
    term.push_input(&sgr_wheel_down(5, 3));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[1].contains("line 3"),
        "wheel scrolled the body: {lines:#?}"
    );
    assert!(lines[5].contains("BELOW"), "card extent held: {lines:#?}");
}

// ===========================================================================
// Idle honesty: cards parked (one folded, one unfolded + capped scroll)
// cost zero bytes on idle turns.
// ===========================================================================

#[test]
fn parked_cards_cost_zero_idle_bytes() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("folded", "hidden").view(cx))
            .child(
                Disclosure::text("open", twelve_lines())
                    .initially_folded(false)
                    .max_body_rows(3)
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let _ = term.take_bytes();

    for i in 0..8 {
        let turn = driver.turn(&mut app, &mut term).expect("idle turn");
        assert!(turn.idle, "turn {i} must be idle");
        assert!(!turn.rendered, "turn {i} rendered");
    }
    assert!(
        term.bytes().is_empty(),
        "idle turns wrote bytes: {:?}",
        String::from_utf8_lossy(term.bytes())
    );
}

// ===========================================================================
// Damage containment: toggling a card repaints its own band, never the
// rows above it.
// ===========================================================================

#[test]
fn toggle_damage_stays_inside_the_cards_band() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("top status row"))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("second static row"))
                    .build(),
            )
            .child(Disclosure::text("deep card", "one\ntwo").view(cx))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let before = screen_lines(&term);
    let _ = term.take_bytes();

    // Toggle the card (header at screen row 2 = SGR row 3).
    term.push_input(&sgr_click(4, 3));
    settle(&mut driver, &mut app, &mut term);
    let bytes = term.take_bytes();
    let rows = cup_rows(&bytes);
    assert!(
        !rows.is_empty(),
        "the toggle must repaint something: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        rows.iter().all(|&r| r >= 3),
        "damage leaked above the card (CUP rows {rows:?}): {:?}",
        String::from_utf8_lossy(&bytes)
    );
    let after = screen_lines(&term);
    assert_eq!(before[0], after[0], "static row 0 untouched");
    assert_eq!(before[1], after[1], "static row 1 untouched");
    assert!(
        after.iter().any(|l| l.contains("one")),
        "the card did unfold: {after:#?}"
    );
}

// ===========================================================================
// Modal composition: a Disclosure inside a Modal toggles and its
// measured card height stays honest inside the panel.
// ===========================================================================

#[test]
fn disclosure_composes_inside_a_modal() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let ss = scope_slot.clone();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .focusable()
            .child(text("host app row"))
            .build()
    })
    .expect("mount");
    let overlays = app.overlays();
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let cx = scope_slot.borrow().expect("scope");
    // 24x8 panel centered in 32x12 -> bounds (4,2), 1-cell padding ->
    // content at (5,3): the card header renders on screen row 3.
    let _modal = Modal::open(&overlays, cx, size, Size::new(24, 8), |mcx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::text("in modal", "modal body line").view(mcx))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("FOOTER"))
                    .build(),
            )
            .build()
    });
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[3].contains('▸') && lines[3].contains("in modal"),
        "card header inside the panel: {lines:#?}"
    );
    assert!(
        lines[4].contains("FOOTER"),
        "folded card measures one row — the footer sits right below: {lines:#?}"
    );

    // Click the title (row 3 -> SGR row 4): the body opens IN the panel
    // and pushes the footer down by exactly the body height.
    term.push_input(&sgr_click(8, 4));
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines.iter().any(|l| l.contains("modal body line")),
        "body renders inside the modal: {lines:#?}"
    );
    assert!(
        lines[5].contains("FOOTER"),
        "unfolded card measures header + 1 body row: {lines:#?}"
    );
    assert!(
        lines[0].contains("host app row"),
        "the host row above the panel is untouched: {lines:#?}"
    );
}

// ===========================================================================
// Feed item press through the wire: SGR click reports (key,
// row_within_item); gap rows are silent.
// ===========================================================================

#[test]
fn feed_sgr_click_reports_key_and_row_within_item() {
    let size = Size::new(W, H);
    let log: Rc<RefCell<Vec<(String, i32)>>> = Rc::default();
    let sink = log.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        let feed = FeedState::new(cx);
        feed.push("a", FeedItem::text("alpha"));
        feed.push("b", FeedItem::text("b one\nb two"));
        let sink = sink.clone();
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Feed::new(&feed)
                    .on_item_press(move |key, row| sink.borrow_mut().push((key.into(), row)))
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    // Content rows: 0 = a[0], 1 = gap, 2 = b[0], 3 = b[1] (SGR +1).
    term.push_input(&sgr_click(3, 3)); // b, row 0
    settle(&mut driver, &mut app, &mut term);
    term.push_input(&sgr_click(3, 2)); // gap: silent
    settle(&mut driver, &mut app, &mut term);
    term.push_input(&sgr_click(3, 4)); // b, row 1
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        &[("b".into(), 0), ("b".into(), 1)],
        "wire presses map to (key, row_within_item); the gap is silent"
    );
}

// ===========================================================================
// Outer Scroll over a column of Disclosures: wheel/keys must move by CONTENT
// rows, not by card count (laurent 2026-07-28 — one panel ≠ one line).
// ===========================================================================

fn key_down() -> Vec<u8> {
    b"\x1b[B".to_vec()
}

#[test]
fn scroll_column_of_unfolded_disclosures_wheel_moves_content_rows() {
    let size = Size::new(40, 6);
    let body_b = (0..10)
        .map(|i| format!("body-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = App::new(size);
    app.mount(|cx| {
        let t = default_theme().tokens;
        let col = Element::new()
            .style(LayoutStyle::column())
            .child(text("row-0-anchor"))
            .child(Disclosure::text("card-a", "hidden-a").view(cx))
            .child(
                Disclosure::text("card-b", body_b)
                    .initially_folded(false)
                    .max_body_rows(0)
                    .view(cx),
            )
            .child(Disclosure::text("card-c", "hidden-c").view(cx))
            .child(text("row-tail"));
        Scroll::new(col.build()).element(cx, &t).build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[0].contains("row-0-anchor"),
        "starts at the top: {lines:#?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("body-0")),
        "the unfolded body is visible initially: {lines:#?}"
    );

    // Wheel down (+3 content rows): the anchor must scroll off.
    term.push_input(&sgr_wheel_down(5, 3));
    settle(&mut driver, &mut app, &mut term);
    let after_wheel = screen_lines(&term);
    assert!(
        !after_wheel[0].contains("row-0-anchor"),
        "wheel moved the pane by content rows, not one card: {after_wheel:#?}"
    );
    assert!(
        after_wheel[0].contains("body-0") || after_wheel[0].contains("body-1"),
        "after +3 the viewport head lands inside the long body: {after_wheel:#?}"
    );

    // Down arrow (+1 content row) from a focused scroll area.
    term.push_input(b"\x1b[Z"); // focus scroll (tab may work too)
    term.push_input(&key_down());
    settle(&mut driver, &mut app, &mut term);
    let after_down = screen_lines(&term);
    assert_ne!(
        after_down[0], after_wheel[0],
        "Down moves one content row, not one card: before={:?} after={:?}",
        after_wheel[0], after_down[0]
    );
}

#[test]
fn scroll_column_remeasures_when_a_disclosure_unfolds() {
    let size = Size::new(40, 8);
    let body = (0..8)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let folded = Rc::new(RefCell::new(None::<Signal<bool>>));
    let slot = folded.clone();
    let mut app = App::new(size);
    app.mount(move |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(true);
        *slot.borrow_mut() = Some(open);
        let col = Element::new()
            .style(LayoutStyle::column())
            .child(text("TOP"))
            .child(
                Disclosure::text("deep", body)
                    .folded(open)
                    .max_body_rows(0)
                    .view(cx),
            )
            .child(text("BOTTOM"));
        Scroll::new(col.build()).element(cx, &t).build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);
    let folded_sig = folded.borrow().expect("signal");
    let folded_start = screen_lines(&term);
    assert!(
        !folded_start.iter().any(|l| l.contains("line-0")),
        "folded: body hidden: {folded_start:#?}"
    );
    assert!(
        folded_start.iter().any(|l| l.contains("BOTTOM")),
        "folded: tail visible in viewport: {folded_start:#?}"
    );

    folded_sig.set(false);
    settle(&mut driver, &mut app, &mut term);
    let unfolded = screen_lines(&term);
    assert!(
        unfolded.iter().any(|l| l.contains("line-0")),
        "unfolded: body visible: {unfolded:#?}"
    );

    // Wheel should scroll inside the tall content, not jump the whole card band.
    term.push_input(&sgr_wheel_down(5, 4));
    settle(&mut driver, &mut app, &mut term);
    let after_wheel = screen_lines(&term);
    assert!(
        !after_wheel.iter().any(|l| l.contains("TOP")),
        "wheel scrolls the tall column, extent includes the body: {after_wheel:#?}"
    );
    assert!(
        after_wheel.iter().any(|l| l.contains("line-")),
        "still showing body lines after wheel: {after_wheel:#?}"
    );
}

// ===========================================================================
// field-agora 0890: the CAPPED body region under-measures a Feed body whose
// items contain RICH blocks, so the last row silently clips.
//
// The reported shape (docs/backlog/completed/field-agora/0890_*.md) is
// agora-tui's card: one FeedItem carrying a RICH meta line above the message
// text. Under any positive `max_body_rows(n)` the region settles SHORT — the
// capped path sizes itself from `Scroll::extent_signal` (disclosure.rs, the
// `mh.min(cap)` style_signal), and rich rows are reported as contributing
// nothing or only partially to that extent.
//
// Two controls make the failing case falsifiable rather than merely red:
// a pure-TEXT body of the same row count under the same cap, and the same
// rich body UNCAPPED. The report says both of those render completely, so if
// either ever goes red the defect is not the one described here.
// ===========================================================================

/// One item: a rich META line, then two text rows. Three rows total, well
/// under the cap, so nothing should scroll and nothing should clip.
fn rich_meta_card(cx: abstracttui::reactive::Scope) -> FeedState {
    let t = default_theme().tokens;
    let fs = FeedState::new(cx);
    fs.push(
        "card",
        FeedItem::rich_lines(vec![abstracttui::render::RichLine::from_spans(vec![
            abstracttui::render::Span::new("META", abstracttui::render::Style::new().fg(t.accent)),
        ])])
        .block(abstracttui::widgets::FeedBlock::Text(
            "BODY-1\nBODY-2".into(),
        )),
    );
    fs
}

#[test]
fn capped_body_measures_rich_feed_rows_and_clips_nothing() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        let t = default_theme().tokens;
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("card")
                    .initially_folded(false)
                    // Cap of 8 is far above the 3 rows this body needs:
                    // the cap must bound a LONG body, never shorten a
                    // short one.
                    .max_body_rows(8)
                    .body(move |gcx| {
                        let fs = rich_meta_card(gcx);
                        Feed::new(&fs).gap(0).element(gcx, &t).build()
                    })
                    .view(cx),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let _driver = boot(&mut app, &mut term);

    let lines = screen_lines(&term);
    let screen = lines.join("\n");
    assert!(
        screen.contains("META"),
        "the rich meta row paints:\n{screen}"
    );
    assert!(
        screen.contains("BODY-1"),
        "first text row paints:\n{screen}"
    );
    // THE DEFECT: the extent under-reports the rich row, the region settles
    // at 2 instead of 3, and this last row never paints.
    assert!(
        screen.contains("BODY-2"),
        "0890: the last body row must not clip under a cap that is larger \
         than the body:\n{screen}"
    );
}

/// CONTROL 1 — same three rows, same cap, no rich block. The report says a
/// pure-text body renders completely, so this passing is what makes the
/// failure above specific to rich measurement rather than to the cap.
#[test]
fn capped_body_with_a_pure_text_body_clips_nothing() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("card", "META\nBODY-1\nBODY-2")
                    .initially_folded(false)
                    .max_body_rows(8)
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let _driver = boot(&mut app, &mut term);
    let screen = screen_lines(&term).join("\n");
    for row in ["META", "BODY-1", "BODY-2"] {
        assert!(
            screen.contains(row),
            "text body row {row} paints:\n{screen}"
        );
    }
}

/// CONTROL 2 — the same RICH body, uncapped. The report says this renders
/// completely, which places the defect at the capped region's use of the
/// measured extent, not at rich rendering itself.
#[test]
fn uncapped_body_renders_every_rich_row() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    app.mount(|cx| {
        let t = default_theme().tokens;
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("card")
                    .initially_folded(false)
                    .max_body_rows(0)
                    .body(move |gcx| {
                        let fs = rich_meta_card(gcx);
                        Feed::new(&fs).gap(0).element(gcx, &t).build()
                    })
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let _driver = boot(&mut app, &mut term);
    let screen = screen_lines(&term).join("\n");
    for row in ["META", "BODY-1", "BODY-2"] {
        assert!(
            screen.contains(row),
            "uncapped rich body row {row} paints:\n{screen}"
        );
    }
}

/// The report's OTHER two shapes, so "does not reproduce" covers what 0890
/// actually claimed rather than only its headline case: two ITEMS (rich then
/// text), and a body that is rich rows only. Same cap, same expectation —
/// nothing clips when the cap exceeds the body.
#[test]
fn capped_body_measures_rich_across_the_reports_other_shapes() {
    for (name, two_items) in [("two-items", true), ("rich-only", false)] {
        let size = Size::new(W, H);
        let mut app = App::new(size);
        app.mount(move |cx| {
            let t = default_theme().tokens;
            Element::new()
                .style(LayoutStyle::column())
                .child(
                    Disclosure::new("card")
                        .initially_folded(false)
                        .max_body_rows(8)
                        .body(move |gcx| {
                            let t2 = default_theme().tokens;
                            let fs = FeedState::new(gcx);
                            let meta = || {
                                FeedItem::rich_lines(vec![
                                    abstracttui::render::RichLine::from_spans(vec![
                                        abstracttui::render::Span::new(
                                            "META",
                                            abstracttui::render::Style::new().fg(t2.accent),
                                        ),
                                    ]),
                                ])
                            };
                            if two_items {
                                // rich item, then a separate text item
                                fs.push("m", meta());
                                fs.push("b", FeedItem::text("BODY-1\nBODY-2".to_string()));
                            } else {
                                // rich meta + a second rich row, no text at all
                                fs.push("m", meta());
                                fs.push(
                                    "p",
                                    FeedItem::rich_lines(vec![
                                        abstracttui::render::RichLine::from_spans(vec![
                                            abstracttui::render::Span::new(
                                                "BODY-1",
                                                abstracttui::render::Style::new(),
                                            ),
                                        ]),
                                    ]),
                                );
                            }
                            Feed::new(&fs).gap(0).element(gcx, &t).build()
                        })
                        .view(cx),
                )
                .build()
        })
        .expect("mount");
        let mut term = CaptureTerm::new(size);
        let _driver = boot(&mut app, &mut term);
        let screen = screen_lines(&term).join("\n");
        let want: &[&str] = if two_items {
            &["META", "BODY-1", "BODY-2"]
        } else {
            &["META", "BODY-1"]
        };
        for row in want {
            assert!(
                screen.contains(row),
                "0890 [{name}]: row {row} must not clip under a cap larger \
                 than the body:\n{screen}"
            );
        }
    }
}
