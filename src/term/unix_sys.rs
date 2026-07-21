//! Process-global unix machinery shared by the terminal backend: the
//! SIGWINCH claim (self-pipe + handler), the emergency-restore slot for
//! panic hooks, errno access, and small fd helpers.
//!
//! OWNER: KERNEL. Split from unix.rs by topic: everything here is
//! process-wide state or freestanding FFI helpers; the terminal instance
//! itself lives next door. Same unsafe policy: single FFI calls with
//! SAFETY notes.

use crate::base::{Error, Result};
use std::mem;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Process-global state.
//
// Two things are unavoidably process-global on unix and are kept deliberately
// tiny: (a) the SIGWINCH handler (signals are process-wide; exactly one
// terminal instance may claim it, others degrade to poll slices), and (b) the
// emergency-restore slot for the app layer's panic hook, which cannot reach
// the Terminal instance on the panicking stack.
// ---------------------------------------------------------------------------

pub(crate) static WINCH_CLAIMED: AtomicBool = AtomicBool::new(false);
/// Write end of the self-pipe. Read by the signal handler; -1 = none. The
/// pipe is created once and never closed: closing would race a handler
/// mid-flight on another thread against fd-number reuse, and two fds for the
/// process lifetime is the same price signal-handling crates pay.
pub(crate) static WINCH_PIPE_WR: AtomicI32 = AtomicI32::new(-1);
pub(crate) static WINCH_PIPE_RD: AtomicI32 = AtomicI32::new(-1);

// libc::termios is plain data (auto-Send); the fd stays valid because the
// slot is cleared in leave() before any owned fd is closed.
pub(crate) struct EmergencySlot {
    pub(crate) fd: RawFd,
    pub(crate) termios: libc::termios,
    pub(crate) leave_bytes: Vec<u8>,
}
pub(crate) static EMERGENCY: Mutex<Option<EmergencySlot>> = Mutex::new(None);

/// Extend the emergency restore bytes for session verbs used after enter
/// (cursor style, title stack): the panic hook must undo what it cannot
/// know about. No-op when no session is armed.
pub(crate) fn append_emergency_leave(extra: &[u8]) {
    if let Ok(mut g) = EMERGENCY.lock() {
        if let Some(s) = g.as_mut() {
            s.leave_bytes.extend_from_slice(extra);
        }
    }
}

/// Restore the terminal from a context that cannot reach the `Terminal`
/// instance (panic hook, signal-ish last resort). Idempotent; a subsequent
/// `leave()`/`Drop` re-restore is harmless (same bytes, same termios).
pub fn emergency_restore() {
    let slot = match EMERGENCY.lock() {
        Ok(mut g) => g.take(),
        Err(_) => return, // poisoned during panic-in-panic: give up quietly
    };
    if let Some(s) = slot {
        let _ = write_all_fd(s.fd, &s.leave_bytes);
        // SAFETY: fd is a tty fd still owned by the live Terminal instance
        // (the slot is cleared before fds close); termios was read from the
        // same fd by tcgetattr.
        unsafe { libc::tcsetattr(s.fd, libc::TCSANOW, &s.termios) };
    }
}

extern "C" fn on_sigwinch(_: libc::c_int) {
    // SAFETY: async-signal-safe operations only — an atomic load, write(2),
    // and errno save/restore. No allocation, no locks, no Rust runtime.
    unsafe {
        let errno_p = errno_location();
        let saved = *errno_p;
        let fd = WINCH_PIPE_WR.load(Ordering::Relaxed);
        if fd >= 0 {
            let byte = 1u8;
            libc::write(fd, &byte as *const u8 as *const libc::c_void, 1);
        }
        *errno_p = saved;
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__error()
}
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__errno_location()
}

pub(crate) fn last_errno() -> i32 {
    // SAFETY: reading the thread-local errno pointer immediately after a
    // failed FFI call on the same thread.
    unsafe { *errno_location() }
}

pub(crate) fn io_err(ctx: &str) -> Error {
    let e = std::io::Error::from_raw_os_error(last_errno());
    Error::Io(std::io::Error::new(e.kind(), format!("{ctx}: {e}")))
}

/// musl declares `ioctl(fd, c_int, ...)` while glibc/darwin use `c_ulong`;
/// the request constants themselves fit in either.
#[cfg(target_env = "musl")]
pub(crate) type IoctlReq = libc::c_int;
#[cfg(not(target_env = "musl"))]
pub(crate) type IoctlReq = libc::c_ulong;

pub(crate) fn write_all_fd(fd: RawFd, mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        // SAFETY: fd open, pointer/len come from a live slice.
        let n = unsafe { libc::write(fd, bytes.as_ptr() as *const libc::c_void, bytes.len()) };
        if n < 0 {
            let e = last_errno();
            if e == libc::EINTR {
                continue;
            }
            return Err(io_err("write"));
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}

pub(crate) fn set_cloexec_nonblock(fd: RawFd) -> Result<()> {
    // SAFETY: plain fcntl on an fd we just created.
    unsafe {
        if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0 {
            return Err(io_err("fcntl(FD_CLOEXEC)"));
        }
        let fl = libc::fcntl(fd, libc::F_GETFL);
        if fl < 0 || libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK) < 0 {
            return Err(io_err("fcntl(O_NONBLOCK)"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SIGWINCH claim.
// ---------------------------------------------------------------------------

// libc::sigaction is plain data (handler address + mask + flags): auto-Send.
pub(crate) struct WinchClaim {
    pub(crate) pipe_rd: RawFd,
    pub(crate) old_action: libc::sigaction,
}

pub(crate) fn claim_winch() -> Option<WinchClaim> {
    if WINCH_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None; // another instance owns resize signaling
    }
    // Create (or reuse) the process-lifetime pipe.
    let mut rd = WINCH_PIPE_RD.load(Ordering::Acquire);
    if rd < 0 {
        let mut fds = [0 as libc::c_int; 2];
        // SAFETY: pipe(2) writing into a stack array of exactly 2 ints.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            WINCH_CLAIMED.store(false, Ordering::Release);
            return None;
        }
        if set_cloexec_nonblock(fds[0]).is_err() || set_cloexec_nonblock(fds[1]).is_err() {
            // SAFETY: closing the fds we just created.
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            WINCH_CLAIMED.store(false, Ordering::Release);
            return None;
        }
        WINCH_PIPE_RD.store(fds[0], Ordering::Release);
        WINCH_PIPE_WR.store(fds[1], Ordering::Release);
        rd = fds[0];
    }
    // SAFETY: sigaction is zeroable POD; we fill the fields we mean.
    let mut sa: libc::sigaction = unsafe { mem::zeroed() };
    sa.sa_sigaction = on_sigwinch as extern "C" fn(libc::c_int) as usize;
    // SA_RESTART: we do not rely on EINTR for resize (the pipe wakes poll,
    // and poll is never restarted anyway); restarting keeps unrelated slow
    // syscalls elsewhere in the process from spuriously failing.
    sa.sa_flags = libc::SA_RESTART;
    // SAFETY: initializing the mask member of the struct above.
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };
    let mut old: libc::sigaction = unsafe { mem::zeroed() };
    // SAFETY: installing a handler that is async-signal-safe (see above);
    // previous action captured for exact restore.
    if unsafe { libc::sigaction(libc::SIGWINCH, &sa, &mut old) } != 0 {
        WINCH_CLAIMED.store(false, Ordering::Release);
        return None;
    }
    Some(WinchClaim {
        pipe_rd: rd,
        old_action: old,
    })
}

pub(crate) fn release_winch(claim: WinchClaim) {
    // Restore the previous handler first: after this returns, our handler is
    // no longer invoked. The pipe fds intentionally stay open (see above).
    // SAFETY: restoring the sigaction captured at claim time.
    unsafe { libc::sigaction(libc::SIGWINCH, &claim.old_action, std::ptr::null_mut()) };
    WINCH_CLAIMED.store(false, Ordering::Release);
}
