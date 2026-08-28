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
fn tabs_keep_full_semantic_labels_and_support_keyboard_toggle() {
    let size = Size::new(W, H);
    let (root, mut tree, open, _files, _chat) = dock(size);
    let access = tree.accessibility_tree();
    assert!(access
        .entries
        .iter()
        .any(|entry| entry.role == Role::Tabs && entry.label == "drawers"));
    let labels: Vec<&str> = access
        .entries
        .iter()
        .filter(|entry| entry.role == Role::Tab)
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, ["Files", "Chat"]);

    key(&mut tree, Key::Tab); // focus Files
    let focused = tree
        .accessibility_tree()
        .focused()
        .map(|entry| (entry.role, entry.label.clone()));
    assert_eq!(focused, Some((Role::Tab, "Files".into())));
    key(&mut tree, Key::Enter);
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked().as_deref(), Some("files"));

    key(&mut tree, Key::Char(' ')); // active tab toggles closed
    let _ = settle(&mut tree, size);
    assert_eq!(open.get_untracked(), None);
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

/// Build a dock whose two drawers declare different widths, over a
/// content pane narrow enough not to clamp either (see the sibling
/// guard for what happens when it does).
fn width_dock(size: Size) -> (crate::reactive::RootScope, UiTree, Signal<Option<String>>) {
    let mut open_probe = None;
    let (root, tree) = mount_widget(size, |cx| {
        let t = default_theme().tokens;
        let open = cx.signal(None);
        open_probe = Some(open);
        DrawerDock::new(crate::ui::text("x"))
            .drawer("files", "Files", |_cx| crate::ui::text("file list body"))
            .drawer("board", "Board", |_cx| crate::ui::text("leaderboard body"))
            .drawer_width(28) // the board only
            .open(open)
            .panel_width(12) // everything else
            .element(cx, &t)
            .build()
    });
    (root, tree, open_probe.unwrap())
}

/// The panel's painted left seam, which is where the panel column
/// starts. Measured off the SCREEN rather than off the field: the value
/// passes through a `dyn_view_scoped` rebuild and a `Dimension::Cells`
/// before it means anything, and the field being set proves none of it.
fn seam_x(tree: &mut UiTree, size: Size) -> i32 {
    render(tree, size)
        .row_text(2)
        .chars()
        .position(|c| c == '\u{2502}')
        .map(|x| x as i32)
        .expect("an open panel paints a left seam")
}

/// **A per-drawer width reaches the solved panel, and only that
/// drawer's.** One wide drawer beside a narrow sibling, in one dock.
///
/// Requested by @agora-tui: a leaderboard drawer needs five reputation
/// columns plus rank, name and score, beside Members and Files panels
/// that are fine narrow. `panel_width` is one value for the whole dock
/// and the builder runs once, so it could not express that.
#[test]
fn a_per_drawer_width_overrides_the_dock_width_for_that_drawer_only() {
    let size = Size::new(W, H);
    let (_root, mut tree, open) = width_dock(size);

    open.set(Some("files".into()));
    let _ = settle(&mut tree, size);
    let narrow = seam_x(&mut tree, size);

    open.set(Some("board".into()));
    let _ = settle(&mut tree, size);
    let wide = seam_x(&mut tree, size);

    assert_eq!(
        narrow - wide,
        28 - 12,
        "the board declared 28 against the dock's 12, so their seams must differ by \
         exactly 16 cells; they sit at {wide} (board) and {narrow} (files)"
    );

    // A per-drawer override that leaks into its siblings is worse than
    // no override at all.
    open.set(Some("files".into()));
    let _ = settle(&mut tree, size);
    assert_eq!(
        seam_x(&mut tree, size),
        narrow,
        "re-opening the narrow drawer after the wide one kept the wide width"
    );
}

/// **A declared panel width is a REQUEST, not a guarantee — the content
/// pane clamps it.** True of `panel_width` since it shipped, and
/// `drawer_width` inherits it exactly; pinned here because it was
/// discovered by a guard rather than documented, and because a consumer
/// sizing a panel to fit N columns will meet it in the field.
///
/// At 40 columns with a 17-cell content pane, a drawer asking for 28
/// gets 23. The content pane will not shrink below the text it holds,
/// so the panel takes what is left. Shrink the content and the SAME
/// declaration is honoured exactly.
#[test]
fn a_declared_panel_width_is_clamped_by_what_the_content_pane_will_not_give_up() {
    let size = Size::new(W, H);
    let panel_for = |content: &'static str, declared: i32| {
        let mut open_probe = None;
        let (root, mut tree) = mount_widget(size, |cx| {
            let t = default_theme().tokens;
            let open = cx.signal(None);
            open_probe = Some(open);
            DrawerDock::new(crate::ui::text(content))
                .drawer("files", "Files", |_cx| crate::ui::text("body"))
                .open(open)
                .panel_width(declared)
                .element(cx, &t)
                .build()
        });
        let open = open_probe.unwrap();
        open.set(Some("files".into()));
        let _ = settle(&mut tree, size);
        let x = seam_x(&mut tree, size);
        drop(root);
        W - x
    };

    // Room to spare: the declaration is honoured to the cell. The +3 is
    // the rail, which sits inside the measured span.
    assert_eq!(
        panel_for("x", 28),
        28 + 3,
        "with a one-cell content pane there is nothing to clamp against"
    );
    // The same declaration, against a content pane that needs 17 cells.
    assert_eq!(
        panel_for("main content pane", 28),
        23 + 3,
        "a 17-cell content pane must hold its width and clamp the panel to 23"
    );
    // And a modest request is unaffected either way — the clamp is a
    // ceiling, not a scaling.
    assert_eq!(panel_for("main content pane", 12), 12 + 3);
}
