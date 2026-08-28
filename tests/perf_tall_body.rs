//! One TALL item, not many items: does a scroll notch cost more when the
//! body the reader is looking at is longer?
//!
//! ```sh
//! cargo test --release --test perf_tall_body -- --ignored --nocapture --test-threads=1
//! ```
//!
//! **`--test-threads=1` is not decoration on this file.** Both stopwatch
//! tests here are `#[ignore]`d, so `--ignored` runs them TOGETHER and
//! cargo runs them on separate threads by default — they then contend
//! for the same cores and each one's first arm absorbs the other's
//! warm-up. Measured on this host: the box-height question arm reads a
//! flat 1.0x serialized (167.9µs → 160.0µs across a 64x box) and a
//! spurious 0.6x-shaped 1.7x spread run in parallel, with nothing about
//! the engine different between the two runs. The per-test warm-up call
//! cannot fix that, because the interference is another thread.
//!
//! `tests/perf_tree_size.rs` answered the other axis — cost as a function
//! of how many rows are MOUNTED — and windowing removes it. `agora-tui`
//! then measured their own tree (`commons#312`) and found the half that
//! windowing cannot touch:
//!
//! > Column windowing skips CARDS, and a long message is ONE card — you
//! > cannot skip the card the reader is reading.
//!
//! Their TALL notch grows **8.29x** from 100 to 1600 body rows. That
//! number contradicts a property this crate documents about `Feed`:
//!
//! > Prefix sums over item heights: first visible item by binary search,
//! > walk until off-screen. […] 100k items cost only the visible rows
//! > per frame.
//!
//! So either the windowed painter is not flat in the length of a single
//! item, or their cost is somewhere else in the card and `Feed` is
//! innocent. Their instrument cannot tell those apart — it measures a
//! card, and a card is a `Feed` plus a header plus a rail plus chrome.
//! This one measures the engine surface alone, which is the only way the
//! answer lands on the right side of the seam.
//!
//! ## What it found
//!
//! `Feed` is **flat** — 1.0x across a 16x body, both when `Scroll`
//! drives it and when it sits in a content-sized box. The documented
//! property holds along the length of one item.
//!
//! **But a third arm reproduces the consumer's number at 7.8x**, and it
//! is the one that matches their code: a `FeedState` constructed
//! *inside* a closure that re-runs. A `FeedState` owns its typeset
//! cache, so a rebuilt closure does not get a stale cache — it gets no
//! cache, and the whole body re-typesets every frame. The cost is not
//! in `Feed`'s painter; it is in the LIFETIME of the state handed to it.
//!
//! That is the difference between "I cannot reproduce your bug" and
//! "here is the shape that produces it", and only the second is worth
//! sending to the seat who has to fix it.
//!
//! Ratios in one run on one host, for the reason `perf_tree_size.rs`
//! states at length: absolutes do not transfer between machines and this
//! fleet has already been bitten by quoting one across a migration.
//!
//! OWNER: tui.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::term::Capabilities;
use abstracttui::testing::{sink, time_median, CaptureTerm};
use abstracttui::widgets::{Feed, FeedItem, FeedState, Scroll};

use abstracttui::prelude::*;

const VIEWPORT: Size = Size::new(100, 30);

/// Body lengths swept, in source lines. The top matches the tall arm of
/// `agora-tui`'s own sweep so the two files can be read side by side.
const BODIES: [usize; 3] = [100, 400, 1600];

/// Declared box heights swept by the bisect below, in cell rows. Reaches
/// 6400 for the reason `perf_tree_size.rs` states: that is the real worst
/// case in the consumer's tree, not a round number.
const BOXES: [i32; 4] = [100, 400, 1600, 6400];

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.sync_output_2026 = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..512 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle");
}

/// A long agent answer: ordinary markdown paragraphs, one per two source
/// lines. Not one giant paragraph — that shape is the streaming path's
/// pathological case (`decision:streaming-cost-is-bounded-by-the-open-block`)
/// and would measure a different thing.
fn long_body(lines: usize) -> String {
    let mut s = String::new();
    for i in 0..lines / 2 {
        s.push_str(&format!(
            "Paragraph {i} of a long answer, with enough words to wrap at a \
             typical terminal width and land on more than one row.\n\n"
        ));
    }
    s
}

/// How the feed is sized by whatever contains it — the variable this
/// file exists to separate.
#[derive(Copy, Clone, PartialEq)]
enum Sizing {
    /// `Scroll` drives it: the feed fills the viewport and paints a
    /// window. What the widget is designed for.
    Scrolled,
    /// The feed sits in a CONTENT-SIZED box, so the solver asks it how
    /// tall it wants to be. `claim:tui-feed-measures-from-paint-not-layout`
    /// records what that costs: an intrinsic measure typesets at the
    /// width the solver offers — the WHOLE body, not a screenful.
    /// A card that says "be as tall as your content" is asking for that
    /// on every solve, which is the mechanism I suspect under
    /// `agora-tui`'s 8.29x.
    AutoHeight,
    /// The consumer's actual shape, from `commons#317`: the `FeedState`
    /// is constructed **inside** the card's body closure, not once at
    /// mount. A `FeedState` owns its typeset cache, so if that closure
    /// re-runs, the cache is not stale — it never existed. Every notch
    /// then re-typesets the whole body from source.
    ///
    /// This arm makes the closure re-run on every notch by reading the
    /// offset inside it. That is the worst case, not a claim about
    /// their code: whether their closure actually re-runs is the thing
    /// they are measuring. What this arm settles is whether the shape
    /// CAN produce the 8.29x, because "I cannot reproduce it" is a much
    /// weaker answer than "here is the shape that does".
    RebuiltPerNotch,
    /// `AutoHeight` with the ONE difference `agora-tui` named at
    /// `commons#341`: the content-sized box also declares a
    /// PERCENTAGE WIDTH, as their card does inside its pane.
    ///
    /// Their instrument killed the rebuild explanation (0 body-closure
    /// runs per notch, 3 turns to settle at both 100 and 1600 rows),
    /// so the cache survives and the cost is still proportional to the
    /// body. Their remaining hypothesis is the one I refuted with the
    /// arm above — an intrinsic measure typesetting the whole mounted
    /// body per solve — and their objection to my refutation is that
    /// my `AutoHeight` arm declared a height and left the width to the
    /// default, which is not the shape they run. A definite width and
    /// a content-sized width can take different solver paths, so
    /// "flat" measured on one says nothing about the other.
    ///
    /// This arm removes that difference and nothing else.
    AutoHeightInPercentWidth,
}

/// Median cost of one scroll notch over a single feed item of `lines`
/// source lines, viewport fixed.
fn notch_cost_sized(lines: usize, sizing: Sizing) -> Duration {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let holder: Rc<RefCell<Option<Signal<i32>>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        *h.borrow_mut() = Some(offset);
        let content = match sizing {
            Sizing::Scrolled => {
                let feed = FeedState::new(cx);
                feed.push("one", FeedItem::markdown(long_body(lines)));
                Feed::new(&feed).view(cx)
            }
            // The card shape: the feed inside a content-sized column, so
            // the solver must ask it for an intrinsic height.
            Sizing::AutoHeight => {
                let feed = FeedState::new(cx);
                feed.push("one", FeedItem::markdown(long_body(lines)));
                Element::new()
                    .style(LayoutStyle::column().height(Dimension::Auto))
                    .child(Feed::new(&feed).view(cx))
                    .build()
            }
            // Their shape exactly: percentage width, content height.
            Sizing::AutoHeightInPercentWidth => {
                let feed = FeedState::new(cx);
                feed.push("one", FeedItem::markdown(long_body(lines)));
                Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Auto),
                    )
                    .child(Feed::new(&feed).view(cx))
                    .build()
            }
            // The consumer's shape: FeedState built INSIDE a closure that
            // re-runs per notch, so its typeset cache never survives a
            // frame. `dyn_view_scoped` gives each generation its own
            // scope, which is what a card body closure has.
            Sizing::RebuiltPerNotch => dyn_view_scoped(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
                move |cx| {
                    offset.get(); // the re-render key: every notch rebuilds
                    let feed = FeedState::new(cx);
                    feed.push("one", FeedItem::markdown(long_body(lines)));
                    Feed::new(&feed).view(cx)
                },
            ),
        };
        Scroll::new(content).offset_y(offset).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let offset = holder.borrow().expect("offset signal published");
    let _ = term.take_bytes();

    // Scroll in the MIDDLE of the body, not at its head: a notch at the
    // top could be flat merely because the binary search starts there.
    let mid = (lines as i32) / 2;
    let m = time_median(&format!("notch @ {lines} body rows"), 3, 7, 8, |i| {
        offset.set(if i % 2 == 0 { mid } else { mid + 2 });
        let turn = driver.turn(&mut app, &mut term).expect("scroll turn");
        sink(turn.emitted);
    });
    m.median
}

/// An empty box of a declared height: content space with no content in
/// it, and exactly one node however tall it is.
fn spacer(h: i32) -> View {
    Element::new()
        .style(LayoutStyle::default().height(Dimension::Cells(h.max(0))))
        .build()
}

/// What varies in the bisect below.
#[derive(Copy, Clone, PartialEq)]
enum BoxKind {
    /// THE QUESTION. A `Scroll` over a column of a DECLARED height, with
    /// one line of text in the middle of it. Three nodes at every size:
    /// the box, a spacer, the text. Nothing to typeset, nothing cached,
    /// no tree to walk — the only thing the sweep changes is how TALL
    /// the mounted box says it is.
    TallAndEmpty,
    /// THE CONTROL, and this arm is why the question arm's answer means
    /// anything. Same harness, same notch, same viewport, but the box is
    /// FILLED — one text node per row — so the mounted tree grows with
    /// the sweep. `perf_tree_size.rs` already proved that shape scales.
    ///
    /// Without it, "flat" is indistinguishable from "this harness cannot
    /// see cost at all": a timing rig that measures nothing prints the
    /// same 1.0x as an engine that charges nothing. The control has to
    /// go UP in the same run for the question arm's flat to be evidence.
    TallAndFull,
}

/// Median cost of one scroll notch deep inside a box of `h` declared
/// rows, viewport fixed.
/// The tree both the timing arm and the screen check mount — ONE
/// function, so a change to the shape cannot leave the vacuity check
/// guarding something the stopwatch is no longer measuring.
fn boxed_column(h: i32, kind: BoxKind) -> View {
    let mut col = Element::new().style(
        LayoutStyle::column()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Cells(h)),
    );
    match kind {
        // The text sits mid-box so the notch has something REAL on
        // screen. A notch over blank space could be flat merely because
        // nothing was damaged — a vacuous pass dressed as a finding.
        // `the_tall_empty_box_really_draws_its_content_at_the_offset_
        // the_notch_uses` is what turns that from a hope into a check.
        BoxKind::TallAndEmpty => {
            col = col.child(spacer(h / 2));
            col = col.child(text("the only line of content in an otherwise empty box"));
            col = col.child(spacer(h - h / 2 - 1));
        }
        BoxKind::TallAndFull => {
            for i in 0..h {
                col = col.child(text(format!("row {i} of a filled box, ordinary prose")));
            }
        }
    }
    col.build()
}

fn notch_cost_boxed(h: i32, kind: BoxKind) -> Duration {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let holder: Rc<RefCell<Option<Signal<i32>>>> = Default::default();
    let hold = holder.clone();
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        *hold.borrow_mut() = Some(offset);
        Scroll::new(boxed_column(h, kind)).offset_y(offset).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let offset = holder.borrow().expect("offset signal published");
    let _ = term.take_bytes();

    let mid = h / 2;
    let m = time_median(&format!("notch @ box of {h} rows"), 3, 7, 8, |i| {
        offset.set(if i % 2 == 0 { mid } else { mid + 2 });
        let turn = driver.turn(&mut app, &mut term).expect("scroll turn");
        sink(turn.emitted);
    });
    m.median
}

/// The screen a box of `h` declared rows produces at content row `at`,
/// through the real driver and a VT parse of the emitted bytes.
fn screen_at_box(h: i32, kind: BoxKind, at: i32) -> String {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let holder: Rc<RefCell<Option<Signal<i32>>>> = Default::default();
    let hold = holder.clone();
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        *hold.borrow_mut() = Some(offset);
        Scroll::new(boxed_column(h, kind)).offset_y(offset).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    holder.borrow().expect("offset signal published").set(at);
    settle(&mut driver, &mut app, &mut term);
    term.screen().to_text()
}

/// The bisect's absent-input case, and it runs in DEBUG, unignored.
///
/// The timing arm above is worthless without this one. Its whole claim
/// is that a notch deep inside a very tall box costs no more than the
/// same notch inside a short one — and a box that draws NOTHING would
/// satisfy that perfectly at every size while measuring nothing at all.
/// Three nodes and a blank screen is the exact decoration shape this
/// package has refused repeatedly; it would print a beautiful 1.0x.
///
/// So: the one line of content must actually reach the screen at the
/// offset the notch lands on, at the largest box in the sweep — where
/// a bug that swallowed deep content would show up first.
#[test]
fn the_tall_empty_box_really_draws_its_content_at_the_offset_the_notch_uses() {
    let biggest = *BOXES.last().expect("swept");
    for &h in &BOXES {
        let screen = screen_at_box(h, BoxKind::TallAndEmpty, h / 2);
        assert!(
            screen.contains("the only line of content"),
            "the tall+empty arm draws nothing at content row {} of a {h}-row box, so its \
             flat timing measures an empty screen rather than a cheap one:\n{screen}",
            h / 2
        );
    }
    // And the control must draw too, or "the control scales" is a
    // statement about mounting cost with no frame behind it.
    let full = screen_at_box(biggest, BoxKind::TallAndFull, biggest / 2);
    assert!(
        full.contains(&format!("row {} of a filled box", biggest / 2)),
        "the control arm is not drawing the band it claims to at content row {}:\n{full}",
        biggest / 2
    );
}

/// THE BISECT, and it is a variable rather than a sixth story.
///
/// Two seats proposed five mechanisms for `agora-tui`'s 8.3x tall-notch
/// growth tonight and the tree refuted all five — a rebuilt `FeedState`,
/// a non-flat `Feed` painter, an intrinsic measure per solve, `rows_ruled`
/// re-pricing per notch, and the selection underlay. Three were mine.
/// Every one died to a COUNT of the event being claimed, not to an
/// argument, and the agreed conclusion was to stop proposing shapes.
///
/// What survives every refutation is one measured fact nobody has
/// explained: applying a 24-row cap to the card's body collapses the
/// notch from milliseconds to ~1µs. The cap provably changes the card's
/// BOX HEIGHT. It provably does not change typesetting, cache lifetime
/// or frame count — all four counted, all four flat.
///
/// So the question left is not "what is the mechanism" but "which side
/// of the seam is the variable on":
///
/// - **Flat** → the engine does not charge for a tall mounted box, the
///   cap's win comes from something inside the card, and this half is
///   answered and closed.
/// - **Scaling** → it is the engine's, and it is the mechanism five
///   hypotheses missed.
///
/// The four arms in the test above cannot answer it. Every one varies
/// CONTENT length inside a `Feed`, which is paint-level virtualized —
/// none isolates a tall BOX from tall CONTENT. This one holds content at
/// three nodes and moves only the declared height.
#[test]
#[ignore = "perf: release-only, run explicitly (--ignored)"]
fn a_scroll_notch_should_not_cost_more_just_because_the_mounted_box_is_tall() {
    // Same warm-up rule as the sweep below: the first arm measured
    // carries one-time process cost and would inflate its own baseline.
    let _ = notch_cost_boxed(BOXES[0], BoxKind::TallAndEmpty);

    let empty: Vec<Duration> = BOXES
        .iter()
        .map(|&h| notch_cost_boxed(h, BoxKind::TallAndEmpty))
        .collect();
    let full: Vec<Duration> = BOXES
        .iter()
        .map(|&h| notch_cost_boxed(h, BoxKind::TallAndFull))
        .collect();

    let (ebase, fbase) = (empty[0].as_secs_f64(), full[0].as_secs_f64());
    eprintln!("\n  box rows    TALL + EMPTY (question)     TALL + FULL (control)");
    for (i, &h) in BOXES.iter().enumerate() {
        eprintln!(
            "  {h:>8}   {:>12.3?} {:>6.1}x   {:>12.3?} {:>6.1}x",
            empty[i],
            empty[i].as_secs_f64() / ebase,
            full[i],
            full[i].as_secs_f64() / fbase
        );
    }
    eprintln!(
        "\n  The question arm mounts THREE nodes at every size; only the \
         declared\n  height moves. The control mounts one node per row. Same \
         viewport ({} rows),\n  same notch, same run.\n",
        VIEWPORT.h
    );

    if cfg!(debug_assertions) {
        eprintln!("[debug build: ratios printed, not asserted]");
        return;
    }

    // THE CONTROL FIRST, because it licenses reading the other column.
    // A rig that cannot see a cost it is known to have prints a flat
    // question arm too, and that flat would be an artefact of the
    // instrument rather than a fact about the engine.
    // 1.5x, not the 2.2x this host measures. A control threshold set at
    // the observed value fails on the first slower machine for a reason
    // that is not the finding, and a control that cries wolf gets
    // deleted by whoever inherits it. What it has to prove is that the
    // rig can SEE growth at all — the question arm spreads 0.004x, so
    // anything two orders of magnitude above that licenses the reading.
    let chi = full.last().expect("swept").as_secs_f64();
    assert!(
        chi / fbase > 1.5,
        "the CONTROL arm grew only {:.1}x from a {}-node tree to a {}-node one — \
         perf_tree_size.rs measures that same shape scaling, so this harness is not \
         seeing cost it is known to have. The question arm's result is UNREADABLE \
         until this goes up again; do not report the bisect from this run",
        chi / fbase,
        BOXES[0],
        BOXES.last().expect("swept")
    );

    let hi = empty.iter().max().expect("swept").as_secs_f64();
    let lo = empty.iter().min().expect("swept").as_secs_f64();
    assert!(
        hi / lo < 2.0,
        "a scroll notch spreads {:.1}x across declared box heights of {} to {} rows \
         while the mounted tree stays at three nodes and the visible rows do not \
         change — the engine charges for BOX HEIGHT itself, which is the variable \
         the 24-row body cap changes and the mechanism five refuted hypotheses \
         missed. This half of commons#296 is the engine's after all.\n\n\
         BEFORE believing that: if this spread is under ~2x and the control above is \
         healthy, re-run with --test-threads=1. Two stopwatch tests on separate \
         threads contend, and the contention lands on whichever arm is measured \
         first — it reads as growth that is not there (see the module header)",
        hi / lo,
        BOXES[0],
        BOXES.last().expect("swept")
    );
}

/// THE question, and it decides which side of the seam the residual
/// cost lives on.
#[test]
#[ignore = "perf: release-only, run explicitly (--ignored)"]
fn a_scroll_notch_over_one_tall_item_should_not_cost_more_as_the_body_grows() {
    // Absorb process warm-up before the sweep. Without it the FIRST arm
    // measured carries one-time cost (649µs against a 358µs steady
    // state, measured), which inflates the baseline and would let real
    // growth hide behind a vacuous ratio.
    let _ = notch_cost_sized(BODIES[0], Sizing::Scrolled);

    let measured: Vec<(usize, Duration)> = BODIES
        .iter()
        .map(|&n| (n, notch_cost_sized(n, Sizing::Scrolled)))
        .collect();
    let auto: Vec<Duration> = BODIES
        .iter()
        .map(|&n| notch_cost_sized(n, Sizing::AutoHeight))
        .collect();
    let pct: Vec<Duration> = BODIES
        .iter()
        .map(|&n| notch_cost_sized(n, Sizing::AutoHeightInPercentWidth))
        .collect();
    let rebuilt: Vec<Duration> = BODIES
        .iter()
        .map(|&n| notch_cost_sized(n, Sizing::RebuiltPerNotch))
        .collect();
    let base = measured[0].1.as_secs_f64();
    let abase = auto[0].as_secs_f64();
    let pbase = pct[0].as_secs_f64();
    let rbase = rebuilt[0].as_secs_f64();

    eprintln!(
        "\n  body rows      SCROLLED           AUTO-HEIGHT        AUTO IN %-WIDTH      \
         FEEDSTATE REBUILT/NOTCH"
    );
    for (i, (n, d)) in measured.iter().enumerate() {
        eprintln!(
            "  {n:>9}   {:>10.3?} {:>5.1}x   {:>10.3?} {:>5.1}x   {:>10.3?} {:>5.1}x   \
             {:>10.3?} {:>6.1}x",
            d,
            d.as_secs_f64() / base,
            auto[i],
            auto[i].as_secs_f64() / abase,
            pct[i],
            pct[i].as_secs_f64() / pbase,
            rebuilt[i],
            rebuilt[i].as_secs_f64() / rbase
        );
    }
    eprintln!(
        "\n  Viewport fixed at {} rows; the notch lands mid-body in every case.\n",
        VIEWPORT.h
    );

    if cfg!(debug_assertions) {
        eprintln!("[debug build: ratio printed, not asserted]");
        return;
    }

    // The reproduction must KEEP reproducing. If this ever goes flat,
    // either the cache started surviving a generation (good, but it
    // invalidates the diagnosis handed to agora-tui) or the arm stopped
    // rebuilding and is measuring nothing at all.
    let rhi = rebuilt.iter().max().expect("swept").as_secs_f64();
    let rlo = rebuilt.iter().min().expect("swept").as_secs_f64();
    assert!(
        rhi / rlo > 4.0,
        "the rebuilt-per-notch arm spread only {:.1}x — it exists to REPRODUCE the \
         consumer's 8.29x by discarding the typeset cache every frame. Flat here means \
         the arm is no longer rebuilding, or the cache now survives a generation; either \
         way the diagnosis built on this arm needs re-deriving",
        rhi / rlo
    );

    // Spread as max/min across the sweep, not last/first: a baseline
    // that happens to run slow would otherwise make growth look like a
    // speed-up (the un-warmed run above printed 0.6x while nothing had
    // improved). Max/min cannot be flattered that way.
    for (label, arm) in [
        (
            "scrolled",
            measured.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
        ),
        ("auto-height", auto.clone()),
        ("auto-height in a %-width parent", pct.clone()),
    ] {
        let hi = arm.iter().max().expect("swept").as_secs_f64();
        let lo = arm.iter().min().expect("swept").as_secs_f64();
        assert!(
            hi / lo < 2.0,
            "the {label} arm spreads {:.1}x across bodies of {} to {} rows with the same \
             {} rows visible — Feed's documented property (\"100k items cost only the \
             visible rows per frame\") does not hold along the LENGTH of a single item, \
             and the residual agora-tui measured at commons#312 is this crate's",
            hi / lo,
            BODIES[0],
            BODIES.last().expect("swept"),
            VIEWPORT.h
        );
    }
}
