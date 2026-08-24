//! Live PTY smoke: every example runs under a REAL pseudo-terminal,
//! gets scripted keys, and must (a) exit 0 within the deadline, (b) emit
//! only byte traffic the VT referee fully understands, (c) restore the
//! terminal on the way out, (d) never print panic text.
//!
//! OWNER: REDTEAM. `#[ignore]`d: spawns real processes and takes seconds.
//! Run: `cargo test --test live_smoke -- --ignored --nocapture`
//!
//! This is the strongest end-to-end validation available on this machine:
//! it exercises the real `/dev/tty` open path (the PTY becomes the
//! child's controlling terminal), real raw-mode termios, real signal
//! plumbing, and the full enter->frames->leave byte custody with nothing
//! mocked.

#![cfg(unix)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::Duration;

use abstracttui::base::{Rgba, Size};
use abstracttui::testing::pty::spawn_in_pty_opts;
use abstracttui::testing::{Paint, VtScreen};
use abstracttui::theme::contrast::{contrast_ratio, floors};

const COLS: u16 = 100;
const ROWS: u16 = 30;

/// Build all example binaries exactly once per test-process, so each
/// smoke case runs the prebuilt binary (no cargo latency inside the
/// deadline window, no build lock contention between cases).
///
/// Returns whether the build succeeded. It does NOT panic on a build
/// failure: a non-compiling tree is a TRANSIENT builder state (owners
/// edit in parallel), not a smoke finding — a panic here would poison the
/// `Once` and turn every case into a cryptic "poisoned" cascade. Callers
/// skip cleanly instead (the whole-suite green gate elsewhere is what
/// catches a persistently broken tree).
fn ensure_examples_built() -> bool {
    static BUILD: Once = Once::new();
    static OK: AtomicBool = AtomicBool::new(false);
    BUILD.call_once(|| {
        // --workspace: the extension-family examples (network, workflow,
        // mermaid) land in the same target/debug/examples dir, so the
        // smoke covers the whole repo's example surface (wave 11).
        let ok = std::process::Command::new(env!("CARGO"))
            .args(["build", "--workspace", "--examples"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        OK.store(ok, Ordering::SeqCst);
    });
    OK.load(Ordering::SeqCst)
}

fn example_bin(name: &str) -> Option<String> {
    // target dir relative to the crate root the test runs in.
    let path = format!("target/debug/examples/{name}");
    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

/// Why a case could not run. The two reasons look identical from the
/// outside and get OPPOSITE verdicts, which is the whole point of naming
/// them: one is covered by something else, the other is covered by
/// nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Skip {
    /// The tree does not currently compile. Documented transient builder
    /// state (owners edit in parallel) — a CLEAN skip, because
    /// `cargo test --all` on a non-compiling tree fails on its own long
    /// before it reaches here. Reddening it would fail a peer mid-edit.
    TreeNotBuilding,
    /// The build SUCCEEDED and the binary is still absent: this example
    /// is no longer being built at all (renamed, dropped from
    /// `Cargo.toml`, moved). NOTHING else in the repo notices — the
    /// whole-suite gate stays green while this lane goes quiet. That is
    /// the coverage hole, so it goes RED.
    BinaryAbsent,
}

struct SmokeReport {
    /// Set when the case could not run. `assert_clean`'s verdict depends
    /// on WHICH reason: see `Skip`.
    skipped: Option<Skip>,
    exit_code: i32,
    bytes: usize,
    unknown: u64,
    unknown_samples: Vec<String>,
    alt_screen: bool,
    /// The run entered the alt screen at some point (vs a pure
    /// print-and-exit path that never touched terminal modes).
    alt_screen_was_entered: bool,
    bracketed_paste: bool,
    cursor_visible: bool,
    kitty_depth: u64,
    panic_text: bool,
    /// The painted screen, one entry per row: the display text and the
    /// per-cell paint the VT model ended up holding.
    ///
    /// Every other field here is a property of the BYTE STREAM — it ran,
    /// it restored the terminal, it did not panic. None of them can say
    /// what a reader would SEE, which is how `examples/grounds.rs`
    /// shipped announced-and-unseen and then shipped again with swatches
    /// nobody could put text on. A case that claims something is on the
    /// screen has to be able to look at the screen.
    grid: Vec<Vec<(String, Paint)>>,
}

impl SmokeReport {
    /// Row index whose text contains `needle`, if any.
    fn row_with(&self, needle: &str) -> Option<usize> {
        self.grid.iter().position(|row| {
            row.iter()
                .map(|c| c.0.as_str())
                .collect::<String>()
                .contains(needle)
        })
    }

    /// Every `(fg, bg)` pair painted under the glyphs of `word` on `row`.
    fn painted_word(&self, row: usize, word: &str) -> Vec<(Rgba, Rgba)> {
        let cells = &self.grid[row];
        let text: String = cells.iter().map(|c| c.0.as_str()).collect();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(rel) = text[from..].find(word) {
            let at = from + rel;
            if let (Some(fg), Some(bg)) = (cells[at].1.fg, cells[at].1.bg) {
                out.push((fg, bg));
            }
            from = at + word.len();
        }
        out
    }

    fn skipped(why: Skip) -> SmokeReport {
        SmokeReport {
            skipped: Some(why),
            exit_code: 0,
            bytes: 0,
            unknown: 0,
            unknown_samples: Vec::new(),
            alt_screen: false,
            alt_screen_was_entered: false,
            bracketed_paste: false,
            cursor_visible: true,
            kitty_depth: 0,
            panic_text: false,
            grid: Vec::new(),
        }
    }
}

/// Drive one example end-to-end. `keys` are sent after the initial read
/// window (each entry = one write, 150ms apart — a human-ish cadence that
/// lets splash phases and probe deadlines elapse between strokes).
///
/// RT5-1 CLOSED (cycle 7, KERNEL): every example now runs under a REAL
/// CONTROLLING TERMINAL (`ctty=true` — setsid + TIOCSCTTY in `pty.rs`,
/// KERNEL's recipe). KERNEL's acquisition rewrite prefers the pollable
/// stdin/stdout device fds over the `/dev/tty` alias that Darwin's
/// poll(2) rejects with POLLNVAL, and the read loop has a runtime
/// POLLNVAL→stdin-tty fallback with labeled degradation — so keyboard-
/// dead is now structurally impossible. This is the headline: real
/// terminals take keyboard input.
fn smoke(name: &str, warmup: Duration, keys: &[&[u8]], deadline: Duration) -> SmokeReport {
    smoke_opts(name, warmup, keys, deadline, true)
}

fn smoke_opts(
    name: &str,
    warmup: Duration,
    keys: &[&[u8]],
    deadline: Duration,
    ctty: bool,
) -> SmokeReport {
    if !ensure_examples_built() {
        println!(
            "[smoke] {name}: SKIPPED — examples do not currently compile (transient builder state)"
        );
        return SmokeReport::skipped(Skip::TreeNotBuilding);
    }
    let Some(bin) = example_bin(name) else {
        println!("[smoke] {name}: SKIPPED — example binary not present");
        return SmokeReport::skipped(Skip::BinaryAbsent);
    };
    let mut p = spawn_in_pty_opts(&bin, &[], COLS, ROWS, &[], ctty).expect("spawn under pty");

    p.read_for(warmup);
    for k in keys {
        p.send(k);
        p.read_for(Duration::from_millis(150));
    }
    let code = p.wait_with_deadline(deadline);
    let exit_code = match code {
        Some(c) => c,
        None => {
            p.kill();
            // Feed what we have into the model anyway for diagnostics.
            let mut vt = VtScreen::new(Size::new(COLS as i32, ROWS as i32));
            vt.feed(&p.captured);
            panic!(
                "{name}: HUNG past {:?} deadline ({} bytes captured, unknown={})",
                deadline,
                p.captured.len(),
                vt.unknown_seq_count(),
            );
        }
    };

    let mut vt = VtScreen::new(Size::new(COLS as i32, ROWS as i32));
    vt.feed(&p.captured);
    let text = String::from_utf8_lossy(&p.captured).to_string();
    SmokeReport {
        skipped: None,
        exit_code,
        bytes: p.captured.len(),
        unknown: vt.unknown_seq_count(),
        unknown_samples: vt.unknown_samples().to_vec(),
        alt_screen: vt.modes().alt_screen(),
        alt_screen_was_entered: text.contains("\x1b[?1049h"),
        bracketed_paste: vt.modes().bracketed_paste(),
        cursor_visible: vt.modes().cursor_visible(),
        kitty_depth: vt.counters().kitty_push_depth,
        panic_text: text.contains("panicked at") || text.contains("RUST_BACKTRACE"),
        grid: (0..ROWS as i32)
            .map(|y| {
                (0..COLS as i32)
                    .map(|x| match vt.cell(x, y) {
                        Some(c) => (c.display().to_string(), c.paint),
                        None => (" ".to_string(), Paint::default()),
                    })
                    .collect()
            })
            .collect(),
    }
}

/// Opt-out for a machine that genuinely cannot run the pty suite. A
/// sentence a human has to type, so a green run there is a decision on
/// the record rather than a default.
const ALLOW_UNMEASURED: &str = "ABSTRACTTUI_ALLOW_UNMEASURED";

/// Apply the skip policy. Returns `true` when the caller should stop
/// (the case legitimately did not run); panics when the skip is a
/// coverage hole. Every early `if r.skipped { return }` goes through
/// here — a bare early return is the shape this policy exists to remove.
///
/// A `BinaryAbsent` skip used to return GREEN. That is an
/// absent-input-passes check inside the suite whose whole job is to
/// catch examples that never actually run — the same shape this suite
/// found in `examples/grounds.rs`, one level up.
///
/// `TreeNotBuilding` stays green ON PURPOSE, and that is not the same
/// concession: `ensure_examples_built` documents it as a transient
/// builder state, and a tree that does not compile fails the whole-suite
/// gate on its own. Reddening it would fail a peer who is mid-edit —
/// valid-input-fails, which is the same defect with the polarity
/// flipped and no better than the one being fixed.
#[must_use]
fn skip_is_acceptable(name: &str, r: &SmokeReport) -> bool {
    let Some(why) = r.skipped else { return false };
    match why {
        Skip::TreeNotBuilding => {
            println!("[smoke] {name}: skipped — tree not building (transient); not measured");
            true
        }
        Skip::BinaryAbsent => {
            assert!(
                std::env::var_os(ALLOW_UNMEASURED).is_some(),
                "{name}: the examples BUILT and this binary is still absent, so the \
                 case is UNMEASURED and nothing else in the repo notices. This suite \
                 is the only thing that runs an example's paint path. Reason printed \
                 above. Set {ALLOW_UNMEASURED}=1 to accept an unmeasured run \
                 deliberately."
            );
            println!(
                "[smoke] {name}: binary absent, accepted via {ALLOW_UNMEASURED} — NOT measured"
            );
            true
        }
    }
}

fn assert_clean(name: &str, r: &SmokeReport) {
    if skip_is_acceptable(name, r) {
        return;
    }
    println!(
        "[smoke] {name}: exit={} bytes={} unknown={} alt={} paste={} cursor={} kitty={}",
        r.exit_code,
        r.bytes,
        r.unknown,
        r.alt_screen,
        r.bracketed_paste,
        r.cursor_visible,
        r.kitty_depth
    );
    assert_eq!(r.exit_code, 0, "{name}: nonzero exit");
    assert!(!r.panic_text, "{name}: panic text in output");
    assert!(r.bytes > 0, "{name}: produced no terminal output at all");
    assert_eq!(
        r.unknown, 0,
        "{name}: {} unknown sequences; referee gaps or illegal emission. Samples: {:?}",
        r.unknown, r.unknown_samples
    );
    // Terminal restored: the leave path must undo what enter set.
    assert!(
        !r.alt_screen,
        "{name}: left terminal on the alt screen (1049 not reset)"
    );
    assert!(
        !r.bracketed_paste,
        "{name}: bracketed paste (2004) left enabled"
    );
    assert!(
        r.cursor_visible,
        "{name}: cursor left hidden (25h missing on leave)"
    );
    assert_eq!(
        r.kitty_depth, 0,
        "{name}: kitty keyboard stack not popped to zero"
    );
}

// One test per example: independent pass/fail, parallel-safe (each owns
// its own PTY + process; the shared build happens under Once).

/// Tab the hover-card demo through every chip, dismiss with Escape,
/// then quit — the keyboard path end to end, with no mouse reporting
/// involved at any point.
///
/// Escape is spelled CSI 27u rather than a bare `\x1b`: a lone ESC byte
/// waits out the reader's disambiguation window, so it either resolves
/// late or is swallowed by the next key sent.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_hovercard() {
    let r = smoke(
        "hovercard",
        Duration::from_millis(1500),
        &[b"\t", b"\t", b"\t", b"\t", b"\t", b"\x1b[27u", b"s", b"q"],
        Duration::from_secs(10),
    );
    assert_clean("hovercard", &r);
}

/// The MOUSE half, which nothing covered: point at a chip — no button,
/// no click — and the card must open.
///
/// This is the shape laurent reported: *"the tooltip shows on click, but
/// not on mouseover."* It was true, and it was an engine defect. Hover is
/// recomputed only from mouse REPORTS, and the default session posture
/// (`MouseMode::ButtonDrag`) has the terminal send motion only while a
/// button is DOWN — so pressing produced a report, moving produced
/// nothing, and the tip looked click-triggered.
///
/// **What this case can and cannot prove.** It INJECTS an SGR motion
/// report (button 35 = motion, no button held), so it proves the app
/// turns motion into an open card. It cannot prove the app ASKED the
/// terminal to send motion in the first place — the bytes arrive whether
/// or not mode 1003 was armed. That half is
/// `anchored_layer_tests::mounting_a_tooltip_arms_motion_reporting_without_the_app_asking`,
/// which asserts `[?1003h` reached the terminal. Two halves, two tests;
/// neither alone would have caught it.
///
/// The coordinates are the chip's, one-based, from the example's own
/// layout. If the layout moves, this fails — which is the right failure:
/// the case is asserting that a reader pointing at the chip gets a card.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_hovercard_opens_on_pointer_motion_with_no_button_held() {
    let r = smoke(
        "hovercard",
        Duration::from_millis(1500),
        // Motion onto the `#412` chip, then three unbound keys so the
        // 250ms open delay elapses before the screen is captured.
        &[b"\x1b[<35;58;3M", b"x", b"x", b"x", b"q"],
        Duration::from_secs(10),
    );
    assert_clean("hovercard", &r);
    if r.skipped.is_some() {
        return;
    }
    assert!(
        r.row_with("place_panel prefers below").is_some(),
        "pointing at the chip left the card shut — the hover trigger is \
         dead again, or the chip moved off (58,3). Screen:\n{}",
        r.grid
            .iter()
            .map(|row| row.iter().map(|c| c.0.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Walk the big-text sweep and cycle all three axis keys under a real
/// terminal. The keys are `s` symbols, `w` weight, `a` sampling — sent
/// here in the spelling the example's own legend advertises, so a rename
/// that moves a legend without moving its binding fails this rather than
/// reaching an operator. (It reached one: the legend said `v`/`f` after
/// the words had become symbols and weight.)
///
/// The arrows walk the sixteen-step scale sweep past its ends, which is
/// also the wrap-around guard.
///
/// **The screen assertion is the point, and it is here because a clean
/// exit is exactly what this example gave the operator while printing
/// `clearly apart` over a row of white bars.** The readout carries two
/// independent numbers per class now — the closest pair, and the worst
/// character's fidelity loss — and the second is the one that was
/// missing. A run that painted only the first would still exit 0.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_bigtext() {
    let r = smoke(
        "bigtext",
        Duration::from_millis(1500),
        &[
            b"\x1b[B", b"\x1b[B", b"\x1b[B", b"s", b"s", b"w", b"a", b"\x1b[A", b"q",
        ],
        Duration::from_secs(10),
    );
    assert_clean("bigtext", &r);
    if r.skipped.is_some() {
        return;
    }
    let screen = || {
        r.grid
            .iter()
            .map(|row| row.iter().map(|c| c.0.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let row = r
        .row_with("icons")
        .unwrap_or_else(|| panic!("the readout lost its icons row. Screen:\n{}", screen()));
    let text: String = r.grid[row].iter().map(|c| c.0.as_str()).collect();
    // Both columns, on the row the reported screenshot was about. The
    // shape column is a decimal, so `0.` is enough to tell it from the
    // subpixel count beside it — and deleting that column takes this red
    // rather than leaving a green run with half a readout.
    assert!(
        text.contains("0."),
        "the icons row shows no fidelity loss — the shape column is the term the pairwise \
         number was missing. Row: {text:?}\nScreen:\n{}",
        screen()
    );
    assert!(
        r.row_with("shape").is_some() && r.row_with("apart").is_some(),
        "both column headers must be on screen; one of them alone is the readout that \
         shipped the bug. Screen:\n{}",
        screen()
    );
}

/// Drive `RowSelect` under a real terminal: Tab takes the surface,
/// arrows walk two-line rows past the bottom of the viewport (so
/// ensure-visible has to scroll by CONTENT rows), then `m` moves the
/// selected member's index under it and the selection has to follow the
/// KEY rather than the slot.
///
/// The screen assertion is the point. `index N · key "tui"` is printed
/// by the example's own status line, so a selection that silently
/// reverted to index-following would come back as `key "newcomer-a"` —
/// a green process exit would not have noticed.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_roster() {
    let r = smoke(
        "roster",
        Duration::from_millis(1500),
        &[
            b"\t", b"\x1b[B", b"\x1b[B", b"\x1b[B", b"\r", b"\x1b[A", b"m", b"q",
        ],
        Duration::from_secs(10),
    );
    assert_clean("roster", &r);
    if r.skipped.is_some() {
        return;
    }
    // Three downs then one up leaves `agora-wui` selected at index 2;
    // `m` puts two arrivals in front of them, so the INDEX must be 4 and
    // the KEY unchanged. ONE row carrying BOTH — `index 4` alone would
    // also be satisfied by a selection that had drifted to a different
    // member, and `agora-wui` alone is on screen either way.
    assert!(
        r.row_with("index 4 · key \"agora-wui\"").is_some(),
        "the selection did not follow the key through the mutation. Screen:\n{}",
        r.grid
            .iter()
            .map(|row| row.iter().map(|c| c.0.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_hello() {
    let r = smoke(
        "hello",
        Duration::from_millis(1500),
        &[b"q"],
        Duration::from_secs(8),
    );
    assert_clean("hello", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_themes() {
    // Cycle a few themes with arrows/tab before quitting.
    let r = smoke(
        "themes",
        Duration::from_millis(1500),
        &[b"\x1b[C", b"\x1b[C", b"\x1b[D", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("themes", &r);
}

/// Does `assert_clean` panic on this report?
fn verdict_is_red(name: &str, r: &SmokeReport) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| assert_clean(name, r)));
    std::panic::set_hook(hook);
    caught.is_err()
}

/// The guard on the guard, and it has to falsify BOTH branches.
///
/// A skip is a coverage hole wearing a green tick — but only when
/// nothing else covers it, and the two skip reasons differ on exactly
/// that. Written after the first version of this guard reddened both:
/// it was checked against an absent example name (the case in front of
/// me) and would have failed any peer with a mid-edit tree, which is
/// valid-input-fails dressed up as a fix for absent-input-passes.
///
/// Needs no pty and no build, so it runs in the ordinary suite rather
/// than only in the `--ignored` lane: the policy is what regresses, and
/// a policy check nobody runs is the thing being fixed here.
#[test]
fn skip_policy_reddens_a_dropped_example_and_spares_a_broken_tree() {
    if std::env::var_os(ALLOW_UNMEASURED).is_some() {
        println!("[smoke] policy test: {ALLOW_UNMEASURED} set, the guard is opted out by design");
        return;
    }
    assert!(
        verdict_is_red("dropped", &SmokeReport::skipped(Skip::BinaryAbsent)),
        "an example that BUILT and produced no binary reported as a PASS — \
         nothing else in the repo notices, so this lane can go quiet silently"
    );
    assert!(
        !verdict_is_red("mid-edit", &SmokeReport::skipped(Skip::TreeNotBuilding)),
        "a non-compiling tree went RED here — that fails a peer mid-edit for a \
         state the whole-suite gate already catches"
    );
}

/// The premise the policy test asserts against, over the real plumbing:
/// an absent example name must reach `BinaryAbsent`, not some other
/// skip. Separated because it needs the build and the policy test does
/// not.
#[test]
#[ignore = "live: builds the examples to reach the skip path"]
fn an_absent_example_name_is_a_binary_absent_skip() {
    let r = smoke(
        "definitely-not-an-example",
        Duration::from_millis(0),
        &[],
        Duration::from_secs(2),
    );
    assert_eq!(
        r.skipped,
        Some(Skip::BinaryAbsent),
        "an absent example name must classify as BinaryAbsent for the policy to bite"
    );
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_grounds() {
    // Walk two themes and toggle the declared panel ground before
    // quitting. This example's paint path was shipped compiled-but-never
    // -seen and @laurent reported it launching to nothing; a pty case is
    // the only thing that runs it.
    let r = smoke(
        "grounds",
        Duration::from_millis(1500),
        &[b"\x1b[B", b"\x1b[B", b"p", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("grounds", &r);
    if r.skipped.is_some() {
        return;
    }
    // @laurent, dm#15: "confirm that you can have text on top of those
    // colored panels ... good contrast between colored panel and text".
    // Confirmed HERE, off the painted pty screen, not off an exit code.
    //
    // The BRIGHT panel row deliberately, and this is the whole reason it
    // exists in the example. On the dark themes this case walks, the
    // theme's own grounds are all dark, so `t.text` reads on every one of
    // them and an assertion there cannot tell `ink_on` apart from a
    // hand-picked ink — a green check over a case that cannot fail.
    // Measured: swapping `ink_on` for `t.text` left this case PASSING
    // until the bright band existed. On the bright band a dark theme's
    // body ink measures ~1.38:1, so the polarity choice is load-bearing
    // and the check has teeth.
    let row = r
        .row_with("+ panel bright")
        .expect("the bright declared-panel band should be on the first screen");
    let painted = r.painted_word(row, "Text");
    assert_eq!(
        painted.len(),
        3,
        "expected the word Text painted on all three bands (truecolor, \
         nearest, assigned) of the surface_raised row; found {}",
        painted.len()
    );
    for (fg, bg) in painted {
        let c = contrast_ratio(fg, bg);
        assert!(
            c >= floors::TEXT,
            "text on a ground band measured {c:.2}:1 (floor {:.1}) — \
             fg {} on bg {}. ink_on picked the wrong pole, or the band \
             was painted with a hand-picked ink again.",
            floors::TEXT,
            fg.to_hex(),
            bg.to_hex()
        );
    }
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_widgets() {
    // Tab around, type into whatever input takes focus, then ESC + q.
    let r = smoke(
        "widgets",
        Duration::from_millis(1500),
        &[b"\t", b"\t", b"abc", b"\x1b", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("widgets", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_effects() {
    // Let animations run long enough to bill some frames.
    let r = smoke(
        "effects",
        Duration::from_millis(2500),
        &[b"q"],
        Duration::from_secs(8),
    );
    assert_clean("effects", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_components() {
    // The shareable-component showcase: tab through, poke, quit.
    let r = smoke(
        "components",
        Duration::from_millis(1500),
        &[b"\t", b"\t", b"\r", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("components", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_grid() {
    // The grid layout showcase.
    let r = smoke(
        "grid",
        Duration::from_millis(1500),
        &[b"\t", b"\x1b[C", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("grid", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_dashboard() {
    // Timers fire during warmup; poke tabs + a list scroll first.
    let r = smoke(
        "dashboard",
        Duration::from_millis(2500),
        &[b"\t", b"\x1b[B", b"\x1b[B", b"q"],
        Duration::from_secs(10),
    );
    assert_clean("dashboard", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_splash() {
    // ESC first (skip the splash), then q to quit the app proper.
    let r = smoke(
        "splash",
        Duration::from_millis(1200),
        &[b"\x1b", b"q"],
        Duration::from_secs(12),
    );
    assert_clean("splash", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_splash_unskipped_runs_to_completion() {
    // No skip: the splash must hand over on its own (2.5s hard ceiling)
    // and the app must then quit normally. Guards the wall-clock honesty
    // of the pacing loop on a REAL terminal, not a scripted clock.
    let r = smoke(
        "splash",
        Duration::from_millis(3200),
        &[b"q"],
        Duration::from_secs(12),
    );
    assert_clean("splash-unskipped", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_viewer3d() {
    // GLB load + textured raster; poke spin + mode keys before quitting.
    // Without the workspace asset the example prints usage and exits 0 —
    // still asserted clean (no panic, no unknown bytes).
    let r = smoke(
        "viewer3d",
        Duration::from_millis(3000),
        &[b" ", b"2", b"3", b"q"],
        Duration::from_secs(12),
    );
    if skip_is_acceptable("viewer3d", &r) {
        return;
    }
    assert_eq!(r.exit_code, 0, "viewer3d: nonzero exit");
    assert!(!r.panic_text, "viewer3d: panic text in output");
    assert_eq!(
        r.unknown, 0,
        "viewer3d: unknown sequences: {:?}",
        r.unknown_samples
    );
    if r.alt_screen_was_entered {
        assert_clean("viewer3d", &r);
    } else {
        println!("[smoke] viewer3d: usage-print path (no asset) — exit clean");
    }
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_images() {
    // Procedural bitmap without an arg; toggle dither + protocol + theme.
    let r = smoke(
        "images",
        Duration::from_millis(2000),
        &[b"d", b"p", b"t", b"q"],
        Duration::from_secs(10),
    );
    assert_clean("images", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_voice_mock() {
    // Latch a talk phase on (space), let the fake mic + transcription
    // run, latch off, focus-out escape (capture must stop if somehow
    // still on), then quit. The pty wire is legacy (no kitty flags
    // negotiated by the harness), so space toggles — exactly the
    // Degraded path the fidelity footer names.
    let r = smoke(
        "voice_mock",
        Duration::from_millis(2000),
        &[b" ", b" ", b"\x1b[O", b"q"],
        Duration::from_secs(10),
    );
    assert_clean("voice_mock", &r);
}

/// CHILD half of the RT5-1 characterization (spawned by the probe below
/// under a pty): opens /dev/tty, polls it with input already queued,
/// prints the revents for both /dev/tty and stdin.
#[test]
#[ignore = "diagnostic child — spawned by rt5_1_poll_devtty_characterization"]
fn rt5_1_child_poll_devtty() {
    // SAFETY-FREE ZONE NOTE: this is a characterization CHILD, unix-only,
    // exercising raw libc like the terminal backend does.
    unsafe {
        let fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        println!("CHILD: /dev/tty fd={fd}");
        if fd < 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(300)); // parent queues a byte
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = libc::poll(&mut pfd, 1, 1000);
        println!("CHILD: poll(devtty) rc={rc} revents={:#x}", pfd.revents);
        let mut pfd0 = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc0 = libc::poll(&mut pfd0, 1, 1000);
        println!("CHILD: poll(stdin) rc={rc0} revents={:#x}", pfd0.revents);
    }
}

/// RT5-1 characterization (the evidence): on macOS, poll(2) on a
/// /dev/tty descriptor inside a pty session reports POLLNVAL (0x20)
/// while the SAME queued input polls readable on the stdin descriptor.
/// The engine's read loop masks POLLIN|POLLHUP|POLLERR, so keys are
/// silently invisible on the /dev/tty path.
#[test]
#[ignore = "live: spawns itself under a PTY to characterize poll(/dev/tty)"]
fn rt5_1_poll_devtty_characterization() {
    let me = std::env::current_exe().unwrap();
    let mut p = spawn_in_pty_opts(
        me.to_str().unwrap(),
        &[
            "rt5_1_child_poll_devtty",
            "--ignored",
            "--exact",
            "--nocapture",
        ],
        80,
        24,
        &[],
        true,
    )
    .expect("spawn self under pty");
    p.read_for(Duration::from_millis(200));
    p.send(b"Z"); // queue input BEFORE the child polls
    p.read_for(Duration::from_millis(2500));
    let _ = p.wait_with_deadline(Duration::from_secs(5));
    let out = String::from_utf8_lossy(&p.captured).to_string();
    println!("characterization capture:\n{out}");
    // The probe documents behavior rather than asserting a fix: record
    // both poll outcomes so the failure mode is visible in the log.
    assert!(out.contains("poll(devtty)"), "child never ran: {out}");
}

/// RT5-1 acceptance (P0, KERNEL) — CLOSED cycle 7. With the PTY as the
/// child's CONTROLLING TERMINAL (setsid + TIOCSCTTY, the engine's
/// preferred path — how every real terminal runs apps), a keystroke must
/// reach the app: the example quits ONLY on 'q', so exit 0 within the
/// deadline is proof the key was delivered. Previously POLLNVAL on
/// Darwin's /dev/tty alias made this keyboard-dead; KERNEL's acquisition
/// rewrite + runtime POLLNVAL→stdin fallback fixed it. The whole live
/// suite now runs ctty=true, so this is a focused regression guard.
#[test]
#[ignore = "live: spawns a real example under a controlling-terminal PTY"]
fn live_ctty_input_reaches_app() {
    let r = smoke_opts(
        "hello",
        Duration::from_millis(1500),
        &[b"q"],
        Duration::from_secs(8),
        true,
    );
    if skip_is_acceptable("hello-ctty", &r) {
        return;
    }
    assert_clean("hello-ctty", &r);
    // The app can ONLY have exited via the 'q' keystroke reaching it
    // through the controlling terminal — keyboard input is live.
    assert_eq!(
        r.exit_code, 0,
        "keystroke 'q' must reach the app over the ctty path"
    );
}

/// Appended (wave 3, READER): the markdown reader end-to-end — search
/// (`/needle` + Enter jumps + `n` next), TOC panel jump, theme cycle,
/// scroll, quit. The embedded sample exercises tables (0142), lazy
/// in-flow images incl. an honest missing one (0144), anchors (0146)
/// and the highlight overlay (0148) under a real pty.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_reader() {
    let r = smoke(
        "reader",
        Duration::from_millis(1500),
        &[
            b"/",       // open search
            b"needle",  // type the query
            b"\r",      // submit: jump to first match
            b"n",       // next match (wraps)
            b"t",       // TOC panel
            b"\r",      // activate the selected heading (jump + close)
            b"\x14",    // Ctrl+T: theme cycle
            b"\x1b[B",  // scroll down
            b"\x1b[6~", // PageDown
            b"q",
        ],
        Duration::from_secs(10),
    );
    assert_clean("reader", &r);
}

/// Appended (wave 3, INTEGRATOR — feed doc-vocabulary adoption): the
/// streaming transcript under a real pty. The scripted stream runs a
/// couple of turns during warmup (doc-session typesetting live), then
/// the composer path exits: `/quit` opens the command completion,
/// Enter accepts the candidate, Enter submits. Exit 0 through the
/// composer is the proof the whole surface (Feed + DocStreamSession +
/// TextArea + Completion) survives a real wire.
#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_transcript() {
    let r = smoke(
        "transcript",
        Duration::from_millis(2500),
        &[
            b"/quit", // completion dropdown opens, filtered to /quit
            b"\r",    // accept the candidate (inserts "/quit ")
            b"\r",    // submit: the command quits the app
        ],
        Duration::from_secs(12),
    );
    assert_clean("transcript", &r);
}

// ---------------------------------------------------------------------------
// Wave-11 gap fill (CODE): every example runs under the referee — the
// cases below cover the examples the battery had not yet adopted
// (drawers/shell = 0.2.12, screenshot = 0.2.14, feed/gallery/decide/
// caps, and the extension-family examples via the --workspace build).
// Scripts stay SHORT and end in a state where `q` quits: fixed key
// pacing desyncs on loaded machines, and a desynced long script is a
// flake, not a finding.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_feed() {
    // Toggle follow-tail once, then quit.
    let r = smoke(
        "feed",
        Duration::from_millis(1500),
        &[b" ", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("feed", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_gallery() {
    // One theme step, then quit.
    let r = smoke(
        "gallery",
        Duration::from_millis(1500),
        &[b"t", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("gallery", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_decide() {
    // Open the confirm prompt (gate 1 is dismissable(false): Esc is a
    // deliberate no-op — its options ARE the exits), resolve it with
    // the `k` option key ("Keep my copies"), quit.
    let r = smoke(
        "decide",
        Duration::from_millis(1500),
        &[b"1", b"k", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("decide", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_drawers() {
    // Open the inspector drawer (modal: keys route to the panel),
    // close it with Esc, quit from the restored main surface.
    let r = smoke(
        "drawers",
        Duration::from_millis(1500),
        &[b"i", b"\x1b", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("drawers", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_shell() {
    // One container chord page switch (Ctrl+PgDn), then quit — the
    // PageHost never consumes plain `q`, so the app shortcut fires.
    let r = smoke(
        "shell",
        Duration::from_millis(1500),
        &[b"\x1b[6;5~", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("shell", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_attachments() {
    // Appended (first-app 0273): a bracketed-paste file DROP becomes a
    // chip (never composer text), Ctrl+O opens the FilePicker modal,
    // Esc (the host's) closes it, Ctrl+C quits — the composer owns
    // plain letters, so quit rides the default Ctrl+C path.
    let r = smoke(
        "attachments",
        Duration::from_millis(1500),
        &[
            b"\x1b[200~/tmp/live\\ smoke.png \x1b[201~", // drop -> chip
            b"\x0f",                                     // Ctrl+O: picker modal
            b"\x1b", // Esc: the host closes the modal (drawers precedent)
            b"\x03", // Ctrl+C: quit
        ],
        Duration::from_secs(10),
    );
    assert_clean("attachments", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_screenshot() {
    // Capture once (s writes text/ansi/svg artifacts), then quit.
    let r = smoke(
        "screenshot",
        Duration::from_millis(1500),
        &[b"s", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("screenshot", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_caps() {
    // The capability report screen under a real tty (probe runs).
    let r = smoke(
        "caps",
        Duration::from_millis(1500),
        &[b"q"],
        Duration::from_secs(8),
    );
    assert_clean("caps", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_graph_workflow() {
    // Extension family: select the first card (Enter), then quit.
    let r = smoke(
        "workflow",
        Duration::from_millis(1500),
        &[b"\r", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("workflow", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_graph_network() {
    // Extension family: select, one spatial move, quit.
    let r = smoke(
        "network",
        Duration::from_millis(1500),
        &[b"\r", b"\x1b[C", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("network", &r);
}

#[test]
#[ignore = "live: spawns real example processes under a PTY"]
fn live_mermaid() {
    // Extension family: step through two diagrams, quit.
    let r = smoke(
        "mermaid",
        Duration::from_millis(1500),
        &[b"l", b"l", b"q"],
        Duration::from_secs(8),
    );
    assert_clean("mermaid", &r);
}
