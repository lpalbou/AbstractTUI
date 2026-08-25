//! How many times is one markdown document typeset per frame, and at
//! which widths? A `FenceBlock` answers, because it is a public seam
//! the library already calls once per `layout_doc_all`.
//!
//! WHY THIS FILE EXISTS. `commons#538` fixed the Feed livelock and said
//! plainly what it did NOT fix: the two-width re-typeset underneath it,
//! 66 blocks/chunk against an auto-height control's 1. The sweep that
//! followed — every `.measure(` callback in `src/` — found Feed was the
//! only one writing a signal, so the livelock class has exactly one
//! instance and it is closed. But it turned up something else, and this
//! file is the instrument for it:
//!
//! `MarkdownView` keeps ONE typeset cache and measure and draw share
//! it — `measure_cache` is an `Rc::clone` of `cache`, not a second
//! cache. That sharing is why the control below reads 1 typeset and not
//! 2: with the widths agreeing, measure fills it and the PAINT HITS.
//!
//! It used to hold ONE width. A solver that asks at two per solve —
//! what a flex child beside a fixed sibling produces, and `agora-tui`
//! confirmed at `commons#539` that their every card is exactly that
//! shape — evicted it on every query, so the paint that hits in the
//! control missed in the split:
//!
//! ```text
//! BEFORE (one slot)
//!   control (widths agree)   1 typeset   [99]
//!   flex-beside-fixed        3 typesets  [99, 82, 99]
//!                                          ^   ^   ^
//!                          the offer ─────┘   │   └─ THE PAINT, missing
//!                          the flex fold, ────┘      a slot it filled
//!                          which evicts 99           two calls ago
//!
//! AFTER (WidthCache, two slots, older evicted first)
//!   control (widths agree)   1 typeset   [99]
//!   flex-beside-fixed        2 typesets  [99, 82]     the paint HITS
//! ```
//!
//! The capacity `commons#503` posed as a design choice — (a) two row
//! sets vs (b) a count cache — was answered off this measurement rather
//! than off taste: the distinct width set has exactly TWO members, so
//! two slots recover every available hit and a third would buy nothing
//! while costing another row set on a long body.
//!
//! WHAT THIS DID NOT BUY, and the arms below bound it: a split solve
//! still lays the document out once PER DISTINCT WIDTH — two against
//! the control's one. That is inherent to being asked at two widths and
//! no cache size removes it.
//!
//! THIS IS A COUNTER TEST, not a stopwatch: it reports WHICH WIDTHS the
//! typesetter was asked at and HOW OFTEN, in units that do not move
//! with the machine. `commons#508` convicted a whole class of
//! cross-run subtractions in this package; nothing here subtracts two
//! timings.
//!
//! THE CONTROL IS THE POINT, as in every counter file here. An arm
//! asserting "the fence was asked twice" passes just as well when the
//! document never mounted or the fence was never wired. So the
//! width-agreeing control runs the same document through the same
//! fence in the same process, and every claim below is a COMPARISON
//! between the two, never an absolute read from one.
//!
//! OWNER: tui.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Size};
use abstracttui::testing::CaptureTerm;
use abstracttui::theme::TokenSet;
use abstracttui::ui::StyledCanvas;
use abstracttui::widgets::{FenceBlock, MarkdownView, Scroll};

use abstracttui::prelude::*;

const VIEWPORT: Size = Size::new(100, 30);

/// Records the width of every `layout_doc_all` that reached the fence.
/// `measure` is called once per typeset of the document, so the length
/// of this log IS the typeset count and its contents are the widths.
#[derive(Default)]
struct CountingFence {
    widths: RefCell<Vec<i32>>,
}

impl FenceBlock for CountingFence {
    fn measure(&self, lang: &str, _source: &str, width: i32) -> Option<i32> {
        if lang != "counted" {
            return None; // decline: renders as ordinary code
        }
        self.widths.borrow_mut().push(width);
        Some(3)
    }

    fn draw_row(
        &self,
        _lang: &str,
        _source: &str,
        _row: i32,
        _at: Point,
        _width: i32,
        _tokens: &TokenSet,
        _canvas: &mut dyn StyledCanvas,
    ) {
    }
}

fn config() -> RunConfig {
    RunConfig {
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

/// Long enough to wrap at both queried widths, so the row count is
/// genuinely width-dependent — the property `commons#532` isolated as
/// the one that matters.
fn body() -> String {
    let mut s = String::from("```counted\nblock\n```\n\n");
    for i in 0..40 {
        s.push_str(&format!(
            "Paragraph {i} of a document with enough words on the line to wrap at a \
             typical terminal width and land on more than one row.\n\n"
        ));
    }
    s
}

/// The width-splitting shape: an `Auto`-width flex child beside a
/// fixed-width sibling, so the width the solver OFFERS and the width it
/// DISTRIBUTES differ. Same helper as
/// `tests/feed_typeset_cache_lifetime.rs`.
fn flex_beside_fixed(inner: abstracttui::ui::View) -> abstracttui::ui::View {
    Element::new()
        .style(LayoutStyle::row().width(Dimension::Percent(1.0)))
        .child(
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Auto)
                        .height(Dimension::Auto),
                )
                .child(inner)
                .build(),
        )
        .child(
            Element::new()
                .style(LayoutStyle::default().width(Dimension::Cells(20)))
                .build(),
        )
        .build()
}

/// Mount one document in `shape`, settle, and return the widths the
/// typesetter was asked at.
fn widths_asked(split: bool) -> Vec<i32> {
    let fence: Rc<CountingFence> = Rc::new(CountingFence::default());
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let f = fence.clone();
    app.mount(move |cx| {
        let view = MarkdownView::new(body()).fence_block(f.clone()).view(cx);
        let inner = if split { flex_beside_fixed(view) } else { view };
        Scroll::new(inner).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut turns = 0;
    while turns < 256 {
        turns += 1;
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    assert!(turns < 256, "shape split={split} never reached idle");
    let out = fence.widths.borrow().clone();
    assert!(
        !out.is_empty(),
        "the fence was never asked to measure in shape split={split} — the observer is not \
         wired, so every count below would read zero for the wrong reason"
    );
    out
}

/// The typeset cache serves both widths a split solve asks at, so the
/// paint hits instead of re-laying out what the same frame computed.
#[test]
fn the_two_slot_typeset_cache_serves_both_widths_a_split_solve_asks_at() {
    let control = widths_asked(false);
    let split = widths_asked(true);

    let distinct = |v: &[i32]| {
        let mut d: Vec<i32> = v.to_vec();
        d.sort_unstable();
        d.dedup();
        d
    };
    let (c_widths, s_widths) = (distinct(&control), distinct(&split));
    eprintln!(
        "  control (widths agree)  {} typesets, distinct {c_widths:?}, in order {control:?}",
        control.len()
    );
    eprintln!(
        "  flex-beside-fixed       {} typesets, distinct {s_widths:?}, in order {split:?}",
        split.len()
    );

    // THE CONTROL, asserted rather than assumed: one width means the
    // document reaches idle without ever being asked at a second one.
    // If this ever reads more than one width, the shapes are not
    // discriminating anything and the comparison below is empty.
    assert_eq!(
        c_widths.len(),
        1,
        "the width-agreeing control was itself queried at {c_widths:?}. It is supposed to be \
         the arm where the solver asks once, so nothing below distinguishes the shapes and \
         this file is measuring something other than the width split"
    );

    // THE SPLIT STILL HAPPENS. This is the precondition, not the
    // finding: the fix caches the second width, it does not stop the
    // solver asking at one. If this ever reads a single width the arms
    // below are vacuous — a cache with nothing to hold passes them all.
    assert!(
        s_widths.len() > 1,
        "the flex shape was queried at only {s_widths:?}. The two-width split is what this \
         file is built on and `commons#532` measured it at 99 and 82 — if the solver now \
         asks once, the cache below is being tested against a workload that no longer \
         exercises it, and this arm should be rewritten around the new behaviour"
    );

    // THE FINDING, inverted from the defect it was written against.
    // Recorded before the fix as [99, 82, 99]: the document was typeset
    // at a width it had ALREADY typeset earlier in the same settle,
    // because one slot held one width and the intervening query evicted
    // it. `WidthCache` now keeps the two most recent widths and the
    // sequence is [99, 82] — the paint HITS.
    //
    // Asserted as "no REPEAT" rather than as the literal sequence: the
    // widths are a property of this viewport and would make the test
    // brittle, while "never asked twice at a width it had already done"
    // is exactly what the fix bought.
    let repeats = split.len() - s_widths.len();
    assert_eq!(
        repeats,
        0,
        "the flex shape typeset {} times across only {} distinct widths ({split:?}) — some \
         width was laid out twice, so the cache is being evicted between the queries again. \
         Recorded as [99, 82, 99] before the fix and [99, 82] after. Read \
         `markdown.rs::WidthCache` — most likely it has gone back to one slot, or its miss \
         path is evicting the most-recently-used entry instead of the older one.",
        split.len(),
        s_widths.len()
    );

    // WHAT THE FIX DID NOT BUY, so nobody reads it as free. A split
    // solve still lays the document out ONCE PER DISTINCT WIDTH — two
    // against the agreeing control's one. That is inherent to being
    // asked at two widths, not a cache defect, and no cache size
    // removes it. Bounded rather than deleted: if it ever drops to
    // parity the solver stopped splitting, which is a different (and
    // larger) change than this one.
    assert!(
        split.len() > control.len(),
        "the flex shape typeset {} times against the control's {} — parity. The two-width \
         solve is what makes this shape cost more at all, so parity means the SOLVER \
         changed, not the cache. Verify against `commons#532` before quoting this file.",
        split.len(),
        control.len()
    );
    assert_eq!(
        control.len() - c_widths.len(),
        0,
        "the width-AGREEING control also re-computed a width it had already done \
         ({control:?}). Then the repeat asserted above is not attributable to the width \
         split, and the two-entry conclusion drawn from it does not follow"
    );
}
