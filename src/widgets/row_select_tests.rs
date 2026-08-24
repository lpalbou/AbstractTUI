//! `RowSelect` tests (split file for the size budget — same
//! crate-private module as `row_select.rs` via `#[path]`).
//!
//! The two guards this widget exists to satisfy, both written BEFORE the
//! code and both measured RED with the extraction neutered:
//!
//! 1. `arrows_move_selection_across_multi_line_rows_and_the_selected_element_reports_it`
//!    — a test proving "a key press changed a number" would pass with
//!    the feature absent, so this one reads the PAINTED marker and the
//!    row's SECOND line. The index has to name the right element.
//! 2. `sticky_selection_survives_a_mutation_that_moves_the_selected_row`
//!    — the whole point of a key. An index-based reimplementation passes
//!    every "selection works" test and silently loses this one.

use super::*;
use crate::base::Size;
use crate::reactive::Signal;
use crate::ui::{text, BufferCanvas, MouseButton, MouseKind, UiTree};
use crate::widgets::itest_util::{key, key_mod, mount_widget, mouse, settle};
use crate::widgets::Scroll;

const VIEW: Size = Size { w: 20, h: 4 };

/// Two-line rows: a name, then the thing a `List` cannot render under
/// it. The selected row is the one wearing `>`.
fn member_rows(items: Vec<String>, sel: Signal<usize>) -> View {
    // Width fills, HEIGHT IS INTRINSIC: `fill()` here would squeeze six
    // content rows into the four-row viewport and there would be nothing
    // to scroll (the `Style::fill` trap its own doc comment names).
    crate::ui::dyn_view(
        LayoutStyle::column().width(Dimension::Percent(1.0)),
        move || {
            let s = sel.get();
            let mut col =
                Element::new().style(LayoutStyle::column().width(Dimension::Percent(1.0)));
            for (i, name) in items.iter().enumerate() {
                let mark = if i == s { ">" } else { " " };
                col = col.child(
                    Element::new()
                        .style(
                            LayoutStyle::column()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(2)),
                        )
                        .child(text(format!("{mark} {name}")))
                        .child(text(format!("  mission {i}")))
                        .build(),
                );
            }
            col.build()
        },
    )
}

struct Harness {
    tree: UiTree,
    sel: Signal<usize>,
    sel_key: Signal<String>,
    data: Signal<Vec<String>>,
    _root: crate::reactive::RootScope,
}

impl Harness {
    fn canvas(&mut self) -> BufferCanvas {
        settle(&mut self.tree, VIEW)
    }

    fn rows(&mut self) -> Vec<String> {
        let canvas = self.canvas();
        (0..VIEW.h).map(|y| canvas.row_text(y)).collect()
    }
}

/// A `Scroll` of two-line rows wrapped in a `RowSelect`, rebuilt through
/// a `Dyn` when the data changes — the shape the extraction is FOR.
fn harness(items: &[&str], sticky: bool) -> Harness {
    let mut probes = None;
    let (root, tree) = mount_widget(VIEW, |cx| {
        let sel = cx.signal(0usize);
        let sel_key = cx.signal(String::new());
        let offset = cx.signal(0i32);
        let data = cx.signal(items.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        probes = Some((sel, sel_key, data));
        Element::new()
            .style(LayoutStyle::fill())
            .child(crate::ui::dyn_view_scoped(LayoutStyle::fill(), move |cx| {
                let items = data.get();
                let scroll = Scroll::new(member_rows(items.clone(), sel))
                    .offset_y(offset)
                    .view(cx);
                let mut rs = RowSelect::new(items)
                    .row_heights(|_| 2)
                    .selection(sel)
                    .offset_y(offset);
                if sticky {
                    rs = rs.selection_key(sel_key);
                }
                rs.wrap(cx, scroll).build()
            }))
            .build()
    });
    let (sel, sel_key, data) = probes.unwrap();
    Harness {
        tree,
        sel,
        sel_key,
        data,
        _root: root,
    }
}

/// GUARD 1. Not "a signal changed": the SELECTED ELEMENT is the one that
/// reports selected, and its second line comes with it. Deleting the
/// extraction's `select`/`ensure_visible` leaves the marker on row 0.
#[test]
fn arrows_move_selection_across_multi_line_rows_and_the_selected_element_reports_it() {
    let mut h = harness(&["alpha", "beta", "gamma"], false);
    key(&mut h.tree, Key::Tab);

    let rows = h.rows();
    assert!(rows[0].contains("> alpha"), "{rows:?}");
    assert!(rows[1].contains("mission 0"), "{rows:?}");

    // Down: the marker MOVES to beta's first line, and beta's own second
    // line sits under it. Two 1-row items would pass a weaker assertion.
    key(&mut h.tree, Key::Down);
    let rows = h.rows();
    assert_eq!(h.sel.get_untracked(), 1);
    assert!(!rows[0].contains(">"), "marker left behind: {rows:?}");
    assert!(rows[2].contains("> beta"), "{rows:?}");
    assert!(rows[3].contains("mission 1"), "{rows:?}");

    // Down again: gamma is off the bottom (rows 4..6 of a 4-row view),
    // so ensure-visible has to scroll — the half a bare selection write
    // silently omits.
    key(&mut h.tree, Key::Down);
    let rows = h.rows();
    assert_eq!(h.sel.get_untracked(), 2);
    let marked = rows
        .iter()
        .position(|r| r.contains("> gamma"))
        .unwrap_or_else(|| panic!("gamma never scrolled into view: {rows:?}"));
    assert!(
        rows[marked + 1].contains("mission 2"),
        "the selected row's own second line did not come with it: {rows:?}"
    );

    // Home walks it back, scroll and all.
    key(&mut h.tree, Key::Home);
    let rows = h.rows();
    assert_eq!(h.sel.get_untracked(), 0);
    assert!(rows[0].contains("> alpha"), "{rows:?}");

    // End reaches the last row from anywhere.
    key(&mut h.tree, Key::End);
    let rows = h.rows();
    assert_eq!(h.sel.get_untracked(), 2);
    assert!(rows.iter().any(|r| r.contains("> gamma")), "{rows:?}");
}

/// GUARD 2. Sticky-by-key is the half an index-based reimplementation
/// loses without failing anything else: the data mutates, the selected
/// row MOVES, and the same LOGICAL row stays selected.
#[test]
fn sticky_selection_survives_a_mutation_that_moves_the_selected_row() {
    let mut h = harness(&["alpha", "beta", "gamma"], true);
    key(&mut h.tree, Key::Tab);
    key(&mut h.tree, Key::Down);
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 1);
    assert_eq!(h.sel_key.get_untracked(), "beta");

    // Two arrivals ahead of it: beta is index 3 now, not index 1.
    h.data.set(
        ["zeta", "yeti", "alpha", "beta", "gamma"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    h.canvas();
    assert_eq!(
        h.sel.get_untracked(),
        3,
        "selection did not follow the KEY through the mutation"
    );
    assert_eq!(h.sel_key.get_untracked(), "beta");

    // And it is still beta that paints selected, not the row that
    // inherited index 1. (The Dyn rebuild remounted the wrapper, so the
    // tab stop has to be taken again — same as a `List` under a `Dyn`.)
    key(&mut h.tree, Key::Tab);
    key(&mut h.tree, Key::Up);
    let rows = h.rows();
    assert_eq!(h.sel.get_untracked(), 2);
    assert!(
        rows.iter().any(|r| r.contains("> alpha")),
        "moved off beta to the wrong neighbour: {rows:?}"
    );
}

/// The key is GONE (the selected row was removed): hold the slot,
/// clamped — the next row down, never a phantom index.
#[test]
fn a_removed_selected_row_falls_to_the_slot_it_left() {
    let mut h = harness(&["alpha", "beta", "gamma"], true);
    key(&mut h.tree, Key::Tab);
    key(&mut h.tree, Key::Down);
    h.canvas();
    assert_eq!(h.sel_key.get_untracked(), "beta");

    h.data
        .set(["alpha", "gamma"].iter().map(|s| s.to_string()).collect());
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 1);
    assert_eq!(h.sel_key.get_untracked(), "gamma");
}

/// Click hit-testing over VARIABLE rows: screen row 2 is the second
/// two-line row, not the second item-row. A one-row-per-item assumption
/// selects `alpha` here.
#[test]
fn a_press_selects_the_multi_line_row_under_the_pointer() {
    let mut h = harness(&["alpha", "beta", "gamma"], false);
    h.canvas();
    mouse(&mut h.tree, MouseKind::Down(MouseButton::Left), 3, 3);
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 1, "row 3 is beta's second line");
    let rows = h.rows();
    assert!(rows[2].contains("> beta"), "{rows:?}");
}

/// Activation is the commit, and it is SEPARATE from movement (0250):
/// Enter fires it, an arrow never does.
#[test]
fn enter_activates_and_movement_does_not() {
    let fired: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let f = fired.clone();
    let mut probe = None;
    let (_root, mut tree) = mount_widget(VIEW, |cx| {
        let sel = cx.signal(0usize);
        let offset = cx.signal(0i32);
        probe = Some(sel);
        let items: Vec<String> = ["alpha", "beta"].iter().map(|s| s.to_string()).collect();
        let scroll = Scroll::new(member_rows(items.clone(), sel))
            .offset_y(offset)
            .view(cx);
        Element::new()
            .style(LayoutStyle::fill())
            .child(
                RowSelect::new(items)
                    .row_heights(|_| 2)
                    .selection(sel)
                    .offset_y(offset)
                    .on_activate(move |i| f.borrow_mut().push(i))
                    .wrap(cx, scroll)
                    .build(),
            )
            .build()
    });
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::Down);
    assert_eq!(probe.unwrap().get_untracked(), 1);
    assert!(fired.borrow().is_empty(), "movement is not a commitment");
    key(&mut tree, Key::Enter);
    assert_eq!(*fired.borrow(), vec![1]);
}

/// The capture handler is BOUNDED. It runs before everything in its
/// subtree, so a chord it does not own must pass straight through —
/// otherwise wrapping a surface in a RowSelect makes the surface deaf.
#[test]
fn a_modified_chord_is_not_a_navigation_key() {
    let mut h = harness(&["alpha", "beta", "gamma"], false);
    key(&mut h.tree, Key::Tab);
    key_mod(&mut h.tree, Key::Down, Mods::CTRL);
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 0, "Ctrl+Down is not ours");
    key_mod(&mut h.tree, Key::End, Mods::SHIFT);
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 0, "Shift+End is not ours");
}

/// An empty list has nothing to select and must not name a row: the
/// phantom-row defect `settle` exists to prevent, on the smallest input.
#[test]
fn an_empty_row_select_selects_nothing_and_survives_the_keys() {
    let mut h = harness(&[], false);
    key(&mut h.tree, Key::Tab);
    key(&mut h.tree, Key::Down);
    key(&mut h.tree, Key::End);
    h.canvas();
    assert_eq!(h.sel.get_untracked(), 0);
}
