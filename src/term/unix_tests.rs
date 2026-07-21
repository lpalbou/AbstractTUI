//! Live-pty tests for the unix backend: real openpty pairs, real
//! termios, a real SIGWINCH through the self-pipe. Split from unix.rs
//! to keep the backend itself under the file-size budget.
//!
//! OWNER: KERNEL.

use super::*;
use crate::term::options::{KittyFlags, MouseMode};
use std::ptr;

/// Read whatever the pty master has within `ms`.
fn read_master(fd: RawFd, ms: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut p, 1, ms) };
        if rc <= 0 {
            break;
        }
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    out
}

fn open_pty() -> Option<(RawFd, RawFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if rc == 0 {
        Some((master, slave))
    } else {
        None
    }
}

#[test]
fn pty_enter_emits_modes_and_leave_restores() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    let opts = EnterOptions {
        mouse: MouseMode::ButtonDrag,
        kitty_keyboard: KittyFlags::standard(),
        ..EnterOptions::default()
    };
    term.enter(&opts).expect("enter");
    let entered = read_master(master, 200);
    let s = String::from_utf8_lossy(&entered);
    assert!(s.contains("\x1b[?1049h"), "altscreen on: {s:?}");
    assert!(s.contains("\x1b[?1006h"), "sgr mouse on: {s:?}");
    assert!(s.contains("\x1b[>3u"), "kitty push: {s:?}");

    term.write(b"hello").unwrap();
    term.flush().unwrap();
    assert_eq!(read_master(master, 200), b"hello");

    term.leave().expect("leave");
    let left = String::from_utf8_lossy(&read_master(master, 200)).into_owned();
    assert!(left.contains("\x1b[<u"), "kitty pop: {left:?}");
    assert!(
        left.ends_with("\x1b[?1049l"),
        "altscreen off last: {left:?}"
    );
    drop(term);
    unsafe { libc::close(master) };
}

#[test]
fn pty_read_bytes_and_deadline() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    // No enter(): read path must work on a cooked pty too.
    unsafe {
        libc::write(master, b"ab\n".as_ptr() as *const libc::c_void, 3);
    }
    let got = match term.read(Some(Instant::now() + Duration::from_millis(500))) {
        Ok(TermRead::Input(b)) => b.to_vec(),
        other => panic!("expected input, got {other:?}"),
    };
    assert!(!got.is_empty());
    // Deadline expiry with no traffic.
    let t0 = Instant::now();
    match term.read(Some(t0 + Duration::from_millis(50))).unwrap() {
        TermRead::Idle => {}
        other => panic!("expected idle, got {other:?}"),
    }
    assert!(t0.elapsed() >= Duration::from_millis(45));
    drop(term);
    unsafe { libc::close(master) };
}

#[test]
fn pty_waker_interrupts_blocking_read() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    let waker = term.waker().expect("wake pipe should exist");
    // Coalescing: several wakes before the read still mean ONE Wake.
    waker.wake();
    waker.wake();
    match term.read(Some(Instant::now() + Duration::from_secs(2))) {
        Ok(TermRead::Wake) => {}
        other => panic!("expected wake, got {other:?}"),
    }
    // Wake from another thread while the read is genuinely blocked.
    let w2 = waker.clone();
    let t0 = Instant::now();
    let th = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        w2.wake();
    });
    match term.read(Some(Instant::now() + Duration::from_secs(5))) {
        Ok(TermRead::Wake) => {}
        other => panic!("expected wake, got {other:?}"),
    }
    assert!(t0.elapsed() < Duration::from_secs(2), "woke promptly");
    th.join().unwrap();
    // Input outranks a pending wake WHEN BOTH ARE READY; the wake is
    // delivered right after. Ordering setup matters twice: the pty is
    // still in canonical mode (no enter() here), so the line needs its
    // newline to pass the line discipline at all, and the byte must be
    // pollable on the slave before the read, or the wake legitimately
    // wins (it was the only thing ready).
    unsafe {
        libc::write(master, b"x\n".as_ptr() as *const libc::c_void, 2);
    }
    let mut p = libc::pollfd {
        fd: term.read_fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let rc = unsafe { libc::poll(&mut p, 1, 2000) };
    assert!(rc > 0, "pty byte never became readable");
    waker.wake();
    let deadline = Some(Instant::now() + Duration::from_secs(2));
    match term.read(deadline) {
        Ok(TermRead::Input(b)) => assert!(b.starts_with(b"x"), "got {b:?}"),
        other => panic!("expected input first, got {other:?}"),
    }
    match term.read(deadline) {
        Ok(TermRead::Wake) => {}
        other => panic!("expected the wake to survive, got {other:?}"),
    }
    // A waker outliving its terminal is a harmless no-op.
    drop(term);
    waker.wake();
    unsafe { libc::close(master) };
}

#[test]
fn pty_cell_pixel_size_from_winsize() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_col = 80;
    ws.ws_row = 24;
    ws.ws_xpixel = 0;
    ws.ws_ypixel = 0;
    unsafe { libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws) };
    // Zero pixel fields mean "unknown", never Some(0x0).
    assert_eq!(term.cell_pixel_size(), None);

    ws.ws_xpixel = 80 * 9;
    ws.ws_ypixel = 24 * 18;
    unsafe { libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws) };
    assert_eq!(
        term.cell_pixel_size(),
        Some(crate::base::PixelSize::new(9, 18))
    );
    drop(term);
    unsafe { libc::close(master) };
}

#[test]
fn pty_is_tty_and_pipe_is_not() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let term = UnixTerminal::from_fds(slave, slave, true);
    assert!(term.is_tty(), "a pty slave is a tty");
    drop(term);
    unsafe { libc::close(master) };

    let mut fds = [0 as libc::c_int; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let term = UnixTerminal::from_fds(fds[0], fds[1], true);
    assert!(!term.is_tty(), "a pipe is not a tty");
    drop(term);
}

#[test]
fn pty_session_verbs_emit_and_leave_resets() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    term.enter(&EnterOptions::default()).expect("enter");
    let _ = read_master(master, 100); // discard enter modes

    term.set_cursor_style(crate::term::CursorStyle::SteadyBar)
        .unwrap();
    term.set_title("abstract — demo").unwrap();
    term.set_title("second").unwrap(); // stack pushed once, not twice
    term.clipboard_copy("hi").unwrap();
    term.bell().unwrap();
    term.notify("done", crate::term::NotifyChannel::Osc9)
        .unwrap();
    term.notify("kdone", crate::term::NotifyChannel::Osc99)
        .unwrap();
    term.notify("belled", crate::term::NotifyChannel::BellOnly)
        .unwrap();
    term.flush().unwrap();
    let sent = read_master(master, 200);
    let s = String::from_utf8_lossy(&sent);
    assert!(s.contains("\x1b[6 q"), "cursor style: {s:?}");
    assert_eq!(s.matches("\x1b[22;0t").count(), 1, "one title push: {s:?}");
    assert!(s.contains("\x1b]0;abstract — demo\x1b\\"), "{s:?}");
    assert!(s.contains("\x1b]0;second\x1b\\"), "{s:?}");
    assert!(s.contains("\x1b]52;c;aGk=\x1b\\"), "clipboard: {s:?}");
    assert!(s.contains("\x1b]9;done\x1b\\"), "notify osc9: {s:?}");
    assert!(s.contains("\x1b]99;;kdone\x1b\\"), "notify osc99: {s:?}");
    assert_eq!(s.matches('\x07').count(), 2, "bell + bell-fallback: {s:?}");

    term.leave().expect("leave");
    let left = String::from_utf8_lossy(&read_master(master, 200)).into_owned();
    let altscreen_off = left.find("\x1b[?1049l").expect("altscreen exit");
    let style_reset = left.find("\x1b[0 q").expect("cursor style reset");
    let title_pop = left.find("\x1b[23;0t").expect("title pop");
    assert!(
        style_reset > altscreen_off && title_pop > altscreen_off,
        "verb resets apply to the main screen: {left:?}"
    );
    drop(term);
    unsafe { libc::close(master) };
}

#[test]
fn pty_suspend_restores_then_reenters() {
    // deliver_stop() is a no-op under cfg(test) — a real kill(0, SIGTSTP)
    // would stop the whole test process group. This exercises everything
    // else: full restore, then a fresh enter with the same options.
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    let opts = EnterOptions {
        kitty_keyboard: KittyFlags::standard(),
        ..EnterOptions::default()
    };
    term.enter(&opts).expect("enter");
    term.set_cursor_style(crate::term::CursorStyle::SteadyBar)
        .unwrap();
    term.flush().unwrap();
    let _ = read_master(master, 100);

    term.suspend().expect("suspend round-trip");
    let bytes = read_master(master, 200);
    let s = String::from_utf8_lossy(&bytes);
    // Restore then re-enter, in that order, including the verb reset.
    let off = s.find("\x1b[?1049l").expect("left altscreen");
    let style_reset = s.find("\x1b[0 q").expect("style reset on suspend");
    let back_on = s.rfind("\x1b[?1049h").expect("re-entered altscreen");
    let kitty_back = s.rfind("\x1b[>3u").expect("kitty flags re-pushed");
    assert!(
        off < back_on && style_reset < back_on && kitty_back > off,
        "{s:?}"
    );
    // Kitty + bracketed-paste interplay across the suspend boundary:
    // leave pops kitty flags BEFORE dropping paste mode (the pop's reply
    // bytes, if any, drain while our modes still absorb them), and the
    // re-enter restores paste BEFORE pushing kitty — the exact mirror.
    let kitty_pop = s.find("\x1b[<u").expect("kitty pop on suspend");
    let paste_off = s.find("\x1b[?2004l").expect("paste off on suspend");
    let paste_back = s.rfind("\x1b[?2004h").expect("paste re-enabled");
    assert!(kitty_pop < paste_off, "pop before paste-off: {s:?}");
    assert!(paste_off < paste_back, "restore after teardown: {s:?}");
    assert!(paste_back < kitty_back, "paste on before kitty push: {s:?}");
    // Session is live again: reads and writes still work.
    term.write(b"post").unwrap();
    term.flush().unwrap();
    assert_eq!(read_master(master, 200), b"post");
    term.leave().unwrap();
    drop(term);
    unsafe { libc::close(master) };
}

#[test]
fn pty_resize_via_sigwinch_pipe() {
    let Some((master, slave)) = open_pty() else {
        eprintln!("skipping: openpty unavailable in this environment");
        return;
    };
    let mut term = UnixTerminal::from_fds(slave, slave, true);
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_col = 80;
    ws.ws_row = 24;
    unsafe { libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws) };
    term.enter(&EnterOptions::default()).expect("enter");
    let _ = read_master(master, 100); // discard mode bytes

    ws.ws_col = 132;
    ws.ws_row = 43;
    unsafe {
        libc::ioctl(master, libc::TIOCSWINSZ as IoctlReq, &ws);
        // The kernel raises SIGWINCH for the pty's foreground process
        // group, which a test harness usually is not — raise explicitly
        // to exercise handler -> pipe -> poll wake -> ioctl compare.
        libc::kill(libc::getpid(), libc::SIGWINCH);
    }
    match term.read(Some(Instant::now() + Duration::from_secs(2))) {
        Ok(TermRead::Resize(sz)) => assert_eq!(sz, Size::new(132, 43)),
        other => panic!("expected resize, got {other:?}"),
    }
    term.leave().unwrap();
    drop(term);
    unsafe { libc::close(master) };
}
