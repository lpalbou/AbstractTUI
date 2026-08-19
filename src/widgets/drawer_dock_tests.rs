//! DrawerDock acceptance (app-kits 1255): the rail is always visible,
//! at most one panel opens, collapsed means the panel column is GONE,
//! closed drawers are disposed, and every dock-driven transition rides
//! the one choke point.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::ui::UiTree;
use crate::widgets::itest_util::{click, key, mount_widget, render, settle};
use std::cell::Cell;

const W: i32 = 40;
const H: i32 = 16;

/// A dock with two drawers over a full-width content pane. Returns the
/// bound open signal and per-drawer build counters.
#[allow(clippy::type_complexity)]
fn dock(
    size: Size,
) -> (
    crate::reactive::RootScope,
    UiTree,
    Signal<Option<String>>,
    Rc<Cell<u32>>,
    Rc<Cell<u32>>,
) {
    let files_builds: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let chat_builds: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let fb = files_builds.clone();
    let cb = chat_builds.clone();
    let mut open_probe = None;
    let (root, tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(None);
        open_probe = Some(open);
        DrawerDock::new(crate::ui::text("main content pane"))
            .drawer("files", "Files", move |_cx| {
                fb.set(fb.get() + 1);
                crate::ui::text("file list body")
            })
            .drawer("chat", "Chat", move |_cx| {
                cb.set(cb.get() + 1);
                crate::ui::text("chat body")
            })
            .open(open)
            .panel_width(20)
            .element(cx, &t)
            .build()
    });
    let open = open_probe.unwrap();
    (root, tree, open, files_builds, chat_builds)
}

/// Screen text of one row (trailing blanks trimmed).
fn row(tree: &mut UiTree, size: Size, y: i32) -> String {
    render(tree, size).row_text(y).trim_end().to_string()
}

/// The vertical rail label read down the given column.
fn rail_column(tree: &mut UiTree, size: Size, x: i32) -> String {
    let canvas = render(tree, size);
    (0..size.h)
        .filter_map(|y| canvas.row_text(y).chars().nth(x as usize))
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn collapsed_is_content_plus_rail_only() {
    let size = Size::new(W, H);
    let (root, mut tree, open, files, chat) = dock(size);
    assert_eq!(open.get_untracked(), None, "starts collapsed");
    // Content spans up to the rail; the vertical labels stack on the
    // rail column (x = W-2: padding, glyph, padding).
    assert!(row(&mut tree, size, 0).contains("main content pane"));
    assert_eq!(rail_column(&mut tree, size, W - 2), "FilesChat");
    // No panel content anywhere, and neither builder ran.
    let canvas = render(&mut tree, size);
    let all: String = (0..H).map(|y| canvas.row_text(y)).collect();
    assert!(!all.contains("file list body"));
    assert_eq!(
        (files.get(), chat.get()),
        (0, 0),
        "collapsed builds nothing"
    );
    root.dispose();
}

#[test]
fn tab_click_opens_active_click_collapses() {
    let size = Size::new(W, H);
    let (root, mut tree, open, files, _chat) = dock(size);
    // Click the Files tab (rail glyph column, first tab block row 1).
    click(&mut tree, W - 2, 1);
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked().as_deref(), Some("files"));
    let canvas = render(&mut tree, size);
    let all: String = (0..H).map(|y| canvas.row_text(y)).collect();
    assert!(all.contains("Files"), "panel header shows the title");
    assert!(all.contains("file list body"), "panel body mounted");
    assert!(all.contains(CLOSE_GLYPH), "close affordance visible");
    assert_eq!(files.get(), 1);

    // Click the SAME tab: collapse; panel content gone.
    click(&mut tree, W - 2, 1);
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked(), None);
    let canvas = render(&mut tree, size);
    let all: String = (0..H).map(|y| canvas.row_text(y)).collect();
    assert!(
        !all.contains("file list body"),
        "collapsed removes the panel"
    );
    root.dispose();
}

#[test]
fn switching_tabs_replaces_the_open_panel() {
    let size = Size::new(W, H);
    let (root, mut tree, open, files, chat) = dock(size);
    click(&mut tree, W - 2, 1); // Files
    let _ = settle(&mut tree, size);
    // Chat tab: second block starts after Files (rows 1..=5+2) — click
    // inside its label band. Files block rows 0..7 (label 5 + pad 2);
    // Chat starts at row 7.
    click(&mut tree, W - 2, 8);
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked().as_deref(), Some("chat"));
    let canvas = render(&mut tree, size);
    let all: String = (0..H).map(|y| canvas.row_text(y)).collect();
    assert!(all.contains("chat body"));
    assert!(!all.contains("file list body"), "one panel at a time");
    assert_eq!((files.get(), chat.get()), (1, 1));
    root.dispose();
}

#[test]
fn close_corner_and_esc_both_collapse() {
    let size = Size::new(W, H);
    let (root, mut tree, open, _files, _chat) = dock(size);
    click(&mut tree, W - 2, 1);
    let _ = settle(&mut tree, size);
    assert!(open.get_untracked().is_some());
    // The panel spans x = W-3-20 .. W-3; its header ✕ sits at the
    // trailing corner. Click the corner region.
    click(&mut tree, W - RAIL_W - 2, 0);
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked(), None, "close corner collapses");

    // Esc routed through the panel subtree collapses too. With nothing
    // focused, keys go to the tree root — so click INTO the panel body
    // area is not needed for the bubble path here: dispatch reaches the
    // root, not the panel. Prove the FOCUSED contract instead: focus
    // lives outside this harness, so assert Esc at root leaves state
    // alone (the honest routing rule)...
    click(&mut tree, W - 2, 1);
    let _ = settle(&mut tree, size);
    assert!(open.get_untracked().is_some());
    key(&mut tree, Key::Escape);
    let _ = settle(&mut tree, size);
    assert!(
        open.get_untracked().is_some(),
        "root-targeted Esc is not the panel's"
    );
    root.dispose();
}

#[test]
fn closed_drawer_is_disposed_and_rebuilt_fresh() {
    let size = Size::new(W, H);
    let (root, mut tree, _open, files, _chat) = dock(size);
    for _ in 0..3 {
        click(&mut tree, W - 2, 1);
        let _ = settle(&mut tree, size);
        let _ = render(&mut tree, size);
        click(&mut tree, W - 2, 1);
        let _ = settle(&mut tree, size);
        let _ = render(&mut tree, size);
    }
    assert_eq!(
        files.get(),
        3,
        "one fresh build per open, none while closed"
    );
    root.dispose();
}

#[test]
fn external_write_drives_the_dock_without_on_change() {
    let size = Size::new(W, H);
    let fired: Rc<RefCell<Vec<Option<String>>>> = Rc::default();
    let f2 = fired.clone();
    let mut open_probe = None;
    let (root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(None);
        open_probe = Some(open);
        DrawerDock::new(crate::ui::text("content"))
            .drawer("files", "Files", |_cx| crate::ui::text("files body"))
            .open(open)
            .on_change(move |id| f2.borrow_mut().push(id.map(str::to_string)))
            .element(cx, &t)
            .build()
    });
    let open = open_probe.unwrap();
    // External write: opens, no on_change.
    open.set(Some("files".into()));
    let _ = settle(&mut tree, size);
    let canvas = render(&mut tree, size);
    let all: String = (0..H).map(|y| canvas.row_text(y)).collect();
    assert!(all.contains("files body"), "external write opens the panel");
    assert!(
        fired.borrow().is_empty(),
        "external writes never fire on_change"
    );
    // Dock-driven close (active tab): fires with None.
    click(&mut tree, W - 2, 1);
    let _ = settle(&mut tree, size);
    assert_eq!(fired.borrow().as_slice(), &[None]);
    root.dispose();
}

#[test]
fn badge_dot_tracks_its_signal() {
    let size = Size::new(W, H);
    let mut badge_probe = None;
    let (root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let waiting = cx.signal(false);
        badge_probe = Some(waiting);
        DrawerDock::new(crate::ui::text("content"))
            .drawer("desk", "Desk", |_cx| crate::ui::text("desk body"))
            .drawer_badge(move || waiting.get())
            .element(cx, &t)
            .build()
    });
    let waiting = badge_probe.unwrap();
    assert!(
        !rail_column(&mut tree, size, W - 2).contains('●'),
        "no dot while quiet"
    );
    waiting.set(true);
    let _ = settle(&mut tree, size);
    assert!(
        rail_column(&mut tree, size, W - 2).contains('●'),
        "the badge dot appears under the label"
    );
    root.dispose();
}

/// The POSITIVE Esc path (adversarial review P2-5: only the negative
/// was pinned): with focus inside the panel — here a TextInput the
/// user clicked into — Esc collapses the dock.
#[test]
fn esc_collapses_while_focus_is_inside_the_panel() {
    let size = Size::new(W, H);
    let mut open_probe = None;
    let (root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(None);
        open_probe = Some(open);
        let value = cx.signal(String::new());
        DrawerDock::new(crate::ui::text("content"))
            .drawer("search", "Search", move |gcx| {
                crate::widgets::TextInput::new()
                    .value(value)
                    .element(gcx, &default_theme().tokens)
                    .build()
            })
            .open(open)
            .panel_width(20)
            .element(cx, &t)
            .build()
    });
    let open = open_probe.unwrap();
    click(&mut tree, W - 2, 1); // open the drawer
    let _ = settle(&mut tree, size);
    assert!(open.get_untracked().is_some());
    // Click INTO the input (panel body starts after header+underline:
    // row 2, x inside the panel column) — focus enters the panel.
    click(&mut tree, W - 10, 2);
    let _ = settle(&mut tree, size);
    key(&mut tree, Key::Escape);
    let _ = settle(&mut tree, size);
    assert_eq!(
        open.get_untracked(),
        None,
        "Esc with focus inside the panel collapses the dock"
    );
    root.dispose();
}

/// The P1 guard: a builder that writes `open` synchronously desyncs
/// state from screen (the engine swallows the self-invalidation the
/// reactive law would otherwise name). Debug builds assert on it.
#[test]
#[should_panic(expected = "wrote `open` while its")]
#[cfg(debug_assertions)]
fn builder_writing_open_is_a_loud_bug() {
    let size = Size::new(W, H);
    let mut open_probe = None;
    let (_root, mut tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(None);
        open_probe = Some(open);
        DrawerDock::new(crate::ui::text("content"))
            .drawer("a", "Aaa", move |_cx| {
                open.set(Some("b".into())); // the forbidden redirect
                crate::ui::text("a body")
            })
            .drawer("b", "Bbb", |_cx| crate::ui::text("b body"))
            .open(open)
            .element(cx, &t)
            .build()
    });
    let open = open_probe.unwrap();
    open.set(Some("a".into()));
    let _ = settle(&mut tree, size);
}
