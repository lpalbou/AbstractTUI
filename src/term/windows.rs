//! Windows terminal backend: VT-mode console via windows-sys.
//!
//! OWNER: KERNEL. Rationale: `docs/design/term-input.md` §1.4.
//!
//! Strategy: turn the console into a VT terminal (`ENABLE_VIRTUAL_TERMINAL_
//! PROCESSING` out, `ENABLE_VIRTUAL_TERMINAL_INPUT` in, UTF-8 output
//! codepage) so the byte protocol is identical to unix and one parser
//! serves both. Input is drained as console records rather than ReadFile:
//! key-down records carry the VT bytes one UTF-16 unit at a time (converted
//! to UTF-8 here), and WINDOW_BUFFER_SIZE records give resize detection —
//! reading records also avoids the classic hang where a wait is satisfied
//! by a record ReadFile would filter out.
//!
//! This engine is VT-only by charter: consoles that cannot enable VT output
//! (pre-Windows 10 1607) are refused with a clear error instead of a
//! degraded GDI-era code path.
//!
//! `unsafe` policy: every block is a single FFI call (or reads FFI-owned
//! union fields) with a SAFETY note.

use super::options::EnterOptions;
use super::verbs::{self, CursorStyle};
use super::waker::TerminalWaker;
use super::win_logic::{clamp_repeat, ResizeTracker, Utf16Decoder, WakeLatch};
use super::{TermRead, Terminal};
use crate::base::{Error, Result, Size};
use std::sync::Arc;
use std::time::Instant;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleOutputCP, GetConsoleScreenBufferInfo, GetNumberOfConsoleInputEvents,
    ReadConsoleInputW, SetConsoleMode, SetConsoleOutputCP, CONSOLE_MODE,
    CONSOLE_SCREEN_BUFFER_INFO, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT,
    ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_PROCESSED_OUTPUT,
    ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT, INPUT_RECORD, KEY_EVENT, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    WINDOW_BUFFER_SIZE_EVENT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject, INFINITE,
};

const UTF8_CODEPAGE: u32 = 65001;
/// Console records drained per ReadConsoleInputW call.
const RECORD_BATCH: usize = 64;
/// A hostile wRepeatCount cannot expand one record without bound.
const REPEAT_CAP: u16 = 1024;

// Freestanding machinery (error plumbing, console acquisition, the
// emergency slot, wake-event wrapper) lives in a sibling file; the path
// attribute keeps it a child module so the split is invisible outside.
#[path = "windows_sys.rs"]
mod sys;
pub use sys::emergency_restore;
pub use sys::have_tty;
use sys::{
    open_console, win_err, write_all_handle, EmergencySlot, SavedModes, WakeEvent, EMERGENCY,
};

/// The Windows console backend: VT-mode console (ConPTY-era), console
/// records for input, auto-reset event wakeups. Compile-checked and
/// clippy-clean; never executed on a live host yet (evidence matrix
/// §3.5 in the design doc) — treat as best-effort until the first run.
pub struct WindowsTerminal {
    hin: HANDLE,
    hout: HANDLE,
    owns_in: bool,
    owns_out: bool,
    saved: Option<SavedModes>,
    out: Vec<u8>,
    in_buf: Vec<u8>,
    /// UTF-16 pairing state (platform-free, tested in `win_logic`).
    utf16: Utf16Decoder,
    pending_resize: Option<Size>,
    /// Resize dedupe/sanity (platform-free, tested in `win_logic`).
    resize: ResizeTracker,
    /// Auto-reset wake event (`None` if creation failed — then `waker()`
    /// is honestly `None` too and reads fall back to single-object waits).
    wake_event: Option<Arc<WakeEvent>>,
    waker: Option<TerminalWaker>,
    /// A wake consumed from the auto-reset event but not yet delivered
    /// (input outranked it). MUST be durable state, not a read()-local:
    /// the event resets when the wait consumes it, so this latch is the
    /// only memory of the wake once bytes win the same wakeup (the unix
    /// pipe keeps its byte instead; this is the equivalent). Semantics
    /// tested platform-free in `win_logic`.
    wake: WakeLatch,
    /// A non-default DECSCUSR was emitted: leave restores `Ps 0`.
    cursor_styled: bool,
    /// The title stack was pushed before our first title: leave pops it.
    title_pushed: bool,
    /// SGR-Pixels (1016) is on: leave turns it off.
    pixel_moused: bool,
}

impl WindowsTerminal {
    /// Open the console. Prefers CONIN$/CONOUT$ (the /dev/tty analog: keeps
    /// working when std handles are redirected), falling back to the std
    /// handles.
    pub fn new() -> Result<Self> {
        let (hin, owns_in) = open_console("CONIN$", STD_INPUT_HANDLE)?;
        let (hout, owns_out) = match open_console("CONOUT$", STD_OUTPUT_HANDLE) {
            Ok(v) => v,
            Err(e) => {
                if owns_in {
                    // SAFETY: closing the handle we just opened.
                    unsafe { CloseHandle(hin) };
                }
                return Err(e);
            }
        };
        let (wake_event, waker) = Self::make_wake_event();
        Ok(WindowsTerminal {
            hin,
            hout,
            owns_in,
            owns_out,
            saved: None,
            out: Vec::with_capacity(8192),
            in_buf: Vec::with_capacity(4096),
            utf16: Utf16Decoder::default(),
            pending_resize: None,
            resize: ResizeTracker::default(),
            wake_event,
            waker,
            wake: WakeLatch::default(),
            cursor_styled: false,
            title_pushed: false,
            pixel_moused: false,
        })
    }

    /// Extend the emergency restore bytes for verbs used after enter —
    /// the panic hook must undo what it cannot know about.
    fn append_emergency_leave(extra: &[u8]) {
        if let Ok(mut g) = EMERGENCY.lock() {
            if let Some(s) = g.as_mut() {
                s.leave_bytes.extend_from_slice(extra);
            }
        }
    }

    /// Per-instance wake channel: an unnamed auto-reset event. Auto-reset
    /// gives the coalescing contract for free — N `SetEvent`s between two
    /// waits satisfy exactly one wait.
    fn make_wake_event() -> (Option<Arc<WakeEvent>>, Option<TerminalWaker>) {
        // SAFETY: CreateEventW with null security attributes and no name;
        // manual-reset = 0 (auto), initial = 0 (nonsignaled).
        let h = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if h.is_null() {
            return (None, None);
        }
        let ev = Arc::new(WakeEvent(h));
        let for_waker = ev.clone();
        let waker = TerminalWaker::new(move || {
            // SAFETY: signaling the event handle the Arc keeps alive;
            // documented thread-safe.
            unsafe { SetEvent(for_waker.0) };
        });
        (Some(ev), Some(waker))
    }

    /// RT1-12a: `WINDOW_BUFFER_SIZE_EVENT` can be missed or coalesced on
    /// classic conhost (window-only resizes); the buffer-info query is the
    /// ground truth, mirroring the unix ioctl posture.
    fn check_resize_query(&mut self) -> Option<Size> {
        let fresh = self.query_size().ok()?;
        self.resize.observe(fresh)
    }

    fn query_size(&self) -> Result<Size> {
        // SAFETY: zeroed CONSOLE_SCREEN_BUFFER_INFO is a valid out param.
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: querying the output handle into the struct above.
        if unsafe { GetConsoleScreenBufferInfo(self.hout, &mut info) } == 0 {
            return Err(win_err("GetConsoleScreenBufferInfo"));
        }
        // The visible window, NOT dwSize (dwSize includes scrollback).
        let w = i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1;
        let h = i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1;
        Ok(Size::new(w.max(0), h.max(0)))
    }

    /// Drain every queued console record into `in_buf` / `pending_resize`.
    fn drain_records(&mut self) -> Result<()> {
        loop {
            let mut queued: u32 = 0;
            // SAFETY: count query with an out param on this stack frame.
            if unsafe { GetNumberOfConsoleInputEvents(self.hin, &mut queued) } == 0 {
                return Err(win_err("GetNumberOfConsoleInputEvents"));
            }
            if queued == 0 {
                return Ok(());
            }
            let mut records: [INPUT_RECORD; RECORD_BATCH] =
                // SAFETY: INPUT_RECORD is plain data; zeroed is valid.
                unsafe { std::mem::zeroed() };
            let mut read: u32 = 0;
            let want = (queued as usize).min(RECORD_BATCH) as u32;
            // SAFETY: buffer pointer/capacity match; `read` reports how
            // many records were filled. Never blocks: `queued` records are
            // known to be pending.
            if unsafe { ReadConsoleInputW(self.hin, records.as_mut_ptr(), want, &mut read) } == 0 {
                return Err(win_err("ReadConsoleInputW"));
            }
            for rec in records.iter().take(read as usize) {
                match u32::from(rec.EventType) {
                    KEY_EVENT => {
                        // SAFETY: EventType == KEY_EVENT selects this union
                        // member per the console API contract.
                        let key = unsafe { rec.Event.KeyEvent };
                        if key.bKeyDown == 0 {
                            continue; // VT input arrives on key-down records
                        }
                        // SAFETY: uChar is a u16/i8 union; reading the u16
                        // view is always defined.
                        let unit = unsafe { key.uChar.UnicodeChar };
                        if unit == 0 {
                            continue; // bare modifier / dead key
                        }
                        let repeat = clamp_repeat(key.wRepeatCount, REPEAT_CAP);
                        for _ in 0..repeat {
                            self.utf16.push(unit, &mut self.in_buf);
                        }
                    }
                    WINDOW_BUFFER_SIZE_EVENT => {
                        // The record's dwSize is the buffer; re-query the
                        // window and dedupe (conhost sends bursts).
                        if let Ok(fresh) = self.query_size() {
                            if let Some(sz) = self.resize.observe(fresh) {
                                self.pending_resize = Some(sz);
                            }
                        }
                    }
                    _ => {} // mouse/menu/focus records: VT covers these
                }
            }
        }
    }
}

impl Terminal for WindowsTerminal {
    fn enter(&mut self, opts: &EnterOptions) -> Result<()> {
        if self.saved.is_some() {
            return Ok(());
        }
        let mut in_mode: CONSOLE_MODE = 0;
        let mut out_mode: CONSOLE_MODE = 0;
        // SAFETY: mode queries with out params on this frame.
        unsafe {
            if GetConsoleMode(self.hin, &mut in_mode) == 0 {
                return Err(win_err("GetConsoleMode(in)"));
            }
            if GetConsoleMode(self.hout, &mut out_mode) == 0 {
                return Err(win_err("GetConsoleMode(out)"));
            }
        }

        // Output first: if VT output is impossible, nothing else matters.
        // Microsoft's documented step-down: retry without
        // DISABLE_NEWLINE_AUTO_RETURN before giving up.
        let want_full = out_mode
            | ENABLE_PROCESSED_OUTPUT
            | ENABLE_VIRTUAL_TERMINAL_PROCESSING
            | DISABLE_NEWLINE_AUTO_RETURN;
        // SAFETY: SetConsoleMode on our output handle.
        let ok = unsafe { SetConsoleMode(self.hout, want_full) } != 0
            || unsafe {
                SetConsoleMode(
                    self.hout,
                    out_mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                )
            } != 0;
        if !ok {
            return Err(Error::Unsupported(
                "console cannot enable VT output (Windows 10 1607+ required)".into(),
            ));
        }

        // Input: VT sequences + window records, raw (no line/echo/processed),
        // and no quick-edit (it captures the mouse and pauses output).
        let raw_in =
            (in_mode | ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS)
                & !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_QUICK_EDIT_MODE);
        // SAFETY: SetConsoleMode on our input handle.
        if unsafe { SetConsoleMode(self.hin, raw_in) } == 0 {
            // Undo the output mode before failing so enter() is atomic.
            // SAFETY: restoring the mode read above.
            unsafe { SetConsoleMode(self.hout, out_mode) };
            return Err(win_err("SetConsoleMode(in, VT)"));
        }

        // SAFETY: codepage query/set are plain calls.
        let out_cp = unsafe { GetConsoleOutputCP() };
        unsafe { SetConsoleOutputCP(UTF8_CODEPAGE) };

        self.resize.reset(self.query_size().unwrap_or(Size::ZERO));

        if let Ok(mut g) = EMERGENCY.lock() {
            *g = Some(EmergencySlot {
                hin: self.hin as isize,
                hout: self.hout as isize,
                in_mode,
                out_mode,
                out_cp,
                leave_bytes: opts.leave_bytes(),
            });
        }
        self.saved = Some(SavedModes {
            in_mode,
            out_mode,
            out_cp,
            opts: *opts,
        });
        self.write(&opts.enter_bytes())?;
        self.flush()?;
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        let saved = self.saved.take();
        if saved.is_none() && !self.cursor_styled && !self.title_pushed && !self.pixel_moused {
            return Ok(());
        }
        let mut first_err: Option<Error> = None;
        if let Some(s) = &saved {
            self.out.extend_from_slice(&s.opts.leave_bytes());
        }
        // Verb resets after mode teardown (mirrors unix: they must apply
        // to the screen the user returns to).
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
        let Some(saved) = saved else {
            return match first_err {
                Some(e) => Err(e),
                None => Ok(()),
            };
        };
        // SAFETY: restoring modes/codepage captured by enter().
        unsafe {
            if SetConsoleMode(self.hin, saved.in_mode) == 0 {
                first_err.get_or_insert(win_err("SetConsoleMode(in, restore)"));
            }
            if SetConsoleMode(self.hout, saved.out_mode) == 0 {
                first_err.get_or_insert(win_err("SetConsoleMode(out, restore)"));
            }
            SetConsoleOutputCP(saved.out_cp);
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
        self.query_size()
    }

    fn read(&mut self, deadline: Option<Instant>) -> Result<TermRead<'_>> {
        if let Some(sz) = self.pending_resize.take() {
            return Ok(TermRead::Resize(sz));
        }
        loop {
            self.in_buf.clear();
            self.drain_records()?;
            if !self.in_buf.is_empty() {
                // Bytes outrank the resize here because records were drained
                // in arrival order; the stashed resize is delivered by the
                // next call, and a consumed-but-undelivered wake stays
                // latched in `self.wake` for a later call.
                return Ok(TermRead::Input(&self.in_buf));
            }
            if let Some(sz) = self.pending_resize.take() {
                return Ok(TermRead::Resize(sz));
            }
            // RT1-12a: re-query geometry on EVERY pass (wait wake, wake
            // event, and — via the loop-top — deadline expiry): missed
            // WINDOW_BUFFER_SIZE records must not leave the app blind.
            if let Some(sz) = self.check_resize_query() {
                return Ok(TermRead::Resize(sz));
            }
            if self.wake.take() {
                return Ok(TermRead::Wake);
            }
            let wait_ms = match deadline {
                None => INFINITE,
                Some(d) => {
                    let rem = d.saturating_duration_since(Instant::now());
                    if rem.is_zero() {
                        return Ok(TermRead::Idle);
                    }
                    rem.as_millis().min(u128::from(u32::MAX - 1)) as u32
                }
            };
            let rc = match &self.wake_event {
                Some(ev) => {
                    let handles = [self.hin, ev.0];
                    // SAFETY: waiting on two live handles (console input +
                    // our event); bWaitAll = 0 returns on the first
                    // signaled object.
                    unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, wait_ms) }
                }
                // SAFETY: waiting on our input handle; console handles are
                // waitable and signal when records are queued.
                None => unsafe { WaitForSingleObject(self.hin, wait_ms) },
            };
            const WAIT_WAKE: u32 = WAIT_OBJECT_0 + 1;
            match rc {
                WAIT_OBJECT_0 => continue, // console records: drain next pass
                WAIT_WAKE if self.wake_event.is_some() => {
                    // The satisfied wait consumed (reset) the auto-reset
                    // event: latch it durably, then one more drain pass so
                    // records that arrived with the wake keep the
                    // input-before-wake ordering.
                    self.wake.arm();
                    continue;
                }
                WAIT_TIMEOUT => continue, // loop top re-checks resize + deadline
                _ => return Err(win_err("WaitForConsoleInput")),
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.extend_from_slice(bytes);
        if self.out.len() >= 1 << 16 {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.out.is_empty() {
            return Ok(());
        }
        let res = write_all_handle(self.hout, &self.out);
        self.out.clear();
        res
    }

    fn waker(&self) -> Option<TerminalWaker> {
        self.waker.clone()
    }

    fn is_tty(&self) -> bool {
        // A live console (not a redirected pipe): the mode query answers
        // it — it fails with ERROR_INVALID_HANDLE on non-console handles.
        let mut mode: CONSOLE_MODE = 0;
        // SAFETY: mode query on our output handle.
        unsafe { GetConsoleMode(self.hout, &mut mode) != 0 }
    }

    // `suspend` keeps the default Unsupported: Windows has no SIGTSTP job
    // control; shells emulate ^Z poorly and apps do not expect it.

    fn set_cursor_style(&mut self, style: CursorStyle) -> Result<()> {
        self.write(&verbs::cursor_style_bytes(style))?;
        if style != CursorStyle::Default && !self.cursor_styled {
            self.cursor_styled = true;
            Self::append_emergency_leave(verbs::CURSOR_STYLE_RESET);
        }
        Ok(())
    }

    fn set_title(&mut self, title: &str) -> Result<()> {
        if !self.title_pushed {
            self.title_pushed = true;
            self.write(verbs::TITLE_PUSH)?;
            Self::append_emergency_leave(verbs::TITLE_POP);
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
            Self::append_emergency_leave(verbs::PIXEL_MOUSE_OFF);
        }
        self.pixel_moused = on;
        Ok(())
    }

    // `cell_pixel_size` stays the default `None`: the console API exposes
    // font geometry only for the legacy raster path (GetCurrentConsoleFont
    // is unreliable under ConPTY); the wire probe's `CSI 16 t` is the
    // honest source on Windows Terminal.
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        let _ = self.leave();
        // SAFETY: closing only handles this instance opened (std pseudo-
        // handles are never closed).
        unsafe {
            if self.owns_in {
                CloseHandle(self.hin);
            }
            if self.owns_out {
                CloseHandle(self.hout);
            }
        }
    }
}
