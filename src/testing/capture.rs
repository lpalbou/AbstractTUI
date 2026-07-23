//! `CaptureTerm`: an in-memory implementation of [`crate::term::Terminal`]
//! — captures everything written (simultaneously applying it to a
//! [`VtScreen`]), serves scripted input/resize, and never blocks or
//! sleeps: deadlines are virtual by construction, so `EventReader`
//! deadline logic is tested by setting its timeouts to zero, not by
//! waiting.
//!
//! OWNER: REDTEAM.
//!
//! Blocking semantics translation: a real terminal's `read(deadline)`
//! blocks until bytes/resize/deadline. `CaptureTerm` pops the next
//! scripted item instead; an exhausted script returns `Idle`
//! immediately. Because callers like `EventReader::poll_event` re-poll
//! on spurious idle, an exhausted script under a FUTURE deadline would
//! busy-spin — the idle-storm guard panics after a large number of
//! consecutive empty reads with an actionable message (pass an
//! already-elapsed deadline in tests). `TermRead::Wake` is scripted via
//! `push_wake` (KERNEL's cross-thread waker, landed mid-cycle-2).

use std::collections::VecDeque;
use std::time::Instant;

use crate::base::{Result, Size};
use crate::term::{EnterOptions, TermRead, Terminal};

use super::vt::VtScreen;

/// A scripted thing the "terminal" will hand the app when asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptedRead {
    /// Bytes as if typed / replied by the terminal.
    Input(Vec<u8>),
    /// An out-of-band window resize.
    Resize(Size),
    /// A cross-thread waker fired (`TermRead::Wake`).
    Wake,
    /// "Nothing happened before the deadline."
    Idle,
}

/// Consecutive exhausted-script reads tolerated before the guard panics
/// (a livelocked test is worse than a loud one).
const IDLE_STORM_LIMIT: u32 = 10_000;

/// In-memory terminal double. All writes accumulate in `written` and are
/// simultaneously fed to an internal `VtScreen` sized like the terminal,
/// so assertions can target raw bytes, the screen grid, or both.
pub struct CaptureTerm {
    size: Size,
    entered: Option<EnterOptions>,
    written: Vec<u8>,
    /// Bytes written while un-flushed; a flush moves them to `flushed_len`.
    flushed_len: usize,
    flush_count: u64,
    script: VecDeque<ScriptedRead>,
    screen: VtScreen,
    /// Borrow target for `TermRead::Input` (the trait returns bytes
    /// borrowed from the terminal's internal buffer).
    last_input: Vec<u8>,
    idle_streak: u32,
    /// Optional failure injection: the next write/flush returns this.
    fail_next_write: Option<std::io::ErrorKind>,
    /// Completed suspend round trips (the I-2 acceptance surface).
    suspend_count: u64,
}

impl CaptureTerm {
    pub fn new(size: Size) -> CaptureTerm {
        CaptureTerm {
            size,
            entered: None,
            written: Vec::new(),
            flushed_len: 0,
            flush_count: 0,
            script: VecDeque::new(),
            screen: VtScreen::new(size),
            last_input: Vec::new(),
            idle_streak: 0,
            fail_next_write: None,
            suspend_count: 0,
        }
    }

    // ---- scripting --------------------------------------------------------

    pub fn push_input(&mut self, bytes: &[u8]) {
        self.script.push_back(ScriptedRead::Input(bytes.to_vec()));
    }

    pub fn push_resize(&mut self, size: Size) {
        self.script.push_back(ScriptedRead::Resize(size));
    }

    pub fn push_wake(&mut self) {
        self.script.push_back(ScriptedRead::Wake);
    }

    pub fn push_idle(&mut self) {
        self.script.push_back(ScriptedRead::Idle);
    }

    pub fn script_len(&self) -> usize {
        self.script.len()
    }

    /// Make the next `write`/`flush` fail with an IO error (write-path
    /// robustness tests).
    pub fn fail_next_write(&mut self, kind: std::io::ErrorKind) {
        self.fail_next_write = Some(kind);
    }

    // ---- assertions -------------------------------------------------------

    /// Everything written so far, draining the buffer (screen state is
    /// unaffected — it already consumed the bytes).
    pub fn take_bytes(&mut self) -> Vec<u8> {
        self.flushed_len = 0;
        std::mem::take(&mut self.written)
    }

    /// Peek without draining.
    pub fn bytes(&self) -> &[u8] {
        &self.written
    }

    /// Bytes written after the last flush — a presenter that forgets its
    /// final flush leaves a nonzero tail here.
    pub fn unflushed_bytes(&self) -> &[u8] {
        &self.written[self.flushed_len..]
    }

    pub fn flush_count(&self) -> u64 {
        self.flush_count
    }

    pub fn is_entered(&self) -> bool {
        self.entered.is_some()
    }

    /// Completed suspend round trips (leave + re-enter byte pairs).
    pub fn suspend_count(&self) -> u64 {
        self.suspend_count
    }

    /// The options the session was entered with, while entered.
    pub fn enter_options(&self) -> Option<&EnterOptions> {
        self.entered.as_ref()
    }

    /// The screen produced by everything written so far.
    pub fn screen(&self) -> &VtScreen {
        &self.screen
    }

    /// Convenience for tests that only need the trait object.
    pub fn as_terminal(&mut self) -> &mut dyn Terminal {
        self
    }
}

impl Terminal for CaptureTerm {
    /// Enter emits `opts.enter_bytes()` through the ordinary write path —
    /// exactly what the platform backends do — so the VT model tracks the
    /// session's mode posture and tests can assert enter/leave balance.
    fn enter(&mut self, opts: &EnterOptions) -> Result<()> {
        if self.entered.is_some() {
            return Ok(()); // idempotent per the trait contract
        }
        self.entered = Some(*opts);
        let bytes = opts.enter_bytes();
        Terminal::write(self, &bytes)?;
        self.flush()
    }

    fn leave(&mut self) -> Result<()> {
        let Some(opts) = self.entered.take() else {
            return Ok(()); // idempotent
        };
        let bytes = opts.leave_bytes();
        Terminal::write(self, &bytes)?;
        self.flush()
    }

    fn size(&mut self) -> Result<Size> {
        Ok(self.size)
    }

    /// Scripted, non-blocking read. The deadline is accepted (the trait
    /// requires it) but never slept on: script order IS time here.
    fn read(&mut self, _deadline: Option<Instant>) -> Result<TermRead<'_>> {
        match self.script.pop_front() {
            Some(ScriptedRead::Input(bytes)) => {
                self.idle_streak = 0;
                self.last_input = bytes;
                Ok(TermRead::Input(&self.last_input))
            }
            Some(ScriptedRead::Resize(size)) => {
                self.idle_streak = 0;
                self.size = size;
                // NOTE: the VtScreen does not reflow on resize (fixed
                // grid); byte assertions after a scripted resize should
                // use take_bytes, not the screen.
                Ok(TermRead::Resize(size))
            }
            Some(ScriptedRead::Wake) => {
                self.idle_streak = 0;
                Ok(TermRead::Wake)
            }
            Some(ScriptedRead::Idle) => {
                self.idle_streak = 0;
                Ok(TermRead::Idle)
            }
            None => {
                self.idle_streak += 1;
                assert!(
                    self.idle_streak < IDLE_STORM_LIMIT,
                    "CaptureTerm: {IDLE_STORM_LIMIT} consecutive reads on an exhausted \
                     script — the caller is busy-polling. Use an already-elapsed \
                     deadline (or script an explicit Idle) in tests."
                );
                Ok(TermRead::Idle)
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(kind) = self.fail_next_write.take() {
            return Err(std::io::Error::from(kind).into());
        }
        self.written.extend_from_slice(bytes);
        self.screen.feed(bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if let Some(kind) = self.fail_next_write.take() {
            return Err(std::io::Error::from(kind).into());
        }
        self.flushed_len = self.written.len();
        self.flush_count += 1;
        Ok(())
    }

    /// Mirrors the platform backends: the entered options know the armed
    /// mode; the exact arm/disarm byte pairs go through the ordinary
    /// write path so byte logs and the VT model's mode set both observe
    /// the flip (the tier-2 suspend-verb acceptance surface).
    fn set_mouse_reporting(&mut self, on: bool) -> Result<()> {
        let Some(opts) = self.entered else {
            return Err(crate::base::Error::Term(
                "set_mouse_reporting outside a session — enter() first".into(),
            ));
        };
        let mode = opts.mouse;
        Terminal::write(
            self,
            if on {
                mode.arm_bytes()
            } else {
                mode.disarm_bytes()
            },
        )
    }

    /// Job-control suspend modeled in-memory (the I-2 acceptance
    /// surface): emits the leave bytes, "stops" (nothing — a scripted
    /// terminal has no process to signal), then re-enters with the
    /// same options — the platform contract's byte round trip without
    /// the SIGTSTP. The session stays entered (exactly like the real
    /// backend after resume); `suspend_count` lets tests assert the
    /// round trip happened.
    fn suspend(&mut self) -> Result<()> {
        let Some(opts) = self.entered else {
            return Err(crate::base::Error::Term(
                "suspend outside a session — enter() first".into(),
            ));
        };
        Terminal::write(self, &opts.leave_bytes())?;
        Terminal::write(self, &opts.enter_bytes())?;
        self.flush()?;
        self.suspend_count += 1;
        Ok(())
    }

    /// Mirrors the platform backends (backlog 0293): the delta bytes go
    /// through the ordinary write path (byte log + VT model see the
    /// push/pop) and the STORED options update, so `leave` pops exactly
    /// what is pushed — tests assert the balance via
    /// `screen().counters().kitty_push_depth`.
    fn set_kitty_keyboard(&mut self, flags: crate::term::KittyFlags) -> Result<()> {
        let Some(opts) = &mut self.entered else {
            return Err(crate::base::Error::Term(
                "set_kitty_keyboard outside a session — enter() first".into(),
            ));
        };
        let prev = opts.kitty_keyboard;
        if prev == flags {
            return Ok(());
        }
        opts.kitty_keyboard = flags;
        let mut bytes = Vec::with_capacity(16);
        if !prev.is_empty() {
            bytes.extend_from_slice(crate::term::KittyFlags::POP_BYTES);
        }
        if !flags.is_empty() {
            bytes.extend_from_slice(&flags.push_bytes());
        }
        Terminal::write(self, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_reaches_screen_and_byte_log() {
        let mut t = CaptureTerm::new(Size::new(10, 2));
        Terminal::write(&mut t, b"\x1b[1;3Hhi").unwrap();
        assert_eq!(t.bytes(), b"\x1b[1;3Hhi");
        assert_eq!(t.screen().cell(2, 0).unwrap().ch(), 'h');
        assert_eq!(t.take_bytes(), b"\x1b[1;3Hhi".to_vec());
        assert!(t.bytes().is_empty());
        assert_eq!(t.screen().cell(2, 0).unwrap().ch(), 'h');
    }

    #[test]
    fn enter_and_leave_balance_modes_via_screen() {
        let mut t = CaptureTerm::new(Size::new(8, 2));
        let opts = EnterOptions::default();
        t.enter(&opts).unwrap();
        assert!(t.screen().modes().alt_screen());
        assert!(!t.screen().modes().cursor_visible());
        t.enter(&opts).unwrap(); // idempotent: no double-emission
        t.leave().unwrap();
        assert!(!t.screen().modes().alt_screen());
        assert!(t.screen().modes().cursor_visible());
        t.leave().unwrap(); // idempotent
        assert_eq!(t.screen().unknown_seq_count(), 0);
    }

    #[test]
    fn scripted_reads_fifo_through_the_trait() {
        let mut t = CaptureTerm::new(Size::new(4, 1));
        t.push_input(b"\x1b[A");
        t.push_resize(Size::new(8, 2));
        t.push_idle();
        let term: &mut dyn Terminal = &mut t;
        assert!(matches!(
            term.read(None).unwrap(),
            TermRead::Input(b"\x1b[A")
        ));
        assert!(matches!(term.read(None).unwrap(), TermRead::Resize(s) if s == Size::new(8, 2)));
        assert!(matches!(term.read(None).unwrap(), TermRead::Idle));
        assert!(matches!(term.read(None).unwrap(), TermRead::Idle)); // exhausted
        assert_eq!(term.size().unwrap(), Size::new(8, 2)); // resize stuck
    }

    #[test]
    #[should_panic(expected = "exhausted")]
    fn idle_storm_guard_fires() {
        let mut t = CaptureTerm::new(Size::new(4, 1));
        for _ in 0..20_000 {
            let _ = Terminal::read(&mut t, None);
        }
    }

    #[test]
    fn write_failure_injection() {
        let mut t = CaptureTerm::new(Size::new(4, 1));
        t.fail_next_write(std::io::ErrorKind::BrokenPipe);
        assert!(Terminal::write(&mut t, b"x").is_err());
        Terminal::write(&mut t, b"y").unwrap(); // one-shot
    }
}
