//! ChoicePrompt unit tests, wave-5 cycle-2 half (split file,
//! `#[path]`-included as `choice_prompt::tests::c2` — the file
//! budget): the REVIEWER findings folded in cycle 2 — per-option
//! shortcut letters (F2), must-choose mode (F3), layered Esc from the
//! Other editor (F4, position folded), danger tint (F7), the a11y
//! contract (F1), and the focus affordance (charter A5). Same rig as
//! the parent module.

use super::*;

fn approval(p: ChoicePrompt) -> ChoicePrompt {
    p.option_key("approve", "Approve", 'a')
        .option_key("all", "Approve all", 'A')
        .option_key("deny", "Deny", 'd')
        .danger("deny")
}

// ------------------------------------------------------------- letters

#[test]
fn option_letters_commit_single_and_are_case_sensitive() {
    // Wider rig: the full hint ("a/A/d pick · …") needs ~41 columns;
    // at the default rig width it degrades by design (segments drop
    // from the front — pinned in the narrow-width tests).
    let mut r = rig_sized(Size::new(56, 16));
    let (outcomes, _h) = open_on(&r, approval);
    let text = r.modal_text();
    assert!(text.contains("(a)"), "letter renders in the row: {text}");
    assert!(
        text.contains("a/A/d pick"),
        "hint names the letters (F6/K2): {text}"
    );

    // 'A' (uppercase) commits its OWN option — one resolution.
    r.key(Key::Char('A'));
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["all".into()],
            other: None,
        })],
        "uppercase letter is its own key (case-sensitive)"
    );
    assert!(!r.is_open(), "letter commit closed the gate");

    // Lowercase 'a' on a fresh gate commits "approve".
    let (outcomes, _h) = open_on(&r, approval);
    r.key(Key::Char('a'));
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["approve".into()],
            other: None,
        })]
    );
}

#[test]
fn option_letters_toggle_in_multiple_and_declared_key_beats_digit() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| approval(p).allow_multiple(true));
    r.key(Key::Char('a'));
    r.key(Key::Char('d'));
    let text = r.modal_text();
    assert!(
        text.contains("☑ Approve") && text.contains("☑ Deny"),
        "letters jump-toggle in multiple mode: {text}"
    );
    assert!(outcomes.borrow().is_empty(), "toggles never resolve");
    r.key(Key::Char('d')); // toggle back off
    assert!(r.modal_text().contains("☐ Deny"));
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["approve".into()],
            other: None,
        })]
    );

    // A declared DIGIT key wins over the digit-jump lane: '2' commits
    // its option in single mode instead of merely moving the highlight.
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| {
        p.option("a", "Alpha")
            .option_key("b", "Beta", '2')
            .option("c", "Gamma")
    });
    r.key(Key::Char('2'));
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })],
        "declared keys outrank the movement-only digit jump"
    );
}

#[test]
fn option_letters_type_into_a_focused_other_editor() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| approval(p).allow_other("Other…"));
    r.key(Key::End); // engage Other; the editor autofocuses
    r.type_str("ad"); // letters TYPE — the shield holds (F2 collision rule)
    assert!(outcomes.borrow().is_empty(), "no activation while editing");
    assert!(
        r.modal_text().contains("ad"),
        "letters landed in the draft: {}",
        r.modal_text()
    );
}

// ---------------------------------------------------------- must-choose

#[test]
fn non_dismissable_esc_refuses_visibly_and_clears_on_action() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, |p| basic(p).dismissable(false));
    let text = r.modal_text();
    assert!(!text.contains("Cancel"), "no Cancel button: {text}");
    assert!(
        !text.contains("Esc cancels"),
        "no advertised Esc — a dead key would lie: {text}"
    );

    r.key(Key::Escape);
    assert!(outcomes.borrow().is_empty(), "Esc refused — no resolution");
    assert!(r.is_open(), "gate still open");
    assert!(
        r.modal_text().contains("an answer is required"),
        "refusal is VISIBLE (charter G3): {}",
        r.modal_text()
    );

    r.key(Key::Down); // acting clears the note
    assert!(
        !r.modal_text().contains("an answer is required"),
        "note clears once the user acts: {}",
        r.modal_text()
    );

    // Programmatic cancel stays available (timeout/deadline consumers).
    handle.cancel();
    flush_effects();
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!r.is_open());
}

// ---------------------------------------------------------- layered Esc

#[test]
fn esc_in_other_retreats_first_keeps_draft_then_second_esc_cancels() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…"));
    r.key(Key::End); // engage Other; editor autofocused
    r.type_str("half an answer");
    assert!(
        r.modal_text().contains("Esc back to the list"),
        "hint tells the layered-Esc truth while editing: {}",
        r.modal_text()
    );

    // First Esc: retreat — the gate stays, the draft stays, the
    // highlight stays on Other (the editor merely blurred).
    r.key(Key::Escape);
    assert!(outcomes.borrow().is_empty(), "first Esc never cancels (F4)");
    assert!(r.is_open());
    let text = r.modal_text();
    assert!(text.contains("half an answer"), "draft survives: {text}");
    assert!(text.contains("● Other…"), "still the candidate: {text}");
    assert!(
        !text.contains("Esc back to the list"),
        "editing hint gone after retreat: {text}"
    );

    // Second Esc (focus back on the list): cancels the gate.
    r.key(Key::Escape);
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!r.is_open());
}

#[test]
fn esc_in_other_retreats_then_refuses_on_a_must_choose_gate() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…").dismissable(false));
    r.key(Key::End);
    r.type_str("draft");
    r.key(Key::Escape); // retreat (layered Esc holds in both modes)
    assert!(outcomes.borrow().is_empty());
    assert!(r.is_open());
    r.key(Key::Escape); // second Esc: the must-choose refusal
    assert!(outcomes.borrow().is_empty(), "must-choose never cancels");
    assert!(
        r.modal_text().contains("an answer is required"),
        "refusal visible: {}",
        r.modal_text()
    );
}

// --------------------------------------------------------------- danger

#[test]
fn danger_option_wears_error_ink_except_under_the_selection_pair() {
    let t = crate::theme::default_theme().tokens;
    let mut r = rig();
    let (_outcomes, _h) = open_on(&r, |p| {
        p.option("keep", "Keep everything")
            .option("del", "Delete them")
            .danger("del")
    });
    // Unhighlighted danger row: Error ink on the modal ground.
    let (fg, bg) = r.ink_at("Delete them").expect("danger row");
    assert_eq!(fg, t.error, "danger label rides the Error token (T3)");
    assert_eq!(bg, t.overlay);
    // Highlighted with the list focused: the audited selection pair
    // outranks the tint (the pair is the audited combination).
    r.key(Key::Down);
    let (fg, bg) = r.ink_at("Delete them").expect("danger row");
    assert_eq!((fg, bg), (t.selection_fg, t.selection_bg));
}

// ----------------------------------------------------------------- a11y

#[test]
fn a11y_tree_names_question_options_and_selection_state() {
    let mut r = rig();
    let (_outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…"));
    let snap = r.a11y();
    assert!(
        snap.contains("heading \"Proceed how?\""),
        "the question is in the tree (A1):\n{snap}"
    );
    assert!(
        snap.contains("menu \"options\" = \"Alpha\""),
        "region names the current choice (A2):\n{snap}"
    );
    assert!(
        snap.contains("menuitem \"Alpha\" = \"selected\""),
        "selected option carries its state (A2):\n{snap}"
    );
    assert!(
        snap.contains("menuitem \"Beta\"") && snap.contains("menuitem \"Gamma\""),
        "every option label is enumerable (A2/A3):\n{snap}"
    );
    assert!(
        !snap.contains("input"),
        "no phantom Other editor before engagement (O1/A4):\n{snap}"
    );

    // Selection state tracks movement; the revealed editor is an
    // Input and holds focus (A4).
    r.key(Key::Down);
    let snap = r.a11y();
    assert!(snap.contains("menuitem \"Beta\" = \"selected\""), "{snap}");
    r.key(Key::End);
    let snap = r.a11y();
    assert!(
        snap.contains("input") && snap.contains("[focused]"),
        "revealed editor is an Input with focus truth (A4):\n{snap}"
    );
}

#[test]
fn a11y_multiple_mode_reports_checkbox_state() {
    let mut r = rig();
    let (_outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true));
    let snap = r.a11y();
    assert!(
        snap.contains("checkbox \"Alpha\" = \"off\""),
        "unchecked state readable (A2):\n{snap}"
    );
    r.key(Key::Char(' '));
    let snap = r.a11y();
    assert!(
        snap.contains("checkbox \"Alpha\" = \"on\""),
        "checked state tracks toggles (A2):\n{snap}"
    );
}

// ------------------------------------------------------ focus affordance

#[test]
fn region_focus_affordance_visible_and_unfocused_highlight_distinct() {
    let t = crate::theme::default_theme().tokens;
    let mut r = rig();
    let (_outcomes, _h) = open_on(&r, basic);
    // At open the region holds focus: the highlight wears the audited
    // selection pair, and the engine's focus-visible check passes.
    let (fg, bg) = r.ink_at("Alpha").expect("row");
    assert_eq!((fg, bg), (t.selection_fg, t.selection_bg));
    let mut tree = r.modal_tree().expect("modal tree");
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "focused region must change pixels (charter A5)"
    );

    // Tab to the Cancel button: the highlight degrades to accent ink
    // (still distinct from plain rows — T2 holds unfocused), and the
    // focused BUTTON shows its own affordance.
    r.key(Key::Tab);
    let (fg, bg) = r.ink_at("Alpha").expect("row");
    assert_eq!(
        (fg, bg),
        (t.accent, t.overlay),
        "unfocused highlight is accent ink (the RadioGroup precedent)"
    );
    let (plain_fg, _) = r.ink_at("Beta").expect("plain row");
    assert_ne!(fg, plain_fg, "highlight still distinct unfocused (T2)");
    let mut tree = r.modal_tree().expect("modal tree");
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "focused button shows its affordance (charter A5)"
    );
}
