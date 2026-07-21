//! RT5-1 decisive tests: is `poll(/dev/tty)` POLLNVAL a harness artifact
//! or a real platform behavior — and does the fix (resolving the alias to
//! the REAL terminal device) restore keystroke flow through the engine?
//!
//! OWNER: KERNEL. The parent tests spawn THIS TEST BINARY as a child
//! under a pty with a PROPERLY ESTABLISHED controlling terminal
//! (setsid + TIOCSCTTY — the exact setup a real terminal gives every
//! app), so "the child had no ctty" is eliminated as an explanation by
//! construction. Children are `#[ignore]`d test fns invoked by name.
//!
//! Verdict encoded by `devtty_alias_vs_real_device_poll`: on macOS the
//! /dev/tty ALIAS is not pollable (POLLNVAL with a byte queued) while
//! the ttyname-RESOLVED device fd polls readable for the same byte —
//! a Darwin device limitation, present with a perfect ctty, therefore a
//! REAL engine bug for any code polling /dev/tty (fixed in unix.rs by
//! resolving; guarded everywhere by the runtime POLLNVAL fallback).

use super::unix::IoctlReq;
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Spawn this test binary under a fresh pty, slave as stdio AND
/// controlling terminal; returns the master fd + child.
fn spawn_child_test(name: &str) -> std::io::Result<(RawFd, std::process::Child)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    // SAFETY: openpty writes two fds into the out-params on success.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let ws = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ioctl on the master we just opened.
    unsafe { libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws) };

    let stdio = |fd: RawFd| -> std::io::Result<Stdio> {
        // SAFETY: dup(2) of a live fd; the returned Stdio owns the dup.
        let d = unsafe { libc::dup(fd) };
        if d < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: d is fresh and exclusively owned by the Stdio.
        Ok(unsafe { Stdio::from(std::fs::File::from_raw_fd(d)) })
    };

    let me = std::env::current_exe()?;
    let mut cmd = Command::new(me);
    cmd.arg(name)
        .arg("--ignored")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("TERM", "xterm-256color")
        .stdin(stdio(slave)?)
        .stdout(stdio(slave)?)
        .stderr(stdio(slave)?);
    let slave_for_child = slave;
    // SAFETY: post-fork/pre-exec, async-signal-safe calls only (setsid,
    // ioctl). This is what makes the pty the child's CONTROLLING
    // terminal — /dev/tty resolves and opens exactly as under a real
    // terminal emulator.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(slave_for_child, libc::TIOCSCTTY as IoctlReq, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    // SAFETY: closing our copy; the child holds dups.
    unsafe { libc::close(slave) };
    Ok((master, child))
}

fn read_master_until(master: RawFd, needle: &[u8], wait: Duration) -> (bool, Vec<u8>) {
    let deadline = Instant::now() + wait;
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if got.windows(needle.len().max(1)).any(|w| w == needle) {
            return (true, got);
        }
        let rem = deadline.saturating_duration_since(Instant::now());
        if rem.is_zero() {
            return (false, got);
        }
        let mut p = libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll over one live fd on this stack frame.
        let rc = unsafe { libc::poll(&mut p, 1, rem.as_millis().min(300) as i32) };
        if rc <= 0 {
            continue;
        }
        // SAFETY: read into a stack buffer from our master fd.
        let n = unsafe { libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            return (false, got);
        }
        got.extend_from_slice(&buf[..n as usize]);
    }
}

fn write_master(master: RawFd, bytes: &[u8]) {
    // SAFETY: write to our live master fd.
    unsafe { libc::write(master, bytes.as_ptr() as *const libc::c_void, bytes.len()) };
}

fn cleanup(master: RawFd, mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
    // SAFETY: closing the master fd we own.
    unsafe { libc::close(master) };
}

// ---------------------------------------------------------------------------
// CHILD probes (ignored; run only when invoked by name from the parents).
// ---------------------------------------------------------------------------

/// Child A: with a REAL ctty, put the terminal in raw mode, then poll
/// (1) the /dev/tty alias fd, (2) the ttyname-resolved device fd, and
/// (3) stdin — for the same queued byte. Prints structured verdict lines.
#[test]
#[ignore = "RT5-1 child probe — spawned by devtty_alias_vs_real_device_poll"]
fn rt5_child_alias_vs_real() {
    // SAFETY: characterization child exercising the exact raw calls the
    // backend makes; every call is a plain FFI call on fds it owns.
    unsafe {
        let alias = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        println!(
            "PROBE: alias_open={}",
            if alias >= 0 { "ok" } else { "FAIL" }
        );
        if alias < 0 {
            return;
        }
        // Raw mode so a single queued byte is immediately readable
        // (cooked mode would hold it in the line discipline and muddy
        // the poll verdicts with buffering, not pollability).
        let mut ios: libc::termios = std::mem::zeroed();
        libc::tcgetattr(alias, &mut ios);
        libc::cfmakeraw(&mut ios);
        libc::tcsetattr(alias, libc::TCSANOW, &ios);

        println!("PROBE: READY");
        std::thread::sleep(Duration::from_millis(400)); // parent queues 'Z'

        let mut p = libc::pollfd {
            fd: alias,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = libc::poll(&mut p, 1, 1000);
        let nval = p.revents & libc::POLLNVAL != 0;
        let pin = p.revents & libc::POLLIN != 0;
        println!(
            "PROBE: alias_poll rc={rc} revents={:#x} nval={nval} pollin={pin}",
            p.revents
        );

        // Trap #2 (the reason the fix resolves from STD FDS, not the
        // alias): ttyname of the ALIAS fd answers the alias itself on
        // Darwin — a circular "resolution".
        let mut buf = [0 as libc::c_char; 1024];
        if libc::ttyname_r(alias, buf.as_mut_ptr(), buf.len()) == 0 {
            let path = std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .into_owned();
            println!("PROBE: alias_resolves_to={path}");
        } else {
            println!("PROBE: alias_resolves_to=FAIL");
        }

        // The fix's mechanism: resolve a REAL tty fd (stdin here — the
        // pty slave) to its device path, open fresh, poll: readable.
        if libc::ttyname_r(0, buf.as_mut_ptr(), buf.len()) == 0 {
            let path = std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .into_owned();
            let real = libc::open(buf.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
            println!(
                "PROBE: stdin_resolves_to={path} open={}",
                if real >= 0 { "ok" } else { "FAIL" }
            );
            if real >= 0 {
                let mut pr = libc::pollfd {
                    fd: real,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let rcr = libc::poll(&mut pr, 1, 1000);
                let rin = pr.revents & libc::POLLIN != 0;
                let rnval = pr.revents & libc::POLLNVAL != 0;
                println!(
                    "PROBE: real_poll rc={rcr} revents={:#x} nval={rnval} pollin={rin}",
                    pr.revents
                );
                if rin {
                    let mut b = [0u8; 8];
                    let n = libc::read(real, b.as_mut_ptr() as *mut libc::c_void, b.len());
                    if n > 0 {
                        println!("PROBE: real_read={}", b[0] as char);
                    }
                }
            }
        } else {
            println!("PROBE: stdin_resolves_to=FAIL");
        }
        println!("PROBE: DONE");
    }
}

/// Child B: the ENGINE path end to end — `UnixTerminal::new()` (the
/// default acquisition apps get), enter, one read. Post-fix this must
/// deliver the parent's keystroke.
#[test]
#[ignore = "RT5-1 child probe — spawned by engine_keystrokes_flow_on_ctty_path"]
fn rt5_child_engine_read() {
    use crate::term::{EnterOptions, TermRead, Terminal, UnixTerminal};
    let mut term = match UnixTerminal::new() {
        Ok(t) => t,
        Err(e) => {
            println!("ENGINE: new_failed={e}");
            return;
        }
    };
    println!("ENGINE: acquired");
    if let Err(e) = term.enter(&EnterOptions::default()) {
        println!("ENGINE: enter_failed={e}");
        return;
    }
    println!("ENGINE: READY");
    match term.read(Some(Instant::now() + Duration::from_secs(4))) {
        Ok(TermRead::Input(b)) if !b.is_empty() => {
            println!("ENGINE: got={}", b[0] as char);
        }
        other => println!("ENGINE: got_none ({other:?})"),
    }
    let degraded = term.degraded().map(str::to_owned);
    let _ = term.leave();
    match degraded {
        Some(d) => println!("ENGINE: degraded={d}"),
        None => println!("ENGINE: degraded=none"),
    }
    println!("ENGINE: DONE");
}

// ---------------------------------------------------------------------------
// PARENT tests (run in the normal suite; skip gracefully without a pty).
// ---------------------------------------------------------------------------

/// THE DECISIVE TEST: with a perfect controlling terminal, the /dev/tty
/// ALIAS is not pollable on macOS while the RESOLVED device is — same
/// session, same queued byte. This separates "harness ctty artifact"
/// (eliminated: the ctty is established by construction here) from the
/// real platform behavior the engine must handle.
#[test]
fn devtty_alias_vs_real_device_poll() {
    let (master, child) = match spawn_child_test("term::rt5_live_tests::rt5_child_alias_vs_real") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("skipping: cannot spawn pty child ({e})");
            return;
        }
    };
    let (ready, mut all) = read_master_until(master, b"PROBE: READY", Duration::from_secs(10));
    if !ready {
        cleanup(master, child);
        panic!(
            "child never became ready: {}",
            String::from_utf8_lossy(&all)
        );
    }
    write_master(master, b"Z");
    let (done, out) = read_master_until(master, b"PROBE: DONE", Duration::from_secs(10));
    cleanup(master, child);
    all.extend_from_slice(&out); // assertions read the WHOLE transcript
    let s = String::from_utf8_lossy(&all).into_owned();
    assert!(done, "probe incomplete: {s}");
    // The alias OPENED — the controlling terminal exists by construction,
    // eliminating "harness had no ctty" as an explanation.
    assert!(s.contains("alias_open=ok"), "ctty missing?! {s}");
    // The fix's load-bearing property on EVERY unix: a real tty fd
    // resolves to a real device path that polls readable and delivers
    // the byte.
    assert!(s.contains("real_poll") && s.contains("pollin=true"), "{s}");
    assert!(s.contains("real_read=Z"), "{s}");
    // The Darwin-specific verdict halves, pinned only on macOS (Linux
    // polls the alias fine; if Apple ever fixes either, these asserts
    // tell us to simplify):
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // (1) the alias is not pollable even with the byte queued and a
        // perfect ctty — the RT5-1 root cause;
        assert!(
            s.contains("alias_poll") && s.contains("nval=true"),
            "expected the Darwin /dev/tty POLLNVAL behavior: {s}"
        );
        // (2) resolving the ALIAS is circular (answers "/dev/tty"), which
        // is why acquisition resolves from std fds instead.
        assert!(
            s.contains("alias_resolves_to=/dev/tty"),
            "expected the circular alias resolution: {s}"
        );
    }
}

/// The engine proof: `UnixTerminal::new()` + enter + read delivers a
/// keystroke under a proper ctty (the exact path every real terminal
/// runs). This is the kernel-side twin of REDTEAM's RT5-1 acceptance.
#[test]
fn engine_keystrokes_flow_on_ctty_path() {
    let (master, child) = match spawn_child_test("term::rt5_live_tests::rt5_child_engine_read") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("skipping: cannot spawn pty child ({e})");
            return;
        }
    };
    let (ready, mut all) = read_master_until(master, b"ENGINE: READY", Duration::from_secs(10));
    if !ready {
        cleanup(master, child);
        panic!(
            "engine child never ready: {}",
            String::from_utf8_lossy(&all)
        );
    }
    write_master(master, b"x");
    let (done, out) = read_master_until(master, b"ENGINE: DONE", Duration::from_secs(10));
    cleanup(master, child);
    all.extend_from_slice(&out);
    let s = String::from_utf8_lossy(&all).into_owned();
    assert!(done, "engine probe incomplete: {s}");
    assert!(
        s.contains("ENGINE: got=x"),
        "keystroke did not reach the engine on the ctty path: {s}"
    );
    // The fix makes the PRIMARY path healthy: no degradation label.
    assert!(s.contains("ENGINE: degraded=none"), "{s}");
}
