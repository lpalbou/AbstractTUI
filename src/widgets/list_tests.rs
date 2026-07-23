//! List tests (split file for the size budget — same crate-private
//! module as `list.rs` via `#[path]`): selection/windowing behavior,
//! sticky keys, variable heights, and the 0250 activation contract
//! (movement never activates; Enter/Space/click-on-selected do;
//! callbacks may dispose the List's scope synchronously).

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::itest_util::{key, mount_widget, mouse, render};

fn rows(canvas: &crate::ui::BufferCanvas, n: i32) -> Vec<String> {
    (0..n).map(|y| canvas.row_text(y)).collect()
}

#[test]
fn keyboard_selection_scrolls_window_and_fires_on_select() {
    let t = default_theme().tokens;
    let picked: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let p = picked.clone();
    let (_root, mut tree) = mount_widget(Size::new(12, 3), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..8).map(|i| format!("item {i}")))
                    .on_select(move |i| p.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    for _ in 0..4 {
        key(&mut tree, Key::Down);
    }
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, Size::new(12, 3));
    // Selection 4 scrolled into view (rows 2..5 visible).
    assert!(
        rows(&canvas, 3).iter().any(|r| r.contains("item 4")),
        "{:?}",
        rows(&canvas, 3)
    );
    assert_eq!(*picked.borrow(), vec![1, 2, 3, 4]);
}

#[test]
fn click_selects_by_visible_row_and_wheel_scrolls() {
    let t = default_theme().tokens;
    let mut sel_probe = None;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        let sel = cx.signal(0usize);
        sel_probe = Some(sel);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..20).map(|i| format!("row {i}")))
                    .selection(sel)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let sel = sel_probe.unwrap();
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert_eq!(sel.get_untracked(), 2);
    mouse(&mut tree, MouseKind::ScrollDown, 2, 2);
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, Size::new(12, 4));
    assert!(
        canvas.row_text(0).contains("row 3"),
        "{:?}",
        canvas.row_text(0)
    );
}

#[test]
fn selection_key_survives_data_mutation_rebuild() {
    // STICKY SELECTION (cycle 7): the key signal re-finds its item's
    // NEW index after items shift — a rebuild with an inserted row
    // keeps the same logical item selected.
    let t = default_theme().tokens;
    let mut probes = None;
    let (_root, mut tree) = mount_widget(Size::new(14, 5), |cx| {
        let data = cx.signal(vec!["alpha".to_string(), "beta".into(), "gamma".into()]);
        let sel_key = cx.signal(String::from("beta"));
        let sel_ix = cx.signal(0usize);
        probes = Some((data, sel_key, sel_ix));
        let tokens = t;
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(crate::ui::dyn_view_scoped(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
                move |gen_cx| {
                    List::new(data.get())
                        .key_fn(|_, s| s.to_string())
                        .selection_key(sel_key)
                        .selection(sel_ix)
                        .element(gen_cx, &tokens)
                        .build()
                },
            ))
            .build()
    });
    let (data, sel_key, sel_ix) = probes.unwrap();
    crate::reactive::flush_effects();
    assert_eq!(sel_ix.get_untracked(), 1, "key 'beta' resolved to index 1");
    // Mutate: insert two rows BEFORE beta -> its index becomes 3.
    data.update(|v| {
        v.insert(0, "zero".into());
        v.insert(1, "one".into());
    });
    crate::reactive::flush_effects();
    assert_eq!(
        sel_ix.get_untracked(),
        3,
        "sticky: beta re-found after mutation"
    );
    assert_eq!(sel_key.get_untracked(), "beta");
    // And selecting a different row updates the key.
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down);
    crate::reactive::flush_effects();
    assert_eq!(sel_key.get_untracked(), "gamma");
}

#[test]
fn variable_heights_window_by_content_rows_and_click_maps_rows_to_items() {
    let t = default_theme().tokens;
    let mut sel_probe = None;
    let (_root, mut tree) = mount_widget(Size::new(14, 4), |cx| {
        let sel = cx.signal(0usize);
        sel_probe = Some(sel);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..6).map(|i| format!("it {i}")))
                    .item_heights(|i, _| if i % 2 == 0 { 2 } else { 1 })
                    .selection(sel)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let sel = sel_probe.unwrap();
    tree.layout();
    let canvas = render(&mut tree, Size::new(14, 4));
    // it0 occupies rows 0-1 (h=2), it1 row 2, it2 rows 3+.
    assert!(canvas.row_text(0).contains("it 0"));
    assert_eq!(
        canvas.row_text(1).trim(),
        "│",
        "spacer row of the 2-tall item (+bar)"
    );
    assert!(canvas.row_text(2).contains("it 1"));
    // Clicking the SECOND row of it0 still selects item 0.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 1);
    assert_eq!(sel.get_untracked(), 0);
    // Clicking row 2 selects item 1 (row->item binary search).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert_eq!(sel.get_untracked(), 1);
}

#[test]
fn movement_fires_on_select_never_on_activate() {
    // 0250 ruling clause 1: selection follows movement; activation
    // is never wired to it.
    let t = default_theme().tokens;
    let selects: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let activates: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let (s, a) = (selects.clone(), activates.clone());
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..8).map(|i| format!("item {i}")))
                    .on_select(move |i| s.borrow_mut().push(i))
                    .on_activate(move |i| a.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::End);
    key(&mut tree, Key::Home);
    assert_eq!(*selects.borrow(), vec![1, 2, 7, 0]);
    assert!(
        activates.borrow().is_empty(),
        "movement must never activate: {:?}",
        activates.borrow()
    );
}

#[test]
fn enter_space_and_click_on_selected_row_activate() {
    // 0250 ruling clause 2: Enter always; Space (no toggle meaning
    // in a List); click on the ALREADY-selected row. A click on an
    // unselected row only selects.
    let t = default_theme().tokens;
    let activates: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a = activates.clone();
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..8).map(|i| format!("item {i}")))
                    .on_activate(move |i| a.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down); // selection -> 1, no activation
    assert!(activates.borrow().is_empty());
    assert!(key(&mut tree, Key::Enter), "Enter consumed when bound");
    assert_eq!(*activates.borrow(), vec![1]);
    assert!(key(&mut tree, Key::Char(' ')), "Space consumed when bound");
    assert_eq!(*activates.borrow(), vec![1, 1]);
    // Click row 3 (unselected): selects only.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 3);
    assert_eq!(*activates.borrow(), vec![1, 1]);
    // Click row 3 again (now selected): activates.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 3);
    assert_eq!(*activates.borrow(), vec![1, 1, 3]);
}

/// Double-click on a List row (app-kits 0535): SUBSUMED by the
/// click-on-selected rule — click 1 selects, click 2 lands on the
/// now-selected row and activates. With the click chain live (scripted
/// event time, as under the driver) the second press carries
/// `click_count() == 2` AND satisfies was-selected; the activation
/// must fire EXACTLY once — never once per rule.
#[test]
fn double_click_fires_on_activate_exactly_once() {
    let t = default_theme().tokens;
    let activates: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a = activates.clone();
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..8).map(|i| format!("item {i}")))
                    .on_activate(move |i| a.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let t0 = std::time::Instant::now();
    crate::ui::set_event_time(Some(t0));
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert!(activates.borrow().is_empty(), "click 1 selects only");
    crate::ui::set_event_time(Some(t0 + std::time::Duration::from_millis(100)));
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert_eq!(
        *activates.borrow(),
        vec![2],
        "double-click activates exactly once"
    );
    crate::ui::set_event_time(None);
}

#[test]
fn enter_and_space_pass_through_without_on_activate() {
    // Compatibility pin: an unbound List must leave Enter/Space to
    // the app (the pre-0250 root-shortcut workaround keeps working).
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..4).map(|i| format!("item {i}")))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    assert!(
        !key(&mut tree, Key::Enter),
        "Enter must not be consumed by an activation-less List"
    );
    assert!(
        !key(&mut tree, Key::Char(' ')),
        "Space must not be consumed by an activation-less List"
    );
}

/// The 0250 field crash, pinned: an `on_select` that synchronously
/// disposes the List's scope (the modal-picker close) must not
/// panic — all widget bookkeeping (ensure-visible `offset.update`)
/// lands BEFORE the callback (ruling clause 4).
#[test]
fn on_select_may_dispose_the_lists_scope() {
    let t = default_theme().tokens;
    let mut tree = crate::ui::UiTree::new(Size::new(12, 4));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let picker_cx = cx.child();
        let view = List::of((0..8).map(|i| format!("item {i}")))
            .on_select(move |_| picker_cx.dispose())
            .element(picker_cx, &t)
            .build();
        tree.mount(picker_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down); // fires on_select -> dispose, mid-dispatch
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

/// Ruling clause 4 for the activation event: Enter-activate and
/// click-activate callbacks may dispose the List's scope.
#[test]
fn on_activate_may_dispose_the_lists_scope() {
    let t = default_theme().tokens;
    // Enter path.
    let mut tree = crate::ui::UiTree::new(Size::new(12, 4));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let picker_cx = cx.child();
        let view = List::of((0..8).map(|i| format!("item {i}")))
            .on_activate(move |_| picker_cx.dispose())
            .element(picker_cx, &t)
            .build();
        tree.mount(picker_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Enter); // activate -> dispose, mid-dispatch
    assert_eq!(tree.instance_count(), 0);
    root.dispose();
    // Click-on-selected path (mouse post-dispatch bookkeeping must
    // tolerate the disposed target too).
    let mut tree = crate::ui::UiTree::new(Size::new(12, 4));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let picker_cx = cx.child();
        let view = List::of((0..8).map(|i| format!("item {i}")))
            .on_activate(move |_| picker_cx.dispose())
            .element(picker_cx, &t)
            .build();
        tree.mount(picker_cx, view);
    });
    tree.layout();
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0); // row 0 = selected
    assert_eq!(tree.instance_count(), 0);
    // A follow-up event over the dead tree stays inert.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 0);
    root.dispose();
}

#[test]
fn empty_list_ignores_movement_and_activation_keys() {
    // Movement on an empty list used to index past the prefix sums;
    // activation has nothing to commit. Both must be inert.
    let t = default_theme().tokens;
    let activates: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a = activates.clone();
    let (_root, mut tree) = mount_widget(Size::new(12, 3), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::new(Vec::new())
                    .on_activate(move |i| a.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::End);
    key(&mut tree, Key::Enter);
    key(&mut tree, Key::Char(' '));
    assert!(activates.borrow().is_empty());
}

#[test]
fn scroll_to_command_scrolls_and_consumes() {
    let t = default_theme().tokens;
    let mut probe = None;
    let (_root, mut tree) = mount_widget(Size::new(12, 3), |cx| {
        let req = cx.signal(None::<usize>);
        probe = Some(req);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..30).map(|i| format!("row {i}")))
                    .scroll_to(req)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let req = probe.unwrap();
    req.set(Some(20));
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, Size::new(12, 3));
    assert!(
        canvas.row_text(0).contains("row 20"),
        "{:?}",
        canvas.row_text(0)
    );
    assert_eq!(req.get_untracked(), None, "request consumed");
}
