//! Test scaffolding for the INTERACTIVE widgets (REACT-owned): mount a
//! real component into a real `UiTree`, drive events through the real
//! dispatch, and read pixels from a `BufferCanvas`. Kept separate from
//! DESIGN's `test_util` (which exercises draw closures in isolation) —
//! interactive widgets are only meaningful through routing + reactivity.

use crate::base::{Point, Size};
use crate::reactive::{create_root, RootScope, Scope};
use crate::ui::{
    BufferCanvas, Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, UiEvent, UiTree, View,
};

pub(crate) fn mount_widget(size: Size, build: impl FnOnce(Scope) -> View) -> (RootScope, UiTree) {
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        let view = build(cx);
        tree.mount(cx, view);
    });
    tree.layout();
    (root, tree)
}

pub(crate) fn render(tree: &mut UiTree, size: Size) -> BufferCanvas {
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    canvas
}

pub(crate) fn key(tree: &mut UiTree, k: Key) -> bool {
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(k)))
}

pub(crate) fn key_mod(tree: &mut UiTree, k: Key, mods: Mods) -> bool {
    tree.dispatch(&UiEvent::Key(KeyEvent::new(k, mods)))
}

pub(crate) fn type_str(tree: &mut UiTree, s: &str) {
    for ch in s.chars() {
        key(tree, Key::Char(ch));
    }
}

pub(crate) fn mouse(tree: &mut UiTree, kind: MouseKind, x: i32, y: i32) -> bool {
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(x, y),
        kind,
        mods: Mods::NONE,
    }))
}

/// Full click: move (hover), press, release at one point.
pub(crate) fn click(tree: &mut UiTree, x: i32, y: i32) {
    mouse(tree, MouseKind::Move, x, y);
    mouse(tree, MouseKind::Down(MouseButton::Left), x, y);
    mouse(tree, MouseKind::Up(MouseButton::Left), x, y);
}
