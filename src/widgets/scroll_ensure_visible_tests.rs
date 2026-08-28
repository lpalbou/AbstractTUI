//! field-agora 0910, slice 3: can shape (1) ship WITHOUT exposing shape
//! (2) — and does it need a new identity primitive at all?
//!
//! Slice 2 answered the naming question in the negative for the general
//! case: `Element`/`View` carry no identity, so no builder-time API can
//! hand back a `ViewId`, and every public source of one needs the node
//! reachable by pointer or focus.
//!
//! FOCUS is the loophole, and it is not a small one — it is exactly the
//! case the field report describes. The report's consumer keyboard-
//! selects through a column of cards; if selection moves FOCUS, then
//! `UiTree::focused()` hands back the selected child's `ViewId` for
//! free, and slice 2 already established `rect_of` reads the layout
//! solver rather than paint, so it answers correctly for a child that
//! has never been painted.
//!
//! `keyboard_selection_can_ensure_visible_today_with_no_new_engine_api`
//! builds the whole verb out of nothing but public API — `focus_next`,
//! `focused`, `rect_of`, and a bound `offset_y` — and walks selection
//! from the top of a 20-row content to a row far below the fold,
//! clamping after each step. The selected child is fully visible at
//! every step. No `Element` key, no per-child offset seam, no new
//! primitive.
//!
//! `focus_alone_does_not_scroll_the_selected_child_into_view` is the
//! other half, and it is what keeps 0910 open: the engine does NOT do
//! this for you. Focus moves to a child below the fold and the viewport
//! stays exactly where it was. The gap is real; it is just an ERGONOMIC
//! gap over reachable primitives, not a missing readback.
//!
//! What that does to the item: shape (1) does not need shape (2)
//! underneath it, and it does not need child keys either — for the
//! focus-driven case. A `Scroll` verb here is sugar over a five-line
//! clamp the app can already write, which is a much smaller thing to
//! promise than "Scroll exposes per-child solved offsets". The cases
//! focus does NOT cover — selection that deliberately does not move
//! focus, or locating a child nobody has selected — still have no
//! answer, and that is the honest remaining scope of 0910.

use super::*;
use crate::base::{Rect, Size};
use crate::layout::Style as LayoutStyle;
use crate::reactive::Signal;
use crate::theme::default_theme;
use crate::ui::{text, Element, UiTree};
use crate::widgets::itest_util::{mount_widget, settle};

const VIEW_W: i32 = 12;
const VIEW_H: i32 = 4;
const ROWS: i32 = 20;
/// Far below the fold: the row selection has to walk down to.
const TARGET_ROW: i32 = 14;

/// MIXED heights, which is the whole point of the report: every third
/// child is 3 rows tall (the "expanded card"), the rest are 1 (the
/// "folded" one). A uniform `index * 1` model — the thing a consumer
/// hand-rolls — gets the wrong answer for every child after the first
/// tall one, so a clamp that passes here cannot be doing that.
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

/// A `Scroll` of `ROWS` FOCUSABLE mixed-height children with a bound
/// vertical offset — the report's shape, minus the hand-rolled height
/// model it is filed about.
fn focusable_rows(offset: Signal<i32>) -> impl FnOnce(crate::reactive::Scope) -> View {
    move |cx| {
        let t = &default_theme().tokens;
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..ROWS {
            let mut card = Element::new().style(LayoutStyle::column()).focusable();
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

/// The whole ensure-visible verb, written with PUBLIC API only. This is
/// the thing 0910 asks the engine for; the point of the test is that an
/// app can write it today, in five lines, without a height model.
///
/// `rect_of` is layer-local (already displaced by the current offset),
/// so the child's CONTENT-space top is its solved y plus the offset in
/// force — no per-widget arithmetic, no knowledge of chrome heights.
fn ensure_focused_visible(tree: &UiTree, offset: Signal<i32>, view_h: i32) {
    let Some(id) = tree.focused() else { return };
    let r: Rect = tree.rect_of(id);
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
}

/// Walk keyboard selection from the first row to one far below the
/// fold, clamping after each step, and require the selected child to be
/// fully visible every time. This is the report's acceptance criterion
/// ("the selected card is fully visible after each step") satisfied
/// with no new engine surface at all.
#[test]
fn keyboard_selection_can_ensure_visible_today_with_no_new_engine_api() {
    let size = Size::new(VIEW_W, VIEW_H);
    let mut offset_holder: Option<Signal<i32>> = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let offset = cx.signal(0);
        offset_holder = Some(offset);
        focusable_rows(offset)(cx)
    });
    let offset = offset_holder.expect("built");
    settle(&mut tree, size);

    tree.focus_first();
    settle(&mut tree, size);

    for step in 0..=TARGET_ROW {
        ensure_focused_visible(&tree, offset, VIEW_H);
        settle(&mut tree, size);

        let id = tree.focused().expect("focus stays inside the column");
        let r = tree.rect_of(id);
        assert!(
            r.y >= 0 && r.y + r.h <= VIEW_H,
            "step {step}: the selected child must be FULLY visible in \
             the {VIEW_H}-row viewport, got {r:?} at offset {}",
            offset.get_untracked()
        );

        if step < TARGET_ROW {
            tree.focus_next();
            settle(&mut tree, size);
        }
    }

    // The walk really did leave the first screen — otherwise every
    // assertion above would have held trivially with the offset at 0.
    assert!(
        offset.get_untracked() > 0,
        "control: selection walked past the fold, so the offset must \
         have moved; it is still {}",
        offset.get_untracked()
    );

    // CONTROL on the CONTENT, not the clamp: the heights really are
    // mixed, so the uniform `index * 1` model a consumer hand-rolls
    // would have put this child in the wrong place. Without this the
    // test could pass against a naive model and still look like a
    // demonstration that layout readback is what made it work.
    let id = tree.focused().expect("focus stayed in the column");
    let content_top = tree.rect_of(id).y + offset.get_untracked();
    let uniform_guess = TARGET_ROW;
    assert_ne!(
        content_top, uniform_guess,
        "control: with mixed heights the true content offset of child \
         {TARGET_ROW} must differ from the uniform-height guess, or \
         this test does not exercise the case 0910 is about"
    );
}

/// Why 0910 stays open even though the primitives are reachable: the
/// engine does not do the clamp for you. Move focus down past the fold
/// WITHOUT the app's clamp and the viewport does not follow — the
/// focused child sits outside it, and only the app noticing is what
/// puts it back.
#[test]
fn focus_alone_does_not_scroll_the_selected_child_into_view() {
    let size = Size::new(VIEW_W, VIEW_H);
    let mut offset_holder: Option<Signal<i32>> = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let offset = cx.signal(0);
        offset_holder = Some(offset);
        focusable_rows(offset)(cx)
    });
    let offset = offset_holder.expect("built");
    settle(&mut tree, size);

    tree.focus_first();
    settle(&mut tree, size);
    let first = tree.rect_of(tree.focused().expect("focused"));
    assert!(
        first.y >= 0 && first.y < VIEW_H,
        "sanity: the first row starts inside the viewport, got {first:?}"
    );

    for _ in 0..TARGET_ROW {
        tree.focus_next();
        settle(&mut tree, size);
    }

    let r = tree.rect_of(tree.focused().expect("focus stayed in the column"));
    assert_eq!(
        offset.get_untracked(),
        0,
        "the engine did not scroll: the bound offset never moved"
    );
    assert!(
        r.y >= VIEW_H,
        "THE GAP 0910 IS ABOUT: focus is on a child below the fold and \
         the viewport did not follow it — {r:?} against a {VIEW_H}-row \
         viewport. rect_of reports it correctly, which is what makes \
         the app-side clamp possible; nothing in the engine applies it."
    );
}
