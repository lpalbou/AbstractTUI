//! Combobox + MultiSelect unit tests — child of `select::tests`
//! (split sibling for the file budget; the shared Rig lives in the
//! parent module).
use super::*;

// ---------------------------------------------------------------- Combobox

fn combo_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("apple"),
        SelectOption::new("banana"),
        SelectOption::new("cherry"),
        SelectOption::new("grape").disabled(true),
        SelectOption::new("apricot"),
    ]
}

#[test]
fn combobox_typing_filters_and_enter_commits_a_match_only() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let changes: Rc<RefCell<Vec<usize>>> = Default::default();
    let vh = value_holder.clone();
    let ch = changes.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(usize::MAX);
        *vh.borrow_mut() = Some(value);
        Combobox::new(combo_options())
            .value(value)
            .layout(face_layout())
            .overlays(ov)
            .on_change(move |i| ch.borrow_mut().push(i))
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    rig.key(Key::Enter); // open
    let (bounds, rows) = rig.popup().expect("open");
    assert_eq!(
        bounds.y, 0,
        "anchor row included: popup starts AT the trigger"
    );
    assert!(rows.iter().any(|r| r.contains("5 of 5")), "{rows:?}");
    // Typing lands in the popup-mounted editor and refilters.
    rig.type_str("ap");
    let (_, rows) = rig.popup().expect("still open");
    assert!(
        rows.iter().any(|r| r.contains("ap")),
        "editor shows the query"
    );
    assert!(rows.iter().any(|r| r.contains("apple")));
    assert!(rows.iter().any(|r| r.contains("apricot")));
    assert!(rows.iter().any(|r| r.contains("grape")), "substring match");
    assert!(!rows.iter().any(|r| r.contains("banana")), "filtered out");
    assert!(
        rows.iter().any(|r| r.contains("3 of 5")),
        "count line: {rows:?}"
    );
    // Enter commits the highlighted (first enabled) match: apple.
    rig.key(Key::Enter);
    assert!(rig.popup().is_none());
    assert_eq!(value.get_untracked(), 0, "apple committed");
    assert_eq!(changes.borrow().as_slice(), [0]);
    // The filter text was never the value: reopen shows all options.
    rig.key(Key::Enter);
    let (_, rows) = rig.popup().expect("reopened");
    assert!(rows.iter().any(|r| r.contains("5 of 5")), "filter reset");
    // A non-matching buffer commits NOTHING (popup stays open).
    rig.type_str("zz");
    let (_, rows) = rig.popup().expect("open");
    assert!(rows.iter().any(|r| r.contains("no matches")), "{rows:?}");
    rig.key(Key::Enter);
    assert!(rig.popup().is_some(), "no match: Enter is a no-op");
    assert_eq!(value.get_untracked(), 0, "nothing committed");
    rig.key(Key::Escape);
    assert!(rig.popup().is_none());
    assert_eq!(changes.borrow().as_slice(), [0], "no further on_change");
}

#[test]
fn combobox_navigation_moves_highlight_through_the_filtered_list() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let vh = value_holder.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(usize::MAX);
        *vh.borrow_mut() = Some(value);
        Combobox::new(combo_options())
            .value(value)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    rig.key(Key::Enter);
    rig.type_str("ap"); // apple, grape(disabled), apricot
    rig.key(Key::Down); // apple -> apricot (grape skipped)
    rig.key(Key::Enter);
    assert_eq!(value.get_untracked(), 4, "apricot committed");
    // Home/End stay with the editor (cursor motion), not the list.
    rig.key(Key::Enter);
    rig.type_str("a");
    rig.key(Key::Home); // editor cursor home — popup must stay open
    assert!(rig.popup().is_some());
    rig.key(Key::Escape);
}

// -------------------------------------------------------------- MultiSelect

#[test]
fn multiselect_space_toggles_enter_commits_once_escape_abandons() {
    let values_holder: Rc<RefCell<Option<Signal<Vec<String>>>>> = Default::default();
    let changes: Rc<RefCell<Vec<Vec<String>>>> = Default::default();
    let vh = values_holder.clone();
    let ch = changes.clone();
    let mut rig = rig(move |cx, ov| {
        let values = cx.signal(Vec::<String>::new());
        *vh.borrow_mut() = Some(values);
        MultiSelect::new(vec![
            SelectOption::new("read"),
            SelectOption::new("write"),
            SelectOption::new("exec"),
        ])
        .values(values)
        .layout(face_layout())
        .overlays(ov)
        .on_change(move |set| ch.borrow_mut().push(set))
        .element(cx, &default_theme().tokens)
        .build()
    });
    let values = values_holder.borrow().unwrap();
    rig.key(Key::Enter); // open
    let (_, rows) = rig.popup().expect("open");
    assert!(rows[0].contains("[ ] read"), "unchecked marks: {rows:?}");
    rig.key(Key::Char(' ')); // toggle read
    let (_, rows) = rig.popup().expect("still open — Space never closes");
    assert!(rows[0].contains("[x] read"), "{rows:?}");
    rig.key(Key::Down);
    rig.key(Key::Down);
    rig.key(Key::Char(' ')); // toggle exec
    assert!(
        values.get_untracked().is_empty(),
        "toggles never write the set"
    );
    assert!(changes.borrow().is_empty(), "no on_change before commit");
    rig.key(Key::Enter); // commit
    assert!(rig.popup().is_none());
    assert_eq!(
        values.get_untracked(),
        vec!["read".to_string(), "exec".to_string()],
        "canonical option order"
    );
    assert_eq!(changes.borrow().len(), 1, "one on_change per commit");
    // Escape abandons the working copy.
    rig.key(Key::Enter);
    rig.key(Key::Char(' ')); // untoggle read (working copy only)
    rig.key(Key::Escape);
    assert_eq!(
        values.get_untracked(),
        vec!["read".to_string(), "exec".to_string()],
        "Escape left the committed set alone"
    );
    assert_eq!(changes.borrow().len(), 1);
    // Committing the SAME set fires no on_change.
    rig.key(Key::Enter);
    rig.key(Key::Enter);
    assert_eq!(changes.borrow().len(), 1, "unchanged set = no on_change");
}

#[test]
fn multiselect_click_toggles_and_collapsed_row_degrades_to_count() {
    let values_holder: Rc<RefCell<Option<Signal<Vec<String>>>>> = Default::default();
    let vh = values_holder.clone();
    let mut rig = rig(move |cx, ov| {
        let values = cx.signal(Vec::<String>::new());
        *vh.borrow_mut() = Some(values);
        MultiSelect::new(vec![
            SelectOption::new("alpha metric"),
            SelectOption::new("beta metric"),
            SelectOption::new("gamma metric"),
        ])
        .values(values)
        .layout(face_layout())
        .overlays(ov)
        .element(cx, &default_theme().tokens)
        .build()
    });
    let values = values_holder.borrow().unwrap();
    rig.click(2, 0); // open
    let (bounds, rows) = rig.popup().expect("open");
    let beta = rows.iter().position(|r| r.contains("beta")).unwrap() as i32;
    rig.click(bounds.x + 2, bounds.y + beta); // toggle by click
    let (_, rows) = rig.popup().expect("click toggles, never closes");
    assert!(rows[beta as usize].contains("[x]"), "{rows:?}");
    // Toggle two more so the joined labels overflow the 24-cell row.
    let alpha = rows.iter().position(|r| r.contains("alpha")).unwrap() as i32;
    let gamma = rows.iter().position(|r| r.contains("gamma")).unwrap() as i32;
    rig.click(bounds.x + 2, bounds.y + alpha);
    rig.click(bounds.x + 2, bounds.y + gamma);
    rig.key(Key::Enter);
    assert_eq!(values.get_untracked().len(), 3);
    rig.tree.layout();
    let mut canvas = BufferCanvas::new(VP);
    rig.tree.draw(&mut canvas);
    assert!(
        canvas.row_text(0).contains("3 selected"),
        "overflow degrades to a count: {:?}",
        canvas.row_text(0)
    );
}
