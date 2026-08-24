//! Does a frame cost more just because the tree is BIGGER?
//!
//! ```sh
//! cargo test --release --test perf_tree_size -- --ignored --nocapture
//! ```
//!
//! laurent, via `commons#277`: scrolling a long channel is *"better, but
//! not good enough"*, and his hypothesis is windowing — bound the rows
//! that are loaded rather than make each row cheaper. `WidthMemo`
//! already made the layout unit ~60x cheaper at 400 rows and the
//! application is still not good enough, which is evidence against
//! optimising the same axis a third time.
//!
//! So this asks the question that decides whether windowing is an
//! ENGINE fix or an APPLICATION one: **with the viewport fixed and the
//! same 30 rows visible, does one scroll frame get slower as the tree
//! grows behind it?**
//!
//! - Flat → the engine does not care how much is mounted, windowing
//!   buys nothing here, and the cost is above the engine
//!   (`agora-tui`'s half, `commons/claim:msg-276`).
//! - Scaling → the engine is doing per-instance work for rows nobody
//!   can see, and a virtualization path is owed by this crate
//!   (`commons/claim:msg-277-scroll-windowing-experiments`).
//!
//! ## Why this file asserts a RATIO and not a duration
//!
//! `tests/perf_budgets.rs` says it in its own header: its ceilings are
//! host-specific, a change could make layout 50x slower and every one
//! would still pass, and *"catching a 10x slowdown that stays under the
//! ceiling needs a different instrument — a ratchet normalised against a
//! calibration workload measured in the same run, so host speed divides
//! out. That does not exist yet and is not what this file is."*
//!
//! This is that shape. Every size is measured in ONE run on ONE host and
//! only the ratio between them is asserted, so a slow CI box and a fast
//! laptop agree on the answer. The smallest size IS the calibration
//! workload.
//!
//! Sizes reach 6400 because that is the real worst case, not a round
//! number: `agora-tui` reports `BUF_CAP = 8000` retained rows per
//! channel with one card instance per row and no window
//! (`dm:agora-tui--tui#91`), against a 30-row viewport.
//!
//! OWNER: tui.

use std::time::Duration;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::term::Capabilities;
use abstracttui::testing::{sink, time_median, CaptureTerm};
use abstracttui::ui::{text, Element};
use abstracttui::widgets::Scroll;

use abstracttui::prelude::*;

const VIEWPORT: Size = Size::new(100, 30);

/// Row counts swept. The first is the calibration workload; the last is
/// `agora-tui`'s reported order of magnitude.
const SIZES: [usize; 4] = [100, 400, 1600, 6400];

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
    for _ in 0..256 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("loop failed to settle");
}

/// One row shaped like a real card rather than a bare leaf: a header
/// line and a body line inside a column. `agora-tui` flagged this
/// explicitly — their cards are header + action rail + optional body +
/// trail chrome, so "400 rows" is several thousand instances. A sweep
/// built from single `text()` leaves would measure a tree nobody has.
fn card(i: usize) -> abstracttui::ui::View {
    Element::new()
        .style(LayoutStyle::column())
        .child(text(format!("card {i} — header line with a title")))
        .child(text(format!("    body line for card {i}, some prose")))
        .build()
}

/// Median cost of ONE scroll-notch frame at `rows` rows, viewport fixed.
fn frame_cost(rows: usize) -> Duration {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let holder: std::rc::Rc<std::cell::RefCell<Option<Signal<i32>>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        *h.borrow_mut() = Some(offset);
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..rows {
            col = col.child(card(i));
        }
        Scroll::new(col.build()).offset_y(offset).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let offset = holder.borrow().expect("offset signal published");
    let _ = term.take_bytes();

    let m = time_median(&format!("scroll notch @ {rows} rows"), 3, 7, 8, |i| {
        // A wheel notch: two rows, alternating direction so the sweep
        // never runs off the end and every frame is a real change.
        offset.set(if i % 2 == 0 { 2 } else { 0 });
        let turn = driver.turn(&mut app, &mut term).expect("scroll turn");
        sink(turn.emitted);
    });
    m.median
}

/// Slice 3: WHICH phase is charging for the invisible rows?
///
/// The whole-frame sweep proves the engine scales with tree size but
/// not where. A cheaper full-tree pass and a real virtualization API
/// are different products, and this is the number that picks between
/// them: if one phase dominates and is inherently per-instance, only
/// windowing helps; if the cost is spread, a cheaper pass may be
/// enough.
///
/// Driven against a bare `UiTree` rather than the `Driver`, so `layout`
/// and `draw_damaged` can be timed apart instead of inferred by
/// subtraction — the same rule agora-tui and I applied to the memo's
/// before-column.
#[test]
#[ignore = "perf: release-only, run explicitly (--ignored)"]
fn which_phase_charges_for_rows_nobody_can_see() {
    use abstracttui::reactive::{create_root, flush_effects};
    use abstracttui::render::{Cell, Surface};
    use abstracttui::ui::{SurfaceCanvas, UiTree};

    eprintln!("\n  rows      layout      draw   (median of one scroll-shaped change)");
    let mut first: Option<(f64, f64)> = None;
    for &rows in &SIZES {
        let mut tree = UiTree::new(VIEWPORT);
        let (_root, offset) = create_root(|cx| {
            let offset = cx.signal(0i32);
            let mut col = Element::new().style(LayoutStyle::column());
            for i in 0..rows {
                col = col.child(card(i));
            }
            tree.mount(cx, Scroll::new(col.build()).offset_y(offset).view(cx));
            offset
        });
        tree.layout();
        let _ = tree.take_damage();

        let lay = time_median(&format!("layout @ {rows}"), 3, 7, 4, |i| {
            offset.set(if i % 2 == 0 { 2 } else { 0 });
            flush_effects();
            tree.layout();
        });
        let damage = tree.take_damage();
        let mut surface = Surface::new(VIEWPORT, Cell::default());
        let drw = time_median(&format!("draw @ {rows}"), 3, 7, 4, |_| {
            let mut canvas = SurfaceCanvas::new(&mut surface);
            tree.draw_damaged(&mut canvas, &damage);
        });

        let (l, d) = (lay.median.as_secs_f64(), drw.median.as_secs_f64());
        let base = *first.get_or_insert((l, d));
        eprintln!(
            "  {rows:>5}   {:>9.3?}  {:>9.3?}   ({:>5.1}x / {:>5.1}x)",
            lay.median,
            drw.median,
            l / base.0,
            d / base.1
        );
    }
    eprintln!(
        "\n  Reported, not asserted: this locates the cost, and the \
         whole-frame ratio above is the guard that must go green."
    );
    // Honesty note, because the absolutes here do NOT reconcile with the
    // whole-frame test and someone will otherwise quote them as if they
    // did. This harness forces a full re-solve every iteration (set the
    // offset, flush, layout); the Driver's own phase L benefits from
    // partial invalidation, so its per-frame layout is cheaper in
    // absolute terms than the number printed above. What transfers is
    // the SHAPE, which is what this test is for: layout tracks tree
    // size, draw does not. Reading the layout column as "milliseconds
    // per real frame" is the true-of-the-source/false-of-the-artifact
    // error, and the whole-frame test above is the artifact.
    eprintln!(
        "  Absolutes are NOT per-frame costs — this forces a full re-solve \
         each iteration. Compare the RATIOS, not the milliseconds.\n"
    );
}

/// THE experiment. Reports the sweep, then asserts the ratio.
#[test]
#[ignore = "perf: release-only, run explicitly (--ignored)"]
fn one_scroll_frame_should_not_cost_more_just_because_the_tree_is_bigger() {
    let measured: Vec<(usize, Duration)> = SIZES.iter().map(|&n| (n, frame_cost(n))).collect();

    let base = measured[0].1.as_secs_f64();
    eprintln!("\n  rows   frame (median)   vs {} rows", SIZES[0]);
    for (n, d) in &measured {
        eprintln!("  {n:>5}   {:>12.3?}   {:>6.1}x", d, d.as_secs_f64() / base);
    }
    eprintln!(
        "\n  visible rows constant at {} throughout — any growth is work \
         done for rows nobody can see.\n",
        VIEWPORT.h
    );

    if cfg!(debug_assertions) {
        eprintln!("[debug build: ratio printed, not asserted — debug is ~20x here]");
        return;
    }

    let (biggest, worst) = *measured.last().expect("swept at least one size");
    let ratio = worst.as_secs_f64() / base;
    let growth = biggest as f64 / SIZES[0] as f64;

    // The guard, stated as the claim rather than as a number pulled from
    // the run: a frame that repaints the SAME 30 rows should not track
    // the size of the tree behind it. Sub-linear is not good enough to
    // assert — linear-with-a-small-constant would pass that — so this
    // asks for the cost to stay within a small multiple while the tree
    // grows 64x. Falsified by construction: it is red today.
    assert!(
        ratio < 4.0,
        "one scroll frame costs {ratio:.1}x more at {biggest} rows than at {} — \
         the tree grew {growth:.0}x and the visible rows did not change at all, \
         so the engine is doing per-instance work for rows nobody can see",
        SIZES[0]
    );
}
