//! field-agora 0910, slice 5: the read seam itself —
//! `Element::rect_signal`.
//!
//! Slices 1-4 narrowed the shape until only one survived. A `Scroll`
//! verb cannot be built (no widget can reach the tree: `DrawFn` is
//! `(&mut dyn StyledCanvas, Rect)` and `EventCtx` hands out rects, not
//! `rect_of`), and a `UiTree`-side verb is uncallable from an app that
//! hands its loop to `run_with`. What is left is the element publishing
//! its OWN solved rect into a caller-owned signal, so the clamp is an
//! ordinary effect in the consumer's pane.
//!
//! The three properties that make it usable, each pinned here:
//!
//! - `an_off_screen_child_publishes_its_true_solved_rect` — the child a
//!   clamp must locate is by definition the one outside the viewport,
//!   which is exactly the one paint culls. Publishing from LAYOUT is
//!   what makes the binding answer at all, and the test walks a
//!   selection past the fold using nothing but the signal.
//! - `a_collapsed_child_publishes_none_rather_than_the_origin` — a
//!   zero-area solve is clean absence, not a position. `0x0` reaching a
//!   clamp is a scroll to the origin, which reads as "the list jumped to
//!   the top" and gets debugged in the wrong layer.
//! - `a_publish_scheduled_before_disposal_does_not_land_after_it` — the
//!   consumer's topology is one PANE-owned signal re-bound to whichever
//!   card is selected, so the signal outlives every element that writes
//!   it. Signal liveness therefore answers nothing; the element's own
//!   scope decides. Same hazard `size_probe` had, and the race has to be
//!   constructed deliberately: a disposal test with no pending write
//!   tests nothing about disposal.
//!
//! `a_rebuild_moves_the_binding_to_the_newly_selected_card` is the
//! consumer's shape end to end — a `dyn_view_scoped` column that reads
//! `selected` tracked, with the binding on whichever card is selected.

use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::base::{Rect, Size};
use crate::layout::Style as LayoutStyle;
use crate::reactive::{flush_effects, Signal};
use crate::theme::default_theme;
use crate::ui::{dyn_view_scoped, text, Element};
use crate::widgets::itest_util::{mount_widget, settle};

const VIEW_W: i32 = 12;
const VIEW_H: i32 = 4;
const ROWS: i32 = 20;
/// Far below the fold, and past enough tall children that a uniform
/// height model would land somewhere else entirely.
const TARGET_ROW: i32 = 14;

/// MIXED heights — the report's folded/expanded card column. Every
/// third child is 3 rows, the rest 1.
fn child_height(i: i32) -> i32 {
    if i % 3 == 0 {
        3
    } else {
        1
    }
}

fn content_height() -> i32 {
    (0..ROWS).map(child_height).sum()
}

/// A `Scroll` of mixed-height children with the rect binding on child
/// `bound` — the "one signal per pane, on the selected card" shape.
fn column_with_binding(
    offset: Signal<i32>,
    bound: i32,
    sig: Signal<Option<Rect>>,
    painted: Rc<Cell<bool>>,
) -> impl FnOnce(crate::reactive::Scope) -> View {
    move |cx| {
        let t = &default_theme().tokens;
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..ROWS {
            let mut card = Element::new().style(LayoutStyle::column());
            if i == bound {
                let painted = painted.clone();
                card = card
                    .rect_signal(sig)
                    // The witness for "this did not come from paint":
                    // the bound child is culled while it is below the
                    // fold, so its own draw never runs.
                    .draw(move |_canvas, _rect| painted.set(true));
            }
            for line in 0..child_height(i) {
                card = card.child(text(format!("r{i}.{line}")));
            }
            col = col.child(card.build());
        }
        Scroll::new(col.build())
            .content_size(VIEW_W - 1, content_height())
            .offset_y(offset)
            .element(cx, t)
            .build()
    }
}

/// The consumer's five-line clamp, written against the SIGNAL instead of
/// against a height model — the whole point of the seam. Returns false
/// when there is no usable rect, which is the case `None` exists for.
fn ensure_visible(sig: Signal<Option<Rect>>, offset: Signal<i32>, view_h: i32) -> bool {
    let Some(r) = sig.get_untracked() else {
        return false;
    };
    let cur = offset.get_untracked();
    let top = r.y + cur;
    let bottom = top + r.h;
    let mut next = cur;
    if top < next {
        next = top;
    }
    if bottom > next + view_h {
        next = bottom - view_h;
    }
    if next != cur {
        offset.set(next);
    }
    true
}

/// The load-bearing case: the bound child sits far below the fold, has
/// never painted, and the binding still reports where the solver put it
/// — well enough to scroll it into view with no height model at all.
#[test]
fn an_off_screen_child_publishes_its_true_solved_rect() {
    let size = Size::new(VIEW_W, VIEW_H);
    let painted = Rc::new(Cell::new(false));
    let mut wires: Option<(Signal<i32>, Signal<Option<Rect>>)> = None;
    let (_root, mut tree) = mount_widget(size, {
        let painted = painted.clone();
        |cx| {
            let offset = cx.signal(0);
            let sig = cx.signal(None);
            wires = Some((offset, sig));
            column_with_binding(offset, TARGET_ROW, sig, painted)(cx)
        }
    });
    let (offset, sig) = wires.expect("built");
    settle(&mut tree, size);

    // CONTROL: the child really is outside the viewport, so nothing
    // below can be passing because it happened to be on screen.
    let published = sig.get_untracked().expect("the binding published a rect");
    assert!(
        published.y >= VIEW_H,
        "control: the bound child must start below the {VIEW_H}-row \
         fold, got {published:?}"
    );
    // THE FALSIFIER for the whole shape: the rect arrived from LAYOUT.
    // This child has never painted — it is culled — so a paint-time
    // probe (slice 1's dead end) would have published nothing at all.
    assert!(
        !painted.get(),
        "control: the bound child must NOT have painted while below \
         the fold, or this test says nothing about publishing from \
         layout rather than from paint"
    );

    // The rect is the SOLVED one, not a guess: slice 2 showed this
    // child cannot even be named by hit test while it is off-screen,
    // and the binding is how a caller reaches it anyway.
    assert!(
        published.h == child_height(TARGET_ROW),
        "the published rect is the SOLVED one: child {TARGET_ROW} is \
         {} rows tall, got {published:?}",
        child_height(TARGET_ROW)
    );

    // Now the consumer's clamp, driven by the signal alone.
    assert!(ensure_visible(sig, offset, VIEW_H), "a rect was available");
    settle(&mut tree, size);

    let after = sig
        .get_untracked()
        .expect("still published after scrolling");
    // The witness is wired: the same closure DOES fire once the child is
    // on screen, so the assertion above was a cull and not a dead probe.
    assert!(
        painted.get(),
        "control on the control: the bound child paints once it is \
         visible, so `painted` is a real signal of paint"
    );
    assert!(
        after.y >= 0 && after.y + after.h <= VIEW_H,
        "the bound child must be FULLY visible after one clamp, got \
         {after:?} at offset {}",
        offset.get_untracked()
    );

    // CONTROL on the content: with mixed heights, the child's true
    // content-space top differs from the uniform `index * 1` guess a
    // consumer hand-rolls — so this cannot have passed against one.
    let content_top = after.y + offset.get_untracked();
    assert_ne!(
        content_top, TARGET_ROW,
        "control: mixed heights must make the true offset of child \
         {TARGET_ROW} differ from the uniform guess"
    );
}

/// A child crushed to zero area publishes `None`. It is laid out and it
/// is not painted (an empty rect falls through the cull as clean
/// absence), so "where is it" has no honest answer — and answering
/// `Rect::ZERO` would clamp the consumer's viewport to the origin.
#[test]
fn a_collapsed_child_publishes_none_rather_than_the_origin() {
    let size = Size::new(VIEW_W, VIEW_H);
    let mut wires: Option<(Signal<bool>, Signal<Option<Rect>>)> = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let collapsed = cx.signal(false);
        let sig = cx.signal(None);
        wires = Some((collapsed, sig));
        let card = Element::new()
            .style(LayoutStyle::column())
            .rect_signal(sig)
            .style_signal(move || {
                let mut s = LayoutStyle::column();
                s.height = if collapsed.get() {
                    crate::layout::Dimension::Cells(0)
                } else {
                    crate::layout::Dimension::Cells(2)
                };
                s
            })
            .child(text("card"));
        Element::new()
            .style(LayoutStyle::column())
            .child(card.build())
            .child(text("sibling"))
            .build()
    });
    let (collapsed, sig) = wires.expect("built");
    settle(&mut tree, size);

    let open = sig.get_untracked().expect("an open card has a position");
    assert_eq!(
        open.h, 2,
        "sanity: the card is 2 rows before it collapses, got {open:?}"
    );

    collapsed.set(true);
    settle(&mut tree, size);

    assert_eq!(
        sig.get_untracked(),
        None,
        "a zero-area child must publish None: it is CLEAN ABSENCE, and \
         a 0x0 rect reaching an ensure-visible clamp scrolls to the \
         origin — which reads as a jump to the top, not as a collapse"
    );

    // And back: `None` is a state, not a terminal one.
    collapsed.set(false);
    settle(&mut tree, size);
    assert_eq!(
        sig.get_untracked().map(|r| r.h),
        Some(2),
        "re-expanding republishes a real rect"
    );
}

/// The corpse write. The signal belongs to the PANE and outlives every
/// card, so `sig` being alive says nothing about whether the element
/// that scheduled the publish still exists.
///
/// The race is constructed deliberately: one solve so the publish is
/// scheduled, THEN disposal, and only then the timers. A disposal test
/// that lets the tree settle first has already drained the hazard and
/// is testing nothing — that mistake is why this file exists rather
/// than a one-line assertion (see `size_probe`, `3507c22`).
#[test]
fn a_publish_scheduled_before_disposal_does_not_land_after_it() {
    let size = Size::new(VIEW_W, VIEW_H);
    let mut wires: Option<(Signal<bool>, Signal<Option<Rect>>)> = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let show = cx.signal(true);
        let sig = cx.signal(None);
        wires = Some((show, sig));
        dyn_view_scoped(LayoutStyle::column(), move |_| {
            if show.get() {
                Element::new()
                    .style(LayoutStyle::column())
                    .rect_signal(sig)
                    .child(text("doomed"))
                    .build()
            } else {
                text("replacement")
            }
        })
    });
    let (show, sig) = wires.expect("built");

    // ONE solve: the probe records a changed rect and arms its deferred
    // publish. No timers run, so nothing has been written yet.
    flush_effects();
    tree.layout();
    assert_eq!(
        sig.get_untracked(),
        None,
        "precondition: the publish is deferred, so nothing has landed \
         yet — if this is already Some, the race below is not armed"
    );

    // Dispose with that publish in flight.
    show.set(false);
    settle(&mut tree, size);

    assert_eq!(
        sig.get_untracked(),
        None,
        "an element disposed between scheduling and firing published \
         its rect anyway — the caller's signal is alive, which is not \
         the question that decides this"
    );
}

/// The consumer's actual topology (`panes.rs`): a column rebuilt on
/// every selection change, with the binding on whichever card is
/// selected. Exactly one element publishes into the signal at a time,
/// and the value follows the selection rather than lagging one behind.
#[test]
fn a_rebuild_moves_the_binding_to_the_newly_selected_card() {
    let size = Size::new(VIEW_W, VIEW_H);
    let mut wires: Option<(Signal<i32>, Signal<Option<Rect>>)> = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let selected = cx.signal(0);
        let sig = cx.signal(None);
        wires = Some((selected, sig));
        dyn_view_scoped(LayoutStyle::column(), move |_| {
            let sel = selected.get(); // TRACKED: selection rebuilds the column
            let mut col = Element::new().style(LayoutStyle::column());
            for i in 0..3 {
                let mut card = Element::new().style(LayoutStyle::column());
                if i == sel {
                    card = card.rect_signal(sig);
                }
                card = card.child(text(format!("c{i}")));
                col = col.child(card.build());
            }
            col.build()
        })
    });
    let (selected, sig) = wires.expect("built");
    settle(&mut tree, size);

    let first = sig.get_untracked().expect("the selected card published");
    assert_eq!(first.y, 0, "card 0 is the top row, got {first:?}");

    selected.set(2);
    settle(&mut tree, size);

    let third = sig.get_untracked().expect("the new selection published");
    assert_eq!(
        third.y, 2,
        "the binding must follow the selection to card 2 (row 2), not \
         report the previous card's rect: got {third:?}"
    );
}
