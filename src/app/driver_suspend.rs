//! Job-control suspend orchestration (cycle-2 review I-2) — split from
//! `driver.rs` (file budget, the `driver_images.rs` sibling pattern;
//! same `impl Driver` surface).
//!
//! [`crate::term::Terminal::suspend`] alone restores/stops/re-enters
//! the TERMINAL, but the engine holds state the stop invalidates:
//!
//! - the key-state down-set: releases during the stop are
//!   unobservable, and no repeat ever corrects a stale hold (Ctrl+Z
//!   keeps the window focused — no `FocusLost` arrives). A held
//!   push-to-talk capture would resume "recording" with the chord up:
//!   the media-av/0610 stuck-mic privacy class.
//! - the previous-frame model + presenter pen: the alternate screen
//!   comes back BLANK and the restore reset cursor/pen state, so the
//!   next diff must re-emit everything from an anchored position
//!   (the apply_resize rule — both halves of "the screen is unknown").
//!
//! `Driver::suspend` owns that composition so no caller re-derives it
//! slightly wrong.

use crate::base::Result;
use crate::term::Terminal;

use super::driver::Driver;
use super::App;

impl Driver {
    /// Suspend the whole session (the app's Ctrl+Z): key-state
    /// hygiene, then [`crate::term::Terminal::suspend`] (blocks until
    /// the process resumes), then the resume re-sync (size re-query +
    /// full re-present). For embedders driving their own turns — the
    /// [`Driver::set_mouse_reporting`] precedent; component code has
    /// no terminal access, so a request-flag drain (the
    /// `mouse_capture` shape) is the future public-verb form
    /// (control-plane 0300's lifecycle lane).
    ///
    /// Ordering is load-bearing:
    ///
    /// 1. BEFORE the stop: drain held keys into synthesized releases
    ///    ([`keys::on_suspend`](super::keys)) and flush effects, so
    ///    capture surfaces stop — with
    ///    [`StopReason::Suspended`](super::StopReason::Suspended) —
    ///    while the process can still run their callbacks (an external
    ///    recorder must not keep recording through the stop). If the
    ///    platform then REFUSES the suspend (non-unix), the drain has
    ///    already happened: that fails toward not-held, the safe
    ///    direction by design — a genuinely-held key re-proves itself
    ///    through its next kitty repeat.
    /// 2. The stop itself; returns after the process continues and the
    ///    terminal re-entered with its original options (kitty flags
    ///    re-push, per the session-options accounting — fidelity is
    ///    unchanged across the round trip).
    /// 3. On resume: re-query `size()` (the window may have been
    ///    resized while stopped — `apply_resize` no-ops when equal)
    ///    and re-sync the unknown screen (prev poisoned, presenter
    ///    invalidated, all layers damaged, images re-placed). Session
    ///    verbs the APP set (cursor style, title) were reset by the
    ///    restore and stay the app's to re-apply, per the
    ///    [`crate::term::Terminal::suspend`] contract.
    pub fn suspend(&mut self, app: &mut App, term: &mut dyn Terminal) -> Result<()> {
        super::keys::on_suspend();
        crate::reactive::flush_effects();
        term.suspend()?;
        let size = term.size()?;
        self.apply_resize(app, size);
        self.resync_unknown_screen();
        Ok(())
    }
}
