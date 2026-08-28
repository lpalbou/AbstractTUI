//! field-agora 0910, slice 2: WHICH half of the ask is actually
//! missing — the readback, or the NAME.
//!
//! Slice 1 pinned that the paint idiom (`size_probe` + a signal, the way
//! `extent_signal` works) cannot reach a child below the fold, because
//! `draw.rs` culls it and its probe never fires. The natural conclusion
//! was that 0910 needs a new engine-side rect readback.
//!
//! It does not. `UiTree::rect_of` is already `pub`, and it reads the
//! LAYOUT solver, not paint — culling is a paint optimisation, so the
//! solved rect of an off-screen child is sitting there, correct, behind
//! a public method. `off_screen_child_rect_is_already_public_through_
//! rect_of` pins that.
//!
//! What an app cannot get is the `ViewId` to pass it. `Element` and
//! `View` carry no identity field at all (`view.rs` — style, style_fn,
//! measure, drag_zone, draw, handlers, shortcuts, focusable, focus_trap,
//! focus_memory, autofocus, probe_when_culled, padding_floor, access,
//! children: no id, no key), and every public source of a `ViewId` is
//! post-mount and event-shaped: `UiTree::mount` (the root only),
//! `focused()`, `hit_test(Point)`, `EventCtx::target()/current()`. All of
//! them require the node to be reachable by a pointer or by focus —
//! which the child an ensure-visible verb must locate, by definition,
//! is not. `a_below_the_fold_child_cannot_be_named_by_hit_test` pins
//! that half.
//!
//! Together they answer the design question slice 1 left open: the
//! primitive 0910 is missing is CHILD IDENTITY, not a rect. That is a
//! much smaller promise to keep than "Scroll exposes per-child solved
//! offsets", and it is why neither engine precedent reads a child rect
//! back — `List` builds a prefix sum from a caller-supplied
//! `item_heights` (list.rs), and the graph extension's own
//! `ensure_visible` probes only the VIEWPORT and takes the card rect
//! from the graph layout it authored itself (extensions/graph/src/
//! view.rs). Every working ensure-visible in this tree gets the child's
//! position from a model the caller already owns.

use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::base::{Point, Rect, Size};
use crate::layout::Style as LayoutStyle;
use crate::theme::default_theme;
use crate::ui::{text, Element, MouseKind, UiTree, ViewId};
use crate::widgets::itest_util::{mount_widget, mouse, settle};

const VIEW_W: i32 = 12;
const VIEW_H: i32 = 4;
const ROWS: i32 = 20;
/// Far below a 4-row viewport: the child an ensure-visible verb exists
/// for, and the one nothing on the app side can point at.
const DEEP_ROW: i32 = 12;

/// A `Scroll` of `ROWS` single-row children. The child at `marked_row`
/// carries a paint probe; `culled_probe` opts it into the crate-internal
/// exemption so the engine-side truth can be read even off-screen.
fn scroll_of_rows(
    marked_row: i32,
    probe: Rc<Cell<Option<Rect>>>,
    culled_probe: bool,
) -> impl FnOnce(crate::reactive::Scope) -> View {
    move |cx| {
        let t = &default_theme().tokens;
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..ROWS {
            if i == marked_row {
                let p = probe.clone();
                let mut marked = Element::new()
                    .style(LayoutStyle::column())
                    .child(text(format!("row {i}")))
                    .draw(move |_canvas, rect| p.set(Some(rect)));
                if culled_probe {
                    marked = marked.probe_when_culled();
                }
                col = col.child(marked.build());
            } else {
                col = col.child(text(format!("row {i}")));
            }
        }
        Scroll::new(col.build())
            .content_size(VIEW_W - 1, ROWS)
            .element(cx, t)
            .build()
    }
}

/// The deepest node the app can name at `y`, walking up until the id
/// belongs to something with the row's own height — `hit_test` returns
/// the deepest instance, which for a text row is its text leaf.
fn hit_row(tree: &UiTree, y: i32) -> Option<ViewId> {
    tree.hit_test(Point::new(1, y))
}

/// The rect readback 0910 asks for is ALREADY public and already
/// correct for a culled child: `rect_of` reads the layout solver, which
/// never knew about the cull. What makes it unusable is only that the
/// app has to have obtained the `ViewId` first — here by hit-testing the
/// child while it was still visible, which the real case cannot do.
#[test]
fn off_screen_child_rect_is_already_public_through_rect_of() {
    let size = Size::new(VIEW_W, VIEW_H);
    let probe: Rc<Cell<Option<Rect>>> = Rc::new(Cell::new(None));
    // Marked row 0: visible at mount, so the app CAN name it.
    let (_root, mut tree) = mount_widget(size, scroll_of_rows(0, probe.clone(), false));
    settle(&mut tree, size);

    let id = hit_row(&tree, 0).expect("row 0 is inside the viewport at mount");
    assert_eq!(
        tree.rect_of(id).y,
        0,
        "sanity: the named child is the one at content row 0"
    );
    let painted_while_visible = probe
        .get()
        .expect("sanity: a visible child paints, so its probe fires");
    assert_eq!(painted_while_visible.y, 0);

    // Scroll it off the TOP. Eight wheel notches at the engine's step;
    // the exact distance does not matter, only that the child ends up
    // outside the viewport.
    for _ in 0..8 {
        mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
    }
    settle(&mut tree, size);

    let solved = tree.rect_of(id);
    assert!(
        solved.y < 0,
        "the child really is off the top of the {VIEW_H}-row viewport, \
         got {solved:?}"
    );

    // THE FINDING. The paint probe is stale — slice 1's fact, and the
    // reason the app-side idiom fails — while `rect_of` on the very same
    // node reports the true, current, off-screen offset.
    let stale = probe.get().expect("the probe holds its last painted rect");
    assert_eq!(
        stale.y, painted_while_visible.y,
        "the culled child has not repainted, so the paint idiom still \
         believes it is at row {}",
        painted_while_visible.y
    );
    assert_ne!(
        solved.y, stale.y,
        "rect_of and the paint probe now disagree, and rect_of is the \
         one telling the truth: the readback 0910 asks for is already \
         public — what is missing is the ViewId to pass it"
    );
}

/// The other half: for the child that ensure-visible actually exists
/// for — one that has never been on screen — there is no public way to
/// obtain a `ViewId` at all. `hit_test` is clip-aware by design (it
/// refuses to descend past a `clip_overflow` node, so scrolled-away
/// children are not hit at their invisible positions), so sweeping every
/// cell of the viewport never names it.
#[test]
fn a_below_the_fold_child_cannot_be_named_by_hit_test() {
    let size = Size::new(VIEW_W, VIEW_H);
    let probe: Rc<Cell<Option<Rect>>> = Rc::new(Cell::new(None));
    // The exemption is how the TEST learns where the child is; it is
    // pub(crate), which is precisely why the app cannot do this.
    let (_root, mut tree) = mount_widget(size, scroll_of_rows(DEEP_ROW, probe.clone(), true));
    settle(&mut tree, size);

    let truth = probe
        .get()
        .expect("probe_when_culled keeps the off-screen child's paint alive");
    assert_eq!(
        truth.y, DEEP_ROW,
        "engine-side, the child's solved offset is known exactly"
    );

    // Everything the app can point at. Later siblings win at each level,
    // so this is the complete set of ids reachable from the viewport.
    let named: Vec<Rect> = (0..VIEW_H)
        .filter_map(|y| hit_row(&tree, y))
        .map(|id| tree.rect_of(id))
        .collect();
    assert!(
        !named.is_empty(),
        "control: hit_test must name SOMETHING inside the viewport, or \
         the absence below proves nothing about the deep child"
    );
    assert!(
        named.iter().all(|r| r.y != truth.y),
        "no point in the viewport names the child at content row \
         {DEEP_ROW}; the app can reach {named:?} and nothing else, so \
         it has no ViewId to hand rect_of — CHILD IDENTITY is the \
         missing primitive, not the rect"
    );

    // CONTROL. Scroll the same child into view and the same sweep DOES
    // name it. Without this, the assertion above would pass just as
    // well on a sweep that named nothing useful at any time, and the
    // finding would be an artefact of the probe rather than of the
    // child being off-screen.
    // One notch at a time: the wheel step is several rows and the
    // offset clamps at the end of the content, so a fixed count would
    // overshoot the child rather than land on it.
    let mut now = truth;
    for _ in 0..ROWS {
        if (0..VIEW_H).contains(&now.y) {
            break;
        }
        mouse(&mut tree, MouseKind::ScrollDown, 2, 1);
        settle(&mut tree, size);
        now = probe.get().expect("the probe keeps reporting");
    }
    assert!(
        (0..VIEW_H).contains(&now.y),
        "control: the child is inside the viewport after scrolling, \
         got {now:?}"
    );
    let named_now: Vec<Rect> = (0..VIEW_H)
        .filter_map(|y| hit_row(&tree, y))
        .map(|id| tree.rect_of(id))
        .collect();
    assert!(
        named_now.iter().any(|r| r.y == now.y),
        "control: once visible, hit_test DOES name the child — the \
         sweep works, so the miss above is about the fold and not \
         about the sweep. reachable: {named_now:?}, child at {now:?}"
    );
}
