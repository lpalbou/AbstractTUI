//! [`UnixTerminal`] construction + session plumbing: tty acquisition,
//! termios raw-mode entry, the SIGWINCH self-pipe, and ioctl size
//! ground truth. `#[path]` child of unix.rs (file-size split) — the
//! `Terminal` trait impl stays in the parent; `pub(super)` = exactly
//! the old private-in-module audience.
//!
//! OWNER: KERNEL.

use std::mem;
use std::os::unix::io::RawFd;
use std::sync::Arc;

use crate::base::{Error, Result, Size};

use super::super::waker::TerminalWaker;
use super::sys::{io_err, set_cloexec_nonblock};
use super::{open_named_tty, IoctlReq, UnixTerminal, WakeFd, READ_BUF_LEN};

impl UnixTerminal {
    /// Open the controlling terminal. Prefers `/dev/tty` so apps keep their
    /// interactive terminal when stdin/stdout are pipes (`echo x | app`),
    /// falling back to stdin/stdout when both are ttys (some sandboxes have
    /// no `/dev/tty`).
    /// Acquisition policy (rewritten for RT5-1, live-proven order):
    ///
    /// 1. stdin+stdout both ttys → use them directly. They are REAL
    ///    device fds (`/dev/ttysNNN`), pollable on every unix — unlike
    ///    the `/dev/tty` ALIAS, which Darwin's poll(2) rejects with
    ///    POLLNVAL even under a perfect controlling terminal
    ///    (`rt5_live_tests` pins it). This is how every real terminal
    ///    launches apps, so the common path never touches the alias.
    /// 2. ANY std fd is a tty (pipes on the others) → resolve THAT fd's
    ///    real device via ttyname_r and open it fresh. Resolving a real
    ///    fd yields the true `/dev/ttysNNN` path; resolving the alias
    ///    does NOT (Darwin answers the literal string "/dev/tty" —
    ///    live-proven), which is why resolution starts from std fds.
    /// 3. `/dev/tty` alias as the last resort (all three std fds
    ///    redirected but a controlling terminal exists). Pollable on
    ///    Linux; on Darwin the read loop's POLLNVAL guard makes the
    ///    failure loud or recovers via stdin — never silent.
    pub fn new() -> Result<Self> {
        // SAFETY: isatty on the standard descriptors.
        let (in_tty, out_tty, err_tty) = unsafe {
            (
                libc::isatty(libc::STDIN_FILENO) == 1,
                libc::isatty(libc::STDOUT_FILENO) == 1,
                libc::isatty(libc::STDERR_FILENO) == 1,
            )
        };
        if in_tty && out_tty {
            return Ok(Self::from_fds(
                libc::STDIN_FILENO,
                libc::STDOUT_FILENO,
                false,
            ));
        }
        for (fd, is_tty) in [
            (libc::STDIN_FILENO, in_tty),
            (libc::STDOUT_FILENO, out_tty),
            (libc::STDERR_FILENO, err_tty),
        ] {
            if !is_tty {
                continue;
            }
            if let Some(real) = open_named_tty(fd) {
                return Ok(Self::from_fds(real, real, true));
            }
        }
        // SAFETY: open(2) with a static NUL-terminated path.
        let alias = unsafe { libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if alias >= 0 {
            return Ok(Self::from_fds(alias, alias, true));
        }
        Err(Error::Term(
            "no terminal attached: stdin/stdout/stderr are all redirected and \
             /dev/tty is unavailable — run inside a terminal emulator, or use \
             testing::CaptureTerm for headless/CI runs"
                .into(),
        ))
    }

    /// Build over explicit descriptors (pty tests, embedders). `owns_fds`
    /// closes them on drop (a shared fd is closed once).
    pub fn from_fds(read_fd: RawFd, write_fd: RawFd, owns_fds: bool) -> Self {
        let (wake_rd, waker) = Self::make_wake_pipe();
        UnixTerminal {
            read_fd,
            write_fd,
            owns_fds,
            entered: None,
            out: Vec::with_capacity(8192),
            in_buf: vec![0; READ_BUF_LEN],
            seen_size: Size::ZERO,
            wake_rd,
            waker,
            cursor_styled: false,
            title_pushed: false,
            pixel_moused: false,
            degraded: None,
        }
    }

    /// Labeled degradation state (RT5-1 hardening): `Some(reason)` after
    /// the read loop had to fall back from a non-pollable terminal fd.
    /// Also served through `Terminal::degraded` for `dyn` consumers.
    pub fn degraded(&self) -> Option<&'static str> {
        self.degraded
    }

    /// The job-control stop itself, isolated so tests can exercise the
    /// suspend/resume BYTE ORDER without stopping the test runner's whole
    /// process group (which `kill(0, …)` deliberately targets — the same
    /// group the tty driver stops on a real Ctrl+Z, so pipeline siblings
    /// stop coherently).
    #[cfg(not(test))]
    pub(super) fn deliver_stop() {
        // SAFETY: sending SIGTSTP (default action: stop) to our own
        // process group; execution resumes after SIGCONT.
        unsafe { libc::kill(0, libc::SIGTSTP) };
    }
    #[cfg(test)]
    pub(super) fn deliver_stop() {}

    /// Per-instance wake channel (REACT loop primitive). Distinct from the
    /// process-global SIGWINCH pipe: wakes are per-terminal and must work
    /// even when another instance owns the signal claim.
    pub(super) fn make_wake_pipe() -> (Option<RawFd>, Option<TerminalWaker>) {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: pipe(2) writing into a stack array of exactly 2 ints.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return (None, None);
        }
        if set_cloexec_nonblock(fds[0]).is_err() || set_cloexec_nonblock(fds[1]).is_err() {
            // SAFETY: closing the fds we just created.
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return (None, None);
        }
        let wr = Arc::new(WakeFd(fds[1]));
        let waker = TerminalWaker::new(move || {
            let byte = 1u8;
            // SAFETY: one-byte write to the nonblocking pipe fd the Arc
            // keeps alive; EAGAIN on a full pipe means a wake is already
            // pending — exactly the coalescing contract.
            unsafe { libc::write(wr.0, &byte as *const u8 as *const libc::c_void, 1) };
        });
        (Some(fds[0]), Some(waker))
    }

    pub(super) fn raw_winsize(&self) -> Result<libc::winsize> {
        // SAFETY: TIOCGWINSZ fills a winsize struct; zeroed is a valid init.
        let mut ws: libc::winsize = unsafe { mem::zeroed() };
        // SAFETY: ioctl on our tty fd writing into the struct above.
        let rc = unsafe { libc::ioctl(self.write_fd, libc::TIOCGWINSZ as IoctlReq, &mut ws) };
        if rc != 0 {
            return Err(io_err("ioctl(TIOCGWINSZ)"));
        }
        Ok(ws)
    }

    pub(super) fn ioctl_size(&self) -> Result<Size> {
        let ws = self.raw_winsize()?;
        Ok(Size::new(ws.ws_col as i32, ws.ws_row as i32))
    }

    /// Compare fresh geometry against the last size delivered through
    /// `read()`. The ioctl is the ground truth (signals may coalesce or be
    /// lost — notcurses' documented posture); the pipe is only a wakeup.
    pub(super) fn check_resize(&mut self) -> Option<Size> {
        let now = self.ioctl_size().ok()?;
        if now != self.seen_size && !now.is_empty() {
            self.seen_size = now;
            Some(now)
        } else {
            None
        }
    }

    pub(super) fn apply_raw_mode(&mut self) -> Result<libc::termios> {
        // SAFETY: termios is POD; tcgetattr fills it completely on success.
        let mut saved: libc::termios = unsafe { mem::zeroed() };
        // SAFETY: tcgetattr on our tty fd.
        if unsafe { libc::tcgetattr(self.read_fd, &mut saved) } != 0 {
            return Err(io_err("tcgetattr"));
        }
        let mut raw = saved;
        // cfmakeraw semantics, spelled out (see design doc §1.1):
        // input: no break-to-SIGINT, no CR<->NL mangling, no parity strip,
        // no XON/XOFF flow control (Ctrl+S/Q become ordinary keys).
        raw.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);
        // output: no post-processing (renderer emits \r\n itself).
        raw.c_oflag &= !libc::OPOST;
        // local: no echo, no canonical lines, no signal keys (Ctrl+C is an
        // event, the app decides), no extended processing.
        raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG | libc::IEXTEN);
        // 8-bit characters, no parity.
        raw.c_cflag &= !(libc::CSIZE | libc::PARENB);
        raw.c_cflag |= libc::CS8;
        // Reads are gated by poll(2): VMIN=0/VTIME=0 so a spurious poll wake
        // can never park us inside read(2).
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        // SAFETY: applying a termios derived from the one just read.
        if unsafe { libc::tcsetattr(self.read_fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io_err("tcsetattr(raw)"));
        }
        Ok(saved)
    }

    pub(super) fn drain_winch_pipe(fd: RawFd) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: nonblocking read into a stack buffer; loop ends on
            // EAGAIN (empty) or any error.
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n <= 0 {
                break;
            }
        }
    }
}
