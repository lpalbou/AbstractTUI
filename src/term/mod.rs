//! Terminal kernel: platform I/O, raw mode, alternate screen, resize
//! detection, cross-thread wakeups, and capability detection.
//!
//! OWNER: KERNEL. Design rationale and protocol citations live in
//! `docs/design/term-input.md`.
//!
//! The contract is a thin, object-safe [`Terminal`] trait over a byte pipe
//! plus the things a byte pipe cannot express: session lifecycle
//! (enter/leave with guaranteed restore), window geometry (size + resize),
//! and loop wakeups from other threads. Everything higher-level — escape
//! parsing, capability probing, event delivery — is built on top of these
//! operations, so the platform backends stay small enough to audit line by
//! line.
//!
//! # Minimal session
//!
//! ```no_run
//! use abstracttui::term::{Capabilities, EnterOptions, TermRead, Terminal, UnixTerminal};
//! use std::time::{Duration, Instant};
//!
//! # fn main() -> abstracttui::base::Result<()> {
//! let caps = Capabilities::detect_env(); // free, instant, conservative
//! let mut term = UnixTerminal::new()?;   // real device fd acquisition
//! term.enter(&EnterOptions::default())?; // raw mode + altscreen + modes
//!
//! term.write(b"\r\npress any key (5s)...\r\n")?;
//! term.flush()?; // presenters call flush exactly once per frame
//!
//! match term.read(Some(Instant::now() + Duration::from_secs(5)))? {
//!     TermRead::Input(bytes) => { let _ = bytes; /* feed input::Parser */ }
//!     TermRead::Resize(size) => { let _ = size; /* re-layout */ }
//!     TermRead::Wake => { /* another thread wants the loop */ }
//!     TermRead::Idle => { /* deadline expired */ }
//! }
//!
//! term.leave()?; // also runs on Drop — the terminal always restores
//! # let _ = caps;
//! # Ok(())
//! # }
//! ```
#![warn(missing_docs)]

pub mod caps;
pub mod options;
pub mod probe;
pub mod verbs;
pub mod waker;
// Platform-independent state machines behind the Windows backend,
// compiled (and unit-tested) on every host — RT8-9: the cfg(windows)
// type is compile-only off-Windows, its logic must not be.
#[cfg(unix)]
pub mod unix;
// Platform-independent Windows console state machines. Consumed by the
// cfg(windows) backend and unit-tested on every platform — off Windows the
// non-test build has no caller, so the dead-code lint is silenced there.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) mod win_logic;
#[cfg(windows)]
pub mod windows;

// Live tmux passthrough verification (ignored-by-default; spawns a real
// tmux attached to a pty this harness owns). Kernel-owned because it
// exercises `verbs::tmux_wrap` + the probe's passthrough premise.
#[cfg(all(unix, test))]
mod tmux_live_tests;

// RT5-1 decisive tests: /dev/tty alias pollability vs the resolved real
// device, and the engine keystroke path under a proper controlling
// terminal (self-spawned pty children).
#[cfg(all(unix, test))]
mod rt5_live_tests;

pub use caps::{Capabilities, CapsReply, GraphicsCaps};
pub use options::{EnterOptions, KittyFlags, MouseMode};
pub use probe::{refresh_cell_pixel_size, ActiveProbe};
#[cfg(unix)]
pub use unix::{emergency_restore, have_tty, UnixTerminal};
pub use verbs::{tmux_wrap, CursorStyle, NotifyChannel};
pub use waker::TerminalWaker;
#[cfg(windows)]
pub use windows::{emergency_restore, have_tty, WindowsTerminal};

use crate::base::{Error, PixelSize, Result, Size};
use std::time::Instant;

/// One read outcome. Resize and Wake are out-of-band by design: neither is
/// a byte-stream datum on any platform (unix resize: ioctl is the ground
/// truth; windows: a console INPUT_RECORD; wake: a self-pipe/event), and
/// tunneling them through a private escape would force the parser to trust
/// bytes it must otherwise treat as hostile.
#[derive(Debug, PartialEq, Eq)]
pub enum TermRead<'a> {
    /// Raw bytes from the terminal, valid until the next call. Feed them to
    /// `input::Parser`; never interpret them here.
    Input(&'a [u8]),
    /// The window geometry changed; `Size` is the fresh cell grid.
    Resize(Size),
    /// A [`TerminalWaker`] fired from another thread: return to the caller
    /// so the loop can service cross-thread work. Wakes coalesce.
    Wake,
    /// The deadline expired with nothing to report.
    Idle,
}

/// Platform terminal. Object-safe: the app layer holds `Box<dyn Terminal>`
/// so tests can substitute a scripted terminal without generics spreading
/// through every layer above.
///
/// Stability note (REDTEAM request 1, cycle 1): with cycle 2's additions —
/// `TermRead::Wake`, `waker()`, `cell_pixel_size()` — KERNEL declares this
/// trait STABLE. The two new methods are defaulted so scripted terminals
/// implement exactly the six core operations and opt into the rest.
pub trait Terminal {
    /// Switch to the requested session posture (raw mode + the modes in
    /// `opts`). Idempotent: entering twice is a no-op.
    fn enter(&mut self, opts: &EnterOptions) -> Result<()>;

    /// Undo everything `enter` did, in reverse order. Idempotent and also
    /// invoked by `Drop`, so an early-returning app still restores.
    fn leave(&mut self) -> Result<()>;

    /// Current window size in cells, straight from the platform (never a
    /// cached value — callers cache if they need to).
    fn size(&mut self) -> Result<Size>;

    /// Wait until input bytes arrive, the window resizes, a waker fires,
    /// or `deadline` passes (`None` = wait indefinitely). Returned bytes
    /// borrow the terminal's internal buffer: zero allocation at steady
    /// state.
    fn read(&mut self, deadline: Option<Instant>) -> Result<TermRead<'_>>;

    /// Queue bytes toward the terminal. Bytes are opaque here — escape
    /// emission belongs to the presenter.
    fn write(&mut self, bytes: &[u8]) -> Result<()>;

    /// Push queued bytes to the device.
    fn flush(&mut self) -> Result<()>;

    /// A cheap `Clone + Send + Sync` handle that interrupts a blocking
    /// `read` from another thread (the read returns [`TermRead::Wake`]).
    /// `None` means this terminal cannot be woken (scripted test terminals
    /// may start there; the platform backends always return `Some`).
    fn waker(&self) -> Option<TerminalWaker> {
        None
    }

    /// Labeled degradation state: `Some(reason)` after the terminal had
    /// to abandon its primary path to stay functional (today: the read
    /// loop falling back from a non-pollable terminal fd to stdin —
    /// RT5-1 hardening). `None` = healthy. Apps that surface one
    /// diagnostics line at startup should show this string; the engine
    /// itself never prints.
    fn degraded(&self) -> Option<&'static str> {
        None
    }

    /// One cell's size in pixels, when the platform can measure it
    /// (unix: TIOCGWINSZ pixel fields / rows÷cols). `None` means unknown —
    /// the active probe's `CSI 16 t` query may still fill
    /// `Capabilities::cell_pixel_size` over the wire.
    fn cell_pixel_size(&mut self) -> Option<PixelSize> {
        None
    }

    /// Whether this terminal is a real interactive device — `isatty` on
    /// the actual render handle (DESIGN request 6: the splash gate must
    /// ask the handle the engine renders to, not stdout). Scripted test
    /// terminals default `false` (splash auto-skips under test) and may
    /// override with a scripted value. Distinct from the free
    /// `term::have_tty()`, which answers the same question BEFORE a
    /// terminal is constructed.
    fn is_tty(&self) -> bool {
        false
    }

    /// Job-control suspend (the app's Ctrl+Z binding): restore the
    /// terminal fully (`leave`), deliver the platform stop signal, and on
    /// continuation re-enter with the same options. Returns after the
    /// process resumes. The alternate screen comes back BLANK, the window
    /// may have been resized while stopped, and session verbs (cursor
    /// style, title) were reset by the restore: callers must damage-all,
    /// re-query `size()`, and re-apply verbs on return. Unsupported
    /// off-unix.
    fn suspend(&mut self) -> Result<()> {
        Err(Error::Unsupported(
            "suspend/resume needs unix job control (SIGTSTP) — bind Ctrl+Z \
             only on unix, or hide the binding when suspend() errors"
                .into(),
        ))
    }

    /// DECSCUSR cursor style. Backends latch non-default styles and emit
    /// the reset (`Ps 0` — the user's configured cursor) on leave.
    fn set_cursor_style(&mut self, style: CursorStyle) -> Result<()> {
        self.write(&verbs::cursor_style_bytes(style))
    }

    /// Set the window title (OSC 0, control bytes stripped). Backends
    /// push the title stack (XTWINOPS 22) before the first set and pop it
    /// on leave — best effort: terminals without the stack keep the last
    /// title, which is exactly the pre-existing behavior of every TUI.
    fn set_title(&mut self, title: &str) -> Result<()> {
        self.write(&verbs::set_title_bytes(title))
    }

    /// Copy `text` to the system clipboard via OSC 52 (write-only by
    /// design — the read form is a clipboard-exfiltration vector this
    /// engine never emits; see `docs/design/term-input.md` §1.7). Gate on
    /// `Capabilities::osc52_copy` to report success honestly: terminals
    /// that ignore the frame copy nothing, silently.
    fn clipboard_copy(&mut self, text: &str) -> Result<()> {
        self.write(&verbs::clipboard_copy_bytes(text))
    }

    /// Suspend (`false`) or re-arm (`true`) the mouse reporting this
    /// session entered with — the runtime "native selection mode" verb
    /// (backlog 0270 tier 2): an app suspends capture, the user
    /// drag-selects with the terminal's own machinery (full native
    /// quality, native clipboard), and the app resumes on its next
    /// keypress. While suspended NO mouse events arrive — the terminal
    /// owns the pointer. Sessions entered with `MouseMode::Off` no-op.
    /// Idempotent at the wire (DECSET/DECRST of an already-set mode is
    /// harmless), and `leave` still restores unconditionally. One
    /// interaction to know: job-control [`Terminal::suspend`] re-enters
    /// with the original options, re-arming reporting — suspend again
    /// after resume if you keep it off. The default is an honest refusal
    /// for scripted terminals that do not track a session posture; both
    /// platform backends and `testing::CaptureTerm` implement it.
    fn set_mouse_reporting(&mut self, on: bool) -> Result<()> {
        let _ = on;
        Err(Error::Unsupported(
            "set_mouse_reporting needs a terminal that tracks its entered mouse \
             mode (the platform backends and testing::CaptureTerm do; this \
             scripted terminal does not)"
                .into(),
        ))
    }

    /// Toggle SGR-Pixels mouse reporting (DEC 1016) mid-session — pixel
    /// coordinates for smooth drags over images. Contract: gate on
    /// `Capabilities::sgr_pixel_mouse` AND a known cell size, and switch
    /// `EventReader::enable_pixel_mouse`/`disable_pixel_mouse` in the
    /// SAME act — the wire grammar is identical to cell reporting, so a
    /// reader left unconfigured would surface pixels as cell coords.
    /// Backends latch `on` and reset the mode on leave.
    fn set_pixel_mouse(&mut self, on: bool) -> Result<()> {
        self.write(if on {
            verbs::PIXEL_MOUSE_ON
        } else {
            verbs::PIXEL_MOUSE_OFF
        })
    }

    /// Audible/attention bell (BEL). The terminal maps it to sound,
    /// flash, or urgency hint per user configuration.
    fn bell(&mut self) -> Result<()> {
        self.write(verbs::BELL)
    }

    /// Desktop notification through the terminal's dialect: pass
    /// `caps.notify_channel()` — OSC 9 (iTerm2 convention), OSC 99
    /// (kitty), or the BEL fallback when no dialect exists. One channel
    /// per call, never both (double-notification on terminals speaking
    /// both is the trap the channel argument closes).
    fn notify(&mut self, message: &str, channel: NotifyChannel) -> Result<()> {
        match channel {
            NotifyChannel::Osc9 => self.write(&verbs::notify_bytes(message)),
            NotifyChannel::Osc99 => self.write(&verbs::notify_bytes_osc99(message)),
            NotifyChannel::BellOnly => self.bell(),
        }
    }
}
