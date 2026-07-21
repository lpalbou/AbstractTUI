//! Unix terminal backend: /dev/tty, termios raw mode, poll(2) reads,
//! SIGWINCH self-pipe resize wakeups with ioctl ground truth.
//!
//! OWNER: KERNEL. Rationale: `docs/design/term-input.md` §1.
//!
//! `unsafe` policy: every block is a single FFI call (or reads FFI-owned
//! memory) with a SAFETY note. No unsafe outside this boundary.

use super::options::EnterOptions;
use super::verbs::{self, CursorStyle};
use super::waker::TerminalWaker;
use super::{TermRead, Terminal};
use crate::base::{Error, PixelSize, Result, Size};
use std::mem;
use std::os::unix::io::RawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Process-global machinery (SIGWINCH claim, emergency restore, errno,
// fd helpers) lives in a sibling file; the path attribute keeps it a
// child module so the split is invisible to consumers.
#[path = "unix_sys.rs"]
mod sys;
pub use sys::emergency_restore;
use sys::{
    claim_winch, io_err, last_errno, release_winch, set_cloexec_nonblock, write_all_fd,
    EmergencySlot, WinchClaim, EMERGENCY,
};
// Re-exported for term-level test harnesses (the live tmux rig does its
// own ioctls); the alias itself lives in unix_sys.
pub(crate) use sys::IoctlReq;
// (sys::append_emergency_leave is called by the verb overrides below.)

const READ_BUF_LEN: usize = 4096;
/// Poll slice when we could not claim the SIGWINCH handler: resize is then
/// detected by comparing TIOCGWINSZ on each wake, so the wake cadence bounds
/// resize latency. Only this degraded path spends idle wakeups.
const NO_SIGNAL_SLICE: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// The terminal.
// ---------------------------------------------------------------------------

struct EnteredState {
    saved_termios: libc::termios,
    opts: EnterOptions,
    winch: Option<WinchClaim>,
}

/// Write end of the per-instance wake pipe. Held inside the waker's `Arc`
/// closure: the fd stays open as long as ANY waker clone lives, so a waker
/// outliving its terminal writes into a reader-less pipe (harmless, at
/// worst EAGAIN when full) instead of racing fd-number reuse.
struct WakeFd(RawFd);

impl Drop for WakeFd {
    fn drop(&mut self) {
        // SAFETY: closing the write fd this wrapper exclusively owns.
        unsafe { libc::close(self.0) };
    }
}

/// Open the REAL terminal device behind a tty fd (`ttyname_r` -> fresh
/// O_RDWR open). `None` when the name cannot be resolved/opened, or when
/// it resolves to the un-pollable "/dev/tty" alias string (Darwin answers
/// that for alias fds — a real fd resolves to `/dev/ttysNNN`).
fn open_named_tty(tty_fd: RawFd) -> Option<RawFd> {
    let mut buf = [0 as libc::c_char; 1024];
    // SAFETY: ttyname_r writes a NUL-terminated path into our buffer on
    // success (returns 0); the fd is live.
    if unsafe { libc::ttyname_r(tty_fd, buf.as_mut_ptr(), buf.len()) } != 0 {
        return None;
    }
    // SAFETY: reading the NUL-terminated string ttyname_r produced.
    let name = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
    if name.to_bytes() == b"/dev/tty" {
        return None; // the alias again: not a resolution
    }
    // SAFETY: opening the path ttyname_r just produced.
    let real = unsafe { libc::open(buf.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if real < 0 {
        return None;
    }
    // The resolved fd must still be a terminal.
    // SAFETY: isatty on the fd we just opened.
    if unsafe { libc::isatty(real) } != 1 {
        // SAFETY: closing the fd we just opened.
        unsafe { libc::close(real) };
        return None;
    }
    Some(real)
}

/// True when an interactive terminal is reachable — the same acquisition
/// test `UnixTerminal::new()` performs, without keeping the fd. This is
/// the question boot's auto-skip must ask (DESIGN request 10 / RT1-10c:
/// `isatty(stdout)` is the wrong question, the engine renders to
/// `/dev/tty` even when stdout is a pipe).
pub fn have_tty() -> bool {
    // SAFETY: open(2) with a static NUL-terminated path; closed just below.
    let fd = unsafe { libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if fd >= 0 {
        // SAFETY: closing the fd we just opened.
        unsafe { libc::close(fd) };
        return true;
    }
    // SAFETY: isatty on the standard descriptors.
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 && libc::isatty(libc::STDOUT_FILENO) == 1 }
}

/// The unix terminal backend (macOS/Linux): real device fds, manual
/// termios raw mode, poll(2)-gated reads, SIGWINCH self-pipe, per
/// instance wake pipe. Construct via [`UnixTerminal::new`] (acquisition
/// policy documented there) or [`UnixTerminal::from_fds`] (ptys, tests).
pub struct UnixTerminal {
    read_fd: RawFd,
    write_fd: RawFd,
    owns_fds: bool,
    entered: Option<EnteredState>,
    out: Vec<u8>,
    in_buf: Vec<u8>,
    /// Size cache for resize *detection* only. `size()` never reads it, so
    /// an app calling `size()` cannot swallow a pending Resize event.
    seen_size: Size,
    /// Read end of the wake pipe (`None` if pipe creation failed at
    /// construction — then `waker()` is honestly `None` too).
    wake_rd: Option<RawFd>,
    waker: Option<TerminalWaker>,
    /// A non-default DECSCUSR was emitted: leave restores `Ps 0`.
    cursor_styled: bool,
    /// The title stack was pushed before our first title: leave pops it.
    title_pushed: bool,
    /// SGR-Pixels (1016) is on: leave turns it off.
    pixel_moused: bool,
    /// Labeled degradation, set when the read path had to abandon a
    /// non-pollable fd (POLLNVAL) for a stdin fallback. `None` = healthy
    /// primary path. Apps may surface it; the engine never prints.
    degraded: Option<&'static str>,
}

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
    fn deliver_stop() {
        // SAFETY: sending SIGTSTP (default action: stop) to our own
        // process group; execution resumes after SIGCONT.
        unsafe { libc::kill(0, libc::SIGTSTP) };
    }
    #[cfg(test)]
    fn deliver_stop() {}

    /// Per-instance wake channel (REACT loop primitive). Distinct from the
    /// process-global SIGWINCH pipe: wakes are per-terminal and must work
    /// even when another instance owns the signal claim.
    fn make_wake_pipe() -> (Option<RawFd>, Option<TerminalWaker>) {
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

    fn raw_winsize(&self) -> Result<libc::winsize> {
        // SAFETY: TIOCGWINSZ fills a winsize struct; zeroed is a valid init.
        let mut ws: libc::winsize = unsafe { mem::zeroed() };
        // SAFETY: ioctl on our tty fd writing into the struct above.
        let rc = unsafe { libc::ioctl(self.write_fd, libc::TIOCGWINSZ as IoctlReq, &mut ws) };
        if rc != 0 {
            return Err(io_err("ioctl(TIOCGWINSZ)"));
        }
        Ok(ws)
    }

    fn ioctl_size(&self) -> Result<Size> {
        let ws = self.raw_winsize()?;
        Ok(Size::new(ws.ws_col as i32, ws.ws_row as i32))
    }

    /// Compare fresh geometry against the last size delivered through
    /// `read()`. The ioctl is the ground truth (signals may coalesce or be
    /// lost — notcurses' documented posture); the pipe is only a wakeup.
    fn check_resize(&mut self) -> Option<Size> {
        let now = self.ioctl_size().ok()?;
        if now != self.seen_size && !now.is_empty() {
            self.seen_size = now;
            Some(now)
        } else {
            None
        }
    }

    fn apply_raw_mode(&mut self) -> Result<libc::termios> {
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

    fn drain_winch_pipe(fd: RawFd) {
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

impl Terminal for UnixTerminal {
    fn enter(&mut self, opts: &EnterOptions) -> Result<()> {
        if self.entered.is_some() {
            return Ok(());
        }
        let saved = self.apply_raw_mode()?;
        let winch = claim_winch(); // None => degraded poll-slice detection
        self.seen_size = self.ioctl_size().unwrap_or(Size::ZERO);

        // Arm the emergency slot BEFORE emitting mode changes: if the write
        // below panics mid-way, the panic hook can still undo everything.
        if let Ok(mut g) = EMERGENCY.lock() {
            *g = Some(EmergencySlot {
                fd: self.write_fd,
                termios: saved,
                leave_bytes: opts.leave_bytes(),
            });
        }

        self.entered = Some(EnteredState {
            saved_termios: saved,
            opts: *opts,
            winch,
        });
        self.write(&opts.enter_bytes())?;
        self.flush()?;
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        let state = self.entered.take();
        if state.is_none() && !self.cursor_styled && !self.title_pushed && !self.pixel_moused {
            return Ok(());
        }
        // Best-effort all the way down: a failed write must not prevent the
        // termios restore, which is the difference between "ugly screen" and
        // "unusable shell".
        let mut first_err: Option<Error> = None;
        if let Some(s) = &state {
            self.out.extend_from_slice(&s.opts.leave_bytes());
        }
        // Verb resets AFTER the mode teardown: the cursor style and title
        // restore must apply to the main screen the user returns to (some
        // terminals scope cursor shape per screen buffer).
        if self.pixel_moused {
            self.out.extend_from_slice(verbs::PIXEL_MOUSE_OFF);
            self.pixel_moused = false;
        }
        if self.cursor_styled {
            self.out.extend_from_slice(verbs::CURSOR_STYLE_RESET);
            self.cursor_styled = false;
        }
        if self.title_pushed {
            self.out.extend_from_slice(verbs::TITLE_POP);
            self.title_pushed = false;
        }
        if let Err(e) = self.flush() {
            first_err.get_or_insert(e);
        }
        let Some(state) = state else {
            // Verb-only cleanup (styled/titled without a session): no
            // termios/signal state to restore.
            return match first_err {
                Some(e) => Err(e),
                None => Ok(()),
            };
        };
        // SAFETY: restoring the termios captured by enter() on the same fd.
        if unsafe { libc::tcsetattr(self.read_fd, libc::TCSANOW, &state.saved_termios) } != 0 {
            first_err.get_or_insert(io_err("tcsetattr(restore)"));
        }
        if let Some(claim) = state.winch {
            release_winch(claim);
        }
        if let Ok(mut g) = EMERGENCY.lock() {
            *g = None;
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn size(&mut self) -> Result<Size> {
        self.ioctl_size()
    }

    fn read(&mut self, deadline: Option<Instant>) -> Result<TermRead<'_>> {
        let winch_rd = self
            .entered
            .as_ref()
            .and_then(|s| s.winch.as_ref())
            .map(|w| w.pipe_rd);
        loop {
            // Remaining wait for this poll round. Without the signal claim,
            // cap slices so TIOCGWINSZ re-checks bound resize latency.
            // A deadline already in the past yields a zero-timeout poll:
            // one last drain of anything ready right now, then Idle below.
            let now = Instant::now();
            let remaining = deadline.map(|d| d.saturating_duration_since(now));
            let slice = match (remaining, winch_rd) {
                (Some(r), Some(_)) => Some(r),
                (Some(r), None) => Some(r.min(NO_SIGNAL_SLICE)),
                (None, Some(_)) => None,
                (None, None) => Some(NO_SIGNAL_SLICE),
            };
            let timeout_ms: libc::c_int = match slice {
                None => -1,
                Some(d) => d.as_millis().min(i32::MAX as u128) as libc::c_int,
            };

            // POSIX: a negative fd in a pollfd is skipped (revents = 0),
            // so absent channels are simply holes in a fixed 3-slot array:
            // [tty, sigwinch pipe, wake pipe].
            let mut fds = [
                libc::pollfd {
                    fd: self.read_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: winch_rd.unwrap_or(-1),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake_rd.unwrap_or(-1),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: poll over the 3 pollfds living on this stack frame.
            let rc = unsafe { libc::poll(fds.as_mut_ptr(), 3, timeout_ms) };
            if rc < 0 {
                if last_errno() == libc::EINTR {
                    // A signal (possibly SIGWINCH in degraded mode) landed:
                    // re-check geometry, then re-poll with a fresh timeout.
                    if let Some(sz) = self.check_resize() {
                        return Ok(TermRead::Resize(sz));
                    }
                    continue;
                }
                return Err(io_err("poll"));
            }

            // Keyboard-dead is the worst possible failure and must be
            // structurally impossible (RT5-1): a terminal fd the kernel
            // refuses to poll (POLLNVAL — Darwin's /dev/tty alias is the
            // live-proven case) falls back ONCE to stdin-as-tty with a
            // labeled degradation; with no candidate it fails LOUDLY.
            // Silence is the one outcome this branch forbids.
            if fds[0].revents & libc::POLLNVAL != 0 {
                // SAFETY: isatty on the standard input descriptor.
                let stdin_tty = unsafe { libc::isatty(libc::STDIN_FILENO) == 1 };
                if stdin_tty && self.read_fd != libc::STDIN_FILENO {
                    self.read_fd = libc::STDIN_FILENO;
                    self.degraded =
                        Some("terminal fd not pollable (POLLNVAL); reading stdin instead");
                    continue; // re-poll immediately on the fallback fd
                }
                return Err(Error::Term(
                    "terminal read fd is not pollable (POLLNVAL) and no stdin tty \
                     fallback exists — refusing to go keyboard-dead. This is a \
                     terminal-emulator/platform quirk worth reporting (include \
                     your emulator + OS); rerun with stdin attached to the \
                     terminal as a workaround"
                        .into(),
                ));
            }

            if fds[1].revents & libc::POLLIN != 0 {
                Self::drain_winch_pipe(fds[1].fd);
            }
            // Geometry check on EVERY wake (pipe, input, or timeout): the
            // ioctl is one cheap syscall and closes coalesced/lost-signal
            // races. Resize outranks everything: renderers want the new
            // geometry before processing whatever else queued up.
            if let Some(sz) = self.check_resize() {
                return Ok(TermRead::Resize(sz));
            }

            // Input outranks Wake (input latency budget); the wake pipe is
            // NOT drained on this path, so it stays readable and the very
            // next read returns Wake — nothing lost, nothing starved.
            if fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                // SAFETY: read into our owned buffer; len is the buffer len.
                let n = unsafe {
                    libc::read(
                        self.read_fd,
                        self.in_buf.as_mut_ptr() as *mut libc::c_void,
                        self.in_buf.len(),
                    )
                };
                if n < 0 {
                    let e = last_errno();
                    if e == libc::EINTR || e == libc::EAGAIN {
                        continue;
                    }
                    return Err(io_err("read(tty)"));
                }
                if n == 0 {
                    return Err(Error::Term(
                        "terminal closed (EOF on tty) — the emulator hung up; the app should exit"
                            .into(),
                    ));
                }
                return Ok(TermRead::Input(&self.in_buf[..n as usize]));
            }

            if fds[2].revents & libc::POLLIN != 0 {
                Self::drain_winch_pipe(fds[2].fd); // same drain shape
                return Ok(TermRead::Wake);
            }

            if let Some(d) = deadline {
                if Instant::now() >= d {
                    return Ok(TermRead::Idle);
                }
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.extend_from_slice(bytes);
        // Backstop so a presenter bug cannot balloon memory; ordinary frames
        // flush explicitly long before this.
        if self.out.len() >= 1 << 16 {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.out.is_empty() {
            return Ok(());
        }
        let res = write_all_fd(self.write_fd, &self.out);
        self.out.clear();
        res
    }

    fn waker(&self) -> Option<TerminalWaker> {
        self.waker.clone()
    }

    fn degraded(&self) -> Option<&'static str> {
        self.degraded
    }

    fn is_tty(&self) -> bool {
        // The RENDER handle's ttyness (RT1-10c): /dev/tty or the stdout
        // fallback — whichever this instance actually writes to.
        // SAFETY: isatty on our write fd.
        unsafe { libc::isatty(self.write_fd) == 1 }
    }

    fn suspend(&mut self) -> Result<()> {
        // Full restore -> group stop -> re-enter with the same options.
        // Execution resumes below when SIGCONT arrives. Preconditions the
        // caller owns: SIGTSTP has its default action (an app installing
        // its own handler bypasses this verb anyway), and the process
        // group is the tty's foreground group (true for any normally
        // launched app; an orphaned group ignores the stop and this
        // becomes a fast leave+enter — harmless).
        let opts = self.entered.as_ref().map(|s| s.opts);
        match opts {
            Some(opts) => {
                self.leave()?;
                Self::deliver_stop();
                // The window may have been resized while stopped; enter()
                // re-reads geometry, and the trait doc obliges callers to
                // damage-all + re-query size() after suspend() returns.
                self.enter(&opts)
            }
            None => {
                // Not in a session: still honor the stop request.
                Self::deliver_stop();
                Ok(())
            }
        }
    }

    fn set_cursor_style(&mut self, style: CursorStyle) -> Result<()> {
        self.write(&verbs::cursor_style_bytes(style))?;
        if style != CursorStyle::Default && !self.cursor_styled {
            self.cursor_styled = true;
            // Keep the panic-hook path honest: it must now also reset the
            // style it cannot know about.
            sys::append_emergency_leave(verbs::CURSOR_STYLE_RESET);
        }
        Ok(())
    }

    fn set_title(&mut self, title: &str) -> Result<()> {
        if !self.title_pushed {
            self.title_pushed = true;
            self.write(verbs::TITLE_PUSH)?;
            sys::append_emergency_leave(verbs::TITLE_POP);
        }
        self.write(&verbs::set_title_bytes(title))
    }

    fn set_pixel_mouse(&mut self, on: bool) -> Result<()> {
        self.write(if on {
            verbs::PIXEL_MOUSE_ON
        } else {
            verbs::PIXEL_MOUSE_OFF
        })?;
        if on && !self.pixel_moused {
            sys::append_emergency_leave(verbs::PIXEL_MOUSE_OFF);
        }
        self.pixel_moused = on;
        Ok(())
    }

    fn cell_pixel_size(&mut self) -> Option<PixelSize> {
        // TIOCGWINSZ reports the WHOLE window in pixels; one cell is the
        // quotient. Many terminals report 0 pixels — that is "unknown",
        // and the wire probe (CSI 16 t) may still answer.
        let ws = self.raw_winsize().ok()?;
        if ws.ws_xpixel == 0 || ws.ws_ypixel == 0 || ws.ws_col == 0 || ws.ws_row == 0 {
            return None;
        }
        Some(PixelSize::new(
            ws.ws_xpixel / ws.ws_col,
            ws.ws_ypixel / ws.ws_row,
        ))
    }
}

impl Drop for UnixTerminal {
    fn drop(&mut self) {
        let _ = self.leave();
        if let Some(rd) = self.wake_rd {
            // SAFETY: closing the wake-pipe read end we created; the write
            // end closes when the last waker clone drops (see WakeFd).
            unsafe { libc::close(rd) };
        }
        if self.owns_fds {
            // SAFETY: closing fds we opened; a shared fd closes exactly once.
            unsafe {
                libc::close(self.read_fd);
                if self.write_fd != self.read_fd {
                    libc::close(self.write_fd);
                }
            }
        }
    }
}

// Pty-backed tests live beside this file to keep the backend readable;
// the path attribute keeps them a true child module (private items and
// imports of this module stay visible to them).
#[cfg(test)]
#[path = "unix_tests.rs"]
mod tests;
