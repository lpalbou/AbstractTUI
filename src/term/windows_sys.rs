//! Process-global and freestanding Windows console machinery: error
//! plumbing, console handle acquisition, the emergency-restore slot,
//! byte writes, and the wake-event wrapper.
//!
//! OWNER: KERNEL. Split from windows.rs by topic (the unix twin is
//! unix_sys.rs). Same unsafe policy: single FFI calls with SAFETY notes.

// This file is a child of `windows` (`mod sys`), so crate paths are the
// clear spelling for term-level siblings.
use crate::base::{Error, Result};
use crate::term::options::EnterOptions;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, SetConsoleOutputCP, WriteConsoleA, CONSOLE_MODE,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

pub(crate) fn win_err(ctx: &str) -> Error {
    // SAFETY: plain thread-local error code read.
    let code = unsafe { GetLastError() };
    Error::Term(format!("{ctx}: windows error {code}"))
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Emergency-restore slot for the app layer's panic hook (mirrors unix).
// Handles are stored as isize because raw pointers are not Send; console
// handles are process-global pseudo-handles, valid for the process life.
pub(crate) struct EmergencySlot {
    pub(crate) hin: isize,
    pub(crate) hout: isize,
    pub(crate) in_mode: CONSOLE_MODE,
    pub(crate) out_mode: CONSOLE_MODE,
    pub(crate) out_cp: u32,
    pub(crate) leave_bytes: Vec<u8>,
}
pub(crate) static EMERGENCY: Mutex<Option<EmergencySlot>> = Mutex::new(None);

/// Restore console state from a panic hook. Idempotent; harmless if the
/// terminal also restores through `leave()`/`Drop` afterwards.
pub fn emergency_restore() {
    let slot = match EMERGENCY.lock() {
        Ok(mut g) => g.take(),
        Err(_) => return,
    };
    if let Some(s) = slot {
        let hout = s.hout as HANDLE;
        let hin = s.hin as HANDLE;
        let _ = write_all_handle(hout, &s.leave_bytes);
        // SAFETY: restoring modes/codepage captured at enter() on handles
        // that remain valid for the process lifetime.
        unsafe {
            SetConsoleMode(hin, s.in_mode);
            SetConsoleMode(hout, s.out_mode);
            SetConsoleOutputCP(s.out_cp);
        }
    }
}

/// Write bytes to the console. `WriteConsoleA` rather than `WriteFile`:
/// the crate's windows-sys feature set does not include Win32_System_IO
/// (where WriteFile lives), and our handle is a console by construction —
/// with the UTF-8 output codepage set on enter, the A variant consumes the
/// same byte stream WriteFile would (chars == bytes at codepage 65001).
pub(crate) fn write_all_handle(h: HANDLE, mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        let mut written: u32 = 0;
        let chunk = bytes.len().min(u32::MAX as usize) as u32;
        // SAFETY: WriteConsoleA with a live buffer pointer/length and an
        // out parameter on this stack frame; reserved must be null.
        let ok = unsafe { WriteConsoleA(h, bytes.as_ptr(), chunk, &mut written, std::ptr::null()) };
        if ok == 0 {
            return Err(win_err("WriteConsoleA"));
        }
        if written == 0 {
            return Err(Error::Term("console write made no progress".into()));
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

pub(crate) struct SavedModes {
    pub(crate) in_mode: CONSOLE_MODE,
    pub(crate) out_mode: CONSOLE_MODE,
    pub(crate) out_cp: u32,
    pub(crate) opts: EnterOptions,
}

/// An owned kernel event object for cross-thread wakeups, kept in an `Arc`
/// shared by the terminal and every waker clone: the handle closes only
/// when the last holder drops, so a waker outliving its terminal signals a
/// still-valid (merely unobserved) event instead of racing handle reuse.
pub(crate) struct WakeEvent(pub(crate) HANDLE);

// SAFETY: a Win32 event handle is a reference to a kernel object;
// SetEvent/WaitFor* on it are documented thread-safe, and the wrapped
// HANDLE is never dereferenced as a pointer.
unsafe impl Send for WakeEvent {}
// SAFETY: as above — concurrent SetEvent from multiple threads is the
// object's designed use.
unsafe impl Sync for WakeEvent {}

impl Drop for WakeEvent {
    fn drop(&mut self) {
        // SAFETY: closing the handle this wrapper exclusively owns.
        unsafe { CloseHandle(self.0) };
    }
}

/// True when an interactive console is reachable — the acquisition test
/// `WindowsTerminal::new()` performs, without keeping handles (boot
/// auto-skip; see the unix twin for the rationale).
pub fn have_tty() -> bool {
    fn console_alive(name: &str, std_handle: u32) -> bool {
        match open_console(name, std_handle) {
            Ok((h, owned)) => {
                let mut mode: CONSOLE_MODE = 0;
                // SAFETY: mode query on a live handle.
                let ok = unsafe { GetConsoleMode(h, &mut mode) } != 0;
                if owned {
                    // SAFETY: closing the handle we just opened.
                    unsafe { CloseHandle(h) };
                }
                ok
            }
            Err(_) => false,
        }
    }
    console_alive("CONIN$", STD_INPUT_HANDLE) && console_alive("CONOUT$", STD_OUTPUT_HANDLE)
}

pub(crate) fn open_console(name: &str, std_handle: u32) -> Result<(HANDLE, bool)> {
    let path = wide(name);
    // SAFETY: CreateFileW with a NUL-terminated UTF-16 path on this frame;
    // null security attributes and template are documented-valid.
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h != INVALID_HANDLE_VALUE && !h.is_null() {
        return Ok((h, true));
    }
    // SAFETY: querying a process pseudo-handle.
    let h = unsafe { GetStdHandle(std_handle) };
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Err(Error::Term(format!(
            "no console: {name} and the std handle are both unavailable — run \
             inside Windows Terminal/conhost, or use testing::CaptureTerm for \
             headless runs"
        )));
    }
    Ok((h, false))
}
