//! FilePicker tests (backlog first-app/0273): a FAKE source keeps every
//! test hermetic — navigation, filtering, multi-select, pick payloads,
//! error/empty rendering, breadcrumb truncation, and the pure helpers.
//! `#[path]`-included as `file_picker::tests`; driver-level integration
//! (wire bytes, Modal, zero-idle) lives in `tests/wave_attachments.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::view::{filtered_indices, format_size, join_path, left_truncate_ellipsis};
use super::{FileEntry, FilePicker, FileSource};
use crate::base::Size;
use crate::theme::default_theme;
use crate::ui::{Key, MouseButton, MouseKind, UiTree};
use crate::widgets::itest_util::{key, mount_widget, mouse, render, type_str};

/// Hermetic source: a path -> listing map; unknown paths refuse like a
/// filesystem would.
struct Fake {
    dirs: HashMap<String, Result<Vec<FileEntry>, String>>,
}

impl Fake {
    fn tree() -> Fake {
        let mut dirs = HashMap::new();
        dirs.insert(
            "/root".to_string(),
            Ok(vec![
                FileEntry::dir("docs"),
                FileEntry::dir("locked"),
                FileEntry::dir("src"),
                FileEntry::file("readme.md", Some(120)),
                FileEntry::file("zeta.txt", Some(2048)),
            ]),
        );
        dirs.insert(
            join_path("/root", "docs"),
            Ok(vec![
                FileEntry::file("a note.txt", Some(5)),
                FileEntry::file("big.bin", Some(1_500_000)),
            ]),
        );
        dirs.insert(join_path("/root", "src"), Ok(vec![]));
        // Deliberately NOT listed inside "/root" (that listing is
        // asserted verbatim elsewhere): a deep directory reachable only
        // by starting in it, long enough to overflow the rows area and
        // draw a scrollbar.
        dirs.insert(
            join_path("/root", "many"),
            Ok((0..40)
                .map(|i| FileEntry::file(format!("f{i:02}.txt"), Some(1)))
                .collect()),
        );
        dirs.insert(
            join_path("/root", "locked"),
            Err("permission denied".to_string()),
        );
        Fake { dirs }
    }
}

impl FileSource for Fake {
    fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        self.dirs
            .get(path)
            .cloned()
            .unwrap_or_else(|| Err(format!("no such dir: {path}")))
    }
}

const SIZE: Size = Size::new(34, 9);

fn mount_picker(
    build: impl FnOnce(FilePicker) -> FilePicker,
) -> (
    crate::reactive::RootScope,
    UiTree,
    Rc<RefCell<Vec<Vec<String>>>>,
) {
    let picked: Rc<RefCell<Vec<Vec<String>>>> = Default::default();
    let p2 = picked.clone();
    let (root, tree) = mount_widget(SIZE, |cx| {
        let t = &default_theme().tokens;
        build(FilePicker::new(Fake::tree()).start_in("/root"))
            .on_pick(move |paths| p2.borrow_mut().push(paths))
            .element(cx, t)
            .build()
    });
    (root, tree, picked)
}

fn screen(tree: &mut UiTree) -> Vec<String> {
    let canvas = render(tree, SIZE);
    (0..SIZE.h).map(|y| canvas.row_text(y)).collect()
}

#[test]
fn renders_breadcrumb_glyphs_and_sizes() {
    let (root, mut tree, _) = mount_picker(|p| p);
    let rows = screen(&mut tree);
    assert!(rows[0].contains("/root"), "breadcrumb: {rows:?}");
    // Dirs wear the ▸ glyph, files the · glyph, sizes right-aligned.
    assert!(rows[2].contains("▸ docs"), "{rows:?}");
    assert!(rows[5].contains("· readme.md"), "{rows:?}");
    assert!(rows[5].contains("120 B"), "{rows:?}");
    assert!(rows[6].contains("2.0K"), "{rows:?}");
    root.dispose();
}

#[test]
fn enter_descends_and_backspace_left_return_to_parent() {
    let (root, mut tree, picked) = mount_picker(|p| p);
    key(&mut tree, Key::Enter); // row 0 = docs
    let rows = screen(&mut tree);
    assert!(
        rows[0].contains(&join_path("/root", "docs")),
        "descended: {rows:?}"
    );
    assert!(rows[2].contains("a note.txt"), "{rows:?}");
    key(&mut tree, Key::Backspace); // empty filter: parent
    assert!(screen(&mut tree)[2].contains("docs"), "back at /root");
    key(&mut tree, Key::Enter); // descend again
    key(&mut tree, Key::Left); // Left = parent too (empty filter)
    assert!(screen(&mut tree)[2].contains("docs"));
    assert!(picked.borrow().is_empty(), "navigation never picks");
    root.dispose();
}

#[test]
fn filter_narrows_live_and_backspace_edits_before_navigating() {
    let (root, mut tree, _) = mount_picker(|p| p);
    type_str(&mut tree, "zeta");
    let rows = screen(&mut tree);
    assert!(rows[2].contains("zeta.txt"), "narrowed: {rows:?}");
    assert!(!rows.iter().any(|r| r.contains("readme")), "{rows:?}");
    // Backspace with filter text EDITS (no navigation)…
    key(&mut tree, Key::Backspace);
    assert!(
        screen(&mut tree)[0].contains("/root"),
        "still in /root (edited the filter)"
    );
    // …deleting the rest widens the list again.
    for _ in 0..3 {
        key(&mut tree, Key::Backspace);
    }
    assert!(screen(&mut tree)[5].contains("readme.md"));
    // A filter nobody matches says so.
    type_str(&mut tree, "qqq");
    assert!(screen(&mut tree)[2].contains("no matches"));
    root.dispose();
}

#[test]
fn single_select_enter_picks_the_current_file() {
    let (root, mut tree, picked) = mount_picker(|p| p);
    type_str(&mut tree, "readme"); // narrow to the file, sel 0
    key(&mut tree, Key::Enter);
    assert_eq!(
        *picked.borrow(),
        vec![vec![join_path("/root", "readme.md")]]
    );
    root.dispose();
}

#[test]
fn multi_select_marks_badge_and_commits_in_mark_order() {
    let (root, mut tree, picked) = mount_picker(|p| p.multi_select(true));
    // Mark zeta.txt first (filter to it), then readme.md — the commit
    // must carry MARK order, not list order.
    type_str(&mut tree, "zeta");
    key(&mut tree, Key::Char(' '));
    assert!(screen(&mut tree)[0].contains("1 marked"));
    for _ in 0..4 {
        key(&mut tree, Key::Backspace);
    }
    type_str(&mut tree, "readme");
    key(&mut tree, Key::Char(' '));
    assert!(screen(&mut tree)[0].contains("2 marked"));
    assert!(screen(&mut tree)[2].contains("●"), "mark glyph visible");
    // Space toggles OFF too.
    key(&mut tree, Key::Char(' '));
    assert!(screen(&mut tree)[0].contains("1 marked"));
    key(&mut tree, Key::Char(' ')); // back on
    key(&mut tree, Key::Enter);
    assert_eq!(
        *picked.borrow(),
        vec![vec![
            join_path("/root", "zeta.txt"),
            join_path("/root", "readme.md"),
        ]]
    );
    root.dispose();
}

#[test]
fn multi_select_marks_persist_across_directories() {
    let (root, mut tree, picked) = mount_picker(|p| p.multi_select(true));
    type_str(&mut tree, "zeta");
    key(&mut tree, Key::Char(' ')); // mark /root/zeta.txt
    for _ in 0..4 {
        key(&mut tree, Key::Backspace);
    }
    key(&mut tree, Key::Enter); // descend into docs (row 0)
    assert!(screen(&mut tree)[0].contains("1 marked"), "badge survives");
    key(&mut tree, Key::Char(' ')); // mark docs/a note.txt
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Enter); // Enter on an UNMARKED file commits marks
    assert_eq!(
        *picked.borrow(),
        vec![vec![
            join_path("/root", "zeta.txt"),
            join_path(&join_path("/root", "docs"), "a note.txt"),
        ]]
    );
    root.dispose();
}

#[test]
fn space_toggles_only_files_and_only_in_multi_select() {
    let (root, mut tree, picked) = mount_picker(|p| p.multi_select(true));
    key(&mut tree, Key::Char(' ')); // sel 0 = docs (a dir): no mark
    assert!(!screen(&mut tree)[0].contains("marked"), "dirs never mark");
    root.dispose();
    // Without multi_select, Space types into the filter instead.
    let (root, mut tree, _) = mount_picker(|p| p);
    key(&mut tree, Key::Char('a'));
    key(&mut tree, Key::Char(' '));
    key(&mut tree, Key::Char('n'));
    key(&mut tree, Key::Enter); // "a n" matches nothing at /root
    assert!(screen(&mut tree)[2].contains("no matches"));
    assert!(picked.borrow().is_empty());
    root.dispose();
}

#[test]
fn empty_and_unreadable_directories_render_honestly() {
    let (root, mut tree, _) = mount_picker(|p| p);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Down); // sel = src
    key(&mut tree, Key::Enter);
    assert!(screen(&mut tree)[2].contains("empty directory"));
    key(&mut tree, Key::Backspace);
    key(&mut tree, Key::Down); // sel = locked
    key(&mut tree, Key::Enter);
    let rows = screen(&mut tree);
    assert!(
        rows[2].contains("cannot read: permission denied"),
        "source error rendered: {rows:?}"
    );
    // Backspace walks out of the unreadable directory.
    key(&mut tree, Key::Backspace);
    assert!(screen(&mut tree)[5].contains("readme.md"));
    root.dispose();
}

#[test]
fn arrow_navigation_moves_selection_and_scrolls() {
    let (root, mut tree, picked) = mount_picker(|p| p);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Down);
    key(&mut tree, Key::Down); // sel = zeta.txt (last)
    key(&mut tree, Key::Down); // clamps at the end
    key(&mut tree, Key::Enter);
    assert_eq!(*picked.borrow(), vec![vec![join_path("/root", "zeta.txt")]]);
    root.dispose();
}

#[test]
fn click_selects_then_click_on_selected_activates() {
    let (root, mut tree, picked) = mount_picker(|p| p);
    // Rows start at y=2; readme.md sits at visual row 3 (index 3).
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 4, 5);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 4, 5);
    assert!(picked.borrow().is_empty(), "first click only selects");
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 4, 5);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 4, 5);
    assert_eq!(
        *picked.borrow(),
        vec![vec![join_path("/root", "readme.md")]]
    );
    root.dispose();
}

#[test]
fn on_pick_may_dispose_the_pickers_scope() {
    // Disposal-safety law: all picker bookkeeping lands before the
    // callback, so close-the-modal in on_pick is legal.
    let picked: Rc<RefCell<Vec<Vec<String>>>> = Default::default();
    let p2 = picked.clone();
    let mut tree = UiTree::new(SIZE);
    let (root, ()) = crate::reactive::create_root(|cx| {
        let modal_cx = cx.child();
        let t = &default_theme().tokens;
        let view = FilePicker::new(Fake::tree())
            .start_in("/root")
            .on_pick(move |paths| {
                p2.borrow_mut().push(paths);
                modal_cx.dispose();
            })
            .element(modal_cx, t)
            .build();
        tree.mount(modal_cx, view);
    });
    tree.layout();
    type_str(&mut tree, "readme");
    key(&mut tree, Key::Enter);
    assert_eq!(picked.borrow().len(), 1);
    assert_eq!(tree.instance_count(), 0, "subtree unmounted by dispose");
    root.dispose();
}

#[test]
fn deep_path_breadcrumb_left_truncates_keeping_the_tail() {
    let deep = "/very/long/nested/path/for/the/breadcrumb/row/of/cells";
    let mut dirs = HashMap::new();
    dirs.insert(deep.to_string(), Ok(vec![FileEntry::file("x", Some(1))]));
    let (root, mut tree) = mount_widget(SIZE, |cx| {
        let t = &default_theme().tokens;
        FilePicker::new(Fake { dirs })
            .start_in(deep)
            .element(cx, t)
            .build()
    });
    let crumb = screen(&mut tree)[0].clone();
    assert!(crumb.trim_end().starts_with('…'), "left-cut: {crumb:?}");
    assert!(crumb.contains("of/cells"), "tail kept: {crumb:?}");
    assert!(!crumb.contains("/very"), "head gone: {crumb:?}");
    root.dispose();
}

// ---------------------------------------------------------------------
// Pure helpers.
// ---------------------------------------------------------------------

#[test]
fn filtered_indices_is_case_insensitive_substring() {
    let entries = vec![
        FileEntry::dir("Docs"),
        FileEntry::file("readme.MD", None),
        FileEntry::file("zeta.txt", None),
    ];
    assert_eq!(filtered_indices(&entries, ""), vec![0, 1, 2]);
    assert_eq!(filtered_indices(&entries, "md"), vec![1]);
    assert_eq!(filtered_indices(&entries, "DOC"), vec![0]);
    assert_eq!(filtered_indices(&entries, "nope"), Vec::<usize>::new());
}

#[test]
fn left_truncate_keeps_the_tail_width_aware() {
    assert_eq!(left_truncate_ellipsis("abc", 5), "abc");
    assert_eq!(left_truncate_ellipsis("abcdef", 4), "…def");
    assert_eq!(left_truncate_ellipsis("abc", 1), "…");
    assert_eq!(left_truncate_ellipsis("abc", 0), "");
    // Wide clusters count their real width (… + 日本 = 5 cells).
    assert_eq!(left_truncate_ellipsis("x日本語", 5), "…本語");
}

#[test]
fn format_size_units() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(999), "999 B");
    assert_eq!(format_size(2048), "2.0K");
    assert_eq!(format_size(120_000), "120K");
    assert_eq!(format_size(1_500_000), "1.5M");
    assert_eq!(format_size(3_000_000_000), "3.0G");
}

/// The strip is a DRAG ZONE (first-app/1335) so screen select mode
/// stands down over it and the thumb keeps its gesture — the STRIP
/// only, so filenames stay selectable text.
#[test]
fn only_the_strip_column_owns_drags() {
    let (root, mut tree, _) = mount_picker(|p| p.start_in(join_path("/root", "many")));
    render(&mut tree, SIZE);
    let bar_x = SIZE.w - 1;
    let listing_y = SIZE.h - 2; // inside the rows area, below the chrome
    assert!(
        tree.press_probe_at(crate::base::Point::new(bar_x, listing_y))
            .drag_owner,
        "the strip owns drags"
    );
    assert!(
        !tree
            .press_probe_at(crate::base::Point::new(4, listing_y))
            .drag_owner,
        "filenames stay selectable"
    );
    root.dispose();

    // The default fixture fits in the rows area: no bar, no zone.
    let (root, mut tree, _) = mount_picker(|p| p);
    render(&mut tree, SIZE);
    assert!(
        !tree
            .press_probe_at(crate::base::Point::new(bar_x, listing_y))
            .drag_owner,
        "a listing that fits draws no bar and owns no drags"
    );
    root.dispose();
}

/// The scrollbar column is the bar's. `FilePicker` resolved rows from
/// `pos.y` alone and applied the click-on-selected-activates rule, so a
/// second press on the strip could open a directory or commit a pick the
/// user never aimed at.
#[test]
fn the_scrollbar_column_never_selects_or_activates_a_row() {
    let (root, mut tree, picked) = mount_picker(|p| p.start_in(join_path("/root", "many")));
    let before = screen(&mut tree);
    let bar_x = SIZE.w - 1;
    // Two presses on the same strip cell: the second is exactly the one
    // that used to activate.
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 5);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), bar_x, 5);
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), bar_x, 5);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), bar_x, 5);
    assert!(picked.borrow().is_empty(), "the strip picks nothing");
    let after = screen(&mut tree);
    assert_ne!(before, after, "...it scrolls the listing instead");
    root.dispose();
}

/// Rows answer the pointer here too (§3.2 hover ink). A DIRECTORY row
/// already wears `accent`, so hover also carries BOLD — otherwise the
/// row kind users aim at most would answer the pointer with nothing.
#[test]
fn hover_ink_lights_the_row_under_the_pointer() {
    let (root, mut tree, _) = mount_picker(|p| p);
    // The fixture lists docs/ locked/ src/ then readme.md, zeta.txt.
    let rows = screen(&mut tree);
    let dir_y = rows
        .iter()
        .position(|r| r.contains("locked"))
        .expect("dir row") as i32;
    let file_y = rows
        .iter()
        .position(|r| r.contains("readme.md"))
        .expect("file row") as i32;
    let at = |tree: &mut UiTree, y: i32| {
        let canvas = render(tree, SIZE);
        let p = crate::base::Point::new(4, y);
        let (_, fg, bg) = canvas.cell(p).expect("row cell");
        (fg, bg, canvas.attrs_at(p))
    };
    // A FILE row: `text` -> `accent`, background untouched.
    let (cold_fg, cold_bg, cold_attrs) = at(&mut tree, file_y);
    mouse(&mut tree, MouseKind::Move, 4, file_y);
    let (hot_fg, hot_bg, hot_attrs) = at(&mut tree, file_y);
    assert_ne!(cold_fg, hot_fg, "the hovered row takes accent ink");
    assert_eq!(cold_bg, hot_bg, "hover shifts INK only — bg unchanged");
    assert_ne!(cold_attrs, hot_attrs, "and BOLD marks it");
    // A DIRECTORY row is already accent: BOLD is what answers there.
    let (dir_cold_fg, _, dir_cold_attrs) = at(&mut tree, dir_y);
    mouse(&mut tree, MouseKind::Move, 4, dir_y);
    let (dir_hot_fg, _, dir_hot_attrs) = at(&mut tree, dir_y);
    assert_eq!(dir_cold_fg, dir_hot_fg, "a dir was already accent");
    assert_ne!(
        dir_cold_attrs, dir_hot_attrs,
        "so hover has to answer with BOLD, or the row says nothing"
    );
    // Only the row under the pointer is hot.
    assert_eq!(at(&mut tree, file_y).0, cold_fg);
    root.dispose();
}
