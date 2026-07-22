//! TextArea unit tests (split from textarea.rs for the file budget;
//! `#[path]`-included as its `tests` module). Model cases drive the
//! pure functions; widget cases mount a real tree and dispatch real
//! events (itest_util). The history cases port the reference console's
//! `arrow_nav_action` decision table (backlog 0120 §4).
use super::model::{self, Caret, EditOutcome, History, RowMap, SubmitPolicy};
use super::*;
use crate::base::{Point, Size};
use crate::theme::default_theme;
use crate::ui::{Key, Mods, UiEvent, UiTree};
use crate::widgets::itest_util::{key, key_mod, mount_widget, render, type_str};

// ------------------------------------------------------------------ model

#[test]
fn rowmap_tiles_every_byte_exactly_once() {
    let cases = [
        "",
        "hello",
        "hello world next",
        "a\n\nb",
        "ab\n",
        "aa      b",
        "abcdefgh",
        "城市化进程加快",
        "x👍🏽y\nsecond line with words",
        "tab\there",
        "👨\u{200D}👩\u{200D}👧\u{200D}👦 family",
    ];
    for text in cases {
        for width in [1, 3, 4, 11, 80] {
            let map = RowMap::build(text, width);
            assert!(!map.rows.is_empty(), "{text:?}@{width}: at least one row");
            let mut cursor = 0usize;
            for row in &map.rows {
                assert_eq!(row.start, cursor, "{text:?}@{width}: rows must tile");
                assert!(row.start <= row.text_end && row.text_end <= row.end);
                cursor = row.end;
            }
            assert_eq!(cursor, text.len(), "{text:?}@{width}: full coverage");
        }
    }
}

#[test]
fn rowmap_soft_wrap_matches_editor_expectations() {
    // Word break: the space hangs on the first row, "next" starts clean.
    let map = RowMap::build("hello world next", 11);
    let rows: Vec<_> = map
        .rows
        .iter()
        .map(|r| &"hello world next"[r.start..r.text_end])
        .collect();
    assert_eq!(rows, vec!["hello world ", "next"]);
    // Hard newlines split; empty lines get a row; trailing newline
    // leaves an empty caret row.
    let map = RowMap::build("a\n\nb", 10);
    assert_eq!(map.len(), 3);
    let map = RowMap::build("ab\n", 10);
    assert_eq!(map.len(), 2);
    assert_eq!(map.rows[1].start, 3);
    // CJK wraps by columns, never half a glyph.
    let map = RowMap::build("城市化", 5);
    let rows: Vec<_> = map
        .rows
        .iter()
        .map(|r| &"城市化"[r.start..r.text_end])
        .collect();
    assert_eq!(rows, vec!["城市", "化"]);
    // A ZWJ family never splits internally, even under a hard break —
    // the overflowing row counts as FULL, so the caret's phantom row
    // follows it (an exactly-full last row has no margin cell).
    let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
    let map = RowMap::build(family, 1);
    assert_eq!(
        &family[map.rows[0].start..map.rows[0].text_end],
        family,
        "cluster stays whole"
    );
    assert_eq!(map.len(), 2, "content row + the caret's phantom row");
    // The phantom row appears exactly when the last row is full.
    assert_eq!(RowMap::build("abc", 4).len(), 1, "spare cell: no phantom");
    assert_eq!(RowMap::build("abcd", 4).len(), 2, "full row: phantom");
}

fn apply(text: &mut String, c: &mut Caret, key: Key, mods: Mods, w: i32) -> EditOutcome {
    model::apply_key(text, c, key, mods, w, SubmitPolicy::EnterSubmits)
}

#[test]
fn vertical_moves_keep_goal_column_over_wide_clusters() {
    let mut text = String::from("abcdef\nxy\nlmnopq");
    let mut c = Caret {
        byte: 6, // end of "abcdef" (col 6)
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Down, Mods::NONE, 20);
    assert_eq!(c.byte, 9, "short row clamps to its end (col 2)");
    apply(&mut text, &mut c, Key::Down, Mods::NONE, 20);
    let map = RowMap::build(&text, 20);
    assert_eq!(
        map.col_of(&text, c.byte, 2),
        6,
        "goal column 6 restored past the short row"
    );
    // Wide cluster: the caret never lands inside the emoji. Row 0 cols:
    // a=0, b=1, 👍🏽=2..4, c=4 — a goal of 3 is mid-emoji and snaps to
    // the cluster start; a goal of 4 is the boundary after it.
    let mut text = String::from("ab👍🏽cd\nxyz");
    let mut c = Caret {
        byte: text.len(), // end of "xyz": col 3
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Up, Mods::NONE, 20); // goal col 3
    assert_eq!(c.byte, 2, "mid-emoji goal snapped to the cluster start");
    let mut text = String::from("ab👍🏽cd\nxyzw");
    let mut c = Caret {
        byte: text.len(), // end of "xyzw": col 4
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Up, Mods::NONE, 20); // goal col 4
    assert_eq!(c.byte, 10, "boundary after the emoji is its own stop");
}

#[test]
fn home_end_are_per_visual_row_and_ctrl_spans_the_document() {
    // Width 13: rows are "hello world " (12 cols — one spare) / "next".
    let mut text = String::from("hello world next");
    let mut c = Caret {
        byte: 14, // inside "next" on row 1
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Home, Mods::NONE, 13);
    assert_eq!(c.byte, 12, "Home goes to the visual row start");
    assert!(!c.sticky);
    let mut c2 = Caret {
        byte: 2,
        ..Caret::origin()
    };
    apply(&mut text, &mut c2, Key::End, Mods::NONE, 13);
    assert_eq!(c2.byte, 12, "End of the soft row is the boundary byte");
    assert!(c2.sticky, "…with end affinity (the row has a spare cell)");
    let map = RowMap::build(&text, 13);
    assert_eq!(
        map.visual(&text, c2.byte, c2.sticky).0,
        0,
        "renders on row 0"
    );
    assert_eq!(
        map.visual(&text, c2.byte, false).0,
        1,
        "without affinity the same byte is row 1"
    );
    // On a FULL soft row (width 11 makes row 0 exactly "hello world "
    // with no spare cell) End declines the affinity: the boundary byte
    // renders at the next row's start instead of stomping the margin.
    let mut c3 = Caret {
        byte: 2,
        ..Caret::origin()
    };
    apply(&mut text, &mut c3, Key::End, Mods::NONE, 11);
    assert_eq!(c3.byte, 12);
    assert!(!c3.sticky, "full rows have no margin cell");
    apply(&mut text, &mut c2, Key::Home, Mods::CTRL, 13);
    assert_eq!(c2.byte, 0);
    apply(&mut text, &mut c2, Key::End, Mods::CTRL, 13);
    assert_eq!(c2.byte, text.len());
}

#[test]
fn history_edges_follow_the_arrow_nav_decision_table() {
    // Empty buffer: straight to history.
    let mut text = String::new();
    let mut c = Caret::origin();
    assert_eq!(
        apply(&mut text, &mut c, Key::Up, Mods::NONE, 10),
        EditOutcome::HistoryBack
    );
    assert_eq!(
        apply(&mut text, &mut c, Key::Down, Mods::NONE, 10),
        EditOutcome::HistoryForward
    );
    // Multi-row buffer: Up walks rows first…
    let mut text = String::from("ab\ncd");
    let mut c = Caret {
        byte: 4,
        ..Caret::origin()
    };
    assert_eq!(
        apply(&mut text, &mut c, Key::Up, Mods::NONE, 10),
        EditOutcome::Handled { edited: false }
    );
    // …then jumps to the text start…
    assert_eq!(
        apply(&mut text, &mut c, Key::Up, Mods::NONE, 10),
        EditOutcome::Handled { edited: false }
    );
    assert_eq!(c.byte, 0);
    // …and only the edge recalls history.
    assert_eq!(
        apply(&mut text, &mut c, Key::Up, Mods::NONE, 10),
        EditOutcome::HistoryBack
    );
    // Down mirrors: rows, then end, then history.
    assert_eq!(
        apply(&mut text, &mut c, Key::Down, Mods::NONE, 10),
        EditOutcome::Handled { edited: false }
    );
    assert_eq!(
        apply(&mut text, &mut c, Key::Down, Mods::NONE, 10),
        EditOutcome::Handled { edited: false }
    );
    assert_eq!(c.byte, text.len());
    assert_eq!(
        apply(&mut text, &mut c, Key::Down, Mods::NONE, 10),
        EditOutcome::HistoryForward
    );
    // Shift+arrow at the edge extends the selection, never recalls.
    let mut c = Caret::origin();
    assert_eq!(
        apply(&mut text, &mut c, Key::Down, Mods::SHIFT, 10),
        EditOutcome::Handled { edited: false }
    );
}

#[test]
fn history_store_preserves_the_draft_across_a_round_trip() {
    let mut h = History::new(8);
    h.push("one");
    h.push("two");
    h.push("two"); // consecutive duplicate: skipped
    assert_eq!(h.len(), 2);
    assert_eq!(h.back("draft in progress").as_deref(), Some("two"));
    assert_eq!(h.back("ignored").as_deref(), Some("one"));
    assert_eq!(h.back("ignored"), None, "at the oldest: stays put");
    assert_eq!(h.forward().as_deref(), Some("two"));
    assert_eq!(
        h.forward().as_deref(),
        Some("draft in progress"),
        "past the newest: the draft returns"
    );
    assert_eq!(h.forward(), None, "not navigating anymore");
    // Cap drops oldest-first.
    let mut h = History::new(2);
    h.push("a");
    h.push("b");
    h.push("c");
    assert_eq!(h.len(), 2);
    assert_eq!(h.back("").as_deref(), Some("c"));
    assert_eq!(h.back("").as_deref(), Some("b"));
    assert_eq!(h.back(""), None, "'a' dropped by the cap");
}

#[test]
fn cluster_atomic_edits_over_zwj_and_combining_marks() {
    // Backspace removes a whole ZWJ family.
    let mut text = String::from("x👨\u{200D}👩\u{200D}👧\u{200D}👦");
    let mut c = Caret {
        byte: text.len(),
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Backspace, Mods::NONE, 20);
    assert_eq!(text, "x");
    assert_eq!(c.byte, 1);
    // Inserting a combining mark merges clusters; the caret snaps past
    // the merged whole (never mid-cluster).
    let mut text = String::from("ab");
    let mut c = Caret {
        byte: 1,
        ..Caret::origin()
    };
    apply(&mut text, &mut c, Key::Char('\u{0301}'), Mods::NONE, 20);
    assert_eq!(text, "a\u{0301}b");
    assert_eq!(c.byte, 3, "caret after the merged cluster");
    // Delete removes one whole cluster forward.
    let mut text = String::from("👍🏽y");
    let mut c = Caret::origin();
    apply(&mut text, &mut c, Key::Delete, Mods::NONE, 20);
    assert_eq!(text, "y");
}

#[test]
fn word_jumps_cross_line_boundaries() {
    let mut text = String::from("one two\nthree");
    let mut c = Caret::origin();
    apply(&mut text, &mut c, Key::Right, Mods::ALT, 20);
    assert_eq!(c.byte, 3);
    apply(&mut text, &mut c, Key::Right, Mods::ALT, 20);
    assert_eq!(c.byte, 7);
    apply(&mut text, &mut c, Key::Right, Mods::ALT, 20);
    assert_eq!(c.byte, 13, "jump crosses the newline");
    apply(&mut text, &mut c, Key::Left, Mods::ALT, 20);
    assert_eq!(c.byte, 8, "back to the start of 'three'");
}

#[test]
fn selection_spans_rows_and_replaces_on_type() {
    let mut text = String::from("ab\ncd");
    let mut c = Caret::origin();
    apply(&mut text, &mut c, Key::Down, Mods::SHIFT, 10);
    assert_eq!(c.anchor, Some(0));
    assert_eq!(c.byte, 3, "extended to row 1 col 0");
    apply(&mut text, &mut c, Key::Char('Z'), Mods::NONE, 10);
    assert_eq!(text, "Zcd", "selection (incl. newline) replaced");
    assert_eq!(c.anchor, None);
}

#[test]
fn scroll_window_follows_the_caret() {
    let mut c = Caret::origin();
    model::adjust_top(&mut c, 5, 10, 3);
    assert_eq!(c.top, 3, "caret row 5 visible in a 3-row window");
    model::adjust_top(&mut c, 0, 10, 3);
    assert_eq!(c.top, 0, "moving up pulls the window up");
    model::adjust_top(&mut c, 9, 4, 3);
    assert_eq!(c.top, 1, "top clamps to total - visible");
}

// ----------------------------------------------------------------- widget

fn composer(
    size: Size,
) -> (
    crate::reactive::RootScope,
    UiTree,
    TextAreaState,
    std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) {
    let t = &default_theme().tokens;
    let submitted: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let s2 = submitted.clone();
    let holder: std::rc::Rc<std::cell::RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        let st = state.clone();
        TextArea::new()
            .state(&state)
            .placeholder("say something")
            .rows(1, 3)
            .on_submit(move |v| {
                s2.borrow_mut().push(v.to_string());
                st.push_history(v);
                st.clear();
            })
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab); // focus
    let state = holder.borrow().clone().expect("state");
    (root, tree, state, submitted)
}

#[test]
fn typing_renders_inside_the_frame_and_wraps() {
    let size = Size::new(12, 4);
    let (_root, mut tree, state, _) = composer(size);
    type_str(&mut tree, "hello world next");
    assert_eq!(state.text(), "hello world next");
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert_eq!(canvas.cell(Point::new(1, 0)).unwrap().0, 'h');
    assert!(canvas.row_text(0).contains("hello"));
    assert!(
        canvas.row_text(1).contains("next"),
        "soft wrap: {:?}",
        canvas.row_text(1)
    );
    // Focused frame wears border_focus on every widget row.
    let theme = default_theme();
    assert_eq!(
        canvas.cell(Point::new(0, 0)).unwrap().1,
        theme.tokens.border_focus
    );
    assert_eq!(
        canvas.cell(Point::new(0, 1)).unwrap().1,
        theme.tokens.border_focus
    );
}

#[test]
fn grows_to_cap_then_scrolls_internally() {
    let size = Size::new(12, 6);
    let t = &default_theme().tokens;
    let (root, mut tree) = mount_widget(size, move |cx| {
        let state = TextAreaState::new(cx);
        crate::ui::Element::new()
            .style(crate::layout::Style::column())
            .child(
                TextArea::new()
                    .state(&state)
                    .rows(1, 3)
                    .element(cx, t)
                    .build(),
            )
            .child(crate::ui::text("MARK"))
            .build()
    });
    key(&mut tree, Key::Tab);
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(1).contains("MARK"), "1 row when empty");
    type_str(&mut tree, "aaaa bbbb cccc"); // wraps to 2 rows at width 10
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("MARK"), "grew to 2 rows");
    type_str(&mut tree, " dddd eeee ffff gggg");
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert!(
        canvas.row_text(3).contains("MARK"),
        "capped at 3 rows: {:?}",
        (0..6).map(|y| canvas.row_text(y)).collect::<Vec<_>>()
    );
    assert!(
        !canvas.row_text(0).contains("aaaa"),
        "scrolled: the first row left the window"
    );
    drop(root);
}

#[test]
fn enter_submits_and_alt_shift_enter_insert_newlines() {
    let size = Size::new(20, 5);
    let (_root, mut tree, state, submitted) = composer(size);
    type_str(&mut tree, "hello");
    key_mod(&mut tree, Key::Enter, Mods::ALT);
    type_str(&mut tree, "world");
    assert_eq!(state.text(), "hello\nworld", "Alt+Enter inserts");
    key_mod(&mut tree, Key::Enter, Mods::SHIFT);
    type_str(&mut tree, "!");
    assert_eq!(state.text(), "hello\nworld\n!", "kitty Shift+Enter inserts");
    key(&mut tree, Key::Enter);
    assert_eq!(*submitted.borrow(), vec!["hello\nworld\n!".to_string()]);
    assert_eq!(state.text(), "", "the submit handler cleared the buffer");
}

#[test]
fn enter_inserts_policy_never_submits() {
    let t = &default_theme().tokens;
    let submitted: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let s2 = submitted.clone();
    let holder: std::rc::Rc<std::cell::RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let (_root, mut tree) = mount_widget(Size::new(20, 5), move |cx| {
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        TextArea::new()
            .state(&state)
            .submit_policy(SubmitPolicy::EnterInserts)
            .on_submit(move |v| s2.borrow_mut().push(v.to_string()))
            .element(cx, t)
            .build()
    });
    key(&mut tree, Key::Tab);
    type_str(&mut tree, "a");
    key(&mut tree, Key::Enter);
    type_str(&mut tree, "b");
    let state = holder.borrow().clone().unwrap();
    assert_eq!(state.text(), "a\nb");
    assert!(submitted.borrow().is_empty());
}

#[test]
fn block_paste_inserts_whole_and_never_submits() {
    let size = Size::new(20, 5);
    let (_root, mut tree, state, submitted) = composer(size);
    tree.dispatch(&UiEvent::Paste("line one\r\nline two\rline three".into()));
    assert_eq!(
        state.text(),
        "line one\nline two\nline three",
        "newlines kept, endings normalized"
    );
    assert!(submitted.borrow().is_empty(), "paste is never a submit");
    // Caret at the end, scroll window shows the tail.
    crate::reactive::flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(2).contains("line three"));
}

#[test]
fn history_recall_replaces_buffer_and_draft_survives() {
    let size = Size::new(24, 5);
    let (_root, mut tree, state, _) = composer(size);
    type_str(&mut tree, "first");
    key(&mut tree, Key::Enter);
    type_str(&mut tree, "second");
    key(&mut tree, Key::Enter);
    assert_eq!(state.history_len(), 2);
    type_str(&mut tree, "draft");
    // One-row buffer, caret at the end: first Up jumps to the start…
    key(&mut tree, Key::Up);
    assert_eq!(state.text(), "draft");
    assert_eq!(state.caret_byte(), 0);
    // …the edge Up recalls the newest entry.
    key(&mut tree, Key::Up);
    assert_eq!(state.text(), "second");
    assert_eq!(state.caret_byte(), 6, "recalled entries open at their end");
    key(&mut tree, Key::Up); // to start of "second"
    key(&mut tree, Key::Up); // older
    assert_eq!(state.text(), "first");
    // Down mirrors: end first, then newer, then the draft returns.
    key(&mut tree, Key::Down); // (already at end after recall)
    assert_eq!(state.text(), "second");
    key(&mut tree, Key::Down);
    assert_eq!(state.text(), "draft", "the in-progress draft survived");
}

#[test]
fn caret_cell_tracks_typing_and_clears_on_blur() {
    let size = Size::new(20, 4);
    let (_root, mut tree, state, _) = composer(size);
    let first = state.caret_cell().get_untracked().expect("focused: anchor");
    type_str(&mut tree, "ab");
    let after = state.caret_cell().get_untracked().expect("still focused");
    assert_eq!(after.y, first.y);
    assert_eq!(after.x, first.x + 2, "anchor follows the caret");
    tree.set_focus(None);
    assert_eq!(
        state.caret_cell().get_untracked(),
        None,
        "blur clears the anchor (owner-driven dismissal keys off this)"
    );
}

#[test]
fn placeholder_disabled_and_a11y() {
    let t = &default_theme().tokens;
    let theme = default_theme();
    let size = Size::new(20, 3);
    // Placeholder shows only unfocused + empty (TextInput parity).
    let (_root, mut tree) = mount_widget(size, move |cx| {
        TextArea::new().placeholder("say it").element(cx, t).build()
    });
    let canvas = render(&mut tree, size);
    assert!(canvas.row_text(0).contains("say it"));
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().1,
        theme.tokens.text_faint
    );
    key(&mut tree, Key::Tab);
    let canvas = render(&mut tree, size);
    assert!(!canvas.row_text(0).contains("say it"));
    // a11y: role + live value.
    type_str(&mut tree, "hi");
    let snapshot = tree.accessibility_tree();
    let entry = snapshot.find(crate::ui::Role::TextArea).expect("role");
    assert_eq!(entry.value.as_deref(), Some("hi"));
    assert_eq!(entry.label, "say it");
    // Disabled: not focusable, faint ink, inert.
    let t2 = &default_theme().tokens;
    let holder: std::rc::Rc<std::cell::RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let (_root2, mut tree2) = mount_widget(size, move |cx| {
        let state = TextAreaState::new(cx);
        state.set_text("frozen");
        *h2.borrow_mut() = Some(state.clone());
        TextArea::new()
            .state(&state)
            .disabled(true)
            .element(cx, t2)
            .build()
    });
    key(&mut tree2, Key::Tab);
    assert_eq!(tree2.focused(), None, "disabled is out of the focus order");
    type_str(&mut tree2, "x");
    let state = holder.borrow().clone().unwrap();
    assert_eq!(state.text(), "frozen", "disabled ignores keys");
    crate::reactive::flush_effects();
    tree2.layout();
    let canvas = render(&mut tree2, size);
    assert_eq!(
        canvas.cell(Point::new(1, 0)).unwrap().1,
        theme.tokens.text_faint,
        "disabled ink is text_faint"
    );
}

#[test]
fn replace_range_snaps_to_cluster_boundaries() {
    let (_root, _tree, state, _) = composer(Size::new(20, 4));
    state.set_text("x👍🏽y");
    // Byte 3 sits inside the emoji cluster (1..9): both ends snap.
    state.replace_range(3..3, "@");
    assert_eq!(
        state.text(),
        "x👍🏽@y",
        "mid-cluster insert snapped to the end"
    );
    assert_eq!(state.caret_byte(), 10);
}
