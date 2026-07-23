//! Field-fix wave for the first app's ChoicePrompt approval-adoption
//! filing (backlog first-app 0271): the body-width knob, the dismiss
//! vocabulary, and host retire. Wire BYTES in through `CaptureTerm`,
//! modeled VT screen out, through the REAL `Driver` (the same harness
//! posture as wave_choice_fix.rs — helper duplication across
//! integration files is the house style).
//!
//! The 0271 defects in one line each: the panel width was
//! content-derived from options/prompt/hint/buttons while the BODY was
//! invisible to `measure` (a 72-col card body clipped inside a ~45-col
//! panel sized by three short options); `dismissable(true)` hardcoded
//! "Cancel"/"Esc cancels" on a surface whose Esc DEFERS; and a host
//! retiring the gate fired the same `Cancelled` as the user's Esc.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

const W: i32 = 80;
const H: i32 = 24;

/// Kitty CSI u encoding of Escape (code 27) — the unambiguous wire
/// spelling (a bare `\x1b` byte is a decoder ambiguity, the
/// builder-notes caveat).
const KITTY_ESC: &[u8] = b"\x1b[27u";

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

/// Host app: a focusable root. Returns (app, mount scope).
fn host(size: Size) -> (App, Scope) {
    let mut app = App::new(size);
    let scope_slot: Rc<RefCell<Option<Scope>>> = Rc::default();
    let ss = scope_slot.clone();
    app.mount(move |cx| {
        *ss.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .focusable()
            .autofocus()
            .child(text("host app row"))
            .build()
    })
    .expect("mount");
    let scope = scope_slot.borrow().expect("scope");
    (app, scope)
}

fn boot(app: &mut App, term: &mut CaptureTerm) -> Driver {
    let mut driver = Driver::new(app, term, config()).expect("driver");
    settle(&mut driver, app, term);
    driver
}

/// The approval consumer's option shape: three short options that size
/// the panel to ~45 cols on their own.
fn approval(p: ChoicePrompt) -> ChoicePrompt {
    p.option_key("approve", "Approve", 'a')
        .option_key("all", "Approve all (session)", 'A')
        .option_key("deny", "Deny", 'd')
}

/// A 72-col card row of the shape the consumer pre-wraps: an aligned
/// `$ command` line padded to the card width.
fn card_line() -> String {
    let cmd = "$ cargo build --release --features kitty,sixel --timings";
    format!("{cmd}{}", " .".repeat((72 - cmd.chars().count()) / 2))
}

// ===========================================================================
// 0271 ask 1 — body_width: the body participates in the panel's measure
// ===========================================================================

/// THE blocker repro, both arms: without the knob the 72-col card row
/// clips inside the options-sized panel; `body_width(72)` widens the
/// panel and the row renders whole — and the gate's whole vocabulary
/// (letters) still works through the widened panel.
#[test]
fn body_width_lets_a_72_col_card_body_render_unclipped() {
    let line = card_line();
    assert_eq!(line.chars().count(), 72, "test premise: a 72-col card row");

    // Arm 1 (premise): no knob — the row cannot fit.
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);
    let log: OutcomeLog = Rc::default();
    let body_line = line.clone();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .body(move |_| text(body_line.clone()))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        !screen(&term).contains(line.as_str()),
        "premise: the 72-col row clips without body_width: {}",
        screen(&term)
    );

    // Arm 2 (the fix): the declared width folds into measure.
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);
    let log: OutcomeLog = Rc::default();
    let body_line = line.clone();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .body(move |_| text(body_line.clone()))
        .body_width(72)
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(
        s.contains(line.as_str()),
        "the 72-col card row renders UNCLIPPED with body_width(72): {s}"
    );
    assert!(
        s.contains("● Approve"),
        "options render inside the widened panel: {s}"
    );

    term.push_input(b"a");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        [answered(&["approve"], None)],
        "letters still commit through the widened gate"
    );
}

// ===========================================================================
// 0271 ask 3a — dismiss_label: the rendered contract tells the truth
// ===========================================================================

/// The approval surface's Esc DEFERS: `dismiss_label("Defer")` renames
/// the button AND the hint; the wire outcome stays `Cancelled` — the
/// consumer maps it to its defer lane.
#[test]
fn dismiss_label_defer_renders_and_esc_still_resolves_cancelled() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .dismiss_label("Defer")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Defer"), "the button says Defer: {s}");
    assert!(s.contains("Esc Defer"), "the hint says Esc Defer: {s}");
    assert!(
        !s.contains("Cancel"),
        "no mislabeled consent affordance: {s}"
    );

    term.push_input(KITTY_ESC);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        [ChoiceOutcome::Cancelled],
        "the outcome is unchanged — the label names the caller's wiring"
    );
    assert!(
        !screen(&term).contains("Run 3 tool calls?"),
        "the gate closed on Esc"
    );
}

// ===========================================================================
// 0271 ask 3b — handle.retire(): host-owned close, no outcome
// ===========================================================================

/// A host replacing the prompt (picker-replace, tier-raise auto-close)
/// retires it: the modal leaves the screen, `on_resolve` NEVER fires —
/// distinct by construction from the user's Esc — and a fresh gate
/// opens normally afterwards (the consumer's reopen invariant).
#[test]
fn host_retire_fires_no_outcome_and_the_gate_reopens_cleanly() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    let handle = approval(ChoicePrompt::new("Run 3 tool calls?"))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Run 3 tool calls?"));

    handle.retire();
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(!s.contains("Run 3 tool calls?"), "the gate left the screen");
    assert!(s.contains("host app row"), "the host is back: {s}");
    assert!(
        log.borrow().is_empty(),
        "retire fired NO outcome — the host owns it"
    );
    assert!(!handle.is_open());

    // Stray input + late endings after the retire stay inert.
    term.push_input(b"a");
    term.push_input(KITTY_ESC);
    handle.cancel();
    settle(&mut driver, &mut app, &mut term);
    assert!(log.borrow().is_empty(), "post-retire endings never resolve");

    // The reopen invariant: a replaced prompt must be able to come
    // back — a fresh gate opens and answers normally.
    let log: OutcomeLog = Rc::default();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Run 3 tool calls?"), "reopened");
    term.push_input(b"d");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["deny"], None)]);
}
