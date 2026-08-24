//! ContextMenu tests through the real overlay router. These pin the
//! owned-popup behavior independently of any one opener widget.

use super::*;
use crate::base::{Point, Rect, Size};
use crate::reactive::{create_root, flush_effects, RootScope, Scope};
use crate::ui::{BufferCanvas, KeyEvent, MouseButton, MouseEvent, MouseKind, UiEvent, UiTree};

const VP: Size = Size::new(30, 12);

struct Rig {
    _root: RootScope,
    root_tree: UiTree,
    overlays: Overlays,
}

impl Rig {
    fn send(&mut self, event: &UiEvent) -> bool {
        let consumed = self
            .overlays
            .dispatch(event)
            .unwrap_or_else(|| self.root_tree.dispatch(event));
        flush_effects();
        consumed
    }

    fn key(&mut self, key: Key) {
        self.send(&UiEvent::Key(KeyEvent::plain(key)));
    }

    fn press(&mut self, button: MouseButton, point: Point) {
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: point,
            kind: MouseKind::Down(button),
            mods: Mods::NONE,
        }));
    }

    fn popup_tree(&self) -> Option<(UiTree, Rect)> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .find_map(|(meta, layer)| match &meta.content {
                super::super::overlays::OverlayContent::Tree {
                    tree, modal: true, ..
                } => Some((tree.handle(), layer.bounds())),
                _ => None,
            })
    }

    fn popup_rows(&self) -> Vec<String> {
        let Some((mut tree, bounds)) = self.popup_tree() else {
            return Vec::new();
        };
        tree.layout();
        let mut canvas = BufferCanvas::new(bounds.size());
        tree.draw(&mut canvas);
        (0..bounds.h).map(|y| canvas.row_text(y)).collect()
    }
}

fn rig(build: impl FnOnce(Scope, &Overlays)) -> Rig {
    super::super::viewport::publish_viewport(VP);
    let overlays = Overlays::new();
    overlays.ensure_root(VP);
    let mut root_tree = UiTree::new(VP);
    let ov = overlays.clone();
    let (root, ()) = create_root(|cx| {
        root_tree.mount(cx, crate::ui::text("root"));
        build(cx, &ov);
    });
    root_tree.layout();
    Rig {
        _root: root,
        root_tree,
        overlays,
    }
}

#[test]
fn empty_or_fully_disabled_menu_opens_no_modal() {
    let mut empty_open = true;
    let mut disabled_open = true;
    let r = rig(|cx, overlays| {
        empty_open = ContextMenu::new([])
            .overlays(overlays)
            .open(cx, Point::new(2, 2))
            .is_some();
        disabled_open =
            ContextMenu::new([ContextMenuItem::new("promote", "Promote").disabled(true)])
                .overlays(overlays)
                .open(cx, Point::new(2, 2))
                .is_some();
    });
    assert!(!empty_open);
    assert!(!disabled_open);
    assert!(r.popup_tree().is_none());
}

#[test]
fn keyboard_skips_disabled_clamps_and_closes_before_action() {
    let actions: Rc<RefCell<Vec<(String, bool)>>> = Default::default();
    let dismissals: Rc<RefCell<Vec<DismissReason>>> = Default::default();
    let handle: Rc<RefCell<Option<Popup>>> = Default::default();
    let a = actions.clone();
    let d = dismissals.clone();
    let h_for_action = handle.clone();
    let h_for_open = handle.clone();
    let mut r = rig(move |cx, overlays| {
        let popup = ContextMenu::new([
            ContextMenuItem::new("promote", "Promote").disabled(true),
            ContextMenuItem::new("mission", "Assign mission").hint("A"),
            ContextMenuItem::new("demote", "Demote"),
        ])
        .access_label("seat actions")
        .overlays(overlays)
        .on_dismiss(move |reason| d.borrow_mut().push(reason))
        .on_action(move |key| {
            let already_closed = h_for_action
                .borrow()
                .as_ref()
                .is_some_and(|popup| !popup.is_open());
            a.borrow_mut().push((key.to_string(), already_closed));
        })
        .open(cx, Point::new(VP.w - 1, VP.h - 1))
        .expect("room above the bottom-right anchor");
        *h_for_open.borrow_mut() = Some(popup);
    });

    let (mut tree, bounds) = r.popup_tree().expect("menu tree");
    assert!(bounds.x >= 0 && bounds.right() <= VP.w, "{bounds:?}");
    assert!(bounds.y >= 0 && bounds.bottom() <= VP.h, "{bounds:?}");
    let access = tree.accessibility_tree();
    assert!(access
        .entries
        .iter()
        .any(|entry| { entry.role == Role::Menu && entry.label == "seat actions" }));
    assert_eq!(
        access
            .entries
            .iter()
            .filter(|entry| entry.role == Role::MenuItem)
            .count(),
        3
    );

    // The first enabled row is "mission"; Down skips the disabled row
    // and lands on "demote". The popup must be gone before app code.
    r.key(Key::Down);
    r.key(Key::Enter);
    assert_eq!(
        &*actions.borrow(),
        &[("demote".to_string(), true)],
        "commit fires once after teardown"
    );
    assert_eq!(&*dismissals.borrow(), &[DismissReason::Commit]);
    assert!(r.popup_tree().is_none());
}

#[test]
fn only_left_button_activates_a_menu_row() {
    let actions: Rc<RefCell<Vec<String>>> = Default::default();
    let a = actions.clone();
    let mut r = rig(move |cx, overlays| {
        ContextMenu::new([ContextMenuItem::new("promote", "Promote")])
            .overlays(overlays)
            .on_action(move |key| a.borrow_mut().push(key.to_string()))
            .open(cx, Point::new(2, 1))
            .expect("menu");
    });
    let (_, bounds) = r.popup_tree().expect("menu tree");
    let row = bounds.origin();

    r.press(MouseButton::Right, row);
    assert!(actions.borrow().is_empty());
    assert!(r.popup_tree().is_some(), "right press leaves menu open");

    r.press(MouseButton::Left, row);
    assert_eq!(&*actions.borrow(), &["promote"]);
    assert!(r.popup_tree().is_none());
}

#[test]
fn escape_and_outside_press_never_commit() {
    let actions: Rc<RefCell<Vec<String>>> = Default::default();
    let dismissals: Rc<RefCell<Vec<DismissReason>>> = Default::default();
    let a = actions.clone();
    let d = dismissals.clone();
    let mut r = rig(move |cx, overlays| {
        ContextMenu::new([ContextMenuItem::new("promote", "Promote")])
            .overlays(overlays)
            .on_action(move |key| a.borrow_mut().push(key.to_string()))
            .on_dismiss(move |reason| d.borrow_mut().push(reason))
            .open(cx, Point::new(4, 2))
            .expect("menu");
    });
    r.key(Key::Escape);
    assert!(actions.borrow().is_empty());
    assert_eq!(&*dismissals.borrow(), &[DismissReason::Escape]);

    // A fresh rig proves the outside press has its own named ending and
    // is swallowed rather than acting on the root below.
    let actions2: Rc<RefCell<Vec<String>>> = Default::default();
    let dismissals2: Rc<RefCell<Vec<DismissReason>>> = Default::default();
    let a2 = actions2.clone();
    let d2 = dismissals2.clone();
    let mut r2 = rig(move |cx, overlays| {
        ContextMenu::new([ContextMenuItem::new("promote", "Promote")])
            .overlays(overlays)
            .on_action(move |key| a2.borrow_mut().push(key.to_string()))
            .on_dismiss(move |reason| d2.borrow_mut().push(reason))
            .open(cx, Point::new(4, 2))
            .expect("menu");
    });
    assert!(r2.press_outside(Point::new(29, 11)));
    assert!(actions2.borrow().is_empty());
    assert_eq!(&*dismissals2.borrow(), &[DismissReason::OutsidePress]);
}

impl Rig {
    fn press_outside(&mut self, point: Point) -> bool {
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: point,
            kind: MouseKind::Down(MouseButton::Left),
            mods: Mods::NONE,
        }))
    }
}

#[test]
fn unicode_width_and_visible_window_are_cell_based() {
    let mut r = rig(|cx, overlays| {
        ContextMenu::new([
            ContextMenuItem::new("one", "界").hint("⌘A"),
            ContextMenuItem::new("two", "second"),
            ContextMenuItem::new("three", "third"),
        ])
        .max_visible(2)
        .min_width(4)
        .overlays(overlays)
        .open(cx, Point::new(1, 1))
        .expect("menu");
    });
    let (_, bounds) = r.popup_tree().expect("menu tree");
    assert_eq!(bounds.h, 2);
    assert!(
        bounds.w >= 7,
        "wide label + gap + hint + insets: {bounds:?}"
    );
    let before = r.popup_rows();
    assert!(before.iter().any(|row| row.contains('界')), "{before:?}");
    r.key(Key::End);
    let after = r.popup_rows();
    assert!(after.iter().any(|row| row.contains("third")), "{after:?}");
}

#[test]
fn list_secondary_press_opens_row_specific_menu_end_to_end() {
    super::super::viewport::publish_viewport(VP);
    let overlays = Overlays::new();
    overlays.ensure_root(VP);
    let mut tree = UiTree::new(VP);
    let ov = overlays.clone();
    let opened_for: Rc<RefCell<Vec<usize>>> = Default::default();
    let actions: Rc<RefCell<Vec<String>>> = Default::default();
    let opened = opened_for.clone();
    let acted = actions.clone();
    let (root, ()) = create_root(|cx| {
        let menu_overlays = ov.clone();
        tree.mount(
            cx,
            crate::widgets::List::of(["alice", "bob", "carol"])
                .layout(LayoutStyle::default().w(18).h(3))
                .on_context_menu(move |event| {
                    opened.borrow_mut().push(event.index);
                    let acted = acted.clone();
                    ContextMenu::new([
                        ContextMenuItem::new("promote", "Promote"),
                        ContextMenuItem::new("mission", "Assign mission"),
                    ])
                    .overlays(&menu_overlays)
                    .on_action(move |key| acted.borrow_mut().push(key.to_string()))
                    .open(cx, event.screen_position);
                })
                .view(cx),
        );
    });
    tree.layout();
    let mut r = Rig {
        _root: root,
        root_tree: tree,
        overlays,
    };

    r.press(MouseButton::Right, Point::new(2, 1));
    assert_eq!(&*opened_for.borrow(), &[1]);
    let rows = r.popup_rows();
    assert!(rows.iter().any(|row| row.contains("Promote")), "{rows:?}");
    let (_, bounds) = r.popup_tree().expect("menu tree");
    r.press(MouseButton::Left, bounds.origin());
    assert_eq!(&*actions.borrow(), &["promote"]);
}
