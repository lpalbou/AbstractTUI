//! OWNED + TOOLTIP substrate tests (split file, `#[path]`-included as
//! `owned::tests`). Popups live on a real overlay store and events
//! route the way the driver routes them (overlays first, topmost-z
//! modal wins) — the stacking and dismiss contracts are exercised, not
//! simulated.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::*;
use crate::base::Point;
use crate::reactive::{create_root, flush_effects, run_due_timers};
use crate::ui::{text, KeyEvent, MouseButton, MouseEvent, MouseKind, UiEvent, UiTree};

type SharedLog<T> = Rc<RefCell<Vec<T>>>;

fn key_event(k: Key) -> UiEvent {
    UiEvent::Key(KeyEvent::plain(k))
}

fn press_at(x: i32, y: i32) -> UiEvent {
    UiEvent::Mouse(MouseEvent {
        pos: Point::new(x, y),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    })
}

// ------------------------------------------------------------ placement

#[test]
fn place_owned_plain_mode_matches_place_panel_and_reports_flip() {
    let vp = Size::new(80, 24);
    let list = Size::new(20, 4);
    let width = PanelWidth::Content { min: 8, max: 44 };
    // Plenty below: same rect as place_panel, not flipped.
    let (rect, flipped) =
        place_owned(vp, Rect::new(10, 5, 1, 1), list, width, false).expect("fits");
    assert_eq!(rect, Rect::new(10, 6, 20, 4));
    assert!(!flipped);
    // Cramped below: flipped above, bottom edge touches the anchor row.
    let (rect, flipped) =
        place_owned(vp, Rect::new(10, 22, 1, 1), list, width, false).expect("fits");
    assert_eq!(rect, Rect::new(10, 18, 20, 4));
    assert!(flipped);
    // No room anywhere: honest None.
    assert!(place_owned(
        Size::new(20, 1),
        Rect::new(0, 0, 1, 1),
        list,
        PanelWidth::MatchAnchor,
        false
    )
    .is_none());
}

#[test]
fn place_owned_anchor_row_inclusion_below_and_flipped() {
    let vp = Size::new(60, 20);
    let anchor = Rect::new(5, 3, 14, 1);
    let list = Size::new(14, 5);
    // Below: bounds START at the anchor row; height = anchor + rows.
    let (rect, flipped) =
        place_owned(vp, anchor, list, PanelWidth::MatchAnchor, true).expect("fits");
    assert_eq!(rect, Rect::new(5, 3, 14, 6), "anchor row + 5 list rows");
    assert!(!flipped);
    // Near the bottom: rows go ABOVE, the anchor row is the LAST row.
    let anchor = Rect::new(5, 18, 14, 1);
    let (rect, flipped) =
        place_owned(vp, anchor, list, PanelWidth::MatchAnchor, true).expect("fits");
    assert!(flipped);
    assert_eq!(rect, Rect::new(5, 13, 14, 6));
    assert_eq!(
        rect.bottom(),
        anchor.bottom(),
        "flipped popup ends with the anchor row"
    );
    // Cramped both sides: rows clamp to the roomier side.
    let (rect, _) = place_owned(
        Size::new(60, 6),
        Rect::new(0, 2, 10, 1),
        list,
        PanelWidth::MatchAnchor,
        true,
    )
    .expect("fits");
    assert_eq!(rect.h, 1 + 3, "anchor row + the 3 rows below");
    // Anchor fills the screen's one row: no list row fits -> None.
    assert!(place_owned(
        Size::new(60, 1),
        Rect::new(0, 0, 10, 1),
        list,
        PanelWidth::MatchAnchor,
        true
    )
    .is_none());
}

// ------------------------------------------------------------ owned popup

/// A popup whose content records the keys it receives.
fn open_recording_popup(
    overlays: &Overlays,
    cx: Scope,
    vp: Size,
    anchor: Rect,
) -> (Popup, SharedLog<Key>, SharedLog<DismissReason>) {
    let keys: SharedLog<Key> = Default::default();
    let k2 = keys.clone();
    let popup = Popup::open(
        overlays,
        cx,
        vp,
        PanelAnchor { rect: anchor },
        PanelWidth::MatchAnchor,
        Size::new(anchor.w, 3),
        move |_pcx, _flipped| {
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .on(Phase::Bubble, move |ctx, ev| {
                    if let UiEvent::Key(k) = ev {
                        if k.key != Key::Escape {
                            k2.borrow_mut().push(k.key);
                            ctx.stop_propagation();
                        }
                    }
                })
                .child(text("popup"))
                .build()
        },
    )
    .expect("room below");
    let reasons: SharedLog<DismissReason> = Default::default();
    let r2 = reasons.clone();
    popup.on_dismiss(move |r| r2.borrow_mut().push(r));
    (popup, keys, reasons)
}

#[test]
fn popup_stacks_above_modals_owns_keys_and_returns_them_on_dismiss() {
    let vp = Size::new(60, 20);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    // Two stacked "modals" (the F1 shape): the second must get keys
    // back after the popup closes.
    let modal_keys: Rc<RefCell<Vec<Key>>> = Default::default();
    let mk = modal_keys.clone();
    let (root, popup_state) = create_root(|cx| {
        let _modal1 = overlays.layer_tree(
            1000,
            Rect::new(2, 2, 40, 14),
            true,
            cx,
            Element::new().child(text("modal one")).build(),
        );
        let _modal2 = overlays.layer_tree(
            1001,
            Rect::new(4, 4, 36, 10),
            true,
            cx,
            Element::new()
                .on(Phase::Bubble, move |_ctx, ev| {
                    if let UiEvent::Key(k) = ev {
                        mk.borrow_mut().push(k.key);
                    }
                })
                .child(text("modal two"))
                .build(),
        );
        open_recording_popup(&overlays, cx, vp, Rect::new(6, 6, 20, 1))
    });
    let (popup, keys, reasons) = popup_state;
    assert!(popup.is_open());
    let layer = popup.layer().expect("live layer");
    {
        let store = overlays.store().borrow();
        let max_z = store.layers.iter().map(|l| l.z()).max().unwrap();
        assert_eq!(max_z, 1002, "popup z = top_z() + 1, above both modals");
        drop(store);
        assert_eq!(layer.bounds(), Some(popup.rect()));
    }
    // Keys go TO the popup while open (topmost modal wins)…
    assert_eq!(overlays.dispatch(&key_event(Key::Down)), Some(true));
    assert_eq!(keys.borrow().as_slice(), [Key::Down]);
    assert!(modal_keys.borrow().is_empty(), "modal two never saw it");
    // …Escape (unconsumed by the face) dismisses via the substrate…
    assert_eq!(overlays.dispatch(&key_event(Key::Escape)), Some(true));
    assert!(!popup.is_open());
    assert_eq!(reasons.borrow().as_slice(), [DismissReason::Escape]);
    // …and key ownership returns to the SECOND modal (a modal owns the
    // key whether or not a handler consumed it).
    assert!(overlays.dispatch(&key_event(Key::Up)).is_some());
    assert_eq!(modal_keys.borrow().as_slice(), [Key::Up]);
    assert_eq!(keys.borrow().as_slice(), [Key::Down], "popup heard nothing");
    root.dispose();
}

#[test]
fn outside_press_dismisses_without_acting_below() {
    let vp = Size::new(60, 20);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    let (root, popup_state) =
        create_root(|cx| open_recording_popup(&overlays, cx, vp, Rect::new(6, 6, 20, 1)));
    let (popup, _keys, reasons) = popup_state;
    let rect = popup.rect();
    // A press INSIDE is the popup's own (no dismissal).
    assert_eq!(
        overlays.dispatch(&press_at(rect.x + 1, rect.y + 1)),
        Some(true)
    );
    assert!(popup.is_open());
    // A press OUTSIDE dismisses and is SWALLOWED (never acts below).
    assert_eq!(overlays.dispatch(&press_at(0, 0)), Some(true));
    assert!(!popup.is_open());
    assert_eq!(reasons.borrow().as_slice(), [DismissReason::OutsidePress]);
    // The overlay stack is back to the root layer alone.
    assert_eq!(overlays.store().borrow().layers.len(), 1);
    root.dispose();
}

#[test]
fn dismiss_is_idempotent_and_close_spells_commit() {
    let vp = Size::new(60, 20);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    let (root, popup_state) =
        create_root(|cx| open_recording_popup(&overlays, cx, vp, Rect::new(6, 6, 20, 1)));
    let (popup, _keys, reasons) = popup_state;
    popup.close();
    popup.close(); // idempotent
    popup.dismiss(DismissReason::Escape); // too late — already ended
    assert_eq!(
        reasons.borrow().as_slice(),
        [DismissReason::Commit],
        "on_dismiss fired exactly once, with the first reason"
    );
    assert!(!popup.is_open());
    root.dispose();
}

#[test]
fn opener_scope_death_dismisses_with_anchor_gone() {
    let vp = Size::new(60, 20);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    type Held = Option<(Popup, SharedLog<DismissReason>)>;
    let holder: Rc<RefCell<Held>> = Default::default();
    let h2 = holder.clone();
    let (root, ()) = create_root(|cx| {
        // The opener dies while the popup lives — the dyn_view
        // regeneration shape.
        let opener = cx.child();
        let (popup, _keys, reasons) =
            open_recording_popup(&overlays, opener, vp, Rect::new(6, 6, 20, 1));
        *h2.borrow_mut() = Some((popup, reasons));
        opener.dispose();
    });
    let (popup, reasons) = holder.borrow().clone().expect("state");
    assert!(!popup.is_open(), "scope death closed the popup");
    assert_eq!(reasons.borrow().as_slice(), [DismissReason::AnchorGone]);
    assert_eq!(
        overlays.store().borrow().layers.len(),
        1,
        "only the root layer remains — no orphan"
    );
    root.dispose();
}

/// Cycle-3 review F9: a viewport change while a popup is open ends it
/// with `Resize` — the placement solved at open AND the captured
/// anchor rect are both stale after a resize (the popup could sit
/// off-viewport while still modal-owning all input). Exactly-once and
/// first-reason-wins hold through the resize path, and the dismissal
/// runs from INSIDE the popup's own effect (self-scope disposal — this
/// test is also the pin that the runtime tolerates it).
#[test]
fn viewport_resize_dismisses_open_popup_with_resize_reason_exactly_once() {
    let vp = Size::new(60, 20);
    super::super::super::viewport::publish_viewport(vp);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    let (root, popup_state) =
        create_root(|cx| open_recording_popup(&overlays, cx, vp, Rect::new(6, 6, 20, 1)));
    let (popup, _keys, reasons) = popup_state;
    assert!(popup.is_open());
    // The terminal shrinks while the popup is open.
    super::super::super::viewport::publish_viewport(Size::new(30, 8));
    assert!(!popup.is_open(), "resize closed the popup");
    assert_eq!(reasons.borrow().as_slice(), [DismissReason::Resize]);
    assert_eq!(
        overlays.store().borrow().layers.len(),
        1,
        "only the root layer remains — no orphan"
    );
    // Later dismissals are no-ops: exactly once, first reason wins.
    popup.dismiss(DismissReason::Escape);
    assert_eq!(reasons.borrow().as_slice(), [DismissReason::Resize]);
    root.dispose();
}

// -------------------------------------------------------------- tooltip

#[test]
fn tooltip_shows_after_delay_hides_on_leave_and_stale_timer_never_opens() {
    let vp = Size::new(40, 10);
    super::super::super::viewport::publish_viewport(vp);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    let mut tree = UiTree::new(vp);
    let (root, ()) = create_root(|cx| {
        let target = Element::new()
            .style(LayoutStyle::line(1).w(10))
            .child(text("hover me"))
            .build();
        let wrapped = Tooltip::attach(cx, &overlays, "the tip", Duration::ZERO, target);
        // Realistic mount: the wrapped widget sits inside a filling
        // column — the tooltip wrapper must stay content-tight.
        tree.mount(
            cx,
            Element::new()
                .style(LayoutStyle::column().width(Dimension::Percent(1.0)).h(vp.h))
                .child(wrapped)
                .child(text("other content"))
                .build(),
        );
    });
    tree.layout();
    let layer_count = || overlays.store().borrow().layers.len();
    assert_eq!(layer_count(), 1, "dormant: root layer only");

    // Hover arms the one-shot; the tip is NOT up until the timer fires.
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(2, 0),
        kind: MouseKind::Move,
        mods: Mods::NONE,
    }));
    flush_effects();
    assert_eq!(layer_count(), 1, "not shown before the delay elapses");
    run_due_timers(Instant::now());
    assert_eq!(layer_count(), 2, "tip layer up after the delay");
    {
        // Non-interactive DRAW layer (no tree, no focus), above the
        // stack, one row tall, wide enough for the label + padding.
        let store = overlays.store().borrow();
        let (i, layer) = store
            .layers
            .iter()
            .enumerate()
            .max_by_key(|(_, l)| l.z())
            .unwrap();
        assert!(matches!(
            store.meta[i].content,
            super::super::super::overlays::OverlayContent::Draw { .. }
        ));
        let bounds = layer.bounds();
        assert_eq!(bounds.size().h, 1);
        assert_eq!(bounds.size().w, crate::text::width("the tip") + 2);
        assert_eq!(bounds.y, 1, "below the hovered row");
    }
    // Keys never route to a tooltip (no tree to own them).
    assert_eq!(overlays.dispatch(&key_event(Key::Down)), None);

    // Leaving hides the tip.
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(30, 5),
        kind: MouseKind::Move,
        mods: Mods::NONE,
    }));
    flush_effects();
    assert_eq!(layer_count(), 1, "leave hides");

    // Leave-before-due: a stale one-shot must NOT open the tip.
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(2, 0),
        kind: MouseKind::Move,
        mods: Mods::NONE,
    }));
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(30, 5),
        kind: MouseKind::Move,
        mods: Mods::NONE,
    }));
    run_due_timers(Instant::now());
    assert_eq!(layer_count(), 1, "stale generation never opens");
    root.dispose();
    assert_eq!(layer_count(), 1, "anchor loss leaves no orphan");
}
