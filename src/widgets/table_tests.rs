//! Table widget tests (`#[path]` sibling — the file-size law).
//! Selection/sort coverage plus the 0535 activation suite:
//! Enter/Space when bound (pass-through unbound, the 0980 pin),
//! double-click exactly-once, the adjacent-row guard, wheel reset,
//! the no-time-source isolation rule, and disposal safety.

use super::*;
use crate::base::{Point, Size};
use crate::theme::default_theme;
use crate::ui::{set_event_time, Key, MouseKind, UiTree};
use crate::widgets::itest_util::{click, key, mount_widget, mouse, render};
use std::time::{Duration, Instant};

fn sample(cx: Scope, on_sort: impl FnMut(usize) + 'static) -> Element {
    let t = &default_theme().tokens;
    Table::new(vec![
        Column::new("name", ColWidth::Flex(1.0)),
        Column::new("size", ColWidth::Cells(6)),
    ])
    .rows(
        (0..12)
            .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
            .collect(),
    )
    .sorted(0, true)
    .on_sort_requested(on_sort)
    .element(cx, t)
}

fn activating(cx: Scope, on_activate: impl FnMut(usize) + 'static) -> Element {
    let t = &default_theme().tokens;
    Table::new(vec![
        Column::new("name", ColWidth::Flex(1.0)),
        Column::new("size", ColWidth::Cells(6)),
    ])
    .rows(
        (0..12)
            .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
            .collect(),
    )
    .on_activate(on_activate)
    .element(cx, t)
}

/// Scripted-time click (the ambient event clock the driver would
/// publish): tests own double-click timing deterministically.
fn click_at(tree: &mut UiTree, x: i32, y: i32, t: Instant) {
    set_event_time(Some(t));
    click(tree, x, y);
}

#[test]
fn header_body_selection_and_indicator_render() {
    let size = Size::new(20, 5);
    let (_root, mut tree) = mount_widget(size, |cx| sample(cx, |_| {}).build());
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(0).contains("name▲"),
        "{:?}",
        canvas.row_text(0)
    );
    assert!(canvas.row_text(0).contains("size"));
    assert!(canvas.row_text(1).starts_with("file-0"));
    assert!(
        canvas.attrs_at(Point::new(0, 0)).contains(Attrs::BOLD),
        "header renders bold"
    );
    // Selected row 0 wears selection ground.
    let theme = default_theme();
    assert_eq!(
        canvas.cell(Point::new(0, 1)).unwrap().2,
        theme.tokens.selection_bg
    );
}

#[test]
fn keyboard_navigates_rows_with_ensure_visible() {
    let size = Size::new(20, 5); // 4 body rows
    let (_root, mut tree) = mount_widget(size, |cx| sample(cx, |_| {}).build());
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::End);
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(4).starts_with("file-11"),
        "{:?}",
        canvas.row_text(4)
    );
}

#[test]
fn s_key_requests_sort_round_robin_from_the_sorted_column() {
    // Keyboard parity for header-click sorting (a11y audit): 's'
    // fires on_sort_requested on the column AFTER the sorted one.
    let size = Size::new(20, 5);
    let requested: std::rc::Rc<std::cell::RefCell<Vec<usize>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let r = requested.clone();
    let (_root, mut tree) = mount_widget(size, |cx| {
        sample(cx, move |c| r.borrow_mut().push(c)).build()
    });
    key(&mut tree, Key::Tab); // focus the table
    key(&mut tree, Key::Char('s'));
    key(&mut tree, Key::Char('s'));
    // sorted(0) is a static prop in this build, so each press asks
    // for column 1 (the app would update `sorted` between presses).
    assert_eq!(*requested.borrow(), vec![1, 1]);
}

#[test]
fn header_click_requests_sort_body_click_selects() {
    let size = Size::new(20, 5);
    let sorts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let s2 = sorts.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        sample(cx, move |c| s2.borrow_mut().push(c)).build()
    });
    click(&mut tree, 2, 0); // header, first (flex) column
    assert_eq!(*sorts.borrow(), vec![0]);
    click(&mut tree, 15, 0); // header, "size" column
    assert_eq!(*sorts.borrow(), vec![0, 1]);
    click(&mut tree, 2, 3); // body row 2
    let theme = default_theme();
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, 3)).unwrap().2,
        theme.tokens.selection_bg
    );
}

/// Disposal-safety law (backlog 0297): `on_sort_requested` is the
/// LAST thing its arms run (no widget write follows on either the
/// 's'-key or the header-click path), so the callback may dispose
/// the Table's scope synchronously. Audited clean at filing; pinned.
#[test]
fn on_sort_requested_may_dispose_the_tables_scope() {
    let t = default_theme().tokens;
    let mut tree = crate::ui::UiTree::new(Size::new(20, 5));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let view = Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..3)
                .map(|i| vec![format!("f{i}"), format!("{i}")])
                .collect(),
        )
        .on_sort_requested(move |_| modal_cx.dispose())
        .element(modal_cx, &t)
        .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    key(&mut tree, Key::Tab); // focus
    key(&mut tree, Key::Char('s')); // sort request -> dispose
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

/// 0250 ruling clause 4 mirrored onto Table: `on_select` runs AFTER
/// all widget bookkeeping (the ensure-visible `offset.update` used
/// to run after the callback — the same disposal hazard the List
/// field report names), so a callback may dispose the Table's scope
/// synchronously.
#[test]
fn on_select_may_dispose_the_tables_scope() {
    let t = default_theme().tokens;
    let mut tree = crate::ui::UiTree::new(Size::new(20, 5));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let picker_cx = cx.child();
        let view = Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..12)
                .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
                .collect(),
        )
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

#[test]
fn enter_and_space_activate_the_selected_row_when_bound() {
    let size = Size::new(20, 5);
    let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a2 = activated.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        activating(cx, move |i| a2.borrow_mut().push(i)).build()
    });
    key(&mut tree, Key::Tab); // focus the table
    key(&mut tree, Key::Down); // select row 1 (movement never commits)
    assert!(activated.borrow().is_empty(), "movement must not activate");
    key(&mut tree, Key::Enter);
    assert_eq!(*activated.borrow(), vec![1], "Enter activates");
    key(&mut tree, Key::Char(' '));
    assert_eq!(
        *activated.borrow(),
        vec![1, 1],
        "Space aliases Enter (single-select table: no toggle meaning)"
    );
}

/// The 0980 lesson pinned for the NEW keys: an activation-less
/// Table must leave Enter/Space to app shortcuts — a claimed key
/// without a consumer is a silently dead screen binding.
#[test]
fn enter_passes_through_to_app_shortcuts_when_on_activate_unbound() {
    let size = Size::new(20, 5);
    let confirmed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let c2 = confirmed.clone();
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let c3 = c2.clone();
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .shortcut(crate::ui::KeyChord::plain(Key::Enter), move |_| {
                *c3.borrow_mut() += 1;
            })
            .child(
                Table::new(vec![Column::new("name", ColWidth::Flex(1.0))])
                    .rows((0..3).map(|i| vec![format!("f{i}")]).collect())
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab); // focus the table
    key(&mut tree, Key::Enter);
    assert_eq!(
        *confirmed.borrow(),
        1,
        "unbound Table must not consume Enter"
    );
}

/// The 0980 finding itself, retrofitted: a Table with NO
/// `on_sort_requested` must leave `s` to app shortcuts (the
/// gateway console binds `s` = steer; the table was eating it).
#[test]
fn s_passes_through_to_app_shortcuts_when_no_sort_handler() {
    let size = Size::new(20, 5);
    let steered: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let s2 = steered.clone();
    let t = default_theme().tokens;
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let s3 = s2.clone();
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .shortcut(crate::ui::KeyChord::plain(Key::Char('s')), move |_| {
                *s3.borrow_mut() += 1;
            })
            .child(
                Table::new(vec![Column::new("name", ColWidth::Flex(1.0))])
                    .rows((0..3).map(|i| vec![format!("f{i}")]).collect())
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab); // focus the table
    key(&mut tree, Key::Char('s'));
    assert_eq!(
        *steered.borrow(),
        1,
        "a sort-less Table must not consume 's'"
    );
}

#[test]
fn double_click_activates_once_and_slow_or_single_clicks_never() {
    let size = Size::new(20, 5);
    let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a2 = activated.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        activating(cx, move |i| a2.borrow_mut().push(i)).build()
    });
    let t0 = Instant::now();
    // Click 1 (body y=3 → row 2): selects, never activates.
    click_at(&mut tree, 2, 3, t0);
    assert!(activated.borrow().is_empty(), "single click only selects");
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, 3)).unwrap().2,
        default_theme().tokens.selection_bg,
        "click 1 moved the selection (never suppressed by the chain)"
    );
    // Click 2, same cell, 100ms later: the double-click's second
    // press — activates EXACTLY once.
    click_at(&mut tree, 2, 3, t0 + Duration::from_millis(100));
    assert_eq!(*activated.borrow(), vec![2], "double-click activates");
    // A SLOW second click on the selected row (past the 400ms
    // window, measured from the last press): selects only — the
    // deliberate divergence from List's picker gesture.
    click_at(&mut tree, 2, 3, t0 + Duration::from_millis(2000));
    assert_eq!(
        *activated.borrow(),
        vec![2],
        "slow re-click on the selected row must not activate"
    );
    set_event_time(None);
}

/// The row guard: a chained second press that drifted onto a
/// NEIGHBOR row (1 cell = inside the chain's default tolerance, but
/// a different row) re-selects and must NOT activate — fast
/// click-walking down adjacent rows is browsing, not commitment.
#[test]
fn chained_press_on_an_adjacent_row_selects_but_never_activates() {
    let size = Size::new(20, 5);
    let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a2 = activated.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        activating(cx, move |i| a2.borrow_mut().push(i)).build()
    });
    let t0 = Instant::now();
    click_at(&mut tree, 2, 2, t0); // row 1
    click_at(&mut tree, 2, 3, t0 + Duration::from_millis(80)); // row 2
    assert!(
        activated.borrow().is_empty(),
        "cross-row chain must not activate: {:?}",
        activated.borrow()
    );
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(0, 3)).unwrap().2,
        default_theme().tokens.selection_bg,
        "the second press still selected its row"
    );
    set_event_time(None);
}

/// A wheel between two presses resets the chain (content moves
/// under the pointer — "the same cell" is not the same row): even a
/// clamped no-op scroll resets, because the tree-level chain cannot
/// know what the widget did with the wheel.
#[test]
fn wheel_between_clicks_resets_the_chain() {
    let size = Size::new(20, 5);
    let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a2 = activated.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        activating(cx, move |i| a2.borrow_mut().push(i)).build()
    });
    let t0 = Instant::now();
    click_at(&mut tree, 2, 1, t0); // row 0 selected
                                   // Wheel up at offset 0: the scroll clamps (nothing moves) but
                                   // the chain resets regardless.
    set_event_time(Some(t0 + Duration::from_millis(40)));
    mouse(&mut tree, MouseKind::ScrollUp, 2, 1);
    click_at(&mut tree, 2, 1, t0 + Duration::from_millis(80));
    assert!(
        activated.borrow().is_empty(),
        "press after a wheel starts a fresh chain (count 1)"
    );
    // Control: the SAME second press without a wheel in between
    // chains and activates — isolating the wheel as the reset.
    click_at(&mut tree, 2, 1, t0 + Duration::from_millis(160));
    assert_eq!(*activated.borrow(), vec![0]);
    set_event_time(None);
}

/// The no-time-source rule: driven directly (no driver, no
/// `set_event_time`), presses stay isolated — two immediate
/// programmatic clicks are deterministically NOT a double-click
/// (no hidden wall-clock read to flake on machine speed).
#[test]
fn without_an_event_time_source_presses_stay_isolated() {
    let size = Size::new(20, 5);
    let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let a2 = activated.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        activating(cx, move |i| a2.borrow_mut().push(i)).build()
    });
    set_event_time(None);
    click(&mut tree, 2, 3);
    click(&mut tree, 2, 3);
    click(&mut tree, 2, 3);
    assert!(
        activated.borrow().is_empty(),
        "no time source → every press counts 1: {:?}",
        activated.borrow()
    );
}

/// Disposal-safety law (0250 clause 4 / 0297) on the NEW callback:
/// the activating press changes no selection (on_select silent) and
/// all bookkeeping ran in select(), so `on_activate` is the LAST
/// thing the arm does — it may dispose the Table's scope.
#[test]
fn on_activate_may_dispose_the_tables_scope() {
    let t = default_theme().tokens;
    let mut tree = crate::ui::UiTree::new(Size::new(20, 5));
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let view = Table::new(vec![Column::new("name", ColWidth::Flex(1.0))])
            .rows((0..3).map(|i| vec![format!("f{i}")]).collect())
            .on_activate(move |_| modal_cx.dispose())
            .element(modal_cx, &t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    let t0 = Instant::now();
    click_at(&mut tree, 2, 1, t0);
    click_at(&mut tree, 2, 1, t0 + Duration::from_millis(100));
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    set_event_time(None);
    root.dispose();
}

#[test]
fn column_solver_tiles_exactly() {
    let cols = solve_columns(
        &[
            ColWidth::Cells(4),
            ColWidth::Percent(0.25),
            ColWidth::Flex(1.0),
            ColWidth::Flex(1.0),
        ],
        24,
    );
    let gaps = (cols.len() as i32) - 1;
    assert_eq!(cols.iter().sum::<i32>() + gaps, 24, "{cols:?}");
    assert_eq!(cols[0], 4);
}

/// The strip is a DRAG ZONE (first-app/1335) so screen select mode
/// stands down over it — the STRIP only: `Table` handles its bar on the
/// same element as its cells, which stay selectable text. The header row
/// is outside the bar's body, so it is not a zone either.
#[test]
fn only_the_strip_column_owns_drags() {
    let size = Size::new(20, 6);
    let (_root, mut tree) = mount_widget(size, |cx| {
        let t = &default_theme().tokens;
        Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..60)
                .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
                .collect(),
        )
        .element(cx, t)
        .build()
    });
    render(&mut tree, size);
    let bar_x = size.w - 1;
    assert!(
        tree.press_probe_at(Point::new(bar_x, 3)).drag_owner,
        "the strip owns drags"
    );
    assert!(
        !tree.press_probe_at(Point::new(2, 3)).drag_owner,
        "cell text stays selectable"
    );
    assert!(
        !tree.press_probe_at(Point::new(bar_x, 0)).drag_owner,
        "the header row is above the bar's body"
    );
}

/// The scrollbar column belongs to the BAR, not to the row beside it.
/// `Table` used to resolve rows from `pos.y` alone, so a press on the
/// strip selected whatever row it landed next to — and `FilePicker`, on
/// the same unguarded math, could activate one.
#[test]
fn the_scrollbar_column_scrolls_instead_of_selecting_a_row() {
    let size = Size::new(20, 6);
    let sel_probe: Rc<RefCell<Option<Signal<usize>>>> = Rc::new(RefCell::new(None));
    let out = sel_probe.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = &default_theme().tokens;
        let sel = cx.signal(0usize);
        *out.borrow_mut() = Some(sel);
        Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..60)
                .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
                .collect(),
        )
        .selection(sel)
        .element(cx, t)
        .build()
    });
    let sel = sel_probe.borrow().unwrap();
    let bar_x = size.w - 1;
    let before = canvas_first_row(&mut tree, size);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 4);
    assert_eq!(sel.get_untracked(), 0, "the strip selects nothing");
    assert_ne!(
        canvas_first_row(&mut tree, size),
        before,
        "...it scrolls instead"
    );
    // A body press still selects normally.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 2, 2);
    assert!(
        sel.get_untracked() > 0,
        "rows still answer their own column"
    );
}

fn canvas_first_row(tree: &mut UiTree, size: Size) -> String {
    render(tree, size).row_text(1)
}

/// Parity with `List` and `Scroll`: the strip answers the pointer. A
/// bar that cannot say "grab me" reads as decoration — the field report
/// that followed the seam ("mouse-over works on the other three panes").
#[test]
fn the_scrollbar_thumb_lights_under_the_pointer() {
    let size = Size::new(20, 8);
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = &default_theme().tokens;
        Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..60)
                .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
                .collect(),
        )
        .element(cx, t)
        .build()
    });
    let bar_x = size.w - 1;
    let ink = |tree: &mut UiTree, y: i32| {
        render(tree, size)
            .cell(crate::base::Point::new(bar_x, y))
            .map(|(_, fg, _)| fg)
    };
    let cold = ink(&mut tree, 1).expect("thumb cell");
    mouse(&mut tree, MouseKind::Move, bar_x, 1);
    let hot = ink(&mut tree, 1).expect("thumb cell");
    assert_ne!(cold, hot, "the strip answers the pointer");
    // Moving off the strip cools it; the row body is not the bar.
    mouse(&mut tree, MouseKind::Move, 2, 1);
    assert_eq!(
        ink(&mut tree, 1),
        Some(cold),
        "cools when the pointer leaves"
    );
    // A live grab keeps the ink even while the pointer wanders away.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 3);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 0, 5);
    let dragging = ink(&mut tree, 5).expect("thumb cell");
    assert_eq!(dragging, hot, "still lit mid-drag, off-strip");
}

/// §3.2's borderless-actionable rule, applied to a `Table` row: the row
/// under the pointer shifts its ink to `accent`, bg unchanged, and
/// selection outranks hover (the audited pair stays; BOLD marks the hot
/// row instead, because the hover inks are not audited against
/// `selection_bg`). `List` has always painted rows this way.
#[test]
fn hover_ink_lights_the_row_under_the_pointer() {
    let size = Size::new(20, 8);
    let sel_probe: Rc<RefCell<Option<Signal<usize>>>> = Rc::new(RefCell::new(None));
    let out = sel_probe.clone();
    let (_root, mut tree) = mount_widget(size, move |cx| {
        let t = &default_theme().tokens;
        let sel = cx.signal(0usize);
        *out.borrow_mut() = Some(sel);
        Table::new(vec![
            Column::new("name", ColWidth::Flex(1.0)),
            Column::new("size", ColWidth::Cells(6)),
        ])
        .rows(
            (0..60)
                .map(|i| vec![format!("file-{i}"), format!("{i} kB")])
                .collect(),
        )
        .selection(sel)
        // A fixed height leaves ground BELOW the widget, so the pointer
        // can leave it (`MouseLeave` is synthesized by hit testing —
        // dispatching one by hand is a no-op, by design).
        .layout(LayoutStyle::default().height(Dimension::Cells(6)))
        .element(cx, t)
        .build()
    });
    let sel = sel_probe.borrow().unwrap();
    let cell = |tree: &mut UiTree, y: i32| {
        render(tree, size)
            .cell(crate::base::Point::new(1, y))
            .expect("row cell")
    };
    // Row 2 of the body (selection sits on row 0, out of the way).
    let (_, cold, cold_bg) = cell(&mut tree, 3);
    mouse(&mut tree, MouseKind::Move, 2, 3);
    let (_, hot, hot_bg) = cell(&mut tree, 3);
    assert_ne!(cold, hot, "the hovered row takes accent ink");
    assert_eq!(cold_bg, hot_bg, "hover shifts INK only — bg unchanged");
    assert_eq!(sel.get_untracked(), 0, "hover never selects");
    // The header owns its own row, and the strip owns its column:
    // neither lights a body row.
    mouse(&mut tree, MouseKind::Move, 2, 0);
    assert_eq!(cell(&mut tree, 3).1, cold, "header hover lights no row");
    mouse(&mut tree, MouseKind::Move, size.w - 1, 3);
    assert_eq!(cell(&mut tree, 3).1, cold, "strip hover lights no row");
    // Leaving the widget cools it.
    mouse(&mut tree, MouseKind::Move, 2, 3);
    assert_eq!(cell(&mut tree, 3).1, hot);
    mouse(&mut tree, MouseKind::Move, 2, 7); // off the widget entirely
    assert_eq!(cell(&mut tree, 3).1, cold, "cools when the pointer leaves");
}
