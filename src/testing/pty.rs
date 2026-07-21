//! Real-PTY process harness for live end-to-end smoke tests: spawn a
//! binary with a genuine pseudo-terminal on stdin/stdout/stderr, feed it
//! keys, capture everything it writes, enforce deadlines, kill on hang.
//!
//! OWNER: REDTEAM.
//!
//! ## Sanctioned `unsafe` (cycle-5 integrator order)
//!
//! This file is the ONE approved home for `unsafe` outside `term`'s FFI
//! boundary. Scope of every unsafe block, with safety arguments inline:
//!
//! 1. `libc::openpty` — plain FFI into a well-specified libc call with
//!    out-pointers to stack locals we own.
//! 2. `Stdio::from_raw_fd(dup(slave))` — each `Stdio` receives its OWN
//!    dup'd descriptor (unique ownership; no double-close).
//! 3. `Command::pre_exec(setsid + TIOCSCTTY)` — runs post-fork/pre-exec
//!    in the child; calls only async-signal-safe syscalls (`setsid`,
//!    `ioctl`), touches no allocator, no locks.
//!
//! Unix-only (`#[cfg(unix)]`), like the platform terminal it exercises.

#![cfg(unix)]

use std::io::Read;
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A child process running under a real PTY.
pub struct PtyProcess {
    child: Child,
    /// Master side as a `File` (owned; closes on drop). `None` after
    /// [`PtyProcess::kill`] released it to unblock the child's exit.
    master: Option<std::fs::File>,
    /// Slave device path (diagnostics: termios inspection from outside).
    slave_path: String,
    /// Everything read from the master so far.
    pub captured: Vec<u8>,
}

/// Snapshot of the line discipline on the pty slave, read from OUTSIDE
/// the child — diagnoses "keys don't arrive" (canonical mode? flow
/// stopped?) without touching the child.
#[derive(Debug, Clone, Copy)]
pub struct TtyState {
    pub icanon: bool,
    pub echo: bool,
    pub ixon: bool,
    pub vmin: u8,
    pub vtime: u8,
}

/// Spawn `program` (with `args`) under a fresh PTY of `cols` x `rows`,
/// with a terminal-ish env (`TERM=xterm-256color`, `COLORTERM=truecolor`)
/// plus `extra_env`. The PTY becomes the child's CONTROLLING terminal
/// (like a real terminal emulator would arrange).
pub fn spawn_in_pty(
    program: &str,
    args: &[&str],
    cols: u16,
    rows: u16,
    extra_env: &[(&str, &str)],
) -> std::io::Result<PtyProcess> {
    spawn_in_pty_opts(program, args, cols, rows, extra_env, true)
}

/// [`spawn_in_pty`] with the controlling-terminal step optional.
/// `ctty=false` gives the child a session with NO controlling terminal:
/// `open("/dev/tty")` fails and the engine's stdin/stdout-as-tty
/// fallback engages — the lever that isolates /dev/tty-specific bugs.
pub fn spawn_in_pty_opts(
    program: &str,
    args: &[&str],
    cols: u16,
    rows: u16,
    extra_env: &[(&str, &str)],
    ctty: bool,
) -> std::io::Result<PtyProcess> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut winsize = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: cols.saturating_mul(8),
        ws_ypixel: rows.saturating_mul(16),
    };
    // SAFETY (1): openpty writes two fds into our stack locals and reads
    // the winsize we constructed (the *mut signature never mutates it);
    // NULL name/termios are documented-legal.
    // The winsize parameter is `*mut` on macOS/BSD libc but `*const` on
    // Linux glibc; `&mut` satisfies both signatures, so the Linux-only
    // "unnecessary mut" lint is silenced rather than obeyed.
    #[allow(clippy::unnecessary_mut_passed)]
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut winsize,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY (2): dup() mints an independent descriptor for each Stdio;
    // each from_raw_fd takes unique ownership of its own fd. The original
    // `slave` fd is closed explicitly after the dups (the child holds its
    // three copies; the parent must not keep the slave open or EOF on the
    // master never arrives after child exit). The MASTER is marked
    // CLOEXEC immediately: openpty fds are inheritable by default, and a
    // leaked master copy in the child keeps the pty session alive after
    // the parent closes its side (observed as unkillable EXITING
    // children on macOS).
    let (stdin, stdout, stderr) = unsafe {
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        let s0 = libc::dup(slave);
        let s1 = libc::dup(slave);
        if s0 < 0 || s1 < 0 {
            return Err(std::io::Error::last_os_error());
        }
        (
            Stdio::from_raw_fd(slave),
            Stdio::from_raw_fd(s0),
            Stdio::from_raw_fd(s1),
        )
    };

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env_remove("ABSTRACTTUI_NO_SPLASH")
        .env_remove("NO_COLOR");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    // SAFETY (3): the closure runs in the forked child before exec. It
    // calls only async-signal-safe functions (setsid, ioctl) and never
    // allocates. Making the PTY the CONTROLLING terminal means the
    // engine's /dev/tty open works exactly as in a real terminal.
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // 0 = stdin, which IS the pty slave in the child.
            if ctty && libc::ioctl(0, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;

    // Parent side: non-blocking master reads (poll-with-deadline below).
    // SAFETY: fcntl/ptsname on an fd we own; from_raw_fd takes unique
    // ownership of `master` (nothing else closes it). ptsname's static
    // buffer is copied out immediately.
    let (master_file, slave_path) = unsafe {
        let flags = libc::fcntl(master, libc::F_GETFL);
        libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
        let name = libc::ptsname(master);
        let path = if name.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(name)
                .to_string_lossy()
                .into_owned()
        };
        (std::fs::File::from_raw_fd(master), path)
    };

    Ok(PtyProcess {
        child,
        master: Some(master_file),
        slave_path,
        captured: Vec::new(),
    })
}

impl PtyProcess {
    /// Read whatever arrives on the master for `window` (poll slices);
    /// returns bytes newly captured in this window.
    pub fn read_for(&mut self, window: Duration) -> usize {
        let Some(master) = self.master.as_mut() else {
            return 0;
        };
        let start = Instant::now();
        let before = self.captured.len();
        let mut buf = [0u8; 8192];
        while start.elapsed() < window {
            match master.read(&mut buf) {
                Ok(0) => break, // EOF: child side closed
                Ok(n) => self.captured.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break, // EIO after child exit on some platforms
            }
        }
        self.captured.len() - before
    }

    /// Write bytes to the child's terminal input (keystrokes).
    pub fn send(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let Some(master) = self.master.as_mut() else {
            return;
        };
        let _ = master.write_all(bytes);
        let _ = master.flush();
    }

    /// Wait for exit up to `deadline`, reading output while waiting.
    /// Returns the exit code, or None on timeout (child still running).
    pub fn wait_with_deadline(&mut self, deadline: Duration) -> Option<i32> {
        let start = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                // Drain any final bytes still buffered in the pty.
                self.read_for(Duration::from_millis(120));
                return status.code();
            }
            if start.elapsed() >= deadline {
                return None;
            }
            self.read_for(Duration::from_millis(60));
        }
    }

    /// Bytes sitting UNREAD in the slave's input queue (diagnostics):
    /// distinguishes "child never reads its tty" (queue grows) from
    /// "child read and ignored" (queue drains).
    pub fn slave_pending_input(&self) -> Option<usize> {
        if self.slave_path.is_empty() {
            return None;
        }
        let c_path = std::ffi::CString::new(self.slave_path.as_str()).ok()?;
        // SAFETY: open/ioctl(FIONREAD)/close on our own diagnostic fd;
        // never reads, so it cannot steal the child's input.
        unsafe {
            let fd = libc::open(
                c_path.as_ptr(),
                libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK,
            );
            if fd < 0 {
                return None;
            }
            let mut n: libc::c_int = 0;
            let rc = libc::ioctl(fd, libc::FIONREAD, &mut n);
            libc::close(fd);
            if rc != 0 {
                return None;
            }
            Some(n as usize)
        }
    }

    /// Line-discipline snapshot of the slave (diagnostics only).
    pub fn tty_state(&self) -> Option<TtyState> {
        if self.slave_path.is_empty() {
            return None;
        }
        let c_path = std::ffi::CString::new(self.slave_path.as_str()).ok()?;
        // SAFETY: open/tcgetattr/close on a path we minted from ptsname;
        // O_NOCTTY so this diagnostic never steals the controlling tty.
        unsafe {
            let fd = libc::open(
                c_path.as_ptr(),
                libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK,
            );
            if fd < 0 {
                return None;
            }
            let mut tio: libc::termios = std::mem::zeroed();
            let rc = libc::tcgetattr(fd, &mut tio);
            libc::close(fd);
            if rc != 0 {
                return None;
            }
            Some(TtyState {
                icanon: tio.c_lflag & libc::ICANON != 0,
                echo: tio.c_lflag & libc::ECHO != 0,
                ixon: tio.c_iflag & libc::IXON != 0,
                vmin: tio.c_cc[libc::VMIN] as u8,
                vtime: tio.c_cc[libc::VTIME] as u8,
            })
        }
    }

    /// Hard-kill (used on hang; the test then fails loudly).
    ///
    /// PTY teardown trap (observed live on macOS): a SIGKILL'd session
    /// leader can wedge in the EXITING state (`ps` shows `(name)` with
    /// state `E`) until its pty output queue drains — a blocking
    /// `wait()` right after `kill()` then deadlocks the TEST. So: kill,
    /// drain-while-reaping, and if it still hasn't exited, CLOSE the
    /// master (releases the queue) before the final blocking wait.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        for _ in 0..50 {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            self.read_for(Duration::from_millis(20));
        }
        self.master = None; // close the master: unblocks tty teardown
        let _ = self.child.wait();
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        // Never leak a hung example process past the test.
        if matches!(self.child.try_wait(), Ok(None) | Err(_)) {
            self.kill();
        }
    }
}
