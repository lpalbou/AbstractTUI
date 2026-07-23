//! ReasoningSelect state-coupling tests — child of `reasoning::tests`
//! (split sibling): the non-reasoning lock, the unknown-state
//! override flow, the modal SCREEN-anchor pin (the P1 class), a11y
//! lock values and the zero-idle claim. The Rig and helpers live in
//! the parent module.

use super::*;

// -------------------------------------------- the non-reasoning state

#[test]
fn locked_refuses_open_with_why_line() {
    let (seen, log) = seen_log();
    let mut rig = rig(VP, move |cx| {
        ReasoningSelect::new(ReasoningFacts::non_reasoning())
            .on_change(log)
            .view(cx)
    });
    rig.settle();
    let screen = rig.screen();
    assert!(
        screen.contains("r: none (locked) — model does not reason"),
        "the locked trigger carries the why-line:\n{screen}"
    );
    // Keyboard: the control is OUT of the focus order (the
    // select-family disabled convention) — Tab lands nowhere, Enter
    // opens nothing.
    rig.input(b"\t");
    rig.input(b"\r");
    assert!(rig.popup_bounds().is_none(), "keyboard cannot open");
    // Mouse: a click on the trigger refuses too.
    rig.click(2, 1);
    assert!(rig.popup_bounds().is_none(), "click cannot open");
    assert!(seen.borrow().is_empty());
}

// ------------------------------------------------- the unknown state

#[test]
fn unknown_offers_set_anyway_which_unlocks_the_full_ladder() {
    let (seen, log) = seen_log();
    let mut rig = rig(VP, move |cx| {
        ReasoningSelect::new(ReasoningFacts::unknown())
            .on_change(log)
            .view(cx)
    });
    rig.settle();
    assert!(
        rig.screen()
            .contains("r: none (locked) — capability unknown"),
        "locked-to-none by default:\n{}",
        rig.screen()
    );
    rig.input(b"\t");
    rig.input(b"\r");
    let popup = rig
        .popup_bounds()
        .expect("unknown state OPENS (unlike non-reasoning)");
    assert_eq!(popup.h, 1, "one row: the override");
    let access = rig.popup_access().expect("access");
    assert!(
        access.contains("set anyway (capability unknown — passed verbatim)"),
        "{access}"
    );
    // Activating the override swaps to the FULL ladder in place.
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("ladder popup open");
    assert_eq!(popup.h, 7, "auto + the six ladder steps");
    let access = rig.popup_access().expect("access");
    for row in ["auto", "none", "minimal", "low", "medium", "high", "xhigh"] {
        assert!(
            access.contains(&format!("menuitem \"{row}\"")),
            "full ladder offers {row}:\n{access}"
        );
    }
    // Type-ahead rides the core for free; commit unlocks the display.
    rig.input(b"x");
    rig.input(b"\r");
    assert_eq!(*seen.borrow(), vec!["xhigh"]);
    let screen = rig.screen();
    assert!(
        screen.contains("r: xhigh") && !screen.contains("(locked)"),
        "committed override clears the lock annotation:\n{screen}"
    );
    // Reopen: straight to the ladder (the latch holds for this
    // instance), current value marked.
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("reopen");
    assert_eq!(popup.h, 7, "no second override gate on this instance");
}

#[test]
fn unknown_commit_of_none_keeps_the_lock_annotation() {
    // Committing "none" from the locked-none display is NOT a change:
    // silent close, nothing written, the annotation stays honest.
    let (seen, log) = seen_log();
    let mut rig = rig(VP, move |cx| {
        ReasoningSelect::new(ReasoningFacts::unknown())
            .on_change(log)
            .view(cx)
    });
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r"); // open (override row)
    rig.input(b"\r"); // unlock -> ladder
    rig.input(b"n"); // type-ahead -> none
    rig.input(b"\r");
    assert!(rig.popup_bounds().is_none());
    assert!(seen.borrow().is_empty(), "none == effective none: silent");
    assert!(
        rig.screen()
            .contains("r: none (locked) — capability unknown"),
        "still locked-to-none — nothing was overridden:\n{}",
        rig.screen()
    );
}

#[test]
fn unknown_override_does_not_leak_across_remount() {
    // The model-change recipe: the app remounts with fresh facts; the
    // override latch and the committed value die with the instance.
    let model: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let m = model.clone();
    let mut rig = rig(VP, move |cx| {
        let which = cx.signal(0usize);
        *m.borrow_mut() = Some(which);
        dyn_view_scoped(LayoutStyle::line(1), move |gcx| {
            let _ = which.get(); // model switch remounts the control
            Element::new()
                .style(LayoutStyle::line(1))
                .child(ReasoningSelect::new(ReasoningFacts::unknown()).view(gcx))
                .build()
        })
    });
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r"); // override row
    rig.input(b"\r"); // unlock
    rig.input(b"x");
    rig.input(b"\r"); // commit xhigh
    assert!(rig.screen().contains("r: xhigh"), "{}", rig.screen());
    // The "model" changes: fresh facts, fresh instance.
    model.borrow().expect("signal").set(1);
    rig.settle();
    assert!(
        rig.screen()
            .contains("r: none (locked) — capability unknown"),
        "stale override never leaks onto the next model:\n{}",
        rig.screen()
    );
    rig.input(b"\t");
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup");
    assert_eq!(popup.h, 1, "the override gate is back");
}

// ------------------------------------------------- anchor + a11y + idle

#[test]
fn popup_inside_a_modal_anchors_at_the_screen_cell() {
    // The P1 regression class: anchors are SCREEN cells even when the
    // opener lives on a positioned overlay layer.
    let mut rig = rig(VP, |_cx| text("root under modal"));
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(44, 7), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text("model settings"))
            .child(
                // The footer-row composition apps use: the control's
                // grow default fills the ROW; a bare column child
                // would grow TALL (the Select family default).
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(ReasoningSelect::new(ReasoningFacts::capable(["low", "high"])).view(mcx))
                    .build(),
            )
            .build()
    });
    rig.settle();
    // Locate the trigger by its LEFT STROKE (`▐`) on the label row —
    // char positions, never byte offsets (the modal border glyphs are
    // multi-byte), and no assumption about the modal's padding.
    let shot = rig.driver.screenshot();
    let mut at = None;
    for y in 0..VP.h {
        let row: Vec<char> = (0..VP.w)
            .map(|x| {
                shot.cell(x, y)
                    .map(|c| c.text().chars().next().unwrap_or(' '))
                    .unwrap_or(' ')
            })
            .collect();
        let s: String = row.iter().collect();
        if s.contains("r: auto") {
            let sx = row.iter().position(|&c| c == '▐').expect("stroke") as i32;
            at = Some((sx, y));
        }
    }
    let (sx, sy) = at.expect("trigger rendered inside the modal");
    assert!(sx > 30 && sy > 10, "trigger sits inside the centered modal");
    rig.input(b"\r"); // modal focused its first focusable: the control
    let popup = rig.popup_bounds().expect("popup open above the modal");
    assert_eq!(
        (popup.x, popup.y),
        (sx, sy + 1),
        "popup adjacent to the trigger in SCREEN space; got {popup:?}"
    );
}

#[test]
fn a11y_labels_carry_lock_state() {
    let mut capable = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::capable(["high"])).view(cx)
    });
    capable.settle();
    let access = capable.app.tree().accessibility_tree_text();
    assert!(
        access.contains("button \"reasoning\"") && access.contains("auto"),
        "capable trigger: button role + current value:\n{access}"
    );

    let mut locked = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::non_reasoning()).view(cx)
    });
    locked.settle();
    let access = locked.app.tree().accessibility_tree_text();
    assert!(
        access.contains("button \"reasoning\"")
            && access.contains("none (locked — model does not reason)"),
        "locked trigger names WHY in its value:\n{access}"
    );

    let mut unknown = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::unknown()).view(cx)
    });
    unknown.settle();
    let access = unknown.app.tree().accessibility_tree_text();
    assert!(
        access.contains("none (locked — capability unknown)"),
        "unknown trigger names the unknown lock:\n{access}"
    );
    unknown.input(b"\t");
    unknown.input(b"\r");
    let popup = unknown.popup_access().expect("popup access");
    assert!(
        popup.contains("menu \"reasoning\"") && popup.contains("menuitem"),
        "popup reports menu/menuitem roles:\n{popup}"
    );
}

#[test]
fn closed_control_is_idle() {
    let mut rig = rig(VP, |cx| {
        ReasoningSelect::new(ReasoningFacts::capable(["high"])).view(cx)
    });
    rig.settle();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("turn");
    assert!(turn.idle, "closed control schedules no work");
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"\x1b[27u"); // Escape
    rig.settle();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("turn");
    assert!(turn.idle, "open/close leaves nothing running");
}
