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

/// Settle the deferred geometry loop: draw (probes record), fire due
/// timers (probes publish), flush (pins apply), repeat until quiet.
/// Mirrors what consecutive `Driver::turn`s do in a real app.
///
/// Any widget that measures its viewport through `scroll::size_probe`
/// (`Scroll`, `Feed`, `List::scroll_to`) needs this rather than a bare
/// `render` — one paint only RECORDS the size, it never publishes it.
pub(crate) fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    crate::reactive::flush_effects();
    tree.layout();
    let mut canvas = render(tree, size);
    for _ in 0..4 {
        let fired = crate::reactive::run_due_timers(std::time::Instant::now());
        crate::reactive::flush_effects();
        tree.layout();
        canvas = render(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
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

/// Style-annotated capture: each row's text, then the row's style runs
/// as `x0..x1 fg=rrggbb bg=rrggbb attrs=N` (only rows with visible
/// glyphs). A golden of this pins colours and weights, not just text —
/// the shape "today's rendering, exactly" needs.
pub(crate) fn styled_rows(canvas: &BufferCanvas, size: Size) -> String {
    let hex = |c: crate::base::Rgba| format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    let mut out = String::new();
    for y in 0..size.h {
        let text = canvas.row_text(y);
        if text.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("{y:>2}|{}|\n", text.trim_end()));
        let mut x = 0;
        while x < size.w {
            let key = |x: i32| {
                let (_, fg, bg) = canvas.cell(Point::new(x, y)).expect("in bounds");
                (fg, bg, canvas.attrs_at(Point::new(x, y)).bits())
            };
            let start = x;
            let k = key(x);
            while x < size.w && key(x) == k {
                x += 1;
            }
            let (fg, bg, attrs) = k;
            out.push_str(&format!(
                "   {start}..{x} fg={} bg={} attrs={attrs}\n",
                hex(fg),
                hex(bg)
            ));
        }
    }
    out
}
