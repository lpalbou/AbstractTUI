//! Does a scroll notch re-typeset the body? The counter says, and the
//! answer decides who owns `agora-tui`'s 8.3x.
//!
//! ASKED BY `agora-tui` AT `commons#451`, ask 1, after profiling one
//! notch at 1600 body rows with `/usr/bin/sample`:
//!
//! ```text
//! Feed::elements {closure}          9852 / 10811   91.1%
//!   └─ FeedInner::typeset_entry     8647 / 10811   80.0%
//!        └─ RichText::wrap → Piece::emit → unicode_segmentation
//! ```
//!
//! **80% of a scroll notch is re-typesetting.** `ClippedCanvas::print`
//! is 1.7%, so the paint is not the cost. Their question — and it is
//! about MY counter and MY cache lifetime, which is why it is mine to
//! answer — is whether `blocks_typeset_total()` moves across a notch.
//! `feed_typeset.rs:150` is the branch that would explain everything:
//!
//! ```text
//! if full || entry.segments.is_empty() {
//!     self.blocks_typeset += blocks.len() as u64;
//! ```
//!
//! If a `FeedState` does not survive the closure it is built in, its
//! `segments` are empty on every frame, that guard passes on every
//! frame, and a 1600-row body re-wraps every block on every notch —
//! cost proportional to body length, which is the growth curve nobody
//! could explain.
//!
//! WHY THIS IS A COUNTER TEST AND NOT A STOPWATCH: a timing arm can
//! only say "slower". This says WHICH WORK, in units that cannot drift
//! with the machine — and it is the instrument whose ABSENCE let a
//! wrong refutation stand for hours. `agora-tui` recorded my
//! FeedState-rebuild hypothesis as refuted on `cards::BODY_BUILDS = 0
//! per notch`; that counter is on THEIR card closure, one layer above
//! `Feed::elements`, which is mine. Two closures, one layer apart, and
//! nothing in this crate was counting the lower one. This file is that
//! missing instrument.
//!
//! THE CONTROL IS THE POINT. A test asserting "the counter did not
//! move" passes just as well when the counter is dead, when the feed
//! never mounted, or when no notch was delivered — the absent-input
//! PASS this package keeps finding. So every zero here is paired with
//! a MOVE on the same counter, the same instance and the same run.
//!
//! OWNER: tui.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::testing::CaptureTerm;
use abstracttui::widgets::{Feed, FeedItem, FeedState, Scroll};

use abstracttui::prelude::*;

const VIEWPORT: Size = Size::new(100, 30);
/// Long enough that a per-notch re-typeset is unmistakable in the
/// counter (hundreds of blocks, not a handful).
const BODY_LINES: usize = 400;

/// The feed and the scroll offset, hoisted out of the mount closure so
/// the counter can be read from the test body.
type Published = Rc<RefCell<Option<(FeedState, Signal<i32>)>>>;

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

/// THE ANSWER TO ASK 1: a `FeedState` that outlives the closure it was
/// built in does NOT re-typeset on a scroll notch — and the same
/// counter, on the same instance, moves when the width changes.
#[test]
fn a_scroll_notch_does_not_retypeset_a_feed_whose_state_outlives_its_closure() {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    let holder: Published = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        // The NORMAL shape: state built once at mount, above the
        // closure that re-runs.
        let feed = FeedState::new(cx);
        // MANY entries, not one. `blocks_typeset` counts BLOCKS, and a
        // single `FeedItem::markdown` is ONE block however long its
        // source — so a one-item feed can only ever read 0 or 1 and a
        // zero delta would be nearly unfalsifiable. With 24 entries a
        // re-typeset of even a single one shows up, and a full rebuild
        // reads +24.
        for i in 0..24 {
            feed.push(
                format!("item-{i}"),
                FeedItem::markdown(long_body(BODY_LINES / 24)),
            );
        }
        *h.borrow_mut() = Some((feed.clone(), offset));
        Scroll::new(Feed::new(&feed).view(cx))
            .offset_y(offset)
            .view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    let (feed, offset) = holder.borrow().clone().expect("feed published");

    // The body must actually have been typeset once, or every delta
    // below is zero for the boring reason.
    let after_mount = feed.blocks_typeset_total();
    assert!(
        after_mount > 0,
        "the body was never typeset at all ({after_mount} blocks) — nothing was mounted, so \
         no delta below means anything"
    );

    // EIGHT NOTCHES, in the middle of the body rather than at its head
    // (a notch at the top could be free merely because the search
    // starts there).
    let mid = (BODY_LINES as i32) / 2;
    for i in 0..8 {
        offset.set(mid + i);
        driver.turn(&mut app, &mut term).expect("scroll turn");
    }
    settle(&mut driver, &mut app, &mut term);
    let after_notches = feed.blocks_typeset_total();

    assert_eq!(
        after_notches - after_mount,
        0,
        "EIGHT scroll notches re-typeset {} blocks ({after_mount} → {after_notches}). The \
         typeset cache does not survive a notch, and `feed_typeset.rs:150`'s \
         `entry.segments.is_empty()` guard is passing every frame — this is \
         commons#451's mechanism, at the engine layer.",
        after_notches - after_mount
    );

    // THE CONTROL, same counter, same instance, same run: a width
    // change MUST re-typeset. Without this, the zero above is
    // indistinguishable from a counter that never moves at all.
    term.push_resize(Size::new(VIEWPORT.w - 20, VIEWPORT.h));
    settle(&mut driver, &mut app, &mut term);
    let after_resize = feed.blocks_typeset_total();
    assert!(
        after_resize > after_notches,
        "the counter did not move on a WIDTH CHANGE ({after_notches} → {after_resize}), which \
         must re-wrap every block. So this counter is dead and the zero asserted above proves \
         nothing — fix this control before believing that result."
    );

    eprintln!(
        "\n  blocks typeset: {after_mount} at mount, +{} over 8 notches, +{} on one resize\n",
        after_notches - after_mount,
        after_resize - after_notches
    );
}

/// THE SHAPE SWEEP — `commons#471` ask 1, and the state of the hunt.
///
/// Three measurements now agree instead of competing:
/// - `agora-tui`'s `BODY_BUILDS = 0`/notch — the state persists (their
///   counter was correctly scoped after all; `FeedState::new` sits in
///   the same counted closure with nothing conditional between);
/// - my `+0 over 8 notches` above — the cache survives;
/// - their profile — `typeset_entry` is still **80%** of a notch.
///
/// All three hold at once only if a PERSISTENT state re-typesets
/// anyway, and the one door for that in code either of us has read is
/// mine: `feed_draw.rs:30-38` calls `retypeset_all()` on ANY width
/// delta. My `+24 on one resize` control was that branch firing — I
/// measured the suspect while building a control for something else.
///
/// So: does any card SHAPE make the width move on a scroll notch? The
/// plain scrolled shape does not (asserted above). This sweeps the two
/// shapes that differ from it the way the consumer's card does —
/// content height, and content height inside a percentage width — and
/// REPORTS the per-notch typeset delta for each rather than asserting a
/// number nobody has predicted yet.
///
/// A non-zero delta here IS the reproduction. A row of zeroes says the
/// oscillation is not reachable from shapes this crate can build, and
/// the next place to look is the consumer's pane rather than mine.
#[test]
fn which_card_shapes_retypeset_on_a_scroll_notch() {
    for shape in [
        "scrolled",
        "auto-height",
        "auto-height-in-percent-width",
        // TWO WRITERS OF inner.width, and they can disagree.
        // feed.rs:538 (the MEASURE callback) sets width = avail.w — what
        // the solver OFFERS — and calls retypeset_all(). feed_draw.rs:32
        // sets width = rect.w — what the box was SOLVED to — and calls
        // it too. Whenever those differ the two paths ping-pong and the
        // whole body re-typesets EVERY FRAME with a perfectly persistent
        // FeedState, which is the only thing that fits all three
        // measurements at once. These two shapes exist to split the
        // widths: a flex child beside a fixed sibling, and a padded box.
        "flex-beside-fixed-sibling",
        "padded-container",
    ] {
        let mut term = CaptureTerm::new(VIEWPORT);
        let mut app = App::new(VIEWPORT);
        let holder: Published = Default::default();
        let h = holder.clone();
        app.mount(move |cx| {
            let offset = cx.signal(0i32);
            let feed = FeedState::new(cx);
            for i in 0..24 {
                feed.push(
                    format!("item-{i}"),
                    FeedItem::markdown(long_body(BODY_LINES / 24)),
                );
            }
            *h.borrow_mut() = Some((feed.clone(), offset));
            let inner = Feed::new(&feed).view(cx);
            let content = match shape {
                "scrolled" => inner,
                "auto-height" => Element::new()
                    .style(LayoutStyle::column().height(Dimension::Auto))
                    .child(inner)
                    .build(),
                // The feed shares a row with a fixed-width sibling, so
                // what the solver OFFERS the measure and what the box is
                // SOLVED to are not the same number.
                "flex-beside-fixed-sibling" => Element::new()
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
                    .build(),
                // Padding is deducted between the offer and the solve.
                "padded-container" => Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Auto)
                            .padding(Edges::all(2)),
                    )
                    .child(inner)
                    .build(),
                _ => Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Auto),
                    )
                    .child(inner)
                    .build(),
            };
            Scroll::new(content).offset_y(offset).view(cx)
        })
        .expect("mount");
        let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
        settle(&mut driver, &mut app, &mut term);
        let (feed, offset) = holder.borrow().clone().expect("feed published");
        let at_mount = feed.blocks_typeset_total();
        assert!(
            at_mount > 0,
            "{shape}: nothing was typeset at mount, so the delta below measures nothing"
        );
        // The body is 24 blocks; anything above that at mount is
        // already the ping-pong, before a notch has been delivered.
        let at_mount_body = 24u64;
        // MEASURED, not predicted: of the two shapes built to split the
        // widths, only the flex one actually does. `padded-container`
        // reads +0 — padding is deducted consistently on both paths, so
        // the offer and the solve agree and there is nothing to
        // ping-pong. I expected both to reproduce and one did; the
        // predicate follows the measurement.
        let splits_the_width = matches!(shape, "flex-beside-fixed-sibling");
        let mid = (BODY_LINES as i32) / 2;
        for i in 0..8 {
            offset.set(mid + i);
            driver.turn(&mut app, &mut term).expect("scroll turn");
        }
        settle(&mut driver, &mut app, &mut term);
        let delta = feed.blocks_typeset_total() - at_mount;
        eprintln!("  {shape:<30} {at_mount} at mount, +{delta} over 8 notches");
        // The two width-splitting shapes REPRODUCE the defect; the
        // other three do not. Both directions are asserted, so this
        // sweep goes red if the bug spreads AND when the bug is fixed.
        if splits_the_width {
            assert!(
                delta > 0,
                "{shape}: expected the two-width ping-pong to re-typeset and it did NOT \
                 (+{delta}). If both solver queries now land on the SAME width, or the \
                 typeset cache tolerates two, THE BUG IS FIXED — flip this assertion to \
                 `delta == 0` and delete this branch rather than deleting the test. That \
                 is the signal, not a failure."
            );
            eprintln!(
                "    ^ REPRODUCED: {} blocks per notch against a PERSISTENT FeedState \
                 (body is {at_mount_body} blocks, so that is {:.1} full re-typesets per notch)",
                delta / 8,
                (delta / 8) as f64 / at_mount_body as f64
            );
        } else {
            assert_eq!(
                delta, 0,
                "{shape}: EIGHT notches re-typeset {delta} blocks against a PERSISTENT \
                 FeedState. This shape does not split the two queried widths, so the \
                 ping-pong has reached a new path — find the solver call site that is now \
                 asking this feed at a third width."
            );
        }
    }
}

/// THE OTHER HALF, so the answer to ask 1 is not mistaken for "the
/// engine cannot produce this cost": a `FeedState` built INSIDE a
/// closure that re-runs pays the whole body every time, because its
/// cache never survives to be reused.
///
/// This is the shape `commons#317` reported and the one my
/// `perf_tall_body.rs` rebuilt-per-notch arm already reproduces at 8.0x
/// — the same curve `agora-tui` measured. Here it is in blocks instead
/// of microseconds, which is the unit that names the mechanism.
#[test]
fn a_feedstate_rebuilt_inside_a_closure_retypesets_the_whole_body_every_time() {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    // Every generation's state lands here, so we read the LATEST one.
    let latest: Rc<RefCell<Option<FeedState>>> = Default::default();
    let holder: Rc<RefCell<Option<Signal<i32>>>> = Default::default();
    let (l, h) = (latest.clone(), holder.clone());
    app.mount(move |cx| {
        let offset = cx.signal(0i32);
        *h.borrow_mut() = Some(offset);
        let l = l.clone();
        let content = dyn_view_scoped(LayoutStyle::column().height(Dimension::Percent(1.0)), {
            move |cx| {
                offset.get(); // the re-render key: every notch rebuilds
                let feed = FeedState::new(cx);
                feed.push("one", FeedItem::markdown(long_body(BODY_LINES)));
                *l.borrow_mut() = Some(feed.clone());
                Feed::new(&feed).view(cx)
            }
        });
        Scroll::new(content).offset_y(offset).view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let offset = holder.borrow().expect("offset published");

    let first_generation = latest
        .borrow()
        .clone()
        .expect("feed published")
        .blocks_typeset_total();

    // One notch: the closure re-runs, so a BRAND NEW FeedState appears.
    let mid = (BODY_LINES as i32) / 2;
    offset.set(mid);
    settle(&mut driver, &mut app, &mut term);
    let second = latest.borrow().clone().expect("feed published");
    let second_generation = second.blocks_typeset_total();

    // The new state paid the whole body from scratch. Its counter
    // starts at zero, so a full body's worth of blocks on a state that
    // has existed for one notch IS the re-typeset.
    assert!(
        second_generation > 0,
        "the rebuilt generation typeset nothing — this arm exists to REPRODUCE the cost, so a \
         zero here means the closure did not actually re-run and the arm is measuring nothing"
    );
    // Both generations paid the SAME body. A cache that survived would
    // have made the second one free; instead it starts from zero and
    // re-wraps everything, which is the cost proportional to body
    // length that the profile found.
    assert_eq!(
        second_generation, first_generation,
        "each generation typesets the same body from scratch, so the counts must match \
         ({first_generation} vs {second_generation}) — a difference means this arm is not \
         measuring what it claims"
    );

    eprintln!(
        "\n  rebuilt-per-notch: generation 1 typeset {first_generation} blocks, generation 2 \
         typeset {second_generation} blocks for the SAME body\n"
    );
}

/// DOES STREAMING PAY THE PING-PONG TOO? The design question this
/// answers is which cache shape `FeedInner` should grow.
///
/// `commons#296` is an OPERATOR ask about **streaming long text**, and
/// every measurement in this file so far is a SCROLL notch. Those are
/// different cost profiles and I had been choosing a fix against one
/// while quoting the other:
///
/// - **Scrolling**: content is fixed, so a width-keyed cache is free
///   after the first fill. Both candidate shapes win equally.
/// - **Streaming**: content changes every frame, so a second cache slot
///   at a second width has to be REBUILT every frame. That is where
///   the two shapes diverge, and it is the arm nobody had run.
///
/// So: with 24 static entries plus one streaming entry, feed chunks and
/// watch `blocks_typeset`. The STATIC 24 are the instrument — they are
/// untouched by streaming, so any per-chunk cost proportional to them
/// is `retypeset_all()` firing, not the stream doing its job.
///
/// THE CONTROL IS THE `auto-height` SHAPE, same run, same counter: its
/// two queried widths agree, so whatever it charges per chunk is the
/// HONEST cost of streaming. The flex arm's excess over it is the
/// ping-pong's contribution and nothing else.
///
/// REPORTS rather than asserts a predicted number — nobody has
/// predicted this one, and inventing a threshold before the first run
/// is how a measurement becomes a rubber stamp. The one thing it DOES
/// assert is the comparison that decides the design.
#[test]
fn does_streaming_pay_the_width_ping_pong_as_well_as_scrolling() {
    const CHUNKS: usize = 8;
    let mut per_chunk: Vec<(&str, u64, u64)> = Vec::new();
    let mut idle_turns: Vec<(&str, Option<usize>, Option<usize>)> = Vec::new();

    for shape in ["auto-height", "flex-beside-fixed-sibling"] {
        let mut term = CaptureTerm::new(VIEWPORT);
        let mut app = App::new(VIEWPORT);
        let holder: Published = Default::default();
        let h = holder.clone();
        app.mount(move |cx| {
            let offset = cx.signal(0i32);
            let feed = FeedState::new(cx);
            // The instrument: 24 static entries that streaming never
            // touches. A full rebuild reads +24 of these; an honest
            // stream append reads none of them.
            for i in 0..24 {
                feed.push(
                    format!("item-{i}"),
                    FeedItem::markdown(long_body(BODY_LINES / 24)),
                );
            }
            feed.push_stream("live");
            *h.borrow_mut() = Some((feed.clone(), offset));
            let inner = Feed::new(&feed).view(cx);
            let content = match shape {
                "flex-beside-fixed-sibling" => Element::new()
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
                    .build(),
                _ => Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Auto),
                    )
                    .child(inner)
                    .build(),
            };
            Scroll::new(content).offset_y(offset).view(cx)
        })
        .expect("mount");
        let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
        settle(&mut driver, &mut app, &mut term);
        let (feed, _offset) = holder.borrow().clone().expect("feed published");
        let at_mount = feed.blocks_typeset_total();
        assert!(
            at_mount > 0,
            "{shape}: nothing was typeset at mount, so the per-chunk delta measures nothing"
        );

        // Ordinary prose, one sealed block per chunk — the FAVOURABLE
        // streaming case, so anything charged here is a floor.
        for i in 0..CHUNKS {
            assert!(
                feed.stream_append(
                    "live",
                    &format!(
                        "Chunk {i} of a streamed answer, long enough to wrap at this width \
                         and land on more than one row.\n\n"
                    ),
                ),
                "{shape}: stream_append found no entry named `live` — the arm is measuring \
                 nothing"
            );
            driver.turn(&mut app, &mut term).expect("stream turn");
        }
        // NOT `settle`: whether this shape reaches idle AT ALL is one
        // of the things being measured, so a panic would destroy the
        // observation. Counted, bounded, reported. The budget stays
        // under `CaptureTerm`'s own 10_000-read busy-poll guard so the
        // harness reports the loop instead of the guard tripping.
        let open_idle = turns_to_idle(&mut driver, &mut app, &mut term);
        let delta = feed.blocks_typeset_total() - at_mount;
        // Now SEAL the stream and ask again. This is the discriminator:
        // if the open stream is what keeps the loop alive, sealing it
        // settles; if the shape is, sealing changes nothing.
        assert!(
            feed.stream_finish("live"),
            "{shape}: no `live` entry to finish"
        );
        let sealed_idle = turns_to_idle(&mut driver, &mut app, &mut term);
        eprintln!(
            "  {shape:<30} {at_mount} at mount, +{delta} over {CHUNKS} chunks \
             ({}/chunk) | turns to idle: open {open_idle:?}, sealed {sealed_idle:?}",
            delta / CHUNKS as u64
        );
        idle_turns.push((shape, open_idle, sealed_idle));
        per_chunk.push((shape, delta, delta / CHUNKS as u64));
    }

    // THE REGRESSION GUARD, inverted from the defect it was written
    // against. This arm first recorded a LIVELOCK: the flex shape never
    // reached idle in 2,000 turns, open or sealed, accruing 128,464
    // blocks over the 8 chunks against the control's 8. The cause was
    // `FeedState::publish` bumping the render key unconditionally from
    // the deferred geometry fixup, so a solve that queried two widths
    // scheduled a repaint that re-entered the solve forever. The fixup
    // now repaints only when the extent actually moved
    // (`feed.rs::schedule_geometry_sync`). Both arms are kept and
    // inverted rather than deleted: the numbers below are what the fix
    // bought, and they go red if it is undone.
    let (_, control_open, _) = idle_turns[0];
    let (_, flex_open, flex_sealed) = idle_turns[1];
    assert!(
        control_open.is_some(),
        "the CONTROL shape never reached idle either, so settling is not attributable \
         to the width split — this arm cannot support the claim below and something more \
         basic is wrong with streaming in this harness"
    );
    assert!(
        flex_open.is_some(),
        "THE LIVELOCK IS BACK: the flex shape did not reach idle in {IDLE_BUDGET} turns with \
         the stream OPEN, while the control settled in {control_open:?}. It settled in 2 turns \
         when this was inverted. Do not raise the budget — read \
         `feed.rs::schedule_geometry_sync` and check whether the deferred fixup has gone back \
         to repainting unconditionally."
    );
    assert!(
        flex_sealed.is_some(),
        "the flex shape settled with the stream OPEN but not once SEALED ({flex_sealed:?}). \
         That is a NEW defect and not the one this arm records — the loop would be driven by \
         the sealing path rather than by the geometry fixup — so do not read the note above \
         as an explanation of it."
    );
    eprintln!(
        "\n  NO LIVELOCK: flex reached idle in {flex_open:?} turns open / {flex_sealed:?} \
         sealed, control in {control_open:?}. Pre-fix: never, in {IDLE_BUDGET}.\n"
    );

    let (_, control_total, control_each) = per_chunk[0];
    let (_, _flex_total, flex_each) = per_chunk[1];
    assert!(
        control_total > 0,
        "the CONTROL arm typeset nothing across {CHUNKS} chunks ({control_total}), so the \
         comparison below has no denominator — streaming is not reaching the typesetter \
         at all and this whole arm is dead"
    );
    eprintln!(
        "\n  streaming: control {control_each}/chunk, flex {flex_each}/chunk \u{2014} flex is \
         {:.1}x the control\n",
        flex_each as f64 / control_each as f64
    );

    // WHAT THE FIX DID NOT BUY, stated as a bound so nobody reads the
    // livelock repair as a performance answer. The work is now FINITE
    // and proportional to the chunks, but the flex shape still re-typesets
    // at both queried widths: 66 blocks/chunk against the control's 1 when
    // this was written (down from an unbounded 16,058-and-counting). THAT
    // residue is the width-tolerant cache question, and it is still open.
    //
    // Bounded on BOTH sides on purpose. The upper bound catches a
    // regression toward thrash; the lower bound catches the happier
    // surprise — if the ping-pong is ever actually fixed this goes red
    // and asks for the number to be rewritten rather than letting a
    // stale ceiling silently pass.
    assert!(
        flex_each > control_each,
        "the flex shape now charges {flex_each} blocks/chunk against the control's \
         {control_each} — the two-width re-typeset appears to be GONE, not merely bounded. \
         Recorded at 66 vs 1. Verify against `feed.rs`'s typeset cache and rewrite this arm \
         with the new figure; do not delete it."
    );
    assert!(
        flex_each < control_each * 200,
        "streaming in the two-width shape charged {flex_each} blocks/chunk against the \
         control's {control_each} — a factor of {:.0}, where 66 was recorded after the \
         livelock fix. The work has grown back toward the unbounded regime; re-read the \
         geometry fixup before trusting any cache measurement taken on this tree.",
        flex_each as f64 / control_each as f64
    );
}

/// Turns until the loop reports idle, or `None` if it never does
/// within [`IDLE_BUDGET`]. Deliberately not `settle`: a shape that
/// cannot settle is a RESULT here, and panicking would erase it.
fn turns_to_idle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) -> Option<usize> {
    (0..IDLE_BUDGET).find(|_| driver.turn(app, term).expect("turn").idle)
}

/// Under `CaptureTerm`'s own 10_000-consecutive-read busy-poll guard,
/// so a non-settling shape is reported by this file rather than
/// tripping the harness.
const IDLE_BUDGET: usize = 2_000;

/// WHY the flex shape has no fixed point — the bisect that named the
/// mechanism, and it refutes my own `commons#522` guess.
///
/// `#522` reported the livelock and said the amplifier was unexplained.
/// `agora-tui` at `commons#529` then showed a growing body in the same
/// shape converging in their client, and flagged the difference as
/// *"something the `StreamEntry` leaves behind"*. **Both of us were
/// looking at the wrong ingredient.** Measured here, one variable at a
/// time, all in the flex-beside-fixed-sibling shape:
///
/// ```text
///   statics-only                  settles           +0 blocks
///   stream entry, never appended  settles           +0
///   stream, ONE append            NEVER settles     +75,051
///   stream, one append + finish   NEVER settles     +75,051
///   markdown replaced 8x          NEVER settles     +75,408   <- no stream at all
///   stream append, NO statics     NEVER settles     +3,003    <- no other entries
///   SHORT append (cannot rewrap)  settles in 1      +5
/// ```
///
/// So it was not the stream: a plain `FeedItem::markdown` swapped
/// through `update()` did it too. Not the other entries: one item alone
/// did it. Not open-vs-sealed: finishing did not rescue it. **It was any
/// content mutation whose WRAPPED ROW COUNT DIFFERS BETWEEN THE TWO
/// WIDTHS THE SOLVER QUERIES.**
///
/// # The mechanism the last row pinned
///
/// In this shape the solver asks the measure callback at two widths per
/// solve (99 and 82 in the harness — `place_absolute`'s intrinsic query
/// and the flex-basis fold). Wrapped text has a DIFFERENT total at each,
/// so the callback's `rows != total` test fires at whichever query is
/// not the one `rows` currently holds — every frame, unavoidably. The
/// scheduled fixup then bumped the render key UNCONDITIONALLY, the
/// repaint re-entered the solve, and round it went.
///
/// The last variant was the falsification, and it is why this was a
/// diagnosis rather than a story: text short enough to occupy the same
/// rows at 99 and at 82 gives ONE total, one publication, and idle in a
/// single turn — same shape, same mutation, same code path.
///
/// # The fix, and what it left behind
///
/// `schedule_geometry_sync`'s fixup now repaints only when the extent
/// ACTUALLY MOVED. That is enough: by the time it runs, `inner.width` is
/// the width the feed was last typeset at, so the total it reads is a
/// function of one width. No cache change was needed to end the loop.
///
/// The numbers this file now records:
///
/// ```text
///   stream, ONE append            settles in 2      +101 blocks   (was: never, +75,051)
///   markdown replaced 8x          settles in 2      +458          (was: never, +75,408)
///   stream append, NO statics     settles in 2      +5            (was: never,  +3,003)
///   SHORT append (cannot rewrap)  settles in 1      +5            (unchanged)
/// ```
///
/// **The discriminator survives the fix and is still the guard**: a
/// re-wrapping mutation costs one extra turn and a re-typeset at the
/// second width; one that cannot re-wrap costs neither. That residual
/// two-width re-typeset is the width-tolerant cache question, which this
/// fix does NOT answer.
#[test]
fn any_mutation_that_rewraps_between_the_two_queried_widths_removes_the_fixed_point() {
    // (name, statics, stream entry?, appends, finish?, updates, short text?)
    let variants: [(&str, usize, bool, usize, bool, usize, bool); 7] = [
        ("statics-only", 24, false, 0, false, 0, false),
        ("stream-entry-never-appended", 24, true, 0, false, 0, false),
        ("stream-1-append", 24, true, 1, false, 0, false),
        ("stream-1-append-finished", 24, true, 1, true, 0, false),
        ("markdown-replaced-8x", 24, false, 0, false, 8, false),
        ("stream-append-no-statics", 0, true, 1, false, 0, false),
        ("SHORT-append-cannot-rewrap", 0, true, 1, false, 0, true),
    ];
    let mut idle_after: Vec<(&str, Option<usize>)> = Vec::new();

    for (name, statics, stream, appends, finish, updates, short) in variants {
        let mut term = CaptureTerm::new(VIEWPORT);
        let mut app = App::new(VIEWPORT);
        let holder: Published = Default::default();
        let h = holder.clone();
        app.mount(move |cx| {
            let offset = cx.signal(0i32);
            let feed = FeedState::new(cx);
            for i in 0..statics {
                feed.push(
                    format!("item-{i}"),
                    FeedItem::markdown(long_body(BODY_LINES / 24)),
                );
            }
            if stream {
                feed.push_stream("live");
            }
            if updates > 0 {
                feed.push("grow", FeedItem::markdown(long_body(20)));
            }
            *h.borrow_mut() = Some((feed.clone(), offset));
            Scroll::new(flex_beside_fixed(Feed::new(&feed).view(cx)))
                .offset_y(offset)
                .view(cx)
        })
        .expect("mount");
        let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
        assert!(
            turns_to_idle(&mut driver, &mut app, &mut term).is_some(),
            "{name}: the shape did not even settle at MOUNT, so nothing below is \
             attributable to the mutation"
        );
        let (feed, _) = holder.borrow().clone().expect("feed published");
        let at_mount = feed.blocks_typeset_total();

        let mut grown = String::new();
        for i in 0..appends.max(updates) {
            if appends > 0 {
                // THE DISCRIMINATOR lives here: `short` text occupies the
                // SAME rows at both queried widths, so `total_rows` is one
                // value however the solver asks.
                let chunk = if short {
                    format!("Chunk {i}.\n\n")
                } else {
                    format!(
                        "Chunk {i} of a streamed answer, long enough to wrap here and \
                         land on more than one row.\n\n"
                    )
                };
                assert!(
                    feed.stream_append("live", &chunk),
                    "{name}: no `live` entry"
                );
            }
            if updates > 0 {
                grown.push_str(&format!(
                    "Chunk {i} of a replaced answer, long enough to wrap here and land \
                     on more than one row.\n\n"
                ));
                assert!(
                    feed.update("grow", FeedItem::markdown(grown.clone())),
                    "{name}: no `grow` entry"
                );
            }
            driver.turn(&mut app, &mut term).expect("turn");
        }
        if finish {
            assert!(
                feed.stream_finish("live"),
                "{name}: no `live` entry to finish"
            );
        }
        let after = turns_to_idle(&mut driver, &mut app, &mut term);
        eprintln!(
            "  {name:<30} idle after {after:?}, +{} blocks",
            feed.blocks_typeset_total() - at_mount
        );
        idle_after.push((name, after));
    }

    let get = |n: &str| -> Option<usize> {
        idle_after
            .iter()
            .find(|(name, _)| *name == n)
            .unwrap_or_else(|| panic!("variant {n} missing"))
            .1
    };
    // The load-bearing rows. Every one is asserted in the direction
    // that makes the diagnosis falsifiable.
    assert!(
        get("statics-only").is_some(),
        "the flex shape does not settle even WITHOUT a mutation, so the mutation is not \
         the variable and this whole bisect is measuring something else"
    );
    let one_append = get("stream-1-append").unwrap_or_else(|| {
        panic!(
            "THE LIVELOCK IS BACK: one re-wrapping append did not settle in {IDLE_BUDGET} \
             turns. It settled in 2 when this arm was inverted. Read \
             `feed.rs::schedule_geometry_sync` — the deferred fixup has most likely gone \
             back to repainting whether or not the extent moved."
        )
    });
    let replaced = get("markdown-replaced-8x").unwrap_or_else(|| {
        panic!(
            "a plain markdown replacement no longer settles while the streamed one does. \
             That would mean the mechanism IS stream-specific after all — which is exactly \
             what this arm was built to refute, so re-read it before trusting the header"
        )
    });
    assert_eq!(
        one_append, replaced,
        "the streamed append and the plain markdown replacement settle in DIFFERENT turn \
         counts ({one_append} vs {replaced}). They are the same mechanism in this header's \
         account, so a split here means the account is incomplete"
    );
    let short = get("SHORT-append-cannot-rewrap").unwrap_or_else(|| {
        panic!(
            "text that occupies the SAME rows at both queried widths fails to settle. The \
             diagnosis in this test's header is then WRONG: the loop is not driven by \
             `total_rows` differing between the two widths, and the mechanism is \
             unexplained again"
        )
    });
    // THE DISCRIMINATOR, and the reason the fix is understood rather
    // than merely observed. A mutation that re-wraps between the two
    // queried widths costs one publication the short one never owes.
    // Collapse this gap and the header's account of WHY is unsupported,
    // even though nothing livelocks — which is the failure this file
    // exists to catch.
    assert!(
        one_append > short,
        "a re-wrapping append ({one_append} turns) no longer costs more than one that \
         cannot re-wrap ({short}). Recorded at 2 vs 1. The livelock may still be fixed, but \
         the MECHANISM in this header is then unsupported by its own discriminator: \
         re-measure before quoting it"
    );
}

/// The width-splitting shape, shared by the arms that need it: a feed
/// in an `Auto`-width flex child beside a fixed-width sibling, so the
/// width the solver offers and the width it distributes differ.
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
