//! List tests (split file for the size budget — same crate-private
//! module as `list.rs` via `#[path]`): selection/windowing behavior,
//! sticky keys, variable heights, and the 0250 activation contract
//! (movement never activates; Enter/Space/click-on-selected do;
//! callbacks may dispose the List's scope synchronously).

use super::*;
use crate::base::{Point, Size};
use crate::theme::default_theme;
use crate::widgets::itest_util::{key, mount_widget, mouse, render, settle};

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
    // The spacer row carries no text — only the scrollbar column (track
    // or thumb: the 1320 thumb floor makes this row thumb at this
    // geometry, and which glyph lands here is the bar's business).
    assert!(
        matches!(canvas.row_text(1).trim(), "▏" | "█"),
        "spacer row of the 2-tall item holds only the bar: {:?}",
        canvas.row_text(1)
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
    // Measure the viewport first (the probe publishes on a latched
    // `after(0)`), so the request resolves against a real height.
    let _ = settle(&mut tree, Size::new(12, 3));
    req.set(Some(20));
    let canvas = settle(&mut tree, Size::new(12, 3));
    let visible: String = (0..3).map(|y| canvas.row_text(y)).collect();
    assert!(
        visible.contains("row 20"),
        "row 20 in viewport: {:?}",
        visible
    );
    assert_eq!(req.get_untracked(), None, "request consumed");
}

/// Wave-12 reentrancy pin (the SharedCallback held-borrow contract in
/// widgets/mod.rs): an `on_select` that WRITES THE CONTROLLED
/// SELECTION SIGNAL and then forces a synchronous `flush_effects()`
/// mid-callback. The hosting `dyn_view_scoped` region rebuilds —
/// disposing this very List's generation — while the widget's
/// `on_select` borrow is held and its handler frame is live. Safe
/// because dispatch's Rc clones keep the executing closures and the
/// slot alive through the disposal, and the rebuilt generation owns
/// the next interaction. If effects ever flush eagerly inside
/// dispatch (breaking fact 2 of the contract), this is the tripwire.
#[test]
fn callback_reentrancy_pin_controlled_write_and_flush_mid_callback() {
    use crate::ui::dyn_view_scoped;
    let t = default_theme().tokens;
    let sel_holder: Rc<RefCell<Option<crate::reactive::Signal<usize>>>> = Default::default();
    let sh = sel_holder.clone();
    let picks: Rc<RefCell<Vec<usize>>> = Default::default();
    let pk = picks.clone();
    let (_root, mut tree) = mount_widget(Size::new(14, 4), move |cx| {
        let sel = cx.signal(0usize);
        *sh.borrow_mut() = Some(sel);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(dyn_view_scoped(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
                move |gen_cx| {
                    let _generation_key = sel.get(); // rebuild per selection write
                    let pk = pk.clone();
                    Element::new()
                        .child(
                            List::of((0..9).map(|i| format!("item {i}")))
                                .selection(sel)
                                .on_select(move |i| {
                                    pk.borrow_mut().push(i);
                                    if i != 7 {
                                        sel.set(7); // write the CONTROLLED signal…
                                        crate::reactive::flush_effects(); // …and rebuild NOW
                                    }
                                })
                                .element(gen_cx, &t)
                                .build(),
                        )
                        .build()
                },
            ))
            .build()
    });
    let sel = sel_holder.borrow().unwrap();
    key(&mut tree, Key::Tab); // focus the list
    key(&mut tree, Key::Down); // select 1 -> callback forces 7 + rebuild
    assert_eq!(*picks.borrow(), vec![1], "first pick delivered");
    assert_eq!(sel.get_untracked(), 7, "mid-callback write landed");
    // The rebuilt generation is live and interactive.
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down); // 7 -> 8 -> callback forces 7 again
    assert_eq!(*picks.borrow(), vec![1, 8], "rebuilt list still fires");
    assert_eq!(sel.get_untracked(), 7);
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, Size::new(14, 4));
    assert!(
        (0..4).any(|y| canvas.row_text(y).contains("item")),
        "rebuilt list renders"
    );
}

#[test]
fn accessory_click_does_not_change_selection() {
    let t = default_theme().tokens;
    let mut sel_probe = None;
    let accessory_hits: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let hits = accessory_hits.clone();
    let (_root, mut tree) = mount_widget(Size::new(20, 4), |cx| {
        let sel = cx.signal(0usize);
        sel_probe = Some(sel);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta", "gamma"])
                    .accessory_width(3)
                    .row_accessory(|_, _| Some("×".into()))
                    .selection(sel)
                    .on_accessory_click(move |i| hits.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let sel = sel_probe.unwrap();
    // Width 20, no scrollbar → body 17 + accessory 3 (cells 17..=19).
    // The WHOLE column is the target, not just the glyph cell: a 1-cell
    // hit area is unusable with a mouse, and hover ink lights the same
    // span, so what the pointer highlights is what a press acts on.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 19, 0);
    assert_eq!(*accessory_hits.borrow(), vec![0]);
    assert_eq!(sel.get_untracked(), 0, "accessory must not move selection");
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 17, 1);
    assert_eq!(
        *accessory_hits.borrow(),
        vec![0, 1],
        "the accessory column is one target edge to edge"
    );
    assert_eq!(sel.get_untracked(), 0, "accessory must not move selection");
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 1);
    assert_eq!(sel.get_untracked(), 1, "body click selects");
}

/// Hover ink and clicks resolve through the SAME hit test, so the row
/// the pointer lights is the row a press acts on — no dead cells that
/// look inert and select, and no lit cells that do nothing.
#[test]
fn hover_zone_matches_the_zone_a_click_acts_on() {
    let t = default_theme().tokens;
    let hits: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = hits.clone();
    let (_root, mut tree) = mount_widget(Size::new(20, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta", "gamma"])
                    .accessory_width(3)
                    // Only row 1 carries an accessory: the other rows'
                    // trailing column is plain body and must select.
                    .row_accessory(|i, _| (i == 1).then(|| "×".into()))
                    .on_accessory_click(move |i| sink.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    // Row 1 has an accessory: the trailing column fires it.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 18, 1);
    assert_eq!(*hits.borrow(), vec![1]);
    // Row 0 has none: the same column is body and selects instead.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 18, 0);
    assert_eq!(*hits.borrow(), vec![1], "no accessory on this row to hit");
}

#[test]
fn timed_row_double_click_supersedes_on_activate() {
    let t = default_theme().tokens;
    let activates: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let doubles: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let (a, d) = (activates.clone(), doubles.clone());
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
                    .on_row_double_click(move |i| d.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let t0 = std::time::Instant::now();
    crate::ui::set_event_time(Some(t0));
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    crate::ui::set_event_time(Some(t0 + std::time::Duration::from_millis(100)));
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert!(activates.borrow().is_empty(), "double uses timed path only");
    assert_eq!(*doubles.borrow(), vec![2]);
    crate::ui::set_event_time(None);
}

#[test]
fn scrollbar_column_does_not_select_row() {
    let t = default_theme().tokens;
    let mut sel_probe = None;
    let (_root, mut tree) = mount_widget(Size::new(12, 3), |cx| {
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
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 11, 1);
    assert_eq!(sel.get_untracked(), 0, "scrollbar column is inert");
}

#[test]
fn rich_items_render_styled_body() {
    use crate::render::rich::{RichLine, RichText, Span};
    use crate::render::Style as Ink;
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(24, 3), |cx| {
        let rich = vec![
            RichText::from_lines(vec![RichLine::from_spans(vec![
                Span::plain("● "),
                Span::new("alpha", Ink::new().fg(t.accent)),
            ])]),
            RichText::from_lines(vec![RichLine::from_spans(vec![Span::plain("beta")])]),
        ];
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta"])
                    .rich_items(rich)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, Size::new(24, 3));
    assert!(canvas.row_text(0).contains("alpha"));
}

#[test]
fn scroll_to_tail_keeps_row_in_viewport() {
    let t = default_theme().tokens;
    let mut scroll_req = None;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        let req = cx.signal(None::<usize>);
        scroll_req = Some(req);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..30).map(|i| format!("row {i:02}")))
                    .scroll_to(req)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let _ = settle(&mut tree, Size::new(12, 4)); // measure viewport height
    scroll_req.unwrap().set(Some(29));
    let canvas = settle(&mut tree, Size::new(12, 4));
    assert!(
        canvas.row_text(3).contains("row 29"),
        "tail row visible: {:?}",
        (0..4).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
}

#[test]
fn wheel_at_top_does_not_consume_scroll_up() {
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..20).map(|i| format!("row {i}")))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    mouse(&mut tree, MouseKind::Move, 2, 1);
    assert!(
        !mouse(&mut tree, MouseKind::ScrollUp, 2, 1),
        "at offset 0, scroll-up should bubble to parent scrollers"
    );
}

#[test]
fn wheel_when_scrollable_consumes_scroll_down() {
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of((0..20).map(|i| format!("row {i}")))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    mouse(&mut tree, MouseKind::Move, 2, 1);
    assert!(
        mouse(&mut tree, MouseKind::ScrollDown, 2, 1),
        "scroll-down with room to move should stay on the list"
    );
}

/// Removing the SELECTED row must never leave `selection` naming a row
/// that no longer exists: nothing would highlight, `access_value` would
/// announce a phantom row, and the first arrow key would move the wrong
/// way. Selection holds the slot, clamped into the shorter list.
#[test]
fn removing_the_selected_row_settles_selection_into_range() {
    let t = default_theme().tokens;
    let mut probes = None;
    let (_root, mut tree) = mount_widget(Size::new(16, 5), |cx| {
        let data = cx.signal(vec!["a".to_string(), "b".into(), "c".into()]);
        let sel_key = cx.signal(String::from("c"));
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
                        .on_remove(move |i| {
                            data.update(|v| {
                                v.remove(i);
                            })
                        })
                        .element(gen_cx, &tokens)
                        .build()
                },
            ))
            .build()
    });
    let (data, sel_key, sel_ix) = probes.unwrap();
    crate::reactive::flush_effects();
    assert_eq!(
        sel_ix.get_untracked(),
        2,
        "key 'c' resolved to the last row"
    );

    // Remove the LAST row while it is selected: the key is gone, so
    // selection falls to the new last row rather than dangling at 2.
    data.update(|v| {
        v.remove(2);
    });
    crate::reactive::flush_effects();
    assert_eq!(sel_ix.get_untracked(), 1, "clamped to the new last row");
    assert_eq!(sel_key.get_untracked(), "b", "key rewritten to that row");

    // Remove a MIDDLE row while selected: the slot is kept, so the row
    // that slid up into it is now selected.
    data.update(|v| {
        v.remove(1);
    });
    crate::reactive::flush_effects();
    assert_eq!(sel_ix.get_untracked(), 0);
    assert_eq!(sel_key.get_untracked(), "a");

    // And the highlight is really painted on a real row.
    let canvas = settle(&mut tree, Size::new(16, 5));
    assert!(canvas.row_text(0).contains('a'));
}

/// `on_remove` is the one-call form: it draws the ✕ itself and routes
/// the click, with no `row_accessory`/`accessory_width` boilerplate.
#[test]
fn on_remove_draws_the_dismiss_glyph_and_fires_on_click() {
    let t = default_theme().tokens;
    let removed: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = removed.clone();
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta"])
                    .on_remove(move |i| sink.borrow_mut().push(i))
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, Size::new(12, 4));
    assert!(
        canvas.row_text(0).contains('✕'),
        "on_remove draws its own glyph: {:?}",
        canvas.row_text(0)
    );
    // Trailing column of row 1 (width 12, no bar → body 10 + accessory 2).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 11, 1);
    assert_eq!(*removed.borrow(), vec![1]);
}

/// Removing a row must not scroll the reader back to the top. The
/// rebuild that re-derives selection must not also re-derive the
/// viewport — so the offset is bindable, exactly like `Scroll::offset_y`.
#[test]
fn removing_a_row_while_scrolled_keeps_the_viewport() {
    let t = default_theme().tokens;
    let mut probes = None;
    let (_root, mut tree) = mount_widget(Size::new(14, 4), |cx| {
        let data = cx.signal((0..20).map(|i| format!("ch-{i:02}")).collect::<Vec<_>>());
        let oy = cx.signal(0i32);
        probes = Some((data, oy));
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
                move |gcx| {
                    List::of(data.get())
                        .offset_y(oy)
                        .on_remove(move |i| {
                            data.update(|v| {
                                v.remove(i);
                            })
                        })
                        .element(gcx, &tokens)
                        .build()
                },
            ))
            .build()
    });
    let (_data, oy) = probes.unwrap();
    let _ = settle(&mut tree, Size::new(14, 4));
    // Scroll down, then dismiss the row now at the top of the viewport.
    oy.set(8);
    let canvas = settle(&mut tree, Size::new(14, 4));
    assert!(
        canvas.row_text(0).contains("ch-08"),
        "{:?}",
        canvas.row_text(0)
    );
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 12, 0);
    let canvas = settle(&mut tree, Size::new(14, 4));
    assert_eq!(
        oy.get_untracked(),
        8,
        "the viewport must not jump to the top"
    );
    assert!(
        canvas.row_text(0).contains("ch-09"),
        "the row below slid up into place: {:?}",
        canvas.row_text(0)
    );
}

/// Hover ink, asserted on PIXELS. The accessory belongs to the row, so
/// pointing at the ✕ must not un-highlight the row — and the ✕ must be
/// more lit when you point at it, not less.
#[test]
fn hover_ink_lights_the_row_and_marks_the_hot_zone() {
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta"])
                    .on_remove(|_| {})
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    // Width 12, no scrollbar (2 items ≤ 4 rows) → body 10 + accessory 2.
    let cell = |c: &crate::ui::BufferCanvas, x: i32| {
        let (_, fg, _bg) = c.cell(Point::new(x, 1)).expect("cell");
        (fg, c.attrs_at(Point::new(x, 1)))
    };

    let rest = render(&mut tree, Size::new(12, 4));
    let (rest_body, _) = cell(&rest, 0);
    assert_eq!(rest_body, t.text, "at rest the body is plain text ink");

    // Pointer on the row BODY: row takes accent, body is bold.
    mouse(&mut tree, MouseKind::Move, 1, 1);
    let hot = render(&mut tree, Size::new(12, 4));
    assert_eq!(cell(&hot, 0), (t.accent, Attrs::BOLD), "hot body row");
    assert_eq!(
        cell(&hot, 11),
        (t.accent, Attrs::NONE),
        "the ✕ shares the row ink but is not the hot zone"
    );

    // Pointer on the ✕: the row STAYS lit, and the ✕ goes error+bold.
    mouse(&mut tree, MouseKind::Move, 11, 1);
    let hot = render(&mut tree, Size::new(12, 4));
    assert_eq!(
        cell(&hot, 0),
        (t.accent, Attrs::NONE),
        "the row must not go dark when the pointer enters its accessory"
    );
    assert_eq!(
        cell(&hot, 11),
        (t.error, Attrs::BOLD),
        "a destructive dismiss takes error ink, not the neutral accent"
    );
}

/// A non-destructive accessory (an unread badge) must NOT be painted in
/// the error color — red would misreport the consequence.
#[test]
fn a_plain_row_accessory_takes_accent_not_error() {
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(Size::new(12, 4), |cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                List::of(["alpha", "beta"])
                    .accessory_width(2)
                    .row_accessory(|_, _| Some("2".into()))
                    .on_accessory_click(|_| {})
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    mouse(&mut tree, MouseKind::Move, 11, 1);
    let hot = render(&mut tree, Size::new(12, 4));
    let (_, fg, _bg) = hot.cell(Point::new(11, 1)).expect("cell");
    let attrs = hot.attrs_at(Point::new(11, 1));
    assert_eq!(
        (fg, attrs),
        (t.accent, Attrs::BOLD),
        "badge is not a dismiss"
    );
}

/// The strip used to be inert by design: `List` painted a scrollbar it
/// refused to steer, while `Scroll` painted the same glyph and dragged.
/// One seam, one gesture — the bar drags everywhere it is drawn.
#[test]
fn the_scrollbar_strip_grabs_and_drags_the_window() {
    let t = default_theme().tokens;
    let size = Size::new(12, 6);
    let off_probe: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let out = off_probe.clone();
    let (_root, mut tree) = mount_widget(size, |cx| {
        let off = cx.signal(0i32);
        *out.borrow_mut() = Some(off);
        List::of((0..60).map(|i| format!("item {i}")))
            .offset_y(off)
            .element(cx, &t)
            .build()
    });
    let off = off_probe.borrow().unwrap();
    let bar_x = size.w - 1;
    // Bare track, low down: a teleport that lands the thumb on the row
    // pressed (never the row-selection the press used to fall through to).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 4);
    let after_press = off.get_untracked();
    assert!(after_press > 0, "the strip steers the window now");
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.row_text(4).chars().nth(bar_x as usize),
        Some('█'),
        "row 4 was pressed, row 4 wears the thumb"
    );
    // ...and the drag that follows tracks the pointer.
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), bar_x, 0);
    assert_eq!(off.get_untracked(), 0, "dragged back to the head");
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), bar_x, 0);
    // A bare drag after the release belongs to nobody: it must not steer.
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), bar_x, 5);
    assert_eq!(off.get_untracked(), 0, "no grab, no steering");
}
