//! Overlay store unit tests (split from overlays.rs for the file
//! budget; `#[path]`-included as its `tests` module).
use super::*;
use crate::render::Style;
use crate::ui::{text, MouseEvent, MouseKind, UiEvent};

#[test]
fn handles_survive_store_drop_and_double_remove() {
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(10, 4));
    let h = overlays.layer(5, Rect::new(1, 1, 4, 2));
    assert!(h.is_alive());
    h.remove();
    assert!(!h.is_alive());
    h.remove(); // second remove: silent no-op
    h.set_opacity(0.5); // op on a dead layer: no-op
    let h2 = overlays.layer(5, Rect::new(0, 0, 2, 2));
    drop(overlays);
    // Weak store gone: every op degrades to a no-op, never a panic.
    h2.set_visible(false);
    assert!(!h2.is_alive());
}

#[test]
fn removing_a_layer_damages_the_root_underneath() {
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(10, 4));
    // Drain root's initial damage.
    let mut scratch = Vec::new();
    {
        let mut store = overlays.store().borrow_mut();
        let i = store.index_of(ROOT_LAYER_ID).unwrap();
        store.layers[i].surface_mut().take_damage(&mut scratch);
    }
    let h = overlays.layer(5, Rect::new(2, 1, 4, 2));
    h.remove();
    let mut store = overlays.store().borrow_mut();
    let i = store.index_of(ROOT_LAYER_ID).unwrap();
    scratch.clear();
    store.layers[i].surface_mut().take_damage(&mut scratch);
    assert!(
        scratch.iter().any(|r| r.intersects(Rect::new(2, 1, 4, 2))),
        "vacated region must repaint from the root: {scratch:?}"
    );
}

#[test]
fn draw_layer_repaints_only_when_damaged() {
    use std::cell::Cell as StdCell;
    let overlays = Overlays::new();
    let paints: Rc<StdCell<u32>> = Rc::new(StdCell::new(0));
    let p2 = paints.clone();
    let h = overlays.layer_draw(1, Rect::new(0, 0, 6, 1), move |canvas, rect| {
        p2.set(p2.get() + 1);
        canvas.print_styled(rect.origin(), "toast", &Style::new());
    });
    overlays.draw_all();
    assert_eq!(paints.get(), 1, "initial paint");
    overlays.draw_all();
    assert_eq!(paints.get(), 1, "clean layer skips the closure");
    h.damage();
    overlays.draw_all();
    assert_eq!(paints.get(), 2, "damage() re-runs the closure");
    // The painted content actually landed on the layer surface.
    let store = overlays.store().borrow();
    let i = store.index_of(h.id).unwrap();
    let surface = store.layers[i].surface();
    let cell = surface.get(0, 0).expect("cell in bounds");
    assert_eq!(surface.glyph_str(cell), "t");
}

#[test]
fn top_z_tracks_the_live_maximum() {
    let overlays = Overlays::new();
    assert_eq!(overlays.top_z(), 0, "empty store: baseline 0");
    overlays.ensure_root(Size::new(10, 4));
    assert_eq!(overlays.top_z(), 0, "root layer sits at z 0");
    let a = overlays.layer(5, Rect::new(0, 0, 2, 1));
    let b = overlays.layer(1500, Rect::new(0, 0, 2, 1));
    assert_eq!(overlays.top_z(), 1500, "highest live z wins");
    b.remove();
    assert_eq!(overlays.top_z(), 5, "removal re-derives the maximum");
    a.remove();
    assert_eq!(overlays.top_z(), 0);
}

#[test]
fn non_modal_overlay_with_focus_owns_keys_until_outside_press() {
    use crate::reactive::create_root;
    use crate::ui::{Key, KeyEvent};
    let overlays = Overlays::new();
    let keys: Rc<std::cell::RefCell<u32>> = Rc::new(std::cell::RefCell::new(0));
    let k2 = keys.clone();
    let (root, ()) = create_root(|cx| {
        let view = crate::ui::Element::new()
            .style(
                crate::layout::Style::default()
                    .width(crate::layout::Dimension::Percent(1.0))
                    .height(crate::layout::Dimension::Percent(1.0)),
            )
            .focusable()
            .on(crate::ui::Phase::Bubble, move |c, e| {
                if matches!(e, UiEvent::Key(_)) {
                    *k2.borrow_mut() += 1;
                    c.stop_propagation();
                }
            })
            .child(text("popup"))
            .build();
        overlays.layer_tree(10, Rect::new(4, 1, 10, 2), false, cx, view);
    });
    let key_event = UiEvent::Key(KeyEvent::new(Key::Char('x'), crate::ui::Mods::NONE));
    // Unfocused overlay: keys fall through to the root.
    assert_eq!(overlays.dispatch(&key_event), None);
    assert_eq!(*keys.borrow(), 0);
    // Click inside focuses the overlay's tree (click-to-focus)...
    let click = |pos: Point| {
        UiEvent::Mouse(MouseEvent {
            kind: MouseKind::Down(crate::ui::MouseButton::Left),
            pos,
            mods: crate::ui::Mods::NONE,
        })
    };
    // (owned by the overlay whether or not a handler consumed it)
    assert!(overlays.dispatch(&click(Point::new(5, 2))).is_some());
    // ...and now the overlay owns keys.
    assert_eq!(overlays.dispatch(&key_event), Some(true));
    assert_eq!(*keys.borrow(), 1);
    // A press OUTSIDE (root territory) steals key focus back.
    assert_eq!(overlays.dispatch(&click(Point::new(0, 0))), None);
    assert_eq!(
        overlays.dispatch(&key_event),
        None,
        "keys fall to the root again"
    );
    assert_eq!(*keys.borrow(), 1);
    root.dispose();
}

#[test]
fn non_modal_tree_overlay_owns_pointer_inside_bounds_only() {
    use crate::reactive::create_root;
    let overlays = Overlays::new();
    let hits: Rc<std::cell::RefCell<Vec<Point>>> = Rc::new(std::cell::RefCell::new(Vec::new()));
    let h2 = hits.clone();
    let (root, ()) = create_root(|cx| {
        // Container listener: Bubble phase (Target would only hear
        // hits landing on the container itself, not its children).
        let view = crate::ui::Element::new()
            .style(
                crate::layout::Style::default()
                    .width(crate::layout::Dimension::Percent(1.0))
                    .height(crate::layout::Dimension::Percent(1.0)),
            )
            .on(crate::ui::Phase::Bubble, move |c, e| {
                if matches!(e, UiEvent::Mouse(_)) {
                    // Overlay handlers see LAYER-LOCAL positions.
                    h2.borrow_mut().push(c.target_rect().origin());
                    c.stop_propagation();
                }
            })
            .child(text("panel"))
            .build();
        overlays.layer_tree(10, Rect::new(5, 2, 8, 2), false, cx, view);
    });
    let click = |pos: Point| {
        UiEvent::Mouse(MouseEvent {
            kind: MouseKind::Down(crate::ui::MouseButton::Left),
            pos,
            mods: crate::ui::Mods::NONE,
        })
    };
    // Inside the layer: owned by the overlay (consumed here).
    assert_eq!(overlays.dispatch(&click(Point::new(6, 3))), Some(true));
    assert_eq!(hits.borrow().len(), 1);
    // Outside: falls through to the caller's root tree untouched.
    assert_eq!(overlays.dispatch(&click(Point::new(0, 0))), None);
    assert_eq!(hits.borrow().len(), 1);
    root.dispose();
}
