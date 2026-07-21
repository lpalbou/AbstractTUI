//! REDTEAM cycle-3 attack (part 2): pointer semantics — wheel routing
//! to the nearest scrollable ancestor, pointer capture through mid-drag
//! disposal, the dead Phase::Target finding (RT3-3), and hover
//! exactly-once. Split from adv_widgets.rs (file-size discipline).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::base::{Point, Size};
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::reactive::{create_root, flush_effects};
use abstracttui::theme::{default_theme, TokenSet};
use abstracttui::ui::{
    dyn_view, Element, Mods, MouseButton, MouseEvent, MouseKind, Phase, UiEvent, UiTree,
};
use abstracttui::widgets::Scroll;

fn tokens() -> TokenSet {
    default_theme().tokens
}

fn mouse(kind: MouseKind, x: i32, y: i32) -> UiEvent {
    UiEvent::Mouse(MouseEvent {
        kind,
        pos: Point::new(x, y),
        mods: Mods::NONE,
    })
}

// ---------------------------------------------------------------------------
// Wheel routing to the nearest scrollable ancestor.
// ---------------------------------------------------------------------------

/// FINDING RT3-4 (P1, REACT): Scroll's wheel/key handler clamps the
/// offset against `ctx.target_rect()` — the rect of the (deep) hit
/// TARGET, not the scroll viewport. Whenever the wheeled-over child is
/// as tall as the content (nested scrolls, full-height wrappers), the
/// clamp evaluates to 0 and the wheel is consumed WITHOUT scrolling.
/// Verified end-to-end: dispatch returns handled=true, offsets stay 0.
/// Un-ignore on fix.
#[test]
fn wheel_routes_to_nearest_scrollable_ancestor() {
    let t = tokens();
    let mut tree = UiTree::new(Size::new(40, 14));
    let mut outer_handle = None;
    let mut inner_handle = None;
    let (root, ()) = create_root(|cx| {
        let outer_y = cx.signal(0i32);
        let inner_y = cx.signal(0i32);
        outer_handle = Some(outer_y);
        inner_handle = Some(inner_y);
        // Outer scroll fills the screen; inner scroll occupies the top
        // rows of its content.
        let inner = Scroll::new(abstracttui::ui::text(
            (0..50)
                .map(|i| format!("inner {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
        .content_size(30, 50)
        .offset_y(inner_y)
        .layout(
            LayoutStyle::default()
                .width(Dimension::Cells(34))
                .height(Dimension::Cells(6)),
        )
        .element(cx, &t)
        .build();
        let content = Element::new()
            .child(inner)
            .child(abstracttui::ui::text(
                (0..80)
                    .map(|i| format!("outer {i}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
            .build();
        let outer = Scroll::new(content)
            .content_size(36, 100)
            .offset_y(outer_y)
            .layout(LayoutStyle::default().grow(1.0))
            .element(cx, &t)
            .build();
        tree.mount(cx, Element::new().child(outer).build());
    });
    flush_effects();
    tree.layout();
    let (outer_y, inner_y) = (outer_handle.unwrap(), inner_handle.unwrap());

    // Wheel over the INNER scroll (hover routes wheels): only the inner
    // offset moves.
    tree.dispatch(&mouse(MouseKind::Move, 5, 2));
    tree.dispatch(&mouse(MouseKind::ScrollDown, 5, 2));
    flush_effects();
    assert!(
        inner_y.get_untracked() > 0,
        "inner scroll must consume the wheel"
    );
    assert_eq!(outer_y.get_untracked(), 0, "outer must not double-scroll");

    // Wheel BELOW the inner widget: the outer scroll moves.
    let inner_before = inner_y.get_untracked();
    tree.dispatch(&mouse(MouseKind::Move, 5, 12));
    tree.dispatch(&mouse(MouseKind::ScrollDown, 5, 12));
    flush_effects();
    assert!(
        outer_y.get_untracked() > 0,
        "outer scroll must take wheel outside inner"
    );
    assert_eq!(inner_y.get_untracked(), inner_before, "inner must not move");
    root.dispose();
}

// ---------------------------------------------------------------------------
// Pointer capture: drag greedily routed; disposal mid-drag stays sane.
// ---------------------------------------------------------------------------

#[test]
fn pointer_capture_survives_captured_node_disposal_mid_drag() {
    let drags: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let mut tree = UiTree::new(Size::new(30, 10));
    let mut open_handle = None;
    let (root, ()) = create_root(|cx| {
        let open = cx.signal(true);
        open_handle = Some(open);
        let drags2 = drags.clone();
        let view = Element::new()
            .child(dyn_view(LayoutStyle::default(), move || {
                if open.get() {
                    let drags3 = drags2.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Cells(10))
                                .height(Dimension::Cells(3)),
                        )
                        .on(Phase::Bubble, move |_ctx, ev| {
                            if let UiEvent::Mouse(m) = ev {
                                if matches!(m.kind, MouseKind::Drag(_)) {
                                    *drags3.borrow_mut() += 1;
                                }
                            }
                        })
                        .build()
                } else {
                    abstracttui::ui::text("gone")
                }
            }))
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();

    // Press inside (auto-captures), drag OUTSIDE the widget's rect:
    // capture keeps routing to it.
    tree.dispatch(&mouse(MouseKind::Down(MouseButton::Left), 2, 1));
    tree.dispatch(&mouse(MouseKind::Drag(MouseButton::Left), 20, 8));
    flush_effects();
    assert!(
        *drags.borrow() >= 1,
        "capture must route the off-rect drag to the target"
    );
    assert!(tree.pointer_capture().is_some(), "capture active mid-drag");

    // Dispose the captured node MID-DRAG.
    open_handle.unwrap().set(false);
    flush_effects();
    tree.layout();
    let before = *drags.borrow();
    // Further drags + release: no panic, no stale routing to the corpse.
    tree.dispatch(&mouse(MouseKind::Drag(MouseButton::Left), 21, 9));
    tree.dispatch(&mouse(MouseKind::Up(MouseButton::Left), 21, 9));
    flush_effects();
    assert_eq!(
        *drags.borrow(),
        before,
        "a disposed capture target must not receive further drags"
    );
    assert!(
        tree.pointer_capture().is_none(),
        "capture must clear when its node dies (or at latest on release)"
    );
    root.dispose();
}

/// FINDING RT3-3 (P2, REACT): handlers registered with `Phase::Target`
/// NEVER fire — the phase-match table in `run_handlers` has no
/// `(Target, Target)` arm, so the API's third variant is a silent
/// no-op (capture- and bubble-registered handlers both hear the target
/// phase; an explicit Target registration hears nothing). Silent is
/// the sin: either fire it at the target phase or remove the variant.
/// Un-ignore on fix.
#[test]
fn phase_target_handlers_fire_at_the_target() {
    let fired: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let fired2 = fired.clone();
    let mut tree = UiTree::new(Size::new(10, 4));
    let (root, ()) = create_root(|cx| {
        let _ = cx;
        let view = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(10))
                    .height(Dimension::Cells(4)),
            )
            .on(Phase::Target, move |_ctx, ev| {
                if matches!(ev, UiEvent::Mouse(_)) {
                    *fired2.borrow_mut() += 1;
                }
            })
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();
    tree.dispatch(&mouse(MouseKind::Down(MouseButton::Left), 2, 2));
    assert_eq!(
        *fired.borrow(),
        1,
        "a Target-phase handler on the hit target must fire"
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// Hover enter/leave exactly-once.
// ---------------------------------------------------------------------------

#[test]
fn hover_state_transitions_exactly_once_per_crossing() {
    let mut tree = UiTree::new(Size::new(30, 10));
    let mut target_id = None;
    let (root, ()) = create_root(|cx| {
        let _ = cx;
        let inner = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(10))
                    .height(Dimension::Cells(4)),
            )
            .build();
        let id_holder = tree.mount(cx, Element::new().child(inner).build());
        target_id = Some(id_holder);
    });
    flush_effects();
    tree.layout();
    let root_id = target_id.unwrap();

    // Sweep the pointer across the widget: hovered flips exactly at the
    // boundary crossings, never mid-run (exactly-once semantics read
    // through the state, which is the routing truth).
    let mut transitions = 0;
    let mut prev = tree.is_hovered(root_id);
    for x in -1..31 {
        tree.dispatch(&mouse(MouseKind::Move, x, 2));
        flush_effects();
        let now = tree.is_hovered(root_id);
        if now != prev {
            transitions += 1;
            prev = now;
        }
    }
    assert!(
        transitions <= 2,
        "one horizontal sweep must produce at most enter+leave, got {transitions}"
    );
    // Jitter INSIDE the rect: hover holds true continuously (no flap).
    tree.dispatch(&mouse(MouseKind::Move, 3, 2));
    flush_effects();
    assert!(tree.is_hovered(root_id), "pointer inside must hover");
    for _ in 0..20 {
        tree.dispatch(&mouse(MouseKind::Move, 4, 2));
        assert!(
            tree.is_hovered(root_id),
            "in-rect jitter must not flap hover"
        );
        tree.dispatch(&mouse(MouseKind::Move, 3, 2));
        assert!(
            tree.is_hovered(root_id),
            "in-rect jitter must not flap hover"
        );
    }
    root.dispose();
}
