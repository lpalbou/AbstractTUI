//! Field-fix wave for the first app's ChoicePrompt filings (backlog
//! first-app 0286/0287/0288): the shifted-letter wire-spelling split
//! and the body slot. Wire BYTES in through `CaptureTerm`, modeled VT
//! screen out, through the REAL `Driver` (the same harness posture as
//! wave_choice_review.rs — helper duplication across integration
//! files is the house style).
//!
//! The 0288/0286 defect in one line: a shifted letter has TWO wire
//! spellings — legacy Shift+A arrives as byte `A` (`Char('A')`, no
//! mods; shift baked into the char) while the kitty keyboard protocol
//! sends `CSI 97;2u` (`Char('a')` + SHIFT — the key identity is
//! deliberately the base key) — and every matcher compared exactly
//! one spelling, so the OTHER wire's users pressed a dead key.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

const W: i32 = 56;
const H: i32 = 18;

/// Kitty CSI u encoding of Shift+A: code 97 ('a', base identity),
/// mods 2 = 1 + SHIFT(1). This is the exact byte sequence of the
/// first app's live P0 (its regression test pushes the same bytes).
const KITTY_SHIFT_A: &[u8] = b"\x1b[97;2u";

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

/// The 0.2.8 announcement's approval vocabulary: `a`/`A`/`d` — the
/// exact gate shape 0288 reports dead on kitty wires.
fn approval(p: ChoicePrompt) -> ChoicePrompt {
    p.option_key("approve", "Approve", 'a')
        .option_key("all", "Approve all", 'A')
        .option_key("deny", "Deny", 'd')
}

// ===========================================================================
// 0288 — option_key letters must fire on BOTH wire spellings
// ===========================================================================

/// THE repro (0288): `option_key(…, 'A')` on the kitty wire. Shift+A
/// arrives as `CSI 97;2u` = `Char('a')` + SHIFT; the gate must read it
/// as the uppercase key and commit "all" — exactly once.
#[test]
fn kitty_shift_a_fires_the_uppercase_option_key_and_commits() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(screen(&term).contains("Run 3 tool calls?"));

    term.push_input(KITTY_SHIFT_A);
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(
        log.borrow().as_slice(),
        [answered(&["all"], None)],
        "kitty-wire Shift+A must commit the 'A' option (0288)"
    );
    assert!(
        !screen(&term).contains("Run 3 tool calls?"),
        "the gate closed on the letter commit"
    );
}

/// The legacy spelling of the same key: byte `A` = `Char('A')`, no
/// mods. Pinned so the fold never trades one wire for the other.
#[test]
fn legacy_shift_a_fires_the_uppercase_option_key_and_commits() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    approval(ChoicePrompt::new("Run 3 tool calls?"))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"A");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["all"], None)]);
}

/// Multiple mode: the kitty spelling jump-toggles the uppercase key's
/// mark (never resolves), and Enter then commits the set.
#[test]
fn kitty_shift_a_jump_toggles_in_multiple_mode() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    approval(ChoicePrompt::new("Which batches?"))
        .allow_multiple(true)
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(KITTY_SHIFT_A);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("☑ Approve all"),
        "kitty Shift+A toggles the 'A' mark: {}",
        screen(&term)
    );
    assert!(log.borrow().is_empty(), "toggles never resolve");

    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["all"], None)]);
}

/// The other half of the guarantee: a LOWERCASE-declared key must NOT
/// fire on Shift+letter — on either wire, in either mode. Shift+K
/// means 'K', and 'K' is not declared.
#[test]
fn lowercase_declared_key_refuses_shift_letter_on_both_wires() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Keep the copies?")
        .option_key("keep", "Keep my copies", 'k')
        .option("drop", "Drop them")
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    term.push_input(b"\x1b[107;2u"); // kitty Shift+K ('k' = 107, mods 2)
    settle(&mut driver, &mut app, &mut term);
    assert!(
        log.borrow().is_empty(),
        "kitty Shift+K means 'K' — the lowercase 'k' key must not fire"
    );

    term.push_input(b"K"); // legacy Shift+K
    settle(&mut driver, &mut app, &mut term);
    assert!(
        log.borrow().is_empty(),
        "legacy Shift+K means 'K' — the lowercase 'k' key must not fire"
    );

    term.push_input(b"k"); // the declared key itself still commits
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["keep"], None)]);
}

/// The decide example's gate-1 vocabulary (`o`/`k`/`d`, lowercase) on
/// both wires: legacy bytes and the kitty PLAIN spelling (`CSI code u`,
/// no mods) — every declared key commits its own option.
#[test]
fn decide_example_gate1_keys_fire_on_both_wires() {
    let gate = |status: &OutcomeLog, scope: Scope| {
        ChoicePrompt::new("Overwrite 3 locally modified files?")
            .option_with(
                ChoiceOption::new("overwrite", "Overwrite them")
                    .detail("the local edits are lost")
                    .key('o')
                    .danger(true),
            )
            .option_with(
                ChoiceOption::new("keep", "Keep my copies")
                    .detail("the sync is skipped")
                    .key('k'),
            )
            .option_key("diff", "Show me the diff first", 'd')
            .allow_other("Something else…")
            .initial("keep")
            .dismissable(false)
            .on_resolve(recorder(status))
            .open(scope);
    };
    let cases: [(&[u8], &str); 6] = [
        (b"o", "overwrite"),         // legacy
        (b"\x1b[111u", "overwrite"), // kitty plain 'o'
        (b"k", "keep"),              // legacy
        (b"\x1b[107u", "keep"),      // kitty plain 'k'
        (b"d", "diff"),              // legacy
        (b"\x1b[100u", "diff"),      // kitty plain 'd'
    ];
    for (bytes, expect) in cases {
        let (mut app, scope) = host(Size::new(W, H));
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut driver = boot(&mut app, &mut term);
        let log: OutcomeLog = Rc::default();
        gate(&log, scope);
        settle(&mut driver, &mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(
            log.borrow().as_slice(),
            [answered(&[expect], None)],
            "key bytes {bytes:?} must commit {expect:?}"
        );
    }
}

// ===========================================================================
// 0286 — KeyChord shortcut matching folds the same two spellings
// ===========================================================================

/// Tree shortcuts, both directions of the fold: the natural
/// registration `plain(Char('A'))` fires on the kitty spelling, and
/// the kitty-shaped registration `SHIFT + Char('a')` fires on the
/// legacy spelling. One registration, both wires.
#[test]
fn tree_shortcut_shifted_letter_matches_both_wire_spellings() {
    for (chord, bytes) in [
        (KeyChord::plain(Key::Char('A')), KITTY_SHIFT_A),
        (KeyChord::new(Mods::SHIFT, Key::Char('a')), b"A".as_slice()),
    ] {
        let fired: Rc<RefCell<u32>> = Rc::default();
        let f = fired.clone();
        let mut app = App::new(Size::new(W, H));
        app.mount(move |_cx| {
            let f2 = f.clone();
            Element::new()
                .style(LayoutStyle::column())
                .focusable()
                .autofocus()
                .shortcut(chord, move |_| *f2.borrow_mut() += 1)
                .child(text("host"))
                .build()
        })
        .expect("mount");
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut driver = boot(&mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(
            *fired.borrow(),
            1,
            "chord {chord:?} must fire on wire bytes {bytes:?} (0286)"
        );
    }
}

/// Global actions ride the same fold: `plain(Char('A'))` registered in
/// the app keymap fires when the kitty spelling reaches the driver's
/// action dispatch (nothing in the UI consumed it).
#[test]
fn action_chord_shifted_letter_matches_both_wire_spellings() {
    for (chord, bytes) in [
        (KeyChord::plain(Key::Char('A')), KITTY_SHIFT_A),
        (KeyChord::new(Mods::SHIFT, Key::Char('a')), b"A".as_slice()),
    ] {
        let (mut app, _scope) = host(Size::new(W, H));
        let fired: Rc<RefCell<u32>> = Rc::default();
        let f = fired.clone();
        assert!(app
            .actions()
            .register("approve.all", Some(chord), move || *f.borrow_mut() += 1));
        let mut term = CaptureTerm::new(Size::new(W, H));
        let mut driver = boot(&mut app, &mut term);
        term.push_input(bytes);
        settle(&mut driver, &mut app, &mut term);
        assert_eq!(
            *fired.borrow(),
            1,
            "action {chord:?} must fire on wire bytes {bytes:?} (0286)"
        );
    }
}

// ===========================================================================
// 0287 — the body slot: structure between the prompt and the options
// ===========================================================================

/// The reactive gap 0287 names (the tier-honesty line): a `dyn_view`
/// inside the body re-renders while the gate is up when a
/// caller-owned signal changes — no cancel + reopen.
#[test]
fn body_dyn_view_updates_reactively_while_the_gate_is_up() {
    let (mut app, scope) = host(Size::new(W, H));
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);

    let tier = scope.signal(String::from("tier: safe commands only"));
    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Approve this batch?")
        .option_key("ok", "Approve", 'a')
        .option("deny", "Deny")
        .body(move |_| dyn_view(LayoutStyle::line(1), move || text(tier.get())))
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("tier: safe commands only"),
        "the body renders between prompt and options: {}",
        screen(&term)
    );

    tier.set(String::from("tier: ALL commands"));
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("tier: ALL commands"),
        "the body re-renders live inside the open gate (0287): {}",
        screen(&term)
    );
    assert!(log.borrow().is_empty(), "a body update never resolves");

    term.push_input(b"a");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["ok"], None)]);
}

/// The scroll-routing contract: a `Scroll`-wrapped body scrolls under
/// the WHEEL while the pointer is over it (the Scroll consumes the
/// event before the gate's highlight handler); the wheel elsewhere and
/// the arrows/letters stay the OPTIONS' vocabulary.
#[test]
fn scrollable_body_composes_with_twenty_options() {
    let size = Size::new(W, 24);
    let (mut app, scope) = host(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    let mut gate = ChoicePrompt::new("Run this batch?").body(|mcx| {
        let mut col = Element::new().style(LayoutStyle::column().width(Dimension::Percent(1.0)));
        for i in 1..=30 {
            col = col.child(text(format!("call {i:02} write_file")));
        }
        Scroll::new(col.build()).content_size(40, 30).view(mcx)
    });
    gate = gate.body_rows(6).option_key("go", "Opt 01", 'g');
    for i in 2..=20 {
        gate = gate.option(format!("opt{i}"), format!("Opt {i:02}"));
    }
    gate.on_resolve(recorder(&log)).open(scope);
    settle(&mut driver, &mut app, &mut term);

    let s = screen(&term);
    assert!(s.contains("call 01"), "body top visible: {s}");
    assert!(!s.contains("call 09"), "body clipped to its budget: {s}");
    assert!(s.contains("● Opt 01"), "options render below the body: {s}");

    // Wheel over the BODY: the Scroll consumes it (3-line notch), the
    // highlight does not move.
    let (bx, by) = locate(&term, "call 01").expect("body row on screen");
    term.push_input(format!("\x1b[<65;{};{}M", bx + 1, by + 1).as_bytes());
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(
        s.contains("call 04") && !s.contains("call 01"),
        "wheel over the body scrolls the body: {s}"
    );
    assert!(
        s.contains("● Opt 01"),
        "wheel over the body must not move the highlight: {s}"
    );

    // Arrows stay with the options (keys route focused-first — the
    // body's Scroll never sees them) and the body holds its offset.
    term.push_input(b"\x1b[B");
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("● Opt 02"), "arrow moves the highlight: {s}");
    assert!(s.contains("call 04"), "arrow does not scroll the body: {s}");

    // Wheel over the OPTIONS still moves the highlight (existing gate
    // vocabulary, untouched by the body).
    let (ox, oy) = locate(&term, "Opt 03").expect("option row on screen");
    term.push_input(format!("\x1b[<65;{};{}M", ox + 1, oy + 1).as_bytes());
    settle(&mut driver, &mut app, &mut term);
    assert!(
        screen(&term).contains("● Opt 03"),
        "wheel over the options moves the highlight: {}",
        screen(&term)
    );

    // Declared letters still commit through the body.
    term.push_input(b"g");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["go"], None)]);
}

/// Height honesty at tiny viewports (the 0240 law): the options are
/// allocated FIRST and stay visible and operable; the body degrades to
/// its 1-row floor instead of crushing them.
#[test]
fn body_never_crushes_the_options_at_tiny_heights() {
    let size = Size::new(44, 9);
    let (mut app, scope) = host(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = boot(&mut app, &mut term);

    let log: OutcomeLog = Rc::default();
    ChoicePrompt::new("Proceed?")
        .option("alpha", "Alpha")
        .option("beta", "Beta")
        .option("gamma", "Gamma")
        .body(|_| {
            // Rigid rows (shrink 0), like a real card list: the HOST's
            // clip decides what fits — the first row survives, the
            // second clips below the budget.
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .shrink(0.0),
                )
                .child(text("body line 1"))
                .child(text("body line 2"))
                .build()
        })
        .body_rows(8)
        .on_resolve(recorder(&log))
        .open(scope);
    settle(&mut driver, &mut app, &mut term);

    let s = screen(&term);
    assert!(
        s.contains("● Alpha"),
        "the highlighted option survives a 9-row viewport: {s}"
    );
    assert!(
        s.contains("body line 1"),
        "the body keeps its 1-row floor: {s}"
    );
    assert!(
        !s.contains("body line 2"),
        "the body clips to its solved budget instead of painting over rows: {s}"
    );

    // The gate stays operable: movement + Enter commit.
    term.push_input(b"\x1b[B\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(log.borrow().as_slice(), [answered(&["beta"], None)]);
}

/// A lowercase-registered chord must NOT fire on the shifted letter
/// (Shift+A means 'A'): the fold adds the missing spelling, never a
/// case-insensitive blur.
#[test]
fn lowercase_chord_refuses_the_shifted_letter_on_both_wires() {
    let fired: Rc<RefCell<u32>> = Rc::default();
    let f = fired.clone();
    let mut app = App::new(Size::new(W, H));
    app.mount(move |_cx| {
        let f2 = f.clone();
        Element::new()
            .style(LayoutStyle::column())
            .focusable()
            .autofocus()
            .shortcut(KeyChord::plain(Key::Char('a')), move |_| {
                *f2.borrow_mut() += 1
            })
            .child(text("host"))
            .build()
    })
    .expect("mount");
    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = boot(&mut app, &mut term);
    term.push_input(KITTY_SHIFT_A);
    term.push_input(b"A");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*fired.borrow(), 0, "Shift+A is 'A', never 'a'");
    term.push_input(b"a");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(*fired.borrow(), 1, "the declared 'a' itself still fires");
}
