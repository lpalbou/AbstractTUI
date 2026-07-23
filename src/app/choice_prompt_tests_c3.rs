//! ChoicePrompt unit tests, first-app 0271 half (split file,
//! `#[path]`-included as `choice_prompt::tests::c3` — the file
//! budget): the approval-gate adoption knobs — `body_width` (the panel
//! is content-derived and the body was invisible to `measure`),
//! `dismiss_label` (a defer-shaped Esc must not render "Cancel" on a
//! consent surface), and `ChoicePromptHandle::retire` (host-owned
//! close, distinct from the user's Cancelled). Same rig as the parent
//! module.

use super::*;
use crate::base::Size;
use crate::ui::text;

/// The consumer's approval shape: three short options that size the
/// panel to ~45 cols while the body carries ~72-col card rows.
fn approval(p: ChoicePrompt) -> ChoicePrompt {
    p.option_key("approve", "Approve", 'a')
        .option_key("all", "Approve all", 'A')
        .option_key("deny", "Deny", 'd')
}

/// A deterministic 72-col card row (the consumer pre-wraps at ~72).
fn card_line() -> String {
    format!("$ {}", "x".repeat(70))
}

// ---------------------------------------------------------- body_width

#[test]
fn body_width_widens_the_panel_the_options_could_not() {
    let line = card_line();
    // Without the knob: the panel is sized by the options/hint — the
    // 72-col body row CLIPS (the 0271 blocker).
    let r = rig_sized(Size::new(80, 20));
    let body_line = line.clone();
    let (_o, _h) = open_on(&r, |p| approval(p).body(move |_| text(body_line.clone())));
    let (narrow, rows) = r.modal().expect("gate open");
    assert!(
        !rows.iter().any(|row| row.contains(line.as_str())),
        "premise: without body_width the 72-col row cannot fit ({}w): {rows:?}",
        narrow.w
    );

    // With it: the body's declared width participates in measure —
    // inner 72 + the Modal's 1-cell padding each side = 74.
    let r = rig_sized(Size::new(80, 20));
    let body_line = line.clone();
    let (_o, _h) = open_on(&r, |p| {
        approval(p)
            .body(move |_| text(body_line.clone()))
            .body_width(72)
    });
    let (wide, rows) = r.modal().expect("gate open");
    assert_eq!(wide.w, 74, "panel = declared 72 + 2 (Modal padding)");
    assert!(wide.w > narrow.w, "the knob widened the panel");
    assert!(
        rows.iter().any(|row| row.contains(line.as_str())),
        "the 72-col card row renders UNCLIPPED: {rows:?}"
    );
}

#[test]
fn body_width_clamps_into_narrow_viewports_options_never_clip() {
    // A 40-col terminal cannot honor 72: the existing margins clamp
    // the panel; the BODY clips inside its region — the options and
    // the gate's operability survive untouched.
    let mut r = rig_sized(Size::new(40, 16));
    let line = card_line();
    let body_line = line.clone();
    let (outcomes, _h) = open_on(&r, |p| {
        approval(p)
            .body(move |_| text(body_line.clone()))
            .body_width(72)
    });
    let (bounds, rows) = r.modal().expect("gate open");
    assert!(bounds.w <= 40, "clamped into the viewport: {bounds:?}");
    assert!(
        !rows.iter().any(|row| row.contains(line.as_str())),
        "the body clips honestly on a narrow terminal: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("Approve all")),
        "options never pay for the body's width: {rows:?}"
    );
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1, "narrow gate still commits");
}

#[test]
fn body_width_without_a_body_is_inert() {
    // Like body_rows, the knob declares the BODY's need — no body, no
    // contribution (panel identical to the bare gate's).
    let r = rig_sized(Size::new(80, 20));
    let (_o, _h) = open_on(&r, basic);
    let (bare, _) = r.modal().expect("gate open");

    let r = rig_sized(Size::new(80, 20));
    let (_o, _h) = open_on(&r, |p| basic(p).body_width(72));
    let (knobbed, _) = r.modal().expect("gate open");
    assert_eq!(
        knobbed.size(),
        bare.size(),
        "body_width without a body must not move the panel"
    );
}

#[test]
fn body_width_interplay_prompt_unwraps_and_hint_survives() {
    // The prompt wraps at the SOLVED width: widened by the body it
    // renders whole; the full hint (which fits inside 72) does too —
    // the knob composes with every other measured line.
    let prompt = "Approve this batch of three write_file calls for the agent?";
    let pw = crate::text::width(prompt);
    assert!(
        pw > 52 && pw <= 72,
        "test premise: past the 52-col prompt cap (must wrap without the \
         body's width), within the declared 72 (whole once widened) — got {pw}"
    );
    let mut r = rig_sized(Size::new(80, 20));
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let body_line = card_line();
    approval(ChoicePrompt::new(prompt))
        .body(move |_| text(body_line.clone()))
        .body_width(72)
        .overlays(&r.overlays)
        .on_resolve(move |o| sink.borrow_mut().push(o))
        .open(r.root.scope());
    flush_effects();
    let (_, rows) = r.modal().expect("gate open");
    assert!(
        rows.iter().any(|row| row.contains(prompt)),
        "the 59-col prompt renders on ONE line inside the widened panel: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("a/A/d pick · Enter confirms · Esc cancels")),
        "the full hint survives at the widened width: {rows:?}"
    );
    r.key(Key::Char('d'));
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["deny".into()],
            other: None,
        })],
        "the widened gate keeps its whole vocabulary"
    );
}

// -------------------------------------------------------- dismiss_label

#[test]
fn dismiss_label_renames_button_and_hint_outcome_stays_cancelled() {
    // The approval surface's Esc DEFERS (the run keeps waiting) — the
    // rendered contract must say so on both surfaces...
    let mut r = rig_sized(Size::new(56, 16));
    let (outcomes, _h) = open_on(&r, |p| approval(p).dismiss_label("Defer"));
    let text = r.modal_text();
    assert!(text.contains("Defer"), "button follows the label: {text}");
    assert!(text.contains("Esc Defer"), "hint follows the label: {text}");
    assert!(!text.contains("Cancel"), "no mislabeled button: {text}");
    assert!(!text.contains("Esc cancels"), "no mislabeled hint: {text}");

    // ...while the OUTCOME stays Cancelled — the caller's wiring maps
    // it (renaming the enum variant would be a breaking change).
    r.key(Key::Escape);
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!r.is_open());

    // The relabeled BUTTON rides the same path.
    let mut r = rig_sized(Size::new(56, 16));
    let (outcomes, _h) = open_on(&r, |p| approval(p).dismiss_label("Defer"));
    let (x, y) = r.find("Defer").expect("Defer button");
    r.click(x, y);
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
}

#[test]
fn dismiss_label_default_vocabulary_is_byte_stable() {
    // No dismiss_label call: the built-in pair renders exactly as
    // before — existing gates must not re-render under this wave.
    let r = rig();
    let (_o, _h) = open_on(&r, basic);
    let text = r.modal_text();
    assert!(text.contains("Cancel"), "default button: {text}");
    assert!(text.contains("Esc cancels"), "default hint: {text}");
}

#[test]
fn dismiss_label_is_irrelevant_on_must_choose_gates() {
    // dismissable(false) still refuses: no button, no Esc segment, no
    // label anywhere — the hint stays truthful in both modes.
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).dismiss_label("Defer").dismissable(false));
    let text = r.modal_text();
    assert!(!text.contains("Defer"), "no dismiss affordance: {text}");
    assert!(!text.contains("Cancel"), "{text}");
    r.key(Key::Escape);
    assert!(outcomes.borrow().is_empty(), "must-choose still refuses");
    assert!(r.is_open());
    assert!(
        r.modal_text().contains("an answer is required"),
        "the refusal stays visible: {}",
        r.modal_text()
    );
}

#[test]
fn dismiss_label_long_label_widens_button_and_hint_honestly() {
    // buttons_w/hint_w are computed from the ACTUAL label — a long
    // caller label renders whole, never clipped by "Cancel"-era
    // arithmetic.
    let r = rig_sized(Size::new(72, 16));
    let (_o, _h) = open_on(&r, |p| basic(p).dismiss_label("Postpone this decision"));
    let text = r.modal_text();
    assert!(
        text.contains("Postpone this decision"),
        "the button renders whole: {text}"
    );
    assert!(
        text.contains("Esc Postpone this decision"),
        "the hint segment renders whole: {text}"
    );
}

// ---------------------------------------------------------------- retire

#[test]
fn retire_closes_the_gate_without_resolving() {
    let r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    assert!(handle.is_open());
    handle.retire();
    flush_effects();
    assert!(
        outcomes.borrow().is_empty(),
        "retire NEVER fires on_resolve — the host owns the outcome"
    );
    assert!(!r.is_open(), "the modal is closed");
    assert!(!handle.is_open(), "the handle reads closed");
}

#[test]
fn retire_is_idempotent_and_post_retire_endings_are_inert() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    handle.retire();
    handle.retire(); // idempotent — the flag is already consumed
    flush_effects();
    assert!(outcomes.borrow().is_empty());

    // The exactly-once flag is CONSUMED: no later ending can ever
    // reach the callback — cancel(), Enter, Escape all land on a
    // closed gate.
    handle.cancel();
    r.key(Key::Enter);
    r.key(Key::Escape);
    assert!(
        outcomes.borrow().is_empty(),
        "post-retire endings must never resolve"
    );
    assert!(!r.is_open());

    // The host's reopen invariant: a FRESH gate opens and answers
    // normally after a retire (nothing lingers).
    let (outcomes, _h) = open_on(&r, basic);
    assert!(r.is_open(), "a new gate opens after a retire");
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1);
}

#[test]
fn retire_after_resolution_is_a_no_op() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1, "answered normally");
    handle.retire(); // the answer already reached the flow — no-op
    assert_eq!(outcomes.borrow().len(), 1);
    assert!(!handle.is_open());
}
