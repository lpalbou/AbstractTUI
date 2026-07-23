//! ChoicePrompt unit tests, flows half (split file, `#[path]`-included
//! as `choice_prompt::tests::flows` — the file budget): the Other
//! free-text lane, disposal/reopen under the 0297 law, windowing,
//! render honesty, and the ChoiceSequence chain. Same rig as the
//! parent module.

use super::*;

// -------------------------------------------------------------- other

#[test]
fn other_reveals_input_typing_does_not_fight_list_keys() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…"));
    assert!(
        !r.modal_text().contains("type your answer"),
        "input hidden until engaged"
    );
    r.key(Key::End); // highlight the Other row
    let text = r.modal_text();
    assert!(text.contains("● Other…"), "Other is the candidate: {text}");
    assert!(
        text.contains("type your answer"),
        "engaging Other reveals the input: {text}"
    );
    // Digits type into the focused editor — they must NOT jump-select.
    r.type_str("custom 42");
    let text = r.modal_text();
    assert!(text.contains("custom 42"), "typed text renders: {text}");
    assert!(text.contains("● Other…"), "candidate stayed on Other");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: Some("custom 42".into()),
        })],
        "single-mode Other answer"
    );
}

#[test]
fn other_empty_text_refuses_commit_until_text_or_retreat() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…"));
    r.key(Key::End);
    r.key(Key::Enter); // hollow Other: refused
    assert!(outcomes.borrow().is_empty(), "hollow Other never resolves");
    assert!(r.is_open());
    assert!(
        r.modal_text().contains("needs text"),
        "refusal is visible: {}",
        r.modal_text()
    );
    r.type_str("  "); // whitespace-only is still hollow
    r.key(Key::Enter);
    assert!(outcomes.borrow().is_empty(), "trim guards the answer");
    r.type_str("x");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: Some("x".into()),
        })]
    );
}

#[test]
fn other_in_multiple_mode_rides_the_checked_set() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true).allow_other("Other…"));
    r.key(Key::Char('1')); // check Alpha
    r.key(Key::End); // move to Other
    r.key(Key::Char(' ')); // check Other -> input reveals + focuses
    assert!(r.modal_text().contains("☑ Other…"), "{}", r.modal_text());
    r.type_str("more");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["a".into()],
            other: Some("more".into()),
        })],
        "checked set + other text commit together"
    );
}

#[test]
fn other_retreat_hides_input_and_keeps_typed_text() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_other("Other…"));
    r.key(Key::End);
    r.type_str("draft");
    // Up bubbles out of the editor: candidate retreats, input hides.
    r.key(Key::Up);
    let text = r.modal_text();
    assert!(!text.contains("draft"), "input hidden on retreat: {text}");
    assert!(text.contains("● Gamma"), "candidate moved up");
    r.key(Key::End);
    assert!(
        r.modal_text().contains("draft"),
        "typed text survives the round trip"
    );
    // Enter now commits Other with the kept text.
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: Some("draft".into()),
        })]
    );
}

#[test]
fn other_only_question_is_a_free_text_gate() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| p.allow_other("Name it…"));
    assert!(r.is_open(), "zero options + Other is answerable");
    r.type_str("tycho");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: Some("tycho".into()),
        })]
    );
}

// ------------------------------------------------- disposal + reopen

#[test]
fn resolve_may_dispose_the_opener_scope() {
    // The 0297 law end-to-end: on_resolve disposes the scope that
    // OPENED the prompt, synchronously, from inside the commit path.
    let mut r = rig();
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let overlays = r.overlays.clone();
    let (root, ()) = create_root(move |cx| {
        let opener = cx.child();
        ChoicePrompt::new("Delete everything?")
            .option("y", "Yes")
            .option("n", "No")
            .overlays(&overlays)
            .on_resolve(move |o| {
                sink.borrow_mut().push(o);
                opener.dispose(); // the callback tears its opener down
            })
            .open(opener);
    });
    flush_effects();
    assert!(r.is_open());
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1, "resolved despite disposal");
    assert!(!r.is_open(), "modal gone");
    root.dispose();
}

#[test]
fn on_resolve_may_open_the_next_prompt_and_gate_is_reopenable() {
    let mut r = rig();
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let overlays = r.overlays.clone();
    let (root, ()) = create_root(move |cx| {
        let ov = overlays.clone();
        ChoicePrompt::new("First?")
            .option("1", "One")
            .overlays(&overlays)
            .on_resolve(move |o| {
                sink.borrow_mut().push(o);
                let sink2 = sink.clone();
                // Chained gate opened from inside the resolution.
                ChoicePrompt::new("Second?")
                    .option("2", "Two")
                    .overlays(&ov)
                    .on_resolve(move |o| sink2.borrow_mut().push(o))
                    .open(cx);
            })
            .open(cx);
    });
    flush_effects();
    assert!(r.modal_text().contains("First?"));
    r.key(Key::Enter);
    assert!(
        r.modal_text().contains("Second?"),
        "next gate opened from on_resolve: {}",
        r.modal_text()
    );
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 2);
    assert!(!r.is_open());
    root.dispose();
}

#[test]
#[should_panic(expected = "cannot be answered")]
fn unanswerable_question_asserts_loudly_in_debug() {
    // Zero options and no Other: debug builds name the mistake
    // (release builds resolve `Cancelled` instead of hanging the
    // gated flow — the same resolve path this panic interrupts).
    let r = rig();
    let _ = ChoicePrompt::new("Nothing to pick")
        .overlays(&r.overlays)
        .open(r.root.scope());
}

#[test]
#[should_panic(expected = "no Overlays available")]
fn open_without_overlay_store_asserts_loudly_in_debug() {
    let r = rig();
    // No `.overlays(..)` and no App context on this bare scope.
    let _ = ChoicePrompt::new("Where do I mount?")
        .option("a", "Alpha")
        .open(r.root.scope());
}

// ---------------------------------------------------------- windowing

#[test]
fn windowing_keeps_highlight_reachable_with_twenty_options() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |mut p| {
        for i in 1..=20 {
            p = p.option(format!("o{i}"), format!("Option {i:02}"));
        }
        p.max_visible(6)
    });
    let text = r.modal_text();
    assert!(text.contains("Option 01"), "top of the window: {text}");
    assert!(
        !text.contains("Option 09"),
        "beyond the 6-row budget is windowed out: {text}"
    );
    assert!(text.contains("1/20"), "position note renders: {text}");
    for _ in 0..19 {
        r.key(Key::Down);
    }
    let text = r.modal_text();
    assert!(
        text.contains("● Option 20"),
        "highlight reached the tail through the window: {text}"
    );
    assert!(!text.contains("Option 01"), "head windowed out: {text}");
    assert!(text.contains("20/20"), "position note follows: {text}");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["o20".into()],
            other: None,
        })]
    );
}

#[test]
fn wheel_moves_the_highlight() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, basic);
    r.wheel(true);
    r.wheel(true);
    assert!(r.modal_text().contains("● Gamma"), "{}", r.modal_text());
    r.wheel(false);
    assert!(r.modal_text().contains("● Beta"));
    assert!(
        outcomes.borrow().is_empty(),
        "wheel is movement, not commit"
    );
}

// ------------------------------------------------------------- render

#[test]
fn detail_rows_render_muted_under_the_label() {
    let mut r = rig();
    let (_outcomes, _h) = open_on(&r, |p| {
        p.option_detail("del", "Delete them", "the working copies are lost")
            .option("keep", "Keep everything")
    });
    let text = r.modal_text();
    assert!(
        text.contains("the working copies are lost"),
        "detail line renders: {text}"
    );
    // Highlighted row (row 0 at open): its detail joins the selection
    // pair. After moving away, the same detail wears the MUTED tone.
    let t = crate::theme::default_theme().tokens;
    let (fg, bg) = r.ink_at("the working copies").expect("detail cell");
    assert_eq!((fg, bg), (t.selection_fg, t.selection_bg));
    r.key(Key::Down);
    let (fg, bg) = r.ink_at("the working copies").expect("detail cell");
    assert_eq!(fg, t.text_muted, "unhighlighted detail is muted");
    assert_eq!(bg, t.overlay, "on the modal ground");
}

#[test]
fn narrow_width_stays_honest_and_operable() {
    let mut r = rig_sized(Size::new(24, 10));
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    ChoicePrompt::new("A very long prompt that must wrap honestly")
        .option_detail("a", "Alpha with a long label", "and a longer detail line")
        .option("b", "Beta")
        .overlays(&r.overlays)
        .on_resolve(move |o| sink.borrow_mut().push(o))
        .open(r.root.scope());
    flush_effects();
    let (bounds, rows) = r.modal().expect("narrow gate open");
    assert!(bounds.w <= 24 && bounds.h <= 10, "clamped: {bounds:?}");
    assert!(
        rows.iter().any(|row| row.contains('…')),
        "truncation is visible, not silent: {rows:?}"
    );
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1, "narrow gate still commits");
}

#[test]
fn tab_cycles_the_trapped_focus_between_list_and_buttons() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true));
    // Tab to Confirm, Enter activates IT (not the root commit path —
    // but both commit; prove the button lane by tabbing to CANCEL).
    r.key(Key::Tab);
    r.key(Key::Tab);
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Cancelled],
        "focused Cancel consumed Enter"
    );
}

// ------------------------------------------------------------ sequence

#[test]
fn sequence_completes_in_order_and_cancel_reports_index() {
    let mut r = rig();
    let outcomes: Rc<RefCell<Vec<ChoiceSequenceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let overlays = r.overlays.clone();
    let q1 = {
        let mut q = ChoiceQuestion::new("Pick one?");
        q.options.push(ChoiceOption::new("a", "Alpha"));
        q.options.push(ChoiceOption::new("b", "Beta"));
        q
    };
    let q2 = {
        let mut q = ChoiceQuestion::new("Pick more?");
        q.options.push(ChoiceOption::new("x", "Xen"));
        q.allow_multiple = true;
        q
    };
    let (root, ()) = create_root(move |cx| {
        ChoiceSequence::new(vec![q1, q2])
            .overlays(&overlays)
            .on_resolve(move |o| sink.borrow_mut().push(o))
            .open(cx);
    });
    flush_effects();
    assert!(r.modal_text().contains("Pick one?"));
    r.key(Key::Down);
    r.key(Key::Enter);
    assert!(r.modal_text().contains("Pick more?"), "chained open");
    r.key(Key::Char(' '));
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceSequenceOutcome::Completed(vec![
            ChoiceAnswer {
                selected: vec!["b".into()],
                other: None,
            },
            ChoiceAnswer {
                selected: vec!["x".into()],
                other: None,
            },
        ])]
    );
    root.dispose();

    // Cancel mid-flight reports the index + answers so far.
    let outcomes: Rc<RefCell<Vec<ChoiceSequenceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let overlays = r.overlays.clone();
    let q1 = {
        let mut q = ChoiceQuestion::new("One?");
        q.options.push(ChoiceOption::new("a", "Alpha"));
        q
    };
    let q2 = {
        let mut q = ChoiceQuestion::new("Two?");
        q.options.push(ChoiceOption::new("b", "Beta"));
        q
    };
    let (root, ()) = create_root(move |cx| {
        ChoiceSequence::new(vec![q1, q2])
            .overlays(&overlays)
            .on_resolve(move |o| sink.borrow_mut().push(o))
            .open(cx);
    });
    flush_effects();
    r.key(Key::Enter); // answer One?
    r.key(Key::Escape); // cancel Two?
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceSequenceOutcome::Cancelled {
            index: 1,
            answers: vec![ChoiceAnswer {
                selected: vec!["a".into()],
                other: None,
            }],
        }]
    );
    root.dispose();
}

#[test]
fn sequence_empty_completes_immediately() {
    let r = rig();
    let outcomes: Rc<RefCell<Vec<ChoiceSequenceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let overlays = r.overlays.clone();
    let (root, ()) = create_root(move |cx| {
        ChoiceSequence::new(Vec::new())
            .overlays(&overlays)
            .on_resolve(move |o| sink.borrow_mut().push(o))
            .open(cx);
    });
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceSequenceOutcome::Completed(Vec::new())],
        "empty sequence resolves synchronously"
    );
    assert!(!r.is_open());
    root.dispose();
}
