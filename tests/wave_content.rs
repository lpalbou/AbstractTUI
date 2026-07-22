//! CONTENT wave: Feed (0100) + md::StreamSession (0110) + Scroll
//! follow-tail / measured extent (0130), hardened through the REAL
//! frame loop (`Driver::turn` against `CaptureTerm`) and the public
//! `UiTree` harness.
//!
//! Pins, by spec:
//! - follow-tail acceptance: appends keep the bottom row visible;
//!   wheel-up disengages; the bottom edge re-engages; resize keeps the
//!   tail pinned (0130 validation, driven by real SGR wheel bytes);
//! - append damage containment: streaming into the transcript never
//!   repaints static chrome (byte-identical rows outside the pane) and
//!   every emitted byte is modeled VT traffic;
//! - windowing budget: 10k items inside a measured Scroll draw only a
//!   screenful (draw-call counting canvas);
//! - stream token cost through the widget stack: tail tokens behind
//!   closed blocks typeset O(open block), never the closed prefix;
//! - measured numbers (printed): 100k appends wall time, bytes/frame
//!   during steady streaming, one full-feed scroll repaint. Asserted
//!   ceilings are catastrophic-only (debug builds vary; the printed
//!   numbers are the report).

use std::time::Instant;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Size};
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::reactive::{batch, create_root, flush_effects, run_due_timers, Signal};
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{text, BufferCanvas, Canvas, Element, Key, KeyEvent, UiEvent, UiTree};
use abstracttui::widgets::{Feed, FeedItem, FeedState, Scroll};

fn config() -> RunConfig {
    RunConfig {
        // ADR-0003: `Capabilities` is `#[non_exhaustive]`; this file
        // compiles as a downstream crate, so construction goes
        // through `with`.
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

/// Drive turns until idle (bounded); returns rendered-frame count.
fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) -> usize {
    let mut renders = 0;
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.rendered {
            renders += 1;
        }
        if turn.idle {
            return renders;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// One screen row with its paint — the byte-exact containment currency
/// (identical cells across an operation = that row was never touched).
fn row_dump(screen: &VtScreen, y: i32) -> String {
    let mut out = String::new();
    for x in 0..screen.size().w {
        let c = screen.cell(x, y).unwrap();
        out.push_str(&format!("{}:{:?}:{:?};", c.ch(), c.paint.fg, c.paint.bg));
    }
    out
}

/// SGR wheel bytes (1-based coords), as a real terminal would send.
fn wheel(term: &mut CaptureTerm, up: bool, col: i32, row: i32) {
    let code = if up { 64 } else { 65 };
    term.push_input(format!("\x1b[<{code};{col};{row}M").as_bytes());
}

struct Wiring {
    feed: FeedState,
    follow: Signal<bool>,
}

/// One static chrome row + a measured `Scroll` over a content-sized
/// `Feed` with follow-tail — the transcript shape, no hints anywhere.
fn transcript_app(size: Size) -> (App, Wiring) {
    let mut app = App::new(size);
    let mut wiring = None;
    app.mount(|cx| {
        let feed = FeedState::new(cx);
        let follow = cx.signal(true);
        wiring = Some(Wiring {
            feed: feed.clone(),
            follow,
        });
        Element::new()
            .style(LayoutStyle::column())
            .child(text("HEADER chrome row that must never repaint"))
            .child(
                Scroll::new(Feed::new(&feed).gap(0).view(cx))
                    .follow_tail(follow)
                    .view(cx),
            )
            .build()
    })
    .expect("mount");
    (app, wiring.expect("wiring"))
}

// ---------------------------------------------------------------------------
// 0130 acceptance: follow-tail through real wheel bytes and a resize.
// ---------------------------------------------------------------------------

#[test]
fn follow_tail_acceptance_appends_wheel_and_resize() {
    let size = Size::new(40, 10);
    let mut term = CaptureTerm::new(size);
    let (mut app, w) = transcript_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    // Appends keep the bottom row visible (pane rows 1..=9).
    for i in 0..20 {
        w.feed
            .push(format!("m{i}"), FeedItem::text(format!("line {i}")));
    }
    settle(&mut driver, &mut app, &mut term);
    assert!(
        term.screen()
            .to_text()
            .lines()
            .nth(9)
            .unwrap()
            .contains("line 19"),
        "growth must keep the tail visible:\n{}",
        term.screen().to_text()
    );
    assert!(w.follow.get_untracked(), "still following at the bottom");

    // Wheel-up over the pane disengages; further growth holds the view.
    wheel(&mut term, true, 3, 5);
    settle(&mut driver, &mut app, &mut term);
    assert!(!w.follow.get_untracked(), "wheel-up must disengage");
    let held: Vec<String> = (0..10).map(|y| row_dump(term.screen(), y)).collect();
    w.feed.push("m20", FeedItem::text("line 20"));
    settle(&mut driver, &mut app, &mut term);
    for (y, before) in held.iter().enumerate() {
        assert_eq!(
            &row_dump(term.screen(), y as i32),
            before,
            "row {y} moved while disengaged"
        );
    }

    // Wheel back down: re-engages only ON the bottom edge (the first
    // step lands one row short — still disengaged, by spec), and then
    // pins new growth.
    wheel(&mut term, false, 3, 5);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !w.follow.get_untracked(),
        "one row above the bottom is not the bottom"
    );
    wheel(&mut term, false, 3, 5);
    settle(&mut driver, &mut app, &mut term);
    assert!(w.follow.get_untracked(), "bottom edge must re-arm");
    w.feed.push("m21", FeedItem::text("line 21"));
    settle(&mut driver, &mut app, &mut term);
    assert!(
        term.screen()
            .to_text()
            .lines()
            .nth(9)
            .unwrap()
            .contains("line 21"),
        "re-armed follow pins the new tail"
    );

    // Resize (fewer rows): the tail stays pinned when engaged.
    term.push_resize(Size::new(40, 6));
    settle(&mut driver, &mut app, &mut term);
    assert!(w.follow.get_untracked(), "resize must not disengage");
    assert!(
        term.screen()
            .to_text()
            .lines()
            .nth(5)
            .unwrap()
            .contains("line 21"),
        "tail pinned after shrinking to 6 rows:\n{}",
        term.screen().to_text()
    );
    assert_eq!(term.screen().unknown_seq_count(), 0, "modeled traffic only");
}

// ---------------------------------------------------------------------------
// Damage containment + steady-streaming byte cost (measured, printed).
// ---------------------------------------------------------------------------

#[test]
fn streaming_append_damage_stays_inside_the_pane_and_bytes_stay_bounded() {
    let size = Size::new(40, 12);
    let mut term = CaptureTerm::new(size);
    let (mut app, w) = transcript_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    w.feed.push_stream("ans");
    settle(&mut driver, &mut app, &mut term);
    let header_before = row_dump(term.screen(), 0);
    let _ = term.take_bytes();

    // A markdown answer with a fence, streamed in small tokens — the
    // steady-state cadence a model reply produces.
    let answer = "# Answer\n\nStreaming prose that wraps and grows the pane \
                  row by row as tokens arrive.\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\n\
                  - first point\n- second point\n\nclosing paragraph with a tail long \
                  enough to keep wrapping while pinned to the bottom edge.";
    let tokens: Vec<String> = answer
        .chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect())
        .collect();

    let (mut total_bytes, mut max_bytes, mut frames) = (0usize, 0usize, 0usize);
    for tok in &tokens {
        w.feed.stream_append("ans", tok);
        frames += settle(&mut driver, &mut app, &mut term);
        let bytes = term.take_bytes().len();
        total_bytes += bytes;
        max_bytes = max_bytes.max(bytes);
    }
    w.feed.stream_finish("ans");
    settle(&mut driver, &mut app, &mut term);

    println!(
        "steady streaming: {} tokens, {frames} frames, {total_bytes} bytes total, \
         {:.0} bytes/token avg, {max_bytes} max/token",
        tokens.len(),
        total_bytes as f64 / tokens.len() as f64,
    );

    // Containment: the static chrome row never repainted.
    assert_eq!(
        row_dump(term.screen(), 0),
        header_before,
        "header row repainted during streaming"
    );
    assert_eq!(term.screen().unknown_seq_count(), 0, "modeled traffic only");
    // Catastrophic ceiling only: one token must never cost more than a
    // couple of full-screen paints' worth of bytes (screen ~40x12
    // cells; a full styled repaint measures ~4-8 KB here).
    assert!(
        max_bytes < 32 * 1024,
        "one token cost {max_bytes} bytes — streaming is repainting the world"
    );
    // The fence tinted mid-stream (syntax ink on the code ground).
    assert!(
        term.screen().to_text().contains("let x = 1;"),
        "streamed fence must render as code"
    );
}

// ---------------------------------------------------------------------------
// Windowing budget: 10k items inside a measured Scroll.
// ---------------------------------------------------------------------------

/// Draw-call counting canvas (the feed_tests rig, at the public API).
struct CountingCanvas {
    inner: BufferCanvas,
    puts: std::cell::Cell<usize>,
}

impl CountingCanvas {
    fn new(size: Size) -> CountingCanvas {
        CountingCanvas {
            inner: BufferCanvas::new(size),
            puts: std::cell::Cell::new(0),
        }
    }
}

impl Canvas for CountingCanvas {
    fn size(&self) -> Size {
        self.inner.size()
    }
    fn put(
        &mut self,
        p: Point,
        ch: char,
        fg: abstracttui::base::Rgba,
        bg: abstracttui::base::Rgba,
    ) {
        self.puts.set(self.puts.get() + 1);
        self.inner.put(p, ch, fg, bg);
    }
}
impl abstracttui::ui::StyledCanvas for CountingCanvas {}

/// UiTree settle: draw (probes record), fire due timers (probes
/// publish), flush (pins apply), repeat until quiet.
fn settle_tree(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    for _ in 0..4 {
        let fired = run_due_timers(Instant::now());
        flush_effects();
        tree.layout();
        canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

#[test]
fn feed_10k_inside_measured_scroll_draws_only_a_screenful() {
    let size = Size::new(30, 10);
    let mut tree = UiTree::new(size);
    let mut wiring = None;
    let (root, ()) = create_root(|cx| {
        let feed = FeedState::new(cx);
        let follow = cx.signal(true);
        wiring = Some((feed.clone(), follow));
        let view = Scroll::new(Feed::new(&feed).gap(0).view(cx))
            .follow_tail(follow)
            .view(cx);
        tree.mount(cx, view);
    });
    let (feed, follow) = wiring.expect("wiring");

    let push_started = Instant::now();
    batch(|| {
        for i in 0..10_000 {
            feed.push(format!("m{i}"), FeedItem::text(format!("item number {i}")));
        }
    });
    let push_elapsed = push_started.elapsed();
    let _ = settle_tree(&mut tree, size);
    assert!(follow.get_untracked());

    // Pinned at the tail over 10k items: one full paint costs only the
    // window (same 3x-viewport budget as the List/Feed 10k pins).
    let mut canvas = CountingCanvas::new(size);
    let draw_started = Instant::now();
    tree.draw(&mut canvas);
    let draw_elapsed = draw_started.elapsed();
    let cost = canvas.puts.get();
    let budget = (size.w * size.h) as usize * 3;
    println!(
        "10k in scroll: batched appends {push_elapsed:?}, pinned draw {draw_elapsed:?}, \
         {cost} puts (budget {budget})"
    );
    assert!(
        cost <= budget,
        "drawing 10k items in a Scroll cost {cost} puts (budget {budget}) — windowing broke"
    );
    assert!(
        canvas.inner.row_text(9).contains("item number 9999"),
        "pinned to the true tail: {:?}",
        canvas.inner.row_text(9)
    );

    // Home jump: a full-feed scroll repaint (top of 10k) stays windowed.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Home)));
    let canvas = settle_tree(&mut tree, size);
    assert!(canvas.row_text(0).contains("item number 0"));
    assert!(!follow.get_untracked(), "Home is a user scroll: disengaged");
    root.dispose();
}

// ---------------------------------------------------------------------------
// Stream token cost through the whole stack, follow engaged.
// ---------------------------------------------------------------------------

#[test]
fn tail_tokens_behind_closed_blocks_typeset_only_the_open_block() {
    let size = Size::new(40, 8);
    let mut term = CaptureTerm::new(size);
    let (mut app, w) = transcript_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    w.feed.push_stream("s");
    // Close 40 blocks, then measure 60 tail tokens through the loop.
    for i in 0..40 {
        w.feed
            .stream_append("s", &format!("closed paragraph {i}\n\n"));
    }
    settle(&mut driver, &mut app, &mut term);
    let baseline = w.feed.blocks_typeset_total();
    for _ in 0..60 {
        w.feed.stream_append("s", "token ");
        driver.turn(&mut app, &mut term).expect("turn");
    }
    settle(&mut driver, &mut app, &mut term);
    let cost = w.feed.blocks_typeset_total() - baseline;
    println!("60 tail tokens behind 40 closed blocks re-typeset {cost} blocks");
    assert!(
        cost <= 60,
        "tail tokens re-typeset {cost} blocks — the closed prefix thawed"
    );
    assert!(w.follow.get_untracked(), "follow held through the burst");
    assert!(
        term.screen().to_text().contains("token"),
        "tail visible while pinned"
    );
}

// ---------------------------------------------------------------------------
// Measured numbers: 100k appends + one full-feed repaint (printed).
// ---------------------------------------------------------------------------

#[test]
fn measure_100k_appends_and_full_feed_repaint() {
    let size = Size::new(60, 20);
    let mut tree = UiTree::new(size);
    let mut wiring = None;
    let (root, ()) = create_root(|cx| {
        let feed = FeedState::new(cx);
        wiring = Some(feed.clone());
        let view = Scroll::new(Feed::new(&feed).gap(0).view(cx)).view(cx);
        tree.mount(cx, view);
    });
    let feed = wiring.expect("wiring");
    let _ = settle_tree(&mut tree, size); // width known before the flood

    // Batched: the drain shape (one signal wave for the whole burst).
    let started = Instant::now();
    batch(|| {
        for i in 0..100_000 {
            feed.push(
                format!("m{i}"),
                FeedItem::text(format!("log line {i} with payload")),
            );
        }
    });
    flush_effects();
    let append_elapsed = started.elapsed();

    // Unbatched cadence for comparison: one publish per append.
    let started = Instant::now();
    for i in 0..1_000 {
        feed.push(format!("u{i}"), FeedItem::text(format!("unbatched {i}")));
    }
    let unbatched_elapsed = started.elapsed();

    let _ = settle_tree(&mut tree, size);
    let rows = feed.total_rows().get_untracked();

    // One full repaint over the 101k-item feed (no damage scoping).
    let started = Instant::now();
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    let repaint_elapsed = started.elapsed();

    println!(
        "100k batched appends: {append_elapsed:?} ({:.1} µs/item); \
         1k unbatched: {unbatched_elapsed:?} ({:.1} µs/item); \
         extent {rows} rows; full repaint {repaint_elapsed:?}",
        append_elapsed.as_micros() as f64 / 100_000.0,
        unbatched_elapsed.as_micros() as f64 / 1_000.0,
    );
    assert_eq!(feed.len(), 101_000);
    assert!(rows >= 101_000, "every item contributes its row: {rows}");
    // Catastrophic ceilings only (debug builds; CI boxes vary wildly).
    assert!(
        append_elapsed.as_secs() < 30,
        "100k appends took {append_elapsed:?} — append stopped being O(1)"
    );
    assert!(
        repaint_elapsed.as_millis() < 2_000,
        "windowed repaint took {repaint_elapsed:?} — drawing stopped being windowed"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// 0240 follow-up #2: default-styled one-row controls survive overflow.
// ---------------------------------------------------------------------------

#[test]
fn one_row_controls_survive_overflow_pressure() {
    // A 600-row child above a Button and a Separator in a 6-row column:
    // pre-fix, shrink distributed the 596-row overflow across ALL
    // children and the one-row controls landed at zero (invisible —
    // the 0240 modal class). With shrink(0.0) defaults the flexible
    // child absorbs it all and the controls keep their row.
    use abstracttui::theme::default_theme;
    use abstracttui::widgets::{Button, Separator};

    let t = default_theme().tokens;
    let size = Size::new(16, 6);
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        let tall = Element::new()
            .style(LayoutStyle::default().h(600))
            .child(text("filler"))
            .build();
        let view = Element::new()
            .style(LayoutStyle::column().h(6))
            .child(tall)
            .child(Button::new("Save").element(cx, &t).build())
            .child(Separator::horizontal().element(&t).build())
            .build();
        tree.mount(cx, view);
    });
    let canvas = settle_tree(&mut tree, size);
    let dump: Vec<String> = (0..6).map(|y| canvas.row_text(y)).collect();
    assert!(
        dump.iter().any(|r| r.contains("Save")),
        "default-styled Button must keep its row under overflow:\n{dump:#?}"
    );
    assert!(
        dump.iter().any(|r| r.starts_with('─')),
        "default-styled Separator must keep its row under overflow:\n{dump:#?}"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// clear(): the bounded-window rebuild seam (LIVEDATA pairing).
// ---------------------------------------------------------------------------

#[test]
fn clear_rebuilds_a_bounded_window_and_follow_repins() {
    let size = Size::new(30, 6);
    let mut term = CaptureTerm::new(size);
    let (mut app, w) = transcript_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    for i in 0..30 {
        w.feed
            .push(format!("m{i}"), FeedItem::text(format!("old {i}")));
    }
    settle(&mut driver, &mut app, &mut term);

    // A drop-oldest drain rebuild: clear + re-push the retained window.
    batch(|| {
        w.feed.clear();
        for i in 20..30 {
            w.feed
                .push(format!("m{i}"), FeedItem::text(format!("kept {i}")));
        }
    });
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(w.feed.len(), 10);
    let screen = term.screen().to_text();
    assert!(
        screen.contains("kept 29") && !screen.contains("old 19"),
        "window rebuilt in place:\n{screen}"
    );
    assert!(w.follow.get_untracked(), "follow survives the rebuild");
    assert_eq!(term.screen().unknown_seq_count(), 0);
}
