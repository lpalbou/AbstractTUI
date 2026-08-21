//! ThinkingFold tests (split file, `#[path]`-included as
//! `thinking_fold::tests`): folded-by-default, streaming appends +
//! the data-driven indicator, last-wins completion, post-complete
//! fragment refusal, fold-cycle state survival, transcript-stack /
//! panel composition, the body cap under a 50KB thought, a11y, and
//! the zero-idle claim through the real `Driver`.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::base::{Point, Size};
use crate::layout::{Dimension, Style as LayoutStyleFull};
use crate::reactive::{flush_effects, run_due_timers};
use crate::theme::default_theme;
use crate::ui::{text, BufferCanvas, Element, UiTree};
use crate::widgets::itest_util::{click, key, mount_widget, render};
use crate::widgets::{Feed, FeedItem, FeedState};

/// Settle the deferred geometry loop (feed width fixup + scroll extent
/// probe) — the disclosure_tests recipe.
fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let mut canvas = render(tree, size);
    for _ in 0..4 {
        let fired = run_due_timers(std::time::Instant::now());
        flush_effects();
        tree.layout();
        canvas = render(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

fn dump(canvas: &BufferCanvas, h: i32) -> Vec<String> {
    (0..h).map(|y| canvas.row_text(y)).collect()
}

/// The Dots indicator frames the header may carry while streaming.
fn is_dots_frame(c: char) -> bool {
    SpinnerKind::Dots.frames().contains(&c)
}

fn header_frame(canvas: &BufferCanvas) -> Option<char> {
    canvas.row_text(0).chars().find(|&c| is_dots_frame(c))
}

/// Mount one fold (default folded) and hand the state out.
fn mount_fold(size: Size) -> (ThinkingFoldState, crate::reactive::RootScope, UiTree) {
    let holder: Rc<RefCell<Option<ThinkingFoldState>>> = Default::default();
    let h = holder.clone();
    let t = default_theme().tokens;
    let (root, tree) = mount_widget(size, move |cx| {
        let state = ThinkingFoldState::new(cx);
        *h.borrow_mut() = Some(state.clone());
        Element::new()
            .style(LayoutStyleFull::column())
            .child(ThinkingFold::new(&state).element(cx, &t).build())
            .child(
                Element::new()
                    .style(LayoutStyleFull::line(1))
                    .child(text("BELOW"))
                    .build(),
            )
            .build()
    });
    let state = holder.borrow().clone().expect("state");
    (state, root, tree)
}

// ---------------------------------------------------- fold + streaming

#[test]
fn folded_by_default_muted_title_and_click_expands() {
    let size = Size::new(40, 10);
    let (state, root, mut tree) = mount_fold(size);
    state.append("hidden reasoning body");
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(
        top.contains('▸') && top.contains("Thinking"),
        "folded header: {top:?}"
    );
    assert!(
        !dump(&canvas, size.h).iter().any(|r| r.contains("hidden")),
        "folded by default (the operator ruling): the body never renders"
    );
    // The muted-title knob: the 'T' of "Thinking" wears text_muted
    // (glyph at x1, gap, title from x3).
    let t = default_theme().tokens;
    let ink = canvas.cell(Point::new(3, 0)).expect("title cell").1;
    assert_eq!(ink, t.text_muted, "title renders muted");

    click(&mut tree, 4, 0);
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).contains('▾'), "expanded");
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("hidden reasoning body")),
        "expanded: the body renders:\n{:#?}",
        dump(&canvas, size.h)
    );
    root.dispose();
}

#[test]
fn streamed_appends_advance_the_indicator_data_driven() {
    let size = Size::new(40, 6);
    let (state, root, mut tree) = mount_fold(size);
    let canvas = settle(&mut tree, size);
    assert_eq!(
        header_frame(&canvas),
        None,
        "no indicator before the first fragment"
    );
    state.append("alpha ");
    let canvas = settle(&mut tree, size);
    let first = header_frame(&canvas).expect("indicator visible while streaming");
    state.append("beta ");
    let canvas = settle(&mut tree, size);
    let second = header_frame(&canvas).expect("still streaming");
    assert_ne!(
        first, second,
        "the frame advances PER APPEND — data-driven animation"
    );
    assert!(state.is_streaming());
    state.complete("alpha beta");
    let canvas = settle(&mut tree, size);
    assert_eq!(
        header_frame(&canvas),
        None,
        "completion clears the indicator"
    );
    assert!(!state.is_streaming());
    assert!(state.is_completed());
    root.dispose();
}

#[test]
fn detail_slot_carries_the_token_count_through_both_phases() {
    let size = Size::new(44, 6);
    let (state, root, mut tree) = mount_fold(size);
    state.append("thinking… ");
    state.set_detail("213 tk");
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(top.contains("213 tk"), "detail beside the frame: {top:?}");
    assert!(
        header_frame(&canvas).is_some(),
        "frame + detail while streaming: {top:?}"
    );
    state.complete("thinking done");
    state.set_detail("1.2k tk");
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(
        top.contains("1.2k tk"),
        "detail survives completion: {top:?}"
    );
    assert_eq!(header_frame(&canvas), None, "frame gone: {top:?}");
    root.dispose();
}

#[test]
fn complete_replaces_accumulated_fragments_last_wins() {
    let size = Size::new(44, 10);
    let (state, root, mut tree) = mount_fold(size);
    state.append("draft alpha ");
    state.append("draft beta");
    click(&mut tree, 4, 0); // expand
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("draft alpha")),
        "fragments render while streaming"
    );
    // The trailing aggregate REPLACES — providers may recompose (the
    // aggregate here deliberately differs from the fragment concat).
    state.complete("final omega text");
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows.iter().any(|r| r.contains("final omega text")),
        "aggregate rendered: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("draft")),
        "fragments fully replaced (LAST WINS): {rows:#?}"
    );
    root.dispose();
}

#[test]
fn fragments_after_complete_are_ignored() {
    let size = Size::new(44, 10);
    let (state, root, mut tree) = mount_fold(size);
    state.append("early ");
    state.complete("the complete text");
    assert!(
        !state.append("late straggler"),
        "append after complete refuses (the pinned decision)"
    );
    click(&mut tree, 4, 0);
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(rows.iter().any(|r| r.contains("the complete text")));
    assert!(
        !rows.iter().any(|r| r.contains("late straggler")),
        "a late fragment never corrupts the aggregate: {rows:#?}"
    );
    assert_eq!(
        header_frame(&canvas),
        None,
        "refused appends never re-animate the indicator"
    );
    root.dispose();
}

#[test]
fn double_complete_last_wins_again() {
    let size = Size::new(44, 8);
    let (state, root, mut tree) = mount_fold(size);
    state.append("stream ");
    state.complete("first aggregate");
    state.complete("second aggregate");
    click(&mut tree, 4, 0);
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    assert!(
        rows.iter().any(|r| r.contains("second aggregate")),
        "{rows:#?}"
    );
    assert!(!rows.iter().any(|r| r.contains("first aggregate")));
    root.dispose();
}

#[test]
fn state_survives_fold_cycles() {
    let size = Size::new(44, 10);
    let (state, root, mut tree) = mount_fold(size);
    state.append("durable thought");
    click(&mut tree, 4, 0); // expand
    let canvas = settle(&mut tree, size);
    assert!(dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("durable thought")));
    click(&mut tree, 4, 0); // fold
    let canvas = settle(&mut tree, size);
    assert!(!dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("durable thought")));
    click(&mut tree, 4, 0); // re-expand: no re-typeset, same content
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("durable thought")),
        "the typeset body survives fold cycles"
    );
    root.dispose();
}

type StateAndFold = Option<(ThinkingFoldState, crate::reactive::Signal<bool>)>;

#[test]
fn controlled_fold_signal_is_respected() {
    let size = Size::new(44, 8);
    let holder: Rc<RefCell<StateAndFold>> = Default::default();
    let h = holder.clone();
    let t = default_theme().tokens;
    let (root, mut tree) = mount_widget(size, move |cx| {
        let state = ThinkingFoldState::new(cx);
        let folded = cx.signal(false); // app policy: newest expanded
        *h.borrow_mut() = Some((state.clone(), folded));
        Element::new()
            .style(LayoutStyleFull::column())
            .child(
                ThinkingFold::new(&state)
                    .folded(folded)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let (state, folded) = holder.borrow().clone().expect("state");
    state.append("visible from birth");
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("visible from birth")),
        "bound signal wins over the folded default"
    );
    folded.set(true); // the 0850 collapse-all policy write
    let canvas = settle(&mut tree, size);
    assert!(!dump(&canvas, size.h)
        .iter()
        .any(|r| r.contains("visible from birth")));
    root.dispose();
}

// ------------------------------------------------- size + composition

#[test]
fn a_50kb_thought_stays_capped_and_scrolls() {
    let size = Size::new(44, 14);
    let (state, root, mut tree) = mount_fold(size);
    // ~50KB of markdown lines, streamed in chunks (the honest shape).
    let mut line = 0usize;
    let mut bytes = 0usize;
    while bytes < 50_000 {
        let chunk = format!("reasoning line {line} of the long thought\n");
        bytes += chunk.len();
        line += 1;
        state.append(&chunk);
    }
    click(&mut tree, 4, 0); // expand
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    let body_rows = rows.iter().filter(|r| r.contains("reasoning line")).count();
    assert!(
        body_rows <= 8,
        "the default cap holds under 50KB ({body_rows} body rows): {rows:#?}"
    );
    assert!(
        rows[9].contains("BELOW"),
        "content below the card stays reachable: {rows:#?}"
    );
    // Scrollbar thumb engaged (content far beyond the cap).
    let bar_col = size.w - 2;
    let bar: String = (1..9)
        .filter_map(|y| canvas.cell(Point::new(bar_col, y)).map(|c| c.0))
        .collect();
    assert!(bar.contains('█'), "thumb visible on overflow: {bar:?}");
    root.dispose();
}

#[test]
fn composes_in_a_transcript_stack_and_inside_a_panel() {
    // The honest placement contract: feed blocks are draw-only
    // (first-app/0280 — a widget cannot ride a FeedItem), so the fold
    // sits BESIDE feed segments in the turn column — inside a framed
    // panel here, the transcript shape of examples/reasoning.rs.
    let size = Size::new(50, 14);
    let holder: Rc<RefCell<Option<ThinkingFoldState>>> = Default::default();
    let h = holder.clone();
    let t = default_theme().tokens;
    let (root, mut tree) = mount_widget(size, move |cx| {
        let feed = FeedState::new(cx);
        feed.push("q", FeedItem::markdown("**you** — why is the sky blue?"));
        let state = ThinkingFoldState::new(cx);
        *h.borrow_mut() = Some(state.clone());
        let answer = FeedState::new(cx);
        answer.push("a", FeedItem::text("Rayleigh scattering."));
        super::super::Block::new()
            .title("turn")
            .layout(LayoutStyleFull::column().width(Dimension::Percent(1.0)))
            .child(Feed::new(&feed).gap(0).element(cx, &t).build())
            .child(ThinkingFold::new(&state).element(cx, &t).build())
            .child(Feed::new(&answer).gap(0).element(cx, &t).build())
            .element(&t)
            .build()
    });
    let state = holder.borrow().clone().expect("state");
    state.append("because scattering scales with 1/λ⁴");
    let canvas = settle(&mut tree, size);
    let rows = dump(&canvas, size.h);
    let fold_row = rows
        .iter()
        .position(|r| r.contains("Thinking"))
        .expect("fold header inside the panel") as i32;
    assert!(
        rows.iter().any(|r| r.contains("why is the sky blue"))
            && rows.iter().any(|r| r.contains("Rayleigh")),
        "the turn stack renders around the fold: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("scattering scales")),
        "folded inside the stack"
    );
    click(&mut tree, 4, fold_row);
    let canvas = settle(&mut tree, size);
    assert!(
        dump(&canvas, size.h)
            .iter()
            .any(|r| r.contains("scattering scales")),
        "expands in place inside the panel"
    );
    root.dispose();
}

#[test]
fn a11y_region_and_toggle_button() {
    let size = Size::new(40, 8);
    let (state, root, mut tree) = mount_fold(size);
    state.append("x");
    settle(&mut tree, size);
    let access = tree.accessibility_tree_text();
    assert!(
        access.contains("region \"Thinking\""),
        "the card is a labeled region:\n{access}"
    );
    assert!(
        access.contains("button \"Thinking\"") && access.contains("collapsed"),
        "the header reports its fold state:\n{access}"
    );
    key(&mut tree, crate::ui::Key::Tab);
    key(&mut tree, crate::ui::Key::Enter);
    settle(&mut tree, size);
    let access = tree.accessibility_tree_text();
    assert!(
        access.contains("expanded"),
        "keyboard toggle updates the reported state:\n{access}"
    );
    root.dispose();
}

// -------------------------------------------------------- zero idle

#[test]
fn parked_and_completed_folds_are_idle() {
    use crate::app::{App, Driver, RunConfig};
    use crate::term::Capabilities;
    use crate::testing::CaptureTerm;

    let vp = Size::new(60, 12);
    let mut term = CaptureTerm::new(vp);
    let mut app = App::new(vp);
    let holder: Rc<RefCell<Option<ThinkingFoldState>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        let state = ThinkingFoldState::new(cx);
        *h.borrow_mut() = Some(state.clone());
        Element::new()
            .style(LayoutStyleFull::column())
            .child(ThinkingFold::new(&state).view(cx))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let state = holder.borrow().clone().expect("state");
    let mut settle = |app: &mut App, term: &mut CaptureTerm| {
        for _ in 0..64 {
            if driver.turn(app, term).expect("turn").idle {
                break;
            }
        }
        driver.turn(app, term).expect("turn").idle
    };
    assert!(settle(&mut app, &mut term), "fresh fold is idle");
    // Mid-stream but QUIET: appends stopped arriving — the indicator
    // must not tick on its own (park = silent, the zero-idle law).
    state.append("some ");
    state.append("thinking ");
    assert!(
        settle(&mut app, &mut term),
        "a quiet open stream schedules NOTHING (no timers, no frames)"
    );
    state.complete("some thinking, aggregated");
    assert!(settle(&mut app, &mut term), "completed fold is idle");
}
