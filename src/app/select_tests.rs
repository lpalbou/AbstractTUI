//! Select-family unit tests (split file, `#[path]`-included as
//! `select::tests`). Faces mount into a real `UiTree` with popups on a
//! real overlay store; events route driver-style (overlays first,
//! topmost modal wins) — the 0250 movement-vs-activation split and
//! the dismiss contract are exercised through real dispatch.
use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::base::Point;
use crate::reactive::{create_root, flush_effects, RootScope};
use crate::theme::default_theme;
use crate::ui::{text, BufferCanvas, KeyEvent, MouseEvent, UiEvent, UiTree};

const VP: Size = Size::new(30, 12);

struct Rig {
    _root: RootScope,
    tree: UiTree,
    overlays: Overlays,
}

impl Rig {
    /// Driver-order routing: overlays first, root tree on fall-through.
    fn send(&mut self, ev: &UiEvent) {
        if self.overlays.dispatch(ev).is_none() {
            self.tree.dispatch(ev);
        }
        flush_effects();
    }

    fn key(&mut self, k: Key) {
        self.send(&UiEvent::Key(KeyEvent::plain(k)));
    }

    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.key(Key::Char(ch));
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(x, y),
            kind: MouseKind::Down(MouseButton::Left),
            mods: Mods::NONE,
        }));
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(x, y),
            kind: MouseKind::Up(MouseButton::Left),
            mods: Mods::NONE,
        }));
    }

    /// The one open popup (modal overlay tree), drawn: (bounds, rows).
    fn popup(&self) -> Option<(crate::base::Rect, Vec<String>)> {
        let (tree, bounds) = {
            let store = self.overlays.store().borrow();
            store
                .meta
                .iter()
                .zip(&store.layers)
                .find_map(|(m, l)| match &m.content {
                    super::super::overlays::OverlayContent::Tree {
                        tree, modal: true, ..
                    } => Some((tree.handle(), l.bounds())),
                    _ => None,
                })?
        };
        let mut tree = tree.handle();
        tree.layout();
        let mut canvas = BufferCanvas::new(bounds.size());
        tree.draw(&mut canvas);
        let rows = (0..bounds.h).map(|y| canvas.row_text(y)).collect();
        Some((bounds, rows))
    }

    /// Popup row index (0-based within the popup) wearing the
    /// selection-pair highlight.
    fn highlight_row(&self) -> Option<usize> {
        let sel_bg = default_theme().tokens.selection_bg;
        let (bounds, _) = self.popup()?;
        let store = self.overlays.store().borrow();
        let tree = store.meta.iter().find_map(|m| match &m.content {
            super::super::overlays::OverlayContent::Tree {
                tree, modal: true, ..
            } => Some(tree.handle()),
            _ => None,
        })?;
        drop(store);
        let mut tree = tree.handle();
        tree.layout();
        let mut canvas = BufferCanvas::new(bounds.size());
        tree.draw(&mut canvas);
        (0..bounds.h as usize).find(|y| {
            canvas
                .cell(Point::new(1, *y as i32))
                .is_some_and(|c| c.2 == sel_bg)
        })
    }
}

/// Mount one face view at the top row of a 30x12 world.
fn rig(build: impl FnOnce(Scope, &Overlays) -> crate::ui::View) -> Rig {
    super::super::viewport::publish_viewport(VP);
    let overlays = Overlays::new();
    overlays.ensure_root(VP);
    let mut tree = UiTree::new(VP);
    let ov = overlays.clone();
    let (root, ()) = create_root(|cx| {
        let face = build(cx, &ov);
        let view = Element::new()
            .style(LayoutStyle::column().width(Dimension::Percent(1.0)).h(VP.h))
            .child(face)
            .child(text("below content"))
            .build();
        tree.mount(cx, view);
    });
    tree.layout();
    let mut rig = Rig {
        _root: root,
        tree,
        overlays,
    };
    rig.key(Key::Tab); // focus the trigger
    rig
}

fn fruit_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("alpha").hint("first"),
        SelectOption::new("beta"),
        SelectOption::new("banana"),
        SelectOption::new("gamma").disabled(true),
        SelectOption::new("delta"),
    ]
}

fn face_layout() -> LayoutStyle {
    LayoutStyle::default().w(24).h(1).shrink(0.0)
}

// ------------------------------------------------------------------ Select

#[test]
fn select_arrows_move_highlight_only_and_enter_commits_once() {
    // The 0250 regression as a birth test: movement never writes the
    // bound value; Enter commits exactly once.
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let vh = value_holder.clone();
    let ch = changes.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(0usize);
        *vh.borrow_mut() = Some(value);
        Select::new(fruit_options())
            .value(value)
            .layout(face_layout())
            .overlays(ov)
            .on_change(move |i| ch.borrow_mut().push(i))
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    assert!(rig.popup().is_none(), "closed at mount");
    rig.key(Key::Enter); // open
    let (bounds, rows) = rig.popup().expect("popup open");
    assert_eq!(bounds.y, 1, "below the trigger row");
    assert_eq!(bounds.w, 24, "MatchAnchor width");
    assert!(rows[0].contains("alpha"), "{rows:?}");
    assert!(rows[0].contains("first"), "hint renders");
    assert_eq!(rig.highlight_row(), Some(0), "seeded on the value");
    rig.key(Key::Down);
    rig.key(Key::Down);
    assert_eq!(rig.highlight_row(), Some(2), "highlight moved");
    assert_eq!(value.get_untracked(), 0, "arrows never write the value");
    assert!(changes.borrow().is_empty(), "no on_change on move");
    rig.key(Key::Enter);
    assert_eq!(value.get_untracked(), 2, "Enter committed the highlight");
    assert_eq!(changes.borrow().as_slice(), [2], "exactly one on_change");
    assert!(rig.popup().is_none(), "commit closed the popup");
}

#[test]
fn select_escape_and_outside_press_abandon_without_committing() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let vh = value_holder.clone();
    let ch = changes.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(1usize);
        *vh.borrow_mut() = Some(value);
        Select::new(fruit_options())
            .value(value)
            .layout(face_layout())
            .overlays(ov)
            .on_change(move |i| ch.borrow_mut().push(i))
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    rig.key(Key::Enter);
    rig.key(Key::Down);
    rig.key(Key::Escape);
    assert!(rig.popup().is_none(), "Escape closed");
    assert_eq!(value.get_untracked(), 1, "Escape abandoned the move");
    assert!(changes.borrow().is_empty());
    // Outside press: dismiss without acting (and without commit).
    rig.key(Key::Enter);
    rig.key(Key::Down);
    assert!(rig.popup().is_some());
    rig.click(28, 10); // outside the popup
    assert!(rig.popup().is_none(), "outside press dismissed");
    assert_eq!(value.get_untracked(), 1);
    assert!(changes.borrow().is_empty());
    // The trigger keeps focus: Enter reopens straight away.
    rig.key(Key::Enter);
    assert!(rig.popup().is_some(), "focus returned to the trigger");
}

#[test]
fn select_type_ahead_prefix_jump_cycle_and_disabled_skip() {
    let mut rig = rig(move |cx, ov| {
        Select::new(fruit_options())
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    rig.key(Key::Enter); // open (no value bound: placeholder, seed = first enabled)
    assert_eq!(rig.highlight_row(), Some(0));
    rig.key(Key::Char('b'));
    assert_eq!(rig.highlight_row(), Some(1), "prefix jump to beta");
    rig.key(Key::Char('b'));
    assert_eq!(rig.highlight_row(), Some(2), "same-char cycle to banana");
    rig.key(Key::Char('b'));
    assert_eq!(rig.highlight_row(), Some(1), "cycle wraps back to beta");
    // Accumulated prefix: "de" jumps to delta.
    rig.key(Key::Escape);
    rig.key(Key::Enter);
    rig.type_str("de");
    assert_eq!(rig.highlight_row(), Some(4), "prefix 'de' -> delta");
    // 'g' targets only the DISABLED gamma: the highlight stays put.
    rig.key(Key::Escape);
    rig.key(Key::Enter);
    rig.key(Key::Char('g'));
    assert_eq!(rig.highlight_row(), Some(0), "disabled options never match");
    // Arrow movement skips the disabled row too: from banana(2), Down
    // lands on delta(4). Fresh open — the type-ahead buffer holds "g"
    // within its window (a failed match never resets it).
    rig.key(Key::Escape);
    rig.key(Key::Enter);
    rig.key(Key::Char('b'));
    rig.key(Key::Char('a')); // "ba" -> banana
    assert_eq!(rig.highlight_row(), Some(2));
    rig.key(Key::Down);
    assert_eq!(rig.highlight_row(), Some(4), "skipped disabled gamma");
}

#[test]
fn select_commit_on_move_previews_live_and_escape_restores() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let vh = value_holder.clone();
    let ch = changes.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(0usize);
        *vh.borrow_mut() = Some(value);
        Select::new(fruit_options())
            .value(value)
            .commit_on_move(true)
            .layout(face_layout())
            .overlays(ov)
            .on_change(move |i| ch.borrow_mut().push(i))
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    rig.key(Key::Enter);
    rig.key(Key::Down);
    assert_eq!(value.get_untracked(), 1, "move committed live (opt-in)");
    rig.key(Key::Down);
    assert_eq!(value.get_untracked(), 2);
    assert_eq!(changes.borrow().as_slice(), [1, 2]);
    rig.key(Key::Escape);
    assert_eq!(
        value.get_untracked(),
        0,
        "Escape restored the pre-open value"
    );
    assert_eq!(changes.borrow().as_slice(), [1, 2, 0]);
}

#[test]
fn select_disabled_neither_focuses_nor_opens_and_empty_options_never_open() {
    let mut rig = rig(move |cx, ov| {
        Select::new(fruit_options())
            .disabled(true)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    // Tab found nothing focusable; Enter/click open nothing.
    rig.key(Key::Enter);
    assert!(rig.popup().is_none(), "disabled never opens by key");
    rig.click(2, 0);
    assert!(rig.popup().is_none(), "disabled never opens by click");

    let mut empty_rig = self::rig(move |cx, ov| {
        Select::new(Vec::new())
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    empty_rig.key(Key::Enter);
    assert!(empty_rig.popup().is_none(), "no options = nothing to open");
}

#[test]
fn select_click_trigger_opens_and_click_row_commits() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let vh = value_holder.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(0usize);
        *vh.borrow_mut() = Some(value);
        Select::new(fruit_options())
            .value(value)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    rig.click(2, 0); // the trigger row
    let (bounds, rows) = rig.popup().expect("click opened");
    let beta_row = rows.iter().position(|r| r.contains("beta")).unwrap() as i32;
    rig.click(bounds.x + 2, bounds.y + beta_row);
    assert!(rig.popup().is_none(), "click committed and closed");
    assert_eq!(value.get_untracked(), 1, "beta committed");
    // Clicking a DISABLED row does nothing (popup stays).
    rig.click(2, 0);
    let (bounds, rows) = rig.popup().expect("reopened");
    let gamma_row = rows.iter().position(|r| r.contains("gamma")).unwrap() as i32;
    rig.click(bounds.x + 2, bounds.y + gamma_row);
    assert!(rig.popup().is_some(), "disabled row ignored the click");
    assert_eq!(value.get_untracked(), 1);
}

#[test]
fn select_trigger_renders_value_placeholder_and_a11y_roles() {
    let mut rig = rig(move |cx, ov| {
        Element::new()
            .style(LayoutStyle::column().gap(0))
            .child(
                Select::new(fruit_options())
                    .placeholder("pick a fruit")
                    .layout(face_layout())
                    .overlays(ov)
                    .element(cx, &default_theme().tokens)
                    .build(),
            )
            .build()
    });
    // Placeholder (nothing chosen): faint tone + text.
    rig.tree.layout();
    let mut canvas = BufferCanvas::new(VP);
    rig.tree.draw(&mut canvas);
    assert!(canvas.row_text(0).contains("pick a fruit"));
    assert!(canvas.row_text(0).contains("▾"), "chevron affordance");
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().1,
        default_theme().tokens.text_faint,
        "placeholder ink is text_faint"
    );
    // A11y: closed control reports Button (the trigger IS a button
    // opening a menu; `Role::Select` is parked in the 0.3 budget —
    // 0002 entry 1) + the current choice as value; the popup reports
    // menu/menuitem.
    let snapshot = rig.tree.accessibility_tree();
    let entry = snapshot
        .find(crate::ui::Role::Button)
        .expect("select trigger reports Role::Button");
    assert_eq!(entry.value.as_deref(), Some("pick a fruit"));
    rig.key(Key::Enter);
    let store = rig.overlays.store().borrow();
    let mut popup_tree = store
        .meta
        .iter()
        .find_map(|m| match &m.content {
            super::super::overlays::OverlayContent::Tree {
                tree, modal: true, ..
            } => Some(tree.handle()),
            _ => None,
        })
        .expect("popup");
    drop(store);
    let popup_snapshot = popup_tree.accessibility_tree();
    assert!(popup_snapshot.find(crate::ui::Role::Menu).is_some());
    assert!(popup_snapshot.find(crate::ui::Role::MenuItem).is_some());
}

// Combobox + MultiSelect cases live in a sibling (file budget);
// they reuse this module's Rig through `super::`.
#[path = "select_tests_faces.rs"]
mod faces;
