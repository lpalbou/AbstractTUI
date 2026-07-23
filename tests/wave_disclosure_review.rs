//! Adversarial review of the disclosure wave (first-app 0260 +
//! field-agora 0850): attacks on the builder's five self-named
//! surfaces plus the reviewer's charter — settle-turn storms, extent
//! ownership/disposal edges, pathological header widths, press row
//! math under scroll offsets / fixed boxes / clear+rebuild, and the
//! damage-containment claim re-proven under a FULL cursor model
//! (CUP + relative motion + scroll regions), closing the CUP-only
//! parse hole the builder flagged in `tests/wave_disclosure.rs`.
//!
//! Unit-style harness duplication (mount/settle/click) is the house
//! integration style; only public API is used.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::reactive::{
    create_root, flush_effects, next_timer_deadline, run_due_timers, RootScope,
};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::theme::{default_theme, themes};
use abstracttui::ui::{
    BufferCanvas, KeyEvent, MouseButton, MouseEvent, MouseKind, UiEvent, UiTree,
};
use abstracttui::widgets::{Feed, FeedItem, FeedState};

// ---------------------------------------------------------------------------
// Harness (public-API twin of the widgets' itest_util).
// ---------------------------------------------------------------------------

fn mount(size: Size, build: impl FnOnce(Scope) -> View) -> (RootScope, UiTree) {
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        let view = build(cx);
        tree.mount(cx, view);
    });
    tree.layout();
    (root, tree)
}

fn draw(tree: &mut UiTree, size: Size) -> BufferCanvas {
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    canvas
}

/// One reactive turn WITHOUT timers: flush, layout, draw, consume the
/// damage (as a real frame loop would) — the lens for observing
/// mid-settle state and for a meaningful `has_pending_work`.
fn turn(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let canvas = draw(tree, size);
    let _ = tree.take_damage();
    canvas
}

/// Settle the deferred geometry loop (probe publishes, width fixups).
fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    let mut canvas = turn(tree, size);
    for _ in 0..6 {
        let fired = run_due_timers(Instant::now());
        canvas = turn(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

fn dump(canvas: &BufferCanvas, h: i32) -> Vec<String> {
    (0..h).map(|y| canvas.row_text(y)).collect()
}

fn key(tree: &mut UiTree, k: Key) {
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(k)));
}

fn mouse(tree: &mut UiTree, kind: MouseKind, x: i32, y: i32) {
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(x, y),
        kind,
        mods: Mods::NONE,
    }));
}

fn click(tree: &mut UiTree, x: i32, y: i32) {
    mouse(tree, MouseKind::Move, x, y);
    mouse(tree, MouseKind::Down(MouseButton::Left), x, y);
    mouse(tree, MouseKind::Up(MouseButton::Left), x, y);
}

fn below_line() -> View {
    Element::new()
        .style(LayoutStyle::line(1))
        .child(text("BELOW"))
        .build()
}

fn twelve_lines() -> String {
    (0..12)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Surface 1 — the one-settle-turn cap open.
// ---------------------------------------------------------------------------

/// The handoff claims a re-expand "warm-starts at its last height
/// instead of the cap". Pin it: the cap-flash exists ONLY on the
/// first-ever expand; after fold + re-expand the region must sit at
/// the measured height on the very first frame, no timers.
#[test]
fn warm_start_reexpand_opens_at_measured_height_not_the_cap() {
    let t = default_theme().tokens;
    let size = Size::new(28, 12);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("short", "one\ntwo")
                    .max_body_rows(8)
                    .element(cx, &t)
                    .build(),
            )
            .child(below_line())
            .build()
    });
    let _ = settle(&mut tree, size);

    click(&mut tree, 3, 0); // first expand
    let canvas = turn(&mut tree, size); // one frame, no timers
    let rows = dump(&canvas, size.h);
    assert!(
        rows[9].contains("BELOW"),
        "first expand opens AT the cap (documented flash): {rows:#?}"
    );
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[3].contains("BELOW"),
        "settles to the 2-row content: {rows:#?}"
    );

    click(&mut tree, 3, 0); // fold
    let _ = settle(&mut tree, size);
    click(&mut tree, 3, 0); // re-expand
    let canvas = turn(&mut tree, size); // FIRST frame after re-expand
    let rows = dump(&canvas, size.h);
    assert!(
        rows[3].contains("BELOW"),
        "re-expand must warm-start at the measured 2 rows, not flash the cap: {rows:#?}"
    );
    root.dispose();
}

/// Toggle storm: rapid toggles with no settle between (and a churn
/// round WITH draws, which arms stale probe publishes) must land on
/// exact geometry once the loop quiets.
#[test]
fn toggle_storm_mid_settle_lands_on_true_geometry() {
    let t = default_theme().tokens;
    let size = Size::new(28, 10);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("log", twelve_lines())
                    .max_body_rows(4)
                    .element(cx, &t)
                    .build(),
            )
            .child(below_line())
            .build()
    });
    let _ = settle(&mut tree, size);

    for _ in 0..5 {
        click(&mut tree, 3, 0); // 5 toggles, zero draws between
    }
    let canvas = settle(&mut tree, size); // odd count: unfolded
    let rows = dump(&canvas, size.h);
    assert!(rows[1].contains("line 0"), "{rows:#?}");
    assert!(rows[4].contains("line 3"), "{rows:#?}");
    assert!(rows[5].contains("BELOW"), "capped at 4: {rows:#?}");

    // Churn WITH a draw per toggle: every expansion runs its probe and
    // arms a deferred publish; the fold disposes the generation before
    // the timer fires. Stale publishes hit the durable extent signal
    // and must stay coherent.
    for _ in 0..20 {
        click(&mut tree, 3, 0);
        let _ = turn(&mut tree, size);
    }
    let canvas = settle(&mut tree, size); // 25 toggles: unfolded again
    let rows = dump(&canvas, size.h);
    assert!(
        rows[1].contains("line 0") && rows[4].contains("line 3") && rows[5].contains("BELOW"),
        "churn settles on exact capped geometry: {rows:#?}"
    );
    click(&mut tree, 3, 0); // 26th: folded
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[0].contains('▸') && rows[1].contains("BELOW"),
        "storm settles folded with exact extent: {rows:#?}"
    );
    assert!(!tree.has_pending_work(), "the loop must quiet");
    root.dispose();
}

/// Theme switch MID-SETTLE: the pending probe publish targets a
/// generation the theme rebuild just disposed (the extent signal in a
/// theme-tracked rebuild dies with the card scope's generation). The
/// stale timer must stay inert; the fresh generation re-measures; the
/// controlled fold state survives the rebuild.
#[test]
fn theme_switch_mid_settle_survives_and_remeasures() {
    let size = Size::new(28, 12);
    let holder: Rc<RefCell<Option<Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount(size, move |cx| {
        let folded = cx.signal(true);
        *h.borrow_mut() = Some(folded);
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view_scoped(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .shrink(0.0),
                move |gcx| {
                    // Tracked theme read: set_theme rebuilds this card
                    // from scratch on a fresh generation scope.
                    let t = use_theme(gcx).get().tokens;
                    Element::new()
                        .style(LayoutStyle::column())
                        .child(
                            Disclosure::text("themed", "one\ntwo")
                                .folded(folded)
                                .max_body_rows(6)
                                .element(gcx, &t)
                                .build(),
                        )
                        .child(below_line())
                        .build()
                },
            ))
            .build()
    });
    let folded = holder.borrow().expect("signal");
    let _ = settle(&mut tree, size);

    folded.set(false); // expand...
    let _ = turn(&mut tree, size); // ...one frame: probe armed, unfired
    let other = themes()
        .iter()
        .find(|th| th.id != default_theme().id)
        .expect("a second theme");
    set_theme(other); // rebuild mid-settle
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows.iter().any(|r| r.contains("one")),
        "fold state survived the theme rebuild (controlled): {rows:#?}"
    );
    assert!(
        rows[3].contains("BELOW"),
        "fresh generation re-measured to 2 rows: {rows:#?}"
    );
    set_theme(default_theme());
    root.dispose();
}

/// Dispose the whole card while a probe publish is pending: the
/// outliving timer must stay inert (try-read guard), never panic.
#[test]
fn dispose_with_pending_probe_publish_stays_inert() {
    let t = default_theme().tokens;
    let size = Size::new(28, 10);
    let mut tree = UiTree::new(size);
    let (root, card_cx) = create_root(|cx| {
        let card_cx = cx.child();
        let view = Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("doomed", twelve_lines())
                    .max_body_rows(4)
                    .element(card_cx, &t)
                    .build(),
            )
            .build();
        tree.mount(card_cx, view);
        card_cx
    });
    tree.layout();
    click(&mut tree, 3, 0); // expand
    let _ = turn(&mut tree, size); // probe records, publish armed
    card_cx.dispose(); // extent signal dies with the card
    run_due_timers(Instant::now()); // the armed publish fires now
    flush_effects();
    assert_eq!(tree.instance_count(), 0, "unmounted");
    root.dispose();
}

// ---------------------------------------------------------------------------
// Surface 2 / charter — controlled signal written externally mid-drag.
// ---------------------------------------------------------------------------

/// Fold the card (external signal write) BETWEEN a scrollbar grab and
/// its drag: the capture target is disposed mid-gesture. Later drag /
/// release events must not panic and must not steer anything; a
/// re-expand gets a fresh per-expansion offset and a working wheel.
#[test]
fn external_fold_mid_scrollbar_drag_stays_sane() {
    let t = default_theme().tokens;
    let size = Size::new(28, 10);
    let holder: Rc<RefCell<Option<Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount(size, move |cx| {
        let folded = cx.signal(false);
        *h.borrow_mut() = Some(folded);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("log", twelve_lines())
                    .folded(folded)
                    .max_body_rows(4)
                    .element(cx, &t)
                    .build(),
            )
            .child(below_line())
            .build()
    });
    let folded = holder.borrow().expect("signal");
    let canvas = settle(&mut tree, size);
    assert!(dump(&canvas, size.h)[1].contains("line 0"));

    let bar_x = size.w - 2; // body host right pad = 1, bar column = 1
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 4);
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)[1].contains("line 8"),
        "thumb grab jumped to the bottom: {:#?}",
        dump(&canvas, size.h)
    );

    folded.set(true); // external fold MID-DRAG (button still down)
    let canvas = settle(&mut tree, size);
    assert!(
        !dump(&canvas, size.h).iter().any(|r| r.contains("line")),
        "folded while dragging: body gone"
    );

    // The rest of the gesture lands on a disposed generation.
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), bar_x, 2);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), bar_x, 2);
    let canvas = settle(&mut tree, size);
    assert!(
        !dump(&canvas, size.h).iter().any(|r| r.contains("line")),
        "orphaned drag must not resurrect or steer anything"
    );

    folded.set(false); // re-expand
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[1].contains("line 0"),
        "per-expansion offset is fresh: {rows:#?}"
    );
    mouse(&mut tree, MouseKind::ScrollDown, 4, 2);
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)[1].contains("line 3"),
        "wheel still steers the new generation"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Charter — the folded-zero-cost claim under a mounted timer.
// ---------------------------------------------------------------------------

/// A body-builder interval created on the GENERATION scope must die
/// with the fold: a folded card may not tick, and after the fold no
/// timer entry may bound the idle sleep.
#[test]
fn folded_body_cancels_generation_scoped_intervals() {
    let t = default_theme().tokens;
    let size = Size::new(28, 8);
    let ticks: Rc<Cell<u32>> = Rc::default();
    let sink = ticks.clone();
    let (root, mut tree) = mount(size, move |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("live")
                    .body(move |gcx| {
                        let n = sink.clone();
                        interval(gcx, Duration::from_millis(5), move || {
                            n.set(n.get() + 1);
                        });
                        text("ticking body")
                    })
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    assert_eq!(ticks.get(), 0, "folded at mount: no interval exists");

    click(&mut tree, 3, 0); // expand: the interval arms
    let _ = settle(&mut tree, size);
    let t0 = Instant::now();
    run_due_timers(t0 + Duration::from_millis(7));
    assert!(ticks.get() >= 1, "expanded card ticks");

    click(&mut tree, 3, 0); // fold: generation disposed
    let _ = settle(&mut tree, size);
    let after_fold = ticks.get();
    run_due_timers(t0 + Duration::from_secs(60));
    assert_eq!(
        ticks.get(),
        after_fold,
        "a folded card must not tick — the generation interval dies with the fold"
    );
    assert_eq!(
        next_timer_deadline(),
        None,
        "no timer entry survives the fold (zero idle wakeups)"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Surface 3 — header geometry at pathological widths.
// ---------------------------------------------------------------------------

/// Width 1..3, CJK titles and details: the header must never paint
/// outside its own rect (damage contract §5 — the canvas is NOT
/// clipped to the element), the detail must drop before crushing the
/// title, and the chevron's width assumption stays pinned.
#[test]
fn header_paints_only_inside_its_rect_at_pathological_widths() {
    assert_eq!(abstracttui::text::width("▸"), 1, "chevron width pin");
    assert_eq!(abstracttui::text::width("▾"), 1, "chevron width pin");

    let t = default_theme().tokens;
    for w in 1..=3 {
        let size = Size::new(24, 4);
        let (root, mut tree) = mount(size, |cx| {
            Element::new()
                .style(LayoutStyle::row())
                .child(
                    Element::new()
                        .style(LayoutStyle::column().width(Dimension::Cells(w)))
                        .child(
                            Disclosure::new("日本語タイトル")
                                .detail("九十九")
                                .element(cx, &t)
                                .build(),
                        )
                        .build(),
                )
                .build()
        });
        let canvas = settle(&mut tree, size);
        let row = canvas.row_text(0);
        for (x, ch) in row.chars().enumerate() {
            if x as i32 >= w {
                assert_eq!(
                    ch, ' ',
                    "w={w}: header painted outside its rect at column {x}: {row:?}"
                );
            }
        }
        root.dispose();
    }

    // Tight-but-real width: the CJK detail (6 cells) must drop rather
    // than crush the title below 4 cells; the title ellipsis-truncates.
    let size = Size::new(10, 3);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("日本語タイトル")
                    .detail("九十九")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let row = canvas.row_text(0);
    assert!(
        !row.contains('九'),
        "detail drops whole when the title would fall under 4 cells: {row:?}"
    );
    assert!(row.contains('…'), "wide-glyph title truncates: {row:?}");
    root.dispose();

    // Roomy: the wide detail renders whole, right-aligned, in-rect.
    let size = Size::new(24, 3);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("題名")
                    .detail("九十九")
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let row = canvas.row_text(0);
    assert!(
        row.contains("九十九") && row.contains("題名"),
        "wide detail renders whole beside the title: {row:?}"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Maintainer's literal spec + cap boundaries.
// ---------------------------------------------------------------------------

/// Content EXACTLY at the cap fits: natural height, no scrollbar
/// pixels (the auto-hide boundary is `content <= viewport`).
#[test]
fn cap_equal_content_hides_the_bar_and_takes_natural_height() {
    let t = default_theme().tokens;
    let size = Size::new(28, 10);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("exact", "l0\nl1\nl2\nl3")
                    .initially_folded(false)
                    .max_body_rows(4)
                    .element(cx, &t)
                    .build(),
            )
            .child(below_line())
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[1].contains("l0") && rows[4].contains("l3"),
        "{rows:#?}"
    );
    assert!(rows[5].contains("BELOW"), "no padding row: {rows:#?}");
    assert!(
        !rows.iter().any(|r| r.contains('┃') || r.contains('│')),
        "content == cap fits: the bar must hide: {rows:#?}"
    );
    root.dispose();
}

/// A build body that measures ZERO rows must settle to the 1-row
/// floor, not stand at the full cap forever ("limited to", never
/// "padded to" — the (w, 0) publish is a real measurement, only
/// (0, 0) is the unmeasured sentinel).
#[test]
fn empty_build_body_settles_to_one_row_not_the_cap() {
    let t = default_theme().tokens;
    let size = Size::new(28, 12);
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::new("empty")
                    .body(|_| Element::new().build())
                    .initially_folded(false)
                    .max_body_rows(8)
                    .element(cx, &t)
                    .build(),
            )
            .child(below_line())
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows[2].contains("BELOW"),
        "zero-row body settles to the 1-row floor, not 8 blank cap rows: {rows:#?}"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Surface 4 — press row math.
// ---------------------------------------------------------------------------

/// Press capture: the log handle plus the callback that feeds it.
type PressLog = Rc<RefCell<Vec<(String, i32)>>>;

fn press_log() -> (PressLog, impl FnMut(&str, i32)) {
    let log: PressLog = Rc::default();
    let sink = log.clone();
    (log, move |key: &str, row: i32| {
        sink.borrow_mut().push((key.into(), row))
    })
}

/// Content-sized feed inside a Scroll: presses map through the scroll
/// offset, wrapped rows count as item rows, the LAST row maps, gaps
/// stay silent.
#[test]
fn press_reports_rows_through_a_scrolled_viewport_with_wrapping() {
    let t = default_theme().tokens;
    let size = Size::new(12, 4); // viewport 11 + bar column
    let (log, on_press) = press_log();
    let (root, mut tree) = mount(size, move |cx| {
        let feed = FeedState::new(cx);
        // "aaaa bbbb cccc" wraps at 11 into 2 rows; b is 2 logical rows.
        feed.push("a", FeedItem::text("aaaa bbbb cccc"));
        feed.push("b", FeedItem::text("b one\nb two"));
        Scroll::new(Feed::new(&feed).on_item_press(on_press).view(cx))
            .element(cx, &t)
            .build()
    });
    let _ = settle(&mut tree, size);
    // Rows: a0=0, a1=1 (wrap), gap=2, b0=3, b1=4. Viewport shows 0..3.
    click(&mut tree, 2, 1); // a, wrapped row 1
    click(&mut tree, 2, 2); // gap: silent
    mouse(&mut tree, MouseKind::ScrollDown, 2, 1); // +3, clamps to max_off 1
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // content row 1 = a[1]
    click(&mut tree, 2, 2); // content row 3 = b[0]
    click(&mut tree, 2, 3); // content row 4 = b[1] — the LAST row
    assert_eq!(
        log.borrow().as_slice(),
        &[
            ("a".into(), 1),
            ("a".into(), 1),
            ("b".into(), 0),
            ("b".into(), 1)
        ],
        "press math must survive the scroll offset and wrapping"
    );
    root.dispose();
}

/// Fixed-box feed (explicit layout, taller than content): rows map to
/// the head; the void INSIDE the box past the tail is silent.
#[test]
fn press_maps_the_head_in_a_fixed_box_feed() {
    let size = Size::new(24, 8);
    let (log, on_press) = press_log();
    let (root, mut tree) = mount(size, move |cx| {
        let feed = FeedState::new(cx);
        feed.push("a", FeedItem::text("alpha"));
        feed.push("b", FeedItem::text("b one\nb two"));
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Feed::new(&feed)
                    .on_item_press(on_press)
                    .layout(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Cells(6)),
                    )
                    .view(cx),
            )
            .build()
    });
    let _ = settle(&mut tree, size);
    // Rows: a0=0, gap=1, b0=2, b1=3; box rows 4..5 are void.
    for y in 0..6 {
        click(&mut tree, 2, y);
    }
    assert_eq!(
        log.borrow().as_slice(),
        &[("a".into(), 0), ("b".into(), 0), ("b".into(), 1)],
        "head mapping; gap and in-box void silent"
    );
    root.dispose();
}

/// clear() + rebuild: the old geometry must be forgotten, the new
/// mapping exact (width survives, so re-pushed items map immediately);
/// gap(0) leaves no dead rows between items.
#[test]
fn press_survives_clear_and_rebuild_and_gap_zero() {
    let size = Size::new(24, 8);
    let (log, on_press) = press_log();
    let holder: Rc<RefCell<Option<FeedState>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount(size, move |cx| {
        let feed = FeedState::new(cx);
        feed.push("a", FeedItem::text("alpha"));
        feed.push("b", FeedItem::text("b one\nb two"));
        *h.borrow_mut() = Some(feed.clone());
        Element::new()
            .style(LayoutStyle::column())
            .child(Feed::new(&feed).on_item_press(on_press).view(cx))
            .build()
    });
    let feed = holder.borrow().clone().expect("feed");
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // (a, 0)

    feed.clear();
    assert_eq!(feed.item_at_row(0), None, "cleared: no geometry");
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // empty feed: silent

    feed.push("x", FeedItem::text("xray"));
    feed.push("y", FeedItem::text("y one\ny two"));
    let _ = settle(&mut tree, size);
    click(&mut tree, 2, 0); // (x, 0)
    click(&mut tree, 2, 1); // gap
    click(&mut tree, 2, 2); // (y, 0)
    click(&mut tree, 2, 3); // (y, 1)
    assert_eq!(
        log.borrow().as_slice(),
        &[
            ("a".into(), 0),
            ("x".into(), 0),
            ("y".into(), 0),
            ("y".into(), 1)
        ],
        "rebuild maps the NEW geometry, never the old"
    );

    // gap(0): adjacent items, no dead rows.
    let (log0, on_press0) = press_log();
    let (root0, mut tree0) = mount(size, move |cx| {
        let feed = FeedState::new(cx);
        feed.push("a", FeedItem::text("alpha"));
        feed.push("b", FeedItem::text("b one\nb two"));
        Element::new()
            .style(LayoutStyle::column())
            .child(Feed::new(&feed).gap(0).on_item_press(on_press0).view(cx))
            .build()
    });
    let _ = settle(&mut tree0, size);
    for y in 0..4 {
        click(&mut tree0, 2, y);
    }
    assert_eq!(
        log0.borrow().as_slice(),
        &[("a".into(), 0), ("b".into(), 0), ("b".into(), 1)],
        "gap(0): rows 0..2 map contiguously, row 3 is void"
    );
    root.dispose();
    root0.dispose();
}

// ---------------------------------------------------------------------------
// Surface 5 — damage containment under a FULL cursor model.
// ---------------------------------------------------------------------------

/// Every 0-based row that receives printable bytes or erase/scroll
/// side effects, tracked through CUP/HVP, CUU/CUD/CUF/CUB, CNL/CPL,
/// CHA/VPA, CR/LF/BS, EL/ED, and DECSTBM+SU/SD — the model the
/// builder's CUP-only parse approximates. `start_row` is where the
/// previous frame left the physical cursor: the presenter's trailer
/// parks bottom-left, so a stream captured after any settled frame
/// begins at `h - 1` — relative motion at the stream head is resolved
/// against IT, not against home.
fn touched_rows(bytes: &[u8], start_row: i32) -> BTreeSet<i32> {
    let (mut row, mut touched) = (start_row, BTreeSet::new());
    let mut region: (i32, i32) = (0, i32::MAX);
    let mut i = 0usize;
    let take_params = |bytes: &[u8], mut j: usize| -> (Vec<i32>, usize) {
        let start = j;
        while j < bytes.len()
            && (bytes[j].is_ascii_digit() || matches!(bytes[j], b';' | b':' | b'?' | b' '))
        {
            j += 1;
        }
        let params = String::from_utf8_lossy(&bytes[start..j])
            .trim_start_matches('?')
            .split(';')
            .map(|p| {
                p.split(':')
                    .next()
                    .unwrap_or("")
                    .parse::<i32>()
                    .unwrap_or(0)
            })
            .collect();
        (params, j)
    };
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let (p, j) = take_params(bytes, i + 2);
            let n1 = *p.first().unwrap_or(&0);
            let n = n1.max(1);
            if j < bytes.len() {
                match bytes[j] {
                    b'H' | b'f' => row = n1.max(1) - 1,
                    b'A' => row -= n,
                    b'B' => row += n,
                    b'E' => row += n,
                    b'F' => row -= n,
                    b'd' => row = n1.max(1) - 1,
                    b'S' | b'T' => {
                        // A scroll rewrites every row of the region.
                        let hi = if region.1 == i32::MAX { 512 } else { region.1 };
                        for r in region.0..=hi {
                            touched.insert(r);
                        }
                    }
                    b'r' => {
                        region = (n1.max(1) - 1, p.get(1).copied().unwrap_or(0).max(1) - 1);
                        if p.is_empty() || n1 == 0 {
                            region = (0, i32::MAX);
                        }
                        row = 0; // DECSTBM homes the cursor
                    }
                    b'J' | b'K' => {
                        touched.insert(row); // erase touches this row
                    }
                    _ => {} // SGR, modes, C/D/G — row unaffected
                }
                i = j + 1;
                continue;
            }
            break;
        }
        if b == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            // OSC: skip to BEL or ESC \.
            let mut j = i + 2;
            while j < bytes.len() {
                if bytes[j] == 0x07 {
                    j += 1;
                    break;
                }
                if bytes[j] == 0x1b && j + 1 < bytes.len() && bytes[j + 1] == b'\\' {
                    j += 2;
                    break;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        match b {
            b'\r' => {}
            b'\n' => row += 1,
            0x08 => {}
            0x1b => {} // bare ESC (e.g. ESC \)
            _ if b >= 0x20 && b != 0x7f => {
                touched.insert(row);
            }
            _ => {}
        }
        i += 1;
    }
    touched
}

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

fn drive(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..128 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle");
}

fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

/// Re-proves `toggle_damage_stays_inside_the_cards_band` with the full
/// model: the builder's own harness parses only absolute CUP, but the
/// presenter also moves relatively (CUU/CUD on a shared column, CR,
/// CUF/CUB) — a leak reached through relative motion would have passed
/// the shipped test. Here EVERY touched row is accounted for.
#[test]
fn toggle_damage_containment_under_a_full_cursor_model() {
    let size = Size::new(32, 12);
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
            .child(below_line())
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    drive(&mut driver, &mut app, &mut term);
    let _ = term.take_bytes();

    // Unfold, then fold again: two full repaint waves through the wire.
    term.push_input(&sgr_click(4, 3));
    drive(&mut driver, &mut app, &mut term);
    term.push_input(&sgr_click(4, 3));
    drive(&mut driver, &mut app, &mut term);

    let bytes = term.take_bytes();
    let rows = touched_rows(&bytes, size.h - 1); // parked bottom-left
    assert!(!rows.is_empty(), "the toggles must repaint something");
    assert!(
        rows.iter().all(|&r| r >= 2),
        "damage leaked above the card under the full cursor model \
         (touched 0-based rows {rows:?}): {:?}",
        String::from_utf8_lossy(&bytes)
    );
}

// ---------------------------------------------------------------------------
// Charter — a11y truth and disposal law through the keyboard path.
// ---------------------------------------------------------------------------

/// External signal writes (no gesture) must flip the reported a11y
/// value; a body-less card still toggles its glyph honestly.
#[test]
fn a11y_tracks_external_writes_and_bodyless_glyph_toggles() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let holder: Rc<RefCell<Option<Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount(size, move |cx| {
        let folded = cx.signal(true);
        *h.borrow_mut() = Some(folded);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("Card", "body")
                    .folded(folded)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let folded = holder.borrow().expect("signal");
    let _ = settle(&mut tree, size);
    assert!(tree
        .accessibility_tree_text()
        .contains("button \"Card\" = \"collapsed\""));
    folded.set(false);
    flush_effects();
    assert!(
        tree.accessibility_tree_text()
            .contains("button \"Card\" = \"expanded\""),
        "external write must be reflected without a gesture"
    );
    root.dispose();

    // Body-less: the glyph still reports state.
    let (root, mut tree) = mount(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Disclosure::new("bare").element(cx, &t).build())
            .build()
    });
    let _ = settle(&mut tree, size);
    click(&mut tree, 3, 0);
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(0).contains('▾'),
        "body-less glyph flips honestly"
    );
    root.dispose();
}

/// The disposal law through the KEYBOARD path in controlled mode: the
/// app signal (outliving the card) already holds the new state when
/// `on_toggle` disposes the card's scope.
#[test]
fn enter_path_dispose_in_controlled_mode_writes_before_callback() {
    let t = default_theme().tokens;
    let size = Size::new(28, 6);
    let mut tree = UiTree::new(size);
    let holder: Rc<RefCell<Option<Signal<bool>>>> = Rc::default();
    let h = holder.clone();
    let (root, ()) = create_root(|cx| {
        let folded = cx.signal(true); // app-owned, outlives the card
        *h.borrow_mut() = Some(folded);
        let card_cx = cx.child();
        let view = Disclosure::text("t", "b")
            .folded(folded)
            .on_toggle(move |_| card_cx.dispose())
            .element(card_cx, &t)
            .build();
        tree.mount(card_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab); // focus the header
    key(&mut tree, Key::Enter); // toggle -> write -> dispose
    let folded = holder.borrow().expect("signal");
    assert_eq!(tree.instance_count(), 0, "card unmounted by its callback");
    assert!(
        !folded.get_untracked(),
        "the write landed before the callback disposed the card"
    );
    root.dispose();
}
