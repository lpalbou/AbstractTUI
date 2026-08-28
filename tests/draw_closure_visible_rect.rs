//! A draw closure is handed its whole solved rect — and can now ASK
//! which part of it is on screen.
//!
//! FIELD ORIGIN (commons#367, `agora-tui`). Their chat card paints a
//! selection underlay and a status gutter with
//! `for dy in 0..rect.h { for dx in 0..rect.w { canvas.print(..) } }`.
//! For a selected card whose body is 1600 rows that is **121,752 cells
//! per wheel notch**, in a 30-row viewport, with ~98% of those cells
//! discarded — 8.8% of the notch, measured on their binary
//! (commons#408). Their proposed fix — *clip the loops to the visible
//! rect* — could not be written against the public API, and this file
//! reported that gap.
//!
//! THE AFFORDANCE LANDED: `Canvas::clip_rect()` (commons#419 → #421 →
//! #423, `decision:the-visible-rect-affordance-is-a-canvas-query`).
//! Shape A of two: a query on the canvas, defaulted to the whole
//! surface and overridden by `ClippedCanvas`. Chosen over a second
//! `Rect` argument, which would have broken 194 in-crate call sites to
//! serve the minority of closures that loop.
//!
//! SO THIS FILE INVERTED, as its previous version said it must — but
//! NOT the way that version predicted, and the difference is the point:
//!
//! - it predicted assertion (1) (`the rect is the full box`) would go
//!   RED by construction. That is true for the REJECTED shape and FALSE
//!   for the one that shipped: `clip_rect()` changes what a closure can
//!   ASK, not what it is HANDED. Assertion (1) is unchanged and still
//!   passes — it is now documenting a deliberate choice rather than
//!   reporting a gap.
//! - the trigger moved to `the_clip_a_scrolled_closure_sees_is_tighter
//!   _than_the_screen`, which is the guard against the ONE way this
//!   affordance fails silently (below).
//!
//! THE SILENT FAILURE MODE, named by `agora-tui` at commons#421 before
//! a line was written: a canvas wrapper that FORGETS to override
//! `clip_rect()` inherits "all of me", consumers over-paint, and
//! nothing goes red — correct output, silent cost. That is not
//! hypothetical here: `feed_draw.rs` used `canvas.size()` as its
//! visibility proxy for exactly that reason, since `ClippedCanvas`
//! delegates `size()` to the surface it wraps.
//!
//! OWNER: tui.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rect, Rgba, Size};
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::Element;

use abstracttui::prelude::*;

/// Deliberately taller and wider than the scroll viewport below, so a
/// clip equal to the SCREEN is distinguishable from a clip equal to the
/// scroll's content box. A viewport-sized terminal cannot tell those
/// apart, which is what makes a lost override invisible.
const SCREEN: Size = Size::new(30, 12);
/// Rows of dead space above the scroll: pushes the clip's ORIGIN off
/// the top of the screen, so `clip.y > 0` is a falsifiable claim.
const HEADER_H: i32 = 3;
/// The scroll viewport: strictly shorter than the screen.
const VIEWPORT_H: i32 = 4;
/// Many times the viewport — the gap only bites when the solved box is
/// taller than what is on screen.
const CONTENT_H: i32 = 60;

fn config() -> RunConfig {
    RunConfig {
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

/// What every draw closure in the scrolled tree observed.
#[derive(Default)]
struct Seen {
    /// The `Rect` argument — its whole solved box.
    rects: Vec<Rect>,
    /// What `clip_rect()` answered, at the same instant.
    clips: Vec<Rect>,
    /// Cells a NAIVE loop would walk (the whole box).
    naive_cells: i32,
    /// Cells a CLIPPED loop actually walks.
    clipped_cells: i32,
}

/// Mounts a tall box inside a short scroll viewport, under a header, and
/// records what the draw closure is handed and what it can ask for.
/// ONE builder for every test here, so the shape the pin guards cannot
/// drift from the shape the payoff measures.
fn run_scrolled(seen: &Rc<RefCell<Seen>>) -> String {
    let mut term = CaptureTerm::new(SCREEN);
    let mut app = App::new(SCREEN);
    let s = seen.clone();
    app.mount(move |cx| {
        let s = s.clone();
        let header = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(HEADER_H)),
            )
            .build();
        let body = Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(CONTENT_H)),
            )
            .draw(move |canvas, rect| {
                let clip = canvas.clip_rect();
                {
                    let mut s = s.borrow_mut();
                    s.rects.push(rect);
                    s.clips.push(clip);
                    s.naive_cells += rect.h * rect.w;
                }
                // The consumer's loop, in the shape the affordance
                // exists for: walk the VISIBLE band, not the box.
                let visible = rect.intersect(clip);
                for y in visible.y..visible.bottom() {
                    for x in visible.x..visible.right() {
                        canvas.print(Point::new(x, y), "X", Rgba::WHITE, Rgba::TRANSPARENT);
                        s.borrow_mut().clipped_cells += 1;
                    }
                }
            })
            .build();
        let scroller = Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(VIEWPORT_H)),
            )
            .child(Scroll::new(body).view(cx))
            .build();
        Element::new()
            .style(LayoutStyle::column())
            .child(header)
            .child(scroller)
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    term.screen().to_text()
}

/// THE TRIGGER. `ClippedCanvas` must report the CLIP, not the surface it
/// wraps — the one way this affordance fails silently.
///
/// FALSIFIED BY RUNNING: delete the `clip_rect` override from
/// `impl Canvas for ClippedCanvas` and every assertion below goes red,
/// because the inherited default answers `Rect::from_size(size())` and
/// `size()` delegates to the full terminal. That is exactly the bug the
/// override prevents, and it would otherwise leave the output correct
/// and only the cost wrong.
#[test]
fn the_clip_a_scrolled_closure_sees_is_tighter_than_the_screen() {
    let seen: Rc<RefCell<Seen>> = Default::default();
    let screen = run_scrolled(&seen);
    let seen = seen.borrow();
    let clip = *seen.clips.first().expect("draw closure ran");
    let rect = *seen.rects.first().expect("draw closure ran");

    // The clip starts BELOW the header. A canvas reporting the whole
    // surface would answer y == 0 here.
    assert!(
        clip.y >= HEADER_H,
        "clip starts at y={} but the scroll viewport begins at y={HEADER_H} — a wrapper \
         reporting the SURFACE instead of the CLIP answers y=0. Did `ClippedCanvas` lose \
         its `clip_rect` override? (commons#421)",
        clip.y
    );
    // ...and is shorter than the screen. Same origin: the default
    // answers the full 12 rows.
    assert!(
        clip.h <= VIEWPORT_H,
        "clip is {} rows for a {VIEWPORT_H}-row viewport on a {}-row screen — the default \
         `Rect::from_size(self.size())` would answer {}",
        clip.h,
        SCREEN.h,
        SCREEN.h
    );
    // The premise: this only means anything while the box exceeds the
    // viewport it sits in.
    assert!(
        rect.h > clip.h,
        "the solved box ({} rows) must exceed the clip ({} rows) or this test proves nothing",
        rect.h,
        clip.h
    );
    // NOT VACUOUS: a clip that is tight and WRONG (empty, or off the
    // scroll) would satisfy the bounds above while painting nothing.
    assert!(
        screen.contains('X'),
        "the visible band must actually paint, or a tight-but-wrong clip passes every \
         bound above:\n{screen}"
    );
}

/// THE PAYOFF, in the units the consumer measured: a closure that asks
/// walks the visible band, not the box.
#[test]
fn asking_for_the_clip_collapses_the_cells_a_closure_walks() {
    let seen: Rc<RefCell<Seen>> = Default::default();
    run_scrolled(&seen);
    let seen = seen.borrow();
    let clip = *seen.clips.first().expect("draw closure ran");

    // What the old shape cost, and what the new one costs.
    assert!(
        seen.clipped_cells < seen.naive_cells,
        "clipped walk ({}) must be cheaper than the whole box ({})",
        seen.clipped_cells,
        seen.naive_cells
    );
    // Bounded by what can be SEEN rather than by content length — the
    // property agora-tui pre-committed to (cells must CONVERGE across
    // body sizes, not merely shrink).
    assert!(
        seen.clipped_cells <= clip.h * clip.w,
        "clipped walk ({}) exceeded the clip itself ({}x{})",
        seen.clipped_cells,
        clip.w,
        clip.h
    );
    // ...and bounded ABSOLUTELY, not against the clip the canvas
    // reported. The line above cannot fail when `clip_rect()` is wrong,
    // because it grades the walk against the same wrong answer that
    // produced it — measured: with `ClippedCanvas`'s override deleted
    // this test still PASSED (the clip became the whole screen, the
    // walk grew to match, and the ratio stayed above 5x). An
    // absent-input PASS, in the file whose subject is absent-input
    // PASSes. A closure can never NEED more than the viewport it sits
    // in, so that is the bound that does not move when the answer is
    // wrong.
    assert!(
        seen.clipped_cells <= VIEWPORT_H * SCREEN.w,
        "clipped walk ({}) exceeds the whole scroll viewport ({}x{} = {}) — the closure is \
         painting rows it cannot show, so `clip_rect()` is over-reporting",
        seen.clipped_cells,
        SCREEN.w,
        VIEWPORT_H,
        VIEWPORT_H * SCREEN.w
    );
    let ratio = seen.naive_cells as f64 / seen.clipped_cells.max(1) as f64;
    assert!(
        ratio > 5.0,
        "only {ratio:.1}x saved ({} walked naively / {} clipped) — the premise is that the \
         waste grows with content length",
        seen.naive_cells,
        seen.clipped_cells
    );
    eprintln!(
        "\n  naive walk {} cells; clipped walk {} cells ({ratio:.1}x saved, and the ratio \
         grows with the box)\n",
        seen.naive_cells, seen.clipped_cells
    );
}

/// THE SIGNATURE IS DELIBERATELY UNCHANGED — demoted from a gap report
/// to a record of the choice.
///
/// The rejected shape B passed `(full, visible)` and would have made
/// this assertion red by construction. Shape A leaves the argument
/// alone: a closure that does not care pays nothing and compiles
/// untouched, which is why 194 in-crate call sites did not have to
/// change. If this ever goes red, someone changed the draw signature
/// and `decision:the-visible-rect-affordance-is-a-canvas-query` needs
/// revisiting rather than this line needing a new number.
#[test]
fn a_draw_closure_is_still_handed_its_whole_box() {
    let seen: Rc<RefCell<Seen>> = Default::default();
    run_scrolled(&seen);
    let seen = seen.borrow();
    let rect = *seen.rects.first().expect("draw closure ran");
    assert_eq!(
        rect.h, CONTENT_H,
        "the draw closure still receives its whole solved box; the visible band is a \
         QUESTION it may ask, not a different argument"
    );
}
