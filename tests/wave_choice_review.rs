//! Wave-5 REVIEWER acceptance tests for the decision-gate component
//! (`ChoicePrompt`, backlog 0515) — CYCLE 3: the 16 cycle-1 skeletons
//! activated against the REAL public API, plus the consumer dry-run.
//!
//! Every test pins a clause of `reviews/wave5/acceptance-charter.md`
//! (charter IDs in test names), never implementation trivia; wire
//! bytes in through `CaptureTerm`, modeled VT screen out (the same
//! harness posture as wave_choice_prompt.rs — helper duplication
//! across integration files is the house style).
//!
//! Cycle-3 provenance notes:
//! - The cycle-1 hand-rolled reference gate (Modal + List) is RETIRED:
//!   its five active substrate tests are repointed at the real
//!   component in this file, as promised in the cycle-1 header.
//! - ONE clause stays unactivatable: charter A1–A4's tree-level half
//!   needs the MODAL's accessibility snapshot, and `Overlays` exposes
//!   no public path to an overlay layer's `UiTree` (only the creator's
//!   `LayerHandle::tree()`; `ChoicePrompt` owns its handle privately).
//!   Recorded as finding F10 in reviews/wave5/review-cycle3-verdict.md;
//!   the tree half is pinned by BUILDER's in-crate unit tests
//!   (`a11y_tree_names_question_options_and_selection_state`,
//!   `a11y_multiple_mode_reports_checkbox_state`,
//!   `region_focus_affordance_visible_and_unfocused_highlight_distinct`)
//!   which run in this same tree gate; the OBSERVABLE half (reveal, no
//!   phantom editor, marker visibility) is pinned here at pixel level.
//! - Esc wire byte: `\x1b[27u` (the builder-notes caveat; a bare
//!   `\x1b` is ambiguous to the parser).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

const W: i32 = 52;
const H: i32 = 16;

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..128 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 128 turns");
}

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

/// Column (in CELLS, not bytes) of `needle`'s first char in `line` —
/// glyph rows carry multi-byte chars, so `str::find` byte offsets are
/// not columns.
fn col_of(line: &str, needle: &str) -> Option<i32> {
    line.find(needle)
        .map(|byte| line[..byte].chars().count() as i32)
}

/// Locate `needle` on screen as (col, row) in cells.
fn locate(term: &CaptureTerm, needle: &str) -> Option<(i32, i32)> {
    for (y, line) in screen(term).lines().enumerate() {
        if let Some(x) = col_of(line, needle) {
            return Some((x, y as i32));
        }
    }
    None
}

type OutcomeLog = Rc<RefCell<Vec<ChoiceOutcome>>>;

fn recorder(log: &OutcomeLog) -> impl FnOnce(ChoiceOutcome) + 'static {
    let log = log.clone();
    move |o| log.borrow_mut().push(o)
}

fn answered(ids: &[&str], other: Option<&str>) -> ChoiceOutcome {
    ChoiceOutcome::Answered(ChoiceAnswer {
        selected: ids.iter().map(|s| s.to_string()).collect(),
        other: other.map(str::to_string),
    })
}

/// Host app: a focusable root with an 'x' shortcut — the K1 witness
/// (any 'x' that reaches it while a gate is open proves the gate does
/// not own the keyboard). Returns (app, mount scope, leak counter).
fn host(size: Size) -> (App, Scope, Rc<RefCell<u32>>) {
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let leaked: Rc<RefCell<u32>> = Rc::default();
    let (ss, lk) = (scope_slot.clone(), leaked.clone());
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        let lk2 = lk.clone();
        Element::new()
            .style(LayoutStyle::column())
            .focusable()
            .autofocus()
            .shortcut(KeyChord::plain(Key::Char('x')), move |_| {
                *lk2.borrow_mut() += 1;
            })
            .child(text("host app row"))
            .build()
    })
    .expect("mount");
    let scope = scope_slot.borrow().expect("scope");
    (app, scope, leaked)
}

/// Boot a driver and settle the host paint.
fn boot(app: &mut App, term: &mut CaptureTerm) -> Driver {
    let mut driver = Driver::new(app, term, config()).expect("driver");
    settle(&mut driver, app, term);
    driver
}

// ===========================================================================
// G — gate semantics
// ===========================================================================

/// Charter G5 + the G1 tail: arrows BROWSE without committing (0250
/// clause 1); Enter commits exactly once; a stray Enter after the gate
/// closed cannot re-fire.
#[test]
fn charter_g5_arrows_browse_enter_commits_exactly_once() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Deploy to production?")
        .option("alpha", "Ship it")
        .option("beta", "Hold")
        .option("gamma", "Abort")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Deploy to production?"));

    term.push_input(b"\x1b[B\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        log.borrow().is_empty(),
        "movement must never commit (charter G5)"
    );
    assert!(screen(&term).contains("● Abort"), "{}", screen(&term));

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["gamma"], None)]);
    assert!(
        !screen(&term).contains("Deploy to production?"),
        "gate closed after resolution"
    );

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().len(), 1, "no second resolution (charter G1)");
}

/// Charter G1: hostile double-submit paths resolve exactly once.
/// Path 1: a declared option LETTER and Enter in ONE input batch.
/// Path 2: a click on the selected row and Esc in ONE input batch.
/// Path 3: Esc and a click on the (former) selected row in ONE batch —
/// the cancel lands first, the click dies against the closed layer.
#[test]
fn charter_g1_double_paths_letter_enter_and_click_esc_resolve_once() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    // Path 1: letter + Enter, same batch. The letter commits; the
    // trailing Enter lands after the modal closed and must do nothing.
    let log1: OutcomeLog = Rc::default();
    ChoicePrompt::new("Apply the patch?")
        .option_key("ship", "Ship it", 's')
        .option("hold", "Hold")
        .on_resolve(recorder(&log1))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"s\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log1.borrow().as_slice(), [answered(&["ship"], None)]);

    // Path 2: click-on-selected + Esc, same batch. The click commits
    // (the 0250 mouse ruling); the Esc lands after close and must not
    // append a Cancelled.
    let log2: OutcomeLog = Rc::default();
    ChoicePrompt::new("Apply the patch?")
        .option("ship", "Ship it")
        .option("hold", "Hold")
        .on_resolve(recorder(&log2))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let (x, y) = locate(&term, "● Ship it").expect("selected row rendered");
    // SGR press+release on the row (1-based), then Esc — one batch.
    let mut batch =
        format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", x + 2, y + 1, x + 2, y + 1).into_bytes();
    batch.extend_from_slice(b"\x1b[27u");
    term.push_input(&batch);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log2.borrow().as_slice(),
        [answered(&["ship"], None)],
        "click commits once; the batched Esc must not add Cancelled"
    );

    // Path 3: Esc first, click second, same batch — exactly one
    // Cancelled; the trailing click must not resurrect a commit.
    let log3: OutcomeLog = Rc::default();
    ChoicePrompt::new("Apply the patch?")
        .option("ship", "Ship it")
        .option("hold", "Hold")
        .on_resolve(recorder(&log3))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let (x, y) = locate(&term, "● Ship it").expect("selected row rendered");
    let mut batch = b"\x1b[27u".to_vec();
    batch.extend_from_slice(
        format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", x + 2, y + 1, x + 2, y + 1).as_bytes(),
    );
    term.push_input(&batch);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log3.borrow().as_slice(),
        [ChoiceOutcome::Cancelled],
        "Esc cancels once; the batched click must not add an answer"
    );
}

/// Charter G2: dismissal is an EXPLICIT outcome — Esc resolves
/// `Cancelled` through the same callback, never a silent close.
#[test]
fn charter_g2_esc_delivers_an_explicit_dismissed_outcome() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Proceed?")
        .option("a", "Yes")
        .option("b", "No")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!screen(&term).contains("Proceed?"), "gate closed");
}

/// Charter G3: a must-choose gate refuses Esc VISIBLY (nothing
/// resolves, the refusal is on screen), the note clears on the next
/// action, and the gate remains fully operable.
#[test]
fn charter_g3_non_dismissable_esc_refuses_visibly_then_clears() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick a lane (required)")
        .option("l", "Left")
        .option("r", "Right")
        .dismissable(false)
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    // A dead key must not be advertised (F3/K2 half).
    assert!(!screen(&term).contains("Esc cancels"), "{}", screen(&term));
    // Must-choose single: no Cancel button (its options ARE the endings).
    assert!(!screen(&term).contains("Cancel"), "{}", screen(&term));

    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert!(log.borrow().is_empty(), "Esc must not resolve (charter G3)");
    assert!(
        screen(&term).contains("an answer is required"),
        "refusal must be VISIBLE: {}",
        screen(&term)
    );

    term.push_input(b"\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains("an answer is required"),
        "the refusal note clears on the next action"
    );

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["r"], None)]);
}

/// Charter G4 (the 0297 law at component level): the resolve callback
/// may synchronously dispose the SCOPE THAT OPENED the gate.
#[test]
fn charter_g4_resolve_callback_may_dispose_the_opener_scope() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let opener = scope.child();
    let log: OutcomeLog = Rc::default();
    let sink = log.clone();
    ChoicePrompt::new("Dispose me on answer")
        .option("ok", "Understood")
        .on_resolve(move |o| {
            sink.borrow_mut().push(o);
            opener.dispose(); // the natural choose-close-continue shape
        })
        .open(opener);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["ok"], None)]);
    assert!(
        !screen(&term).contains("Dispose me"),
        "gate gone, no panic, screen repainted: {}",
        screen(&term)
    );
}

/// Charter G6: a gate opened from INSIDE the previous gate's resolve
/// callback works immediately (no dropped modal, no key-eating
/// leftover layer) and starts with FRESH state.
#[test]
fn charter_g6_gate_reopens_cleanly_and_chains_from_resolve() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log1: OutcomeLog = Rc::default();
    let log2: OutcomeLog = Rc::default();
    let sink1 = log1.clone();
    let sink2 = log2.clone();
    ChoicePrompt::new("First decision")
        .option("one", "One")
        .option("two", "Two")
        .on_resolve(move |o| {
            sink1.borrow_mut().push(o);
            // The chained "next stage" — the consumer's picker idiom.
            ChoicePrompt::new("Second decision")
                .option("fresh-a", "Fresh A")
                .option("fresh-b", "Fresh B")
                .on_resolve(move |o| sink2.borrow_mut().push(o))
                .open(scope);
        })
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    // Commit "two" on gate 1 (Down + Enter).
    term.push_input(b"\x1b[B\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log1.borrow().as_slice(), [answered(&["two"], None)]);
    assert!(
        screen(&term).contains("Second decision"),
        "chained gate rendered: {}",
        screen(&term)
    );
    // Fresh state: gate 2's candidate starts at ITS first option, not
    // gate 1's resting row.
    assert!(screen(&term).contains("● Fresh A"), "{}", screen(&term));

    // Keys are live immediately (no leftover key-eating layer).
    term.push_input(b"\x1b[B\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log2.borrow().as_slice(), [answered(&["fresh-b"], None)]);
    assert!(!screen(&term).contains("Second decision"), "gate 2 closed");
}

/// Charter G7: the outcome names the chosen option by STABLE ID, not
/// by display index or label.
#[test]
fn charter_g7_outcome_names_the_option_stably() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Which route?")
        .option("route-primary", "Primary")
        .option("route-fallback", "Fallback")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[B\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        [answered(&["route-fallback"], None)]
    );
}

// ===========================================================================
// O — the "Other" contract
// ===========================================================================

/// Charter O1 (observable half — the tree half is builder-unit-pinned,
/// see the header + F10): before engaging Other there is NO editor (no
/// placeholder, and printables sink nowhere); engaging reveals it.
#[test]
fn charter_o1_other_reveals_editor_no_phantom_before() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type")
        .option("a", "Alpha")
        .option("b", "Beta")
        .allow_other("Something else…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains("type your answer"),
        "no editor before engagement: {}",
        screen(&term)
    );

    // A printable with no meaning must sink NOWHERE (no phantom field
    // absorbing keystrokes): the screen is byte-identical after.
    let before = screen(&term);
    term.push_input(b"z");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(before, screen(&term), "'z' typed into a phantom editor");

    // Engage Other (End jumps to the last row).
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● Something else…"), "{s}");
    assert!(s.contains("type your answer"), "editor revealed: {s}");

    term.push_input(b"hi");
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("hi"), "typing lands in the editor");
}

/// Charter O2: while the Other editor is focused, printables (digits
/// included) and horizontal caret keys belong to the FIELD; Up/Down
/// return to the option list.
#[test]
fn charter_o2_typing_routes_chars_and_digits_to_field_updown_to_list() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type")
        .option("a", "Alpha")
        .option("b", "Beta")
        .allow_other("Something else…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[F"); // engage Other
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"gpt2"); // '2' must TYPE, not digit-jump
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("gpt2"), "digits type while editing: {s}");
    assert!(s.contains("● Something else…"), "highlight unmoved: {s}");

    // Horizontal caret keys belong to the field: Left Left, insert 'x'.
    term.push_input(b"\x1b[D\x1b[Dx");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("gpxt2"),
        "caret editing inside the draft: {}",
        screen(&term)
    );

    // Up returns to the LIST (the defended charter position — the
    // engine's own Combobox routing).
    term.push_input(b"\x1b[A");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● Beta"), "Up moved the selection: {s}");
    assert!(
        !s.contains("type your answer") && !s.contains("gpxt2"),
        "editor hides when disengaged (single mode): {s}"
    );
    assert!(log.borrow().is_empty(), "routing never committed anything");
}

/// Charter O3: the Other draft survives selection excursions AND the
/// Esc-retreat within one gate lifetime; a fresh gate starts empty.
/// Also pins: retreat keeps ENGAGEMENT (Enter after retreat commits
/// the draft), and a blurred editor no longer receives printables.
#[test]
fn charter_o3_other_draft_survives_excursion_and_esc_retreat() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type")
        .option("a", "Alpha")
        .allow_other("Something else…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    // Type a draft, leave, come back: intact.
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"custom");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[A"); // excursion to Alpha (editor hides)
    settle(&mut driver, &mut app, &mut term);
    assert!(!screen(&term).contains("custom"));
    term.push_input(b"\x1b[F"); // back to Other
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("custom"),
        "draft survived the excursion: {}",
        screen(&term)
    );

    // Esc-retreat: draft + engagement kept, focus back on the list —
    // a further printable must NOT type into the blurred editor.
    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("custom"),
        "draft survived the retreat: {}",
        screen(&term)
    );
    term.push_input(b"q");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains("customq"),
        "blurred editor must not keep typing: {}",
        screen(&term)
    );

    // Enter after retreat: engagement held, so the draft commits.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&[], Some("custom"))]);

    // A fresh instance starts with an empty draft (G6 state death).
    let log2: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type")
        .option("a", "Alpha")
        .allow_other("Something else…")
        .on_resolve(recorder(&log2))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains("custom"),
        "fresh gate starts with an empty draft: {}",
        screen(&term)
    );
}

/// Charter K4 (the conceded layered-Esc position, now pinned from the
/// review lane): first Esc while editing RETREATS (draft kept, hint
/// told the truth while editing); the second Esc cancels on a
/// dismissable gate and refuses visibly on a must-choose gate.
#[test]
fn charter_k4_layered_esc_retreats_then_cancels_or_refuses() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    // Dismissable: retreat, then cancel.
    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type")
        .option("a", "Alpha")
        .allow_other("Other…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"half");
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("Esc back to the list"),
        "editing hint tells the layered truth: {}",
        screen(&term)
    );
    term.push_input(b"\x1b[27u"); // retreat
    settle(&mut driver, &mut app, &mut term);
    assert!(log.borrow().is_empty(), "first Esc retreats, never cancels");
    assert!(screen(&term).contains("half"), "draft kept on retreat");
    term.push_input(b"\x1b[27u"); // cancel
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [ChoiceOutcome::Cancelled]);

    // Must-choose: retreat, then visible refusal — never a resolution.
    let log2: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick or type (required)")
        .option("a", "Alpha")
        .allow_other("Other…")
        .dismissable(false)
        .on_resolve(recorder(&log2))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[F");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"wip");
    settle(&mut driver, &mut app, &mut term);
    term.push_input(b"\x1b[27u\x1b[27u"); // retreat, then refused escape
    settle(&mut driver, &mut app, &mut term);
    assert!(log2.borrow().is_empty(), "must-choose never cancels on Esc");
    let s = screen(&term);
    assert!(s.contains("an answer is required"), "visible refusal: {s}");
    assert!(s.contains("wip"), "draft still intact after both Escs: {s}");
}

/// Charter O4: committing an engaged-but-empty Other REFUSES visibly
/// (whitespace-only included); text then commits as the Other answer.
///
/// Geometry note (cycle-3 activation): the refusal note lives in the
/// hint row and truncates with a VISIBLE ellipsis on panels narrower
/// than the sentence (interact.rs `truncate_ellipsis` — the same
/// honesty law the S-clauses pin). The prompt below widens the
/// content-driven panel so the FULL note fits and the clause is
/// pinned at full strength.
#[test]
fn charter_o4_empty_other_commit_refuses_visibly() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick an option below or type your own answer")
        .option("a", "Alpha")
        .allow_other("Other…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[F\r"); // engage + hollow commit
    settle(&mut driver, &mut app, &mut term);
    assert!(log.borrow().is_empty(), "hollow Other must not resolve");
    assert!(
        screen(&term).contains("Other… needs text — type your answer"),
        "refusal is visible: {}",
        screen(&term)
    );

    term.push_input(b" \r"); // whitespace-only is still hollow (trim)
    settle(&mut driver, &mut app, &mut term);
    assert!(log.borrow().is_empty(), "whitespace-only must not resolve");

    term.push_input(b"ok\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&[], Some("ok"))]);
}

// ===========================================================================
// A — accessibility (the publicly reachable half; see header + F10)
// ===========================================================================

/// Charter A2/A3, tree half: BLOCKED from the public integration
/// surface — `Overlays` exposes no path to an overlay layer's UiTree
/// (only the creating `LayerHandle::tree()`, which `ChoicePrompt`
/// owns privately). Finding F10 in review-cycle3-verdict.md. The tree
/// half is pinned by BUILDER's in-crate unit tests (named in the file
/// header) which run in this tree's gate; the roles/labels/values were
/// source-verified this cycle (choice_prompt_parts.rs:270-288 rows,
/// choice_prompt_view.rs:200-201 heading, :326-328 menu region,
/// :460-472 input).
#[test]
#[ignore = "blocked: no public path to an overlay layer's a11y tree (F10) — tree half pinned by builder unit tests + source citations; observable half active in o1/k3/t2"]
fn charter_a3_roles_are_honest_and_options_enumerable() {
    panic!(
        "F10: Overlays lacks a public accessor for an overlay layer's UiTree; \
         activate when the engine exposes one (verdict: cycle-2 follow-up)"
    );
}

// ===========================================================================
// K — keyboard-first
// ===========================================================================

/// Charter K1: an open gate OWNS the keyboard (host shortcuts starve);
/// after resolution the host hears keys again.
#[test]
fn charter_k1_open_gate_owns_the_keyboard() {
    let (mut app, scope, leaked) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Proceed?")
        .option("y", "Yes")
        .option("n", "No")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"x");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*leaked.borrow(), 0, "'x' swallowed while the gate is open");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().len(), 1);

    term.push_input(b"x");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*leaked.borrow(), 1, "host hears keys after the gate closes");
}

/// Charter K2: the hint row names the ACTUAL keys, truthfully per
/// mode: declared letters, Space in multiple mode, Enter always, Esc
/// only when it works. (The editing-state hint truth is pinned in
/// charter_k4.) Also exercises `handle.cancel()` + `is_open` — the
/// programmatic ending resolves through the same exactly-once path.
#[test]
fn charter_k2_hint_names_the_keys_truthfully() {
    let size = Size::new(64, H);
    let (mut app, scope, _) = host(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    // Single + letters + dismissable: all three verb segments.
    let log: OutcomeLog = Rc::default();
    let handle = ChoicePrompt::new("Approve the command?")
        .option_key("approve", "Approve", 'a')
        .option_key("all", "Approve all", 'A')
        .option_key("deny", "Deny", 'd')
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("a/A/d pick"), "letters named: {s}");
    assert!(s.contains("Enter confirms"), "{s}");
    assert!(s.contains("Esc cancels"), "{s}");
    assert!(handle.is_open());
    handle.cancel();
    settle(&mut driver, &mut app, &mut term);
    assert!(!handle.is_open());
    assert_eq!(log.borrow().as_slice(), [ChoiceOutcome::Cancelled]);

    // Multiple mode: Space is named.
    let log2: OutcomeLog = Rc::default();
    let h2 = ChoicePrompt::new("Enable which tools?")
        .option("read", "Read")
        .option("write", "Write")
        .allow_multiple(true)
        .on_resolve(recorder(&log2))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Space toggles"), "{}", screen(&term));
    h2.cancel();
    settle(&mut driver, &mut app, &mut term);

    // Must-choose: Esc is NOT advertised (a dead key would lie).
    let log3: OutcomeLog = Rc::default();
    let h3 = ChoicePrompt::new("Required choice")
        .option("a", "Alpha")
        .option("b", "Beta")
        .dismissable(false)
        .on_resolve(recorder(&log3))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Enter confirms"), "{s}");
    assert!(!s.contains("Esc cancels"), "no dead-key advertising: {s}");
    h3.cancel();
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log3.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
}

/// Charter K3 + the fabricated-selection law: the initial selection is
/// VISIBLY rendered from frame one (Enter can never commit an
/// invisible choice), and the very first key moves it (no
/// dead-keys-until-Tab — the 0230 class).
#[test]
fn charter_k3_first_key_moves_and_initial_selection_is_visible() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Pick a model")
        .option("alpha", "Alpha")
        .option("beta", "Beta")
        .option("gamma", "Gamma")
        .initial("beta")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("● Beta"),
        "initial selection visible before any key: {}",
        screen(&term)
    );

    term.push_input(b"\x1b[B"); // the VERY first key
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("● Gamma"),
        "first key moved the selection: {}",
        screen(&term)
    );

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["gamma"], None)]);
}

// ===========================================================================
// S — honesty at scale
// ===========================================================================

/// Charter S1: 30 options in a short panel — the list windows around
/// the highlight, every option is reachable, the position note tells
/// the truth, and the last option can resolve.
#[test]
fn charter_s1_thirty_options_reachable_windowed() {
    let (mut app, scope, _) = host(Size::new(W, 14));
    let mut term = CaptureTerm::new(Size::new(W, 14));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    let mut prompt = ChoicePrompt::new("Pick one of thirty");
    for i in 1..=30 {
        prompt = prompt.option(format!("id-{i:02}"), format!("option {i:02}"));
    }
    prompt.on_resolve(recorder(&log)).open(scope);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("option 01"), "window starts at the top: {s}");
    assert!(!s.contains("option 30"), "the tail is off-window: {s}");
    assert!(s.contains("1/30"), "position note: {s}");

    term.push_input(&b"\x1b[B".repeat(29));
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● option 30"), "last option reached: {s}");
    assert!(s.contains("30/30"), "position note follows: {s}");
    assert!(!s.contains("option 01"), "window slid off the head: {s}");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["id-30"], None)]);
}

/// Charter S2: a long prompt WRAPS at panel width — every word stays
/// readable, no silent edge clipping.
#[test]
fn charter_s2_long_prompt_wraps_every_word_visible() {
    let (mut app, scope, _) = host(Size::new(44, H));
    let mut term = CaptureTerm::new(Size::new(44, H));
    let mut driver = boot(&mut app, &mut term);

    let prompt = "Which deployment strategy should the release train adopt for this cycle?";
    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new(prompt)
        .option("blue", "Blue-green")
        .option("canary", "Canary")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    let s = screen(&term);
    for word in prompt.split_whitespace() {
        let word = word.trim_end_matches('?');
        assert!(s.contains(word), "word {word:?} lost to clipping:\n{s}");
    }
}

/// Charter S3 (the 0240 floor): under option overflow in a SHORT
/// viewport, the fixed rows — buttons, hint — survive and the gate
/// stays operable. Two geometries: a panel wide enough for the whole
/// hint (every fixed row fully legible), then a narrow panel where
/// the hint degrades by WHOLE segments (the builder-notes caveat) —
/// the tail survives intact, never a mid-word cut.
#[test]
fn charter_s3_fixed_rows_survive_option_overflow() {
    let (mut app, scope, _) = host(Size::new(W, 10));
    let mut term = CaptureTerm::new(Size::new(W, 10));
    let mut driver = boot(&mut app, &mut term);

    // Wide panel (the prompt drives the content-measured width): all
    // fixed rows fully legible under 30-option vertical overflow.
    let log: OutcomeLog = Rc::default();
    let mut prompt = ChoicePrompt::new("Pick exactly one of the thirty options");
    for i in 1..=30 {
        prompt = prompt.option(format!("id-{i:02}"), format!("option {i:02}"));
    }
    prompt.on_resolve(recorder(&log)).open(scope);
    settle(&mut driver, &mut app, &mut term);

    let s = screen(&term);
    assert!(s.contains("Cancel"), "Cancel button survives overflow: {s}");
    assert!(s.contains("Enter confirms"), "hint row survives: {s}");
    assert!(s.contains("Esc cancels"), "hint tail present: {s}");
    assert!(s.contains("1/30"), "position note present: {s}");
    assert!(s.contains("option 01"), "options render in the window: {s}");

    term.push_input(b"\r"); // still operable under pressure
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["id-01"], None)]);

    // Narrow panel: the hint row SURVIVES (the 0240 floor) and
    // degrades by whole segments from the front — "Esc cancels"
    // intact, no "Esc ca…" mid-word cut, position note still there.
    let log2: OutcomeLog = Rc::default();
    let mut prompt = ChoicePrompt::new("Pick one of thirty");
    for i in 1..=30 {
        prompt = prompt.option(format!("id-{i:02}"), format!("option {i:02}"));
    }
    prompt.on_resolve(recorder(&log2)).open(scope);
    settle(&mut driver, &mut app, &mut term);

    let s = screen(&term);
    assert!(s.contains("Cancel"), "Cancel button survives: {s}");
    assert!(
        s.contains("Esc cancels") && !s.contains("Esc ca…"),
        "hint degrades by whole segments, tail intact: {s}"
    );
    assert!(s.contains("1/30"), "position note survives: {s}");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log2.borrow().as_slice(), [answered(&["id-01"], None)]);
}

// ===========================================================================
// T — theming
// ===========================================================================

/// Charter T2: the selected option is visibly distinguishable from an
/// unselected one in EVERY registered theme. Both options carry the
/// same label; any cell difference on the two rows is selection
/// affordance. The marker glyph must additionally be VISIBLE ink
/// (fg != bg) in every theme.
#[test]
fn charter_t2_selection_state_visible_in_every_registered_theme() {
    let mut failing: Vec<String> = Vec::new();
    for th in abstracttui::theme::themes() {
        abstracttui::app::set_theme_by_id(th.id);
        let (mut app, scope, _) = host(Size::new(W, H));
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut driver = boot(&mut app, &mut term);
        let log: OutcomeLog = Rc::default();
        ChoicePrompt::new("Pick")
            .option("c1", "choice")
            .option("c2", "choice")
            .on_resolve(recorder(&log))
            .open(scope);
        settle(&mut driver, &mut app, &mut term);

        let lines: Vec<String> = screen(&term).lines().map(str::to_string).collect();
        let rows: Vec<i32> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("choice"))
            .map(|(i, _)| i as i32)
            .collect();
        assert_eq!(rows.len(), 2, "both rows render ({}): {lines:?}", th.id);
        let vt = term.screen();
        let differs = (0..W).any(|x| {
            let a = vt
                .cell(x, rows[0])
                .map(|c| (c.display().to_string(), c.paint));
            let b = vt
                .cell(x, rows[1])
                .map(|c| (c.display().to_string(), c.paint));
            a != b
        });
        // Marker visibility: the ● cell must be readable ink.
        let marker_visible = lines[rows[0] as usize]
            .find('●')
            .map(|byte| {
                let x = lines[rows[0] as usize][..byte].chars().count() as i32;
                let cell = vt.cell(x, rows[0]).expect("marker cell");
                cell.paint.fg != cell.paint.bg
            })
            .unwrap_or(false);
        if !differs || !marker_visible {
            failing.push(th.id.to_string());
        }
    }
    assert!(
        failing.is_empty(),
        "selection indistinguishable (or marker invisible) in: {failing:?} (charter T2)"
    );
}

/// Charter T3 + the F7 exception's honesty, in one dark and one light
/// theme: a danger option wears `Error` ink while unhighlighted; under
/// the highlight WITH list focus it wears the audited selection pair
/// (error-on-selection-ground is not an audited combination — the
/// on-record exception), never a half-and-half.
#[test]
fn charter_t3_danger_ink_and_selection_ground_exception_in_two_themes() {
    let dark = abstracttui::theme::themes()
        .iter()
        .find(|t| t.dark)
        .expect("a dark theme");
    let light = abstracttui::theme::themes()
        .iter()
        .find(|t| !t.dark)
        .expect("a light theme");
    for th in [dark, light] {
        abstracttui::app::set_theme_by_id(th.id);
        let tokens = abstracttui::app::current_theme().tokens;
        let error = tokens.get(TokenId::Error);
        let sel_fg = tokens.get(TokenId::SelectionFg);
        let sel_bg = tokens.get(TokenId::SelectionBg);

        let (mut app, scope, _) = host(Size::new(W, H));
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut driver = boot(&mut app, &mut term);
        let log: OutcomeLog = Rc::default();
        // Prompt deliberately avoids the word "Delete": `locate` scans
        // top-down and a heading containing the needle would shadow
        // the danger ROW (cycle-3 activation fix).
        ChoicePrompt::new("Remove the branch?")
            .option("keep", "Keep")
            .option_with(ChoiceOption::new("delete", "Delete").danger(true))
            .on_resolve(recorder(&log))
            .open(scope);
        settle(&mut driver, &mut app, &mut term);

        // Unhighlighted danger row: Error ink on the label.
        let (x, y) = locate(&term, "Delete").expect("danger row rendered");
        let cell = term.screen().cell(x, y).expect("label cell");
        assert_eq!(
            cell.paint.fg,
            Some(error),
            "danger label wears Error ink in {}: {:?}",
            th.id,
            cell.paint
        );

        // Highlighted + focused: the audited selection pair, not error
        // ink on the selection ground (the on-record exception).
        term.push_input(b"\x1b[B");
        settle(&mut driver, &mut app, &mut term);
        let (x, y) = locate(&term, "Delete").expect("danger row still rendered");
        let cell = term.screen().cell(x, y).expect("label cell");
        assert_eq!(
            cell.paint.fg,
            Some(sel_fg),
            "highlighted danger label wears the selection pair fg in {}",
            th.id
        );
        assert_eq!(
            cell.paint.bg,
            Some(sel_bg),
            "highlighted danger label wears the selection pair bg in {}",
            th.id
        );
        assert_ne!(
            cell.paint.fg,
            Some(error),
            "no error-on-selection-ground in {}",
            th.id
        );
    }
}

// ===========================================================================
// P — damage contract
// ===========================================================================

/// Charter P1: an open, untouched gate costs nothing — turns are idle
/// and write zero bytes. P2 tail (SHOULD, relative pin): one arrow
/// move repaints locally — strictly fewer bytes than the gate's own
/// opening paint, never a world repaint.
#[test]
fn charter_p1_open_untouched_gate_is_idle_zero_bytes() {
    let (mut app, scope, _) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);
    let _ = term.take_bytes();

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Waiting on you")
        .option("a", "Yes")
        .option("b", "No")
        .allow_other("Other…")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let open_paint = term.take_bytes().len();

    for _ in 0..3 {
        let turn = driver.turn(&mut app, &mut term).expect("turn");
        assert!(turn.idle, "untouched gate must idle (charter P1)");
    }
    assert!(
        term.bytes().is_empty(),
        "idle gate wrote {} bytes (charter P1)",
        term.bytes().len()
    );

    // P2: an arrow move's damage is local (relative bound — no magic
    // numbers: moving the highlight must cost less than painting the
    // whole gate did).
    term.push_input(b"\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    let move_paint = term.take_bytes().len();
    assert!(move_paint > 0, "the move repainted something");
    assert!(
        move_paint < open_paint,
        "arrow-move damage should be local: move {move_paint}B vs open {open_paint}B (charter P2)"
    );
}

// ===========================================================================
// The consumer dry-run — the maintainer's "gate a decision" brief run
// end-to-end in the first consumer's shape (abstractcode-tui tool
// approval): three lettered options with a danger Deny + detail,
// must-choose, resolve chains into the "are you sure" gate, full
// keyboard wire, exactly-once at both gates. Stage evidence is
// screen-level; the modal-tree a11y half rides builder unit tests
// (F10 — see header).
// ===========================================================================

#[test]
fn consumer_dry_run_tool_approval_gate_chains_into_confirm() {
    let size = Size::new(64, 18);
    let (mut app, scope, leaked) = host(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let approval: OutcomeLog = Rc::default();
    let confirm: OutcomeLog = Rc::default();
    let approval_sink = approval.clone();
    let confirm_sink = confirm.clone();
    ChoicePrompt::new("Run `rm -rf build/` in the workspace?")
        .option_key("approve", "Approve", 'a')
        .option_key("approve_all", "Approve all this session", 'A')
        .option_with(
            ChoiceOption::new("deny", "Deny")
                .key('d')
                .danger(true)
                .detail("the agent's request is refused"),
        )
        .dismissable(false) // an approval must be decided
        .on_resolve(move |o| {
            approval_sink.borrow_mut().push(o.clone());
            if o == answered(&["deny"], None) {
                // The consumer's chained "are you sure" stage, opened
                // from INSIDE the resolve callback.
                ChoicePrompt::new("Deny and tell the agent why not?")
                    .option_key("confirm", "Yes, deny it", 'y')
                    .option("back", "No, go back")
                    .on_resolve(move |o| confirm_sink.borrow_mut().push(o))
                    .open(scope);
            }
        })
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    // Stage 1 evidence: the approval gate, fully described on screen.
    let s = screen(&term);
    assert!(s.contains("Run `rm -rf build/` in the workspace?"), "{s}");
    assert!(s.contains("Approve (a)"), "lettered option rendered: {s}");
    assert!(s.contains("Approve all this session (A)"), "{s}");
    assert!(s.contains("Deny (d)"), "{s}");
    assert!(
        s.contains("the agent's request is refused"),
        "deny detail visible: {s}"
    );
    assert!(s.contains("a/A/d pick"), "letters in the hint: {s}");
    assert!(!s.contains("Esc cancels"), "must-choose hides Esc: {s}");
    assert!(!s.contains("Cancel"), "must-choose has no Cancel: {s}");

    // The gate owns the keyboard; Esc is refused VISIBLY, resolving
    // nothing (the approval consumer maps dismissal itself — a silent
    // or accidental close must be impossible).
    term.push_input(b"x");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*leaked.borrow(), 0, "host starves while the gate decides");
    term.push_input(b"\x1b[27u");
    settle(&mut driver, &mut app, &mut term);
    assert!(approval.borrow().is_empty());
    assert!(screen(&term).contains("an answer is required"));

    // 'd' — the consumer's muscle memory — commits Deny exactly once
    // and the confirm stage opens from inside the resolve.
    term.push_input(b"d");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(approval.borrow().as_slice(), [answered(&["deny"], None)]);
    let s = screen(&term);
    assert!(
        s.contains("Deny and tell the agent why not?"),
        "confirm stage rendered: {s}"
    );
    assert!(s.contains("● Yes, deny it (y)"), "fresh initial state: {s}");
    assert!(s.contains("Esc cancels"), "stage 2 is dismissable: {s}");

    // Hostile double-path at stage 2: letter + Enter in ONE batch —
    // exactly one outcome, the trailing Enter dies against the host.
    term.push_input(b"y\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(confirm.borrow().as_slice(), [answered(&["confirm"], None)]);
    assert_eq!(approval.borrow().len(), 1, "stage 1 stayed resolved-once");

    // Both gates gone; the app is back.
    let s = screen(&term);
    assert!(!s.contains("rm -rf") && !s.contains("Deny and tell"), "{s}");
    assert!(s.contains("host app row"), "host repainted: {s}");
    term.push_input(b"x");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*leaked.borrow(), 1, "keyboard returned to the host");
}
