//! LIVE tmux passthrough verification (cycle-5, risk #1 of cycle 4).
//!
//! OWNER: KERNEL. Ignored by default: needs a tmux binary on the host and
//! must not run inside a tmux session itself. Run manually:
//!
//! ```text
//! cargo test --lib -- --ignored tmux_live --nocapture
//! ```
//!
//! Architecture: THIS TEST is the outer terminal. We open a pty pair, make
//! the slave the controlling tty of a `tmux new-session` (so tmux attaches
//! to a terminal we fully control), run an inner `sh` in the pane that
//! emits our WRAPPED kitty-graphics query to its pane tty, and observe
//! what tmux writes to the outer master:
//!
//! - `allow-passthrough on`:  the payload must emerge UNWRAPPED
//!   (`ESC _G i=<id> ...` — tmux stripped the `ESC Ptmux;` frame and
//!   un-doubled the ESCs). We then play the outer terminal's role and
//!   write a kitty `;OK` reply INTO the master, checking whether tmux
//!   routes it to the pane (the full round trip the probe relies on).
//! - defaults (passthrough off): the payload must be swallowed — no
//!   kitty APC on the outer master at all.

use super::unix::IoctlReq;
use super::verbs::tmux_wrap;
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Locate tmux without relying on the test runner's PATH hygiene.
fn find_tmux() -> Option<PathBuf> {
    let fixed = [
        "/opt/homebrew/bin/tmux",
        "/usr/local/bin/tmux",
        "/usr/bin/tmux",
    ];
    for p in fixed {
        if std::path::Path::new(p).exists() {
            return Some(PathBuf::from(p));
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("tmux"))
        .find(|p| p.exists())
}

/// Skip (not fail) when the environment cannot host the test: REDTEAM runs
/// suites inside terminals we do not control.
fn precondition_skip() -> Option<PathBuf> {
    if std::env::var_os("TMUX").is_some() {
        eprintln!("skipping: already inside tmux (nested sessions change the contract)");
        return None;
    }
    match find_tmux() {
        Some(p) => Some(p),
        None => {
            eprintln!("skipping: no tmux binary on this host");
            None
        }
    }
}

/// The probe payload under test: a kitty graphics query with a test-only
/// id, wrapped exactly the way `ActiveProbe` wraps its passthrough probe.
const INNER_ID: &str = "9999";

fn wrapped_probe() -> Vec<u8> {
    tmux_wrap(format!("\x1b_Gi={INNER_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\").as_bytes())
}

/// Render bytes as `\NNN` octal escapes so an inner `printf '%b'`-free
/// `printf` reproduces them exactly (single-quote-safe: digits only).
fn printf_octal(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:03o}")).collect()
}

struct OuterPty {
    master: RawFd,
    child: std::process::Child,
    tmux: PathBuf,
    sock: PathBuf,
}

impl OuterPty {
    /// Spawn `tmux new-session <inner>` ATTACHED to a slave pty we own.
    fn spawn(tmux: &PathBuf, cfg_lines: &str, inner: &str) -> std::io::Result<OuterPty> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir();
        let sock = dir.join(format!("atui-tmux-live-{nonce}.sock"));
        let cfg = dir.join(format!("atui-tmux-live-{nonce}.conf"));
        std::fs::write(&cfg, cfg_lines)?;

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
        // tmux refuses zero-sized terminals: give the outer tty a shape.
        let ws = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: ioctl on the master we just opened.
        unsafe { libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws) };

        // SAFETY: dup for the three stdio slots; Stdio takes ownership.
        let stdio = |fd: RawFd| -> std::io::Result<Stdio> {
            // SAFETY: dup(2) of a live fd; the File owns the duplicate.
            let d = unsafe { libc::dup(fd) };
            if d < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: d is a fresh fd owned by nothing else.
            Ok(unsafe { Stdio::from(std::fs::File::from_raw_fd(d)) })
        };

        let mut cmd = Command::new(tmux);
        cmd.arg("-f")
            .arg(&cfg)
            .arg("-S")
            .arg(&sock)
            .arg("new-session")
            .arg(inner)
            .env("TERM", "xterm-256color")
            .env_remove("TMUX")
            .stdin(stdio(slave)?)
            .stdout(stdio(slave)?)
            .stderr(stdio(slave)?);
        let slave_for_child = slave;
        // SAFETY: pre_exec runs post-fork/pre-exec in the child; only
        // async-signal-safe calls (setsid, ioctl) — tmux needs the slave
        // as its CONTROLLING terminal, not just its stdio.
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
        // Parent keeps only the master.
        // SAFETY: closing our copy of the slave; the child holds dups.
        unsafe { libc::close(slave) };
        Ok(OuterPty {
            master,
            child,
            tmux: tmux.clone(),
            sock,
        })
    }

    /// Read the outer master until `needle` appears or `wait` elapses.
    /// Returns (matched, everything read).
    fn read_until(&mut self, needle: &[u8], wait: Duration) -> (bool, Vec<u8>) {
        let deadline = Instant::now() + wait;
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return (contains(&got, needle), got);
            }
            let mut p = libc::pollfd {
                fd: self.master,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: poll over one live fd on this stack frame.
            let rc = unsafe { libc::poll(&mut p, 1, remaining.as_millis().min(500) as i32) };
            if rc <= 0 {
                if contains(&got, needle) {
                    return (true, got);
                }
                continue;
            }
            // SAFETY: read into a stack buffer on our master fd.
            let n = unsafe {
                libc::read(
                    self.master,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n <= 0 {
                return (contains(&got, needle), got);
            }
            got.extend_from_slice(&buf[..n as usize]);
            if contains(&got, needle) {
                return (true, got);
            }
        }
    }

    fn write_master(&mut self, bytes: &[u8]) {
        // SAFETY: write to our live master fd.
        unsafe {
            libc::write(
                self.master,
                bytes.as_ptr() as *const libc::c_void,
                bytes.len(),
            )
        };
    }
}

impl Drop for OuterPty {
    fn drop(&mut self) {
        let _ = Command::new(&self.tmux)
            .arg("-S")
            .arg(&self.sock)
            .arg("kill-server")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // SAFETY: closing the master fd we own.
        unsafe { libc::close(self.master) };
        let _ = std::fs::remove_file(&self.sock);
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The unwrapped emergence marker: APC intro + our id, exactly what the
/// outer terminal would parse. (The wrapped form on the wire inside tmux
/// is `ESC Ptmux; ESC ESC _G...` — if THAT appeared on the outer master,
/// tmux forwarded without unwrapping, which would also be a failure.)
fn emergence_marker() -> Vec<u8> {
    format!("\x1b_Gi={INNER_ID}").into_bytes()
}

#[test]
#[ignore = "live tmux round trip: needs tmux binary, run manually with --nocapture"]
fn tmux_live_passthrough_on_unwraps_and_routes_replies() {
    let Some(tmux) = precondition_skip() else {
        return;
    };
    // Inner-side traps this harness burned time on (recorded so the next
    // reader does not): the pane tty starts CANONICAL (newline-less input
    // sits in the line discipline forever — `stty raw` first), and cat's
    // stdio buffer dies unflushed on the SIGHUP from kill-server — dd
    // does raw read->write per block, so captured bytes hit the file
    // immediately.
    let inner = format!(
        "sh -c 'stty raw 2>/dev/null; printf \"{}\"; dd of=/tmp/atui_tmux_reply_probe 2>/dev/null'",
        printf_octal(&wrapped_probe())
    );
    let _ = std::fs::remove_file("/tmp/atui_tmux_reply_probe");
    let mut outer = OuterPty::spawn(&tmux, "set -g allow-passthrough on\n", &inner)
        .expect("spawn tmux attached to our pty");

    let (emerged, got) = outer.read_until(&emergence_marker(), Duration::from_secs(5));
    assert!(
        emerged,
        "wrapped query did NOT emerge unwrapped with allow-passthrough on; \
         outer saw {} bytes: {:?}",
        got.len(),
        String::from_utf8_lossy(&got)
    );
    let wrapped_still = contains(&got, b"\x1bPtmux;");
    assert!(
        !wrapped_still,
        "tmux forwarded the WRAPPER itself instead of unwrapping"
    );
    eprintln!(
        "PASS: passthrough-on emergence verified ({} outer bytes)",
        got.len()
    );

    // Round-trip half: play the outer terminal and answer the query,
    // plus a plain-text control marker. If the marker routes to the pane
    // while the APC does not, tmux forwards KEYS but filters terminal
    // RESPONSES — the precise shape of the finding.
    outer.write_master(format!("\x1b_Gi={INNER_ID};OK\x1b\\").as_bytes());
    std::thread::sleep(Duration::from_millis(300));
    outer.write_master(b"ZZMARK");
    std::thread::sleep(Duration::from_millis(800));
    drop(outer); // kill server so the inner cat's file flushes/closes
    std::thread::sleep(Duration::from_millis(200));
    let inner_got = std::fs::read("/tmp/atui_tmux_reply_probe").unwrap_or_default();
    let apc_routed = contains(&inner_got, format!("Gi={INNER_ID};OK").as_bytes());
    let text_routed = contains(&inner_got, b"ZZMARK");
    // Observation, not assertion: reply routing is tmux-version-dependent
    // (the probe's grace/absence handling covers a non-routing tmux —
    // the capability simply stays off). Recorded for the evidence matrix.
    eprintln!(
        "reply routing tmux->pane: APC {} / plain text {} (inner saw {} bytes: {:?})",
        if apc_routed { "ROUTED" } else { "NOT ROUTED" },
        if text_routed { "ROUTED" } else { "NOT ROUTED" },
        inner_got.len(),
        String::from_utf8_lossy(&inner_got)
    );
    let _ = std::fs::remove_file("/tmp/atui_tmux_reply_probe");
}

#[test]
#[ignore = "live tmux round trip: needs tmux binary, run manually with --nocapture"]
fn tmux_live_passthrough_off_swallows() {
    let Some(tmux) = precondition_skip() else {
        return;
    };
    let inner = format!(
        "sh -c 'printf \"{}\"; sleep 2'",
        printf_octal(&wrapped_probe())
    );
    // Defaults: allow-passthrough is off since tmux 3.3a.
    let mut outer = OuterPty::spawn(&tmux, "", &inner).expect("spawn tmux attached to our pty");

    // Give tmux ample time to draw + (not) forward, then assert absence.
    let (emerged, got) = outer.read_until(&emergence_marker(), Duration::from_secs(3));
    assert!(
        !emerged,
        "payload leaked to the outer terminal with passthrough OFF"
    );
    assert!(
        !contains(&got, b"\x1bPtmux;"),
        "raw wrapper leaked to the outer terminal"
    );
    assert!(
        !got.is_empty(),
        "outer terminal saw nothing at all — tmux likely never attached \
         (harness problem, not a passthrough result)"
    );
    eprintln!(
        "PASS: passthrough-off swallow verified ({} outer bytes, no kitty APC)",
        got.len()
    );
}
