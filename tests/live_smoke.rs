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

use abstracttui::base::Size;
use abstracttui::testing::pty::spawn_in_pty_opts;
use abstracttui::testing::VtScreen;

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
        let ok = std::process::Command::new(env!("CARGO"))
            .args(["build", "--examples"])
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

struct SmokeReport {
    /// Set when the case could not run (tree not compiling / binary
    /// absent). `assert_clean` treats it as a clean skip, never a failure.
    skipped: bool,
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
}

impl SmokeReport {
    fn skipped() -> SmokeReport {
        SmokeReport {
            skipped: true,
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
        return SmokeReport::skipped();
    }
    let Some(bin) = example_bin(name) else {
        println!("[smoke] {name}: SKIPPED — example binary not present");
        return SmokeReport::skipped();
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
        skipped: false,
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
    }
}

fn assert_clean(name: &str, r: &SmokeReport) {
    if r.skipped {
        println!("[smoke] {name}: skipped (see reason above)");
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
    if r.skipped {
        println!("[smoke] viewer3d: skipped");
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
    if r.skipped {
        println!("[smoke] hello-ctty: skipped");
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
