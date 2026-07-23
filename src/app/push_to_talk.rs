//! Push-to-talk capture gesture (backlog media-av/0610): hold-to-record
//! over the key-state service — games/0700's first named consumer, with
//! the voice-specific degradation policy.
//!
//! Every voice app has a capture gesture, and every one re-derives the
//! same three decisions: which gesture on which wire, what happens on
//! legacy terminals, and how recording state stays truthful when focus
//! is lost mid-hold. This helper owns those decisions once:
//!
//! - **Hold mode** ([`PttMode::Hold`], [`KeyFidelity::Full`]): press
//!   starts capture, release stops it — real press/release semantics,
//!   only offered where the kitty protocol actually delivers releases.
//! - **Latch mode** ([`PttMode::Latch`], [`KeyFidelity::Degraded`]):
//!   the same chord toggles capture on/off. A legacy wire cannot report
//!   releases, and faking a hold from repeat cadence is forbidden — a
//!   dropped repeat would stop recording mid-sentence. The mode is
//!   exposed so the UI labels the gesture truthfully
//!   ([`PushToTalk::gesture_label`]: "hold Space" vs "press Space to
//!   start/stop"). Caveat, stated rather than papered over: legacy
//!   auto-repeat arrives as more presses, so HOLDING the chord on a
//!   Degraded wire toggles repeatedly — the truthful label ("press…")
//!   is the mitigation, never a synthetic release.
//! - **Focus stops the mic, always** (both modes): the terminal losing
//!   focus ends capture with [`StopReason::FocusLost`]. A recording
//!   indicator that keeps running while the user cannot see the app is
//!   a privacy bug, not a UI bug. Capture also never auto-restarts when
//!   focus returns mid-hold — a fresh press is required (the safe
//!   direction for a microphone).
//! - **Suspend stops the mic too** (both modes, cycle-2 review I-2): a
//!   job-control suspend driven through
//!   [`Driver::suspend`](super::Driver::suspend) ends capture with
//!   [`StopReason::Suspended`] BEFORE the process stops — a stopped
//!   process cannot observe the release that would end a hold, and
//!   Ctrl+Z keeps the window focused so no focus event covers it. Same
//!   no-auto-restart rule on resume.
//!
//! One truth for dependent UI: the meter (widgets/0620), the recording
//! badge, and the transcription feed all derive from the same
//! [`PushToTalk::state`] signal.
//!
//! Scope note (0610): terminals cannot see unfocused keys — there are
//! no global hotkeys here, and audio capture itself is app-side.
//!
//! OWNER: INPUTAV (wave 3). Spec: docs/backlog/completed/media-av/0610.

use std::cell::RefCell;
use std::rc::Rc;

use crate::reactive::{Scope, Signal};
use crate::ui::KeyChord;

use super::keys::{hold_gesture_label, use_key_state, KeyFidelity, KeyState};

/// Capture state machine, readable by every dependent surface through
/// [`PushToTalk::state`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CaptureState {
    /// Not capturing.
    #[default]
    Idle,
    /// Capturing while the chord is physically held (Hold mode).
    Held,
    /// Capturing until the chord is pressed again (Latch mode).
    Latched,
}

impl CaptureState {
    /// True in either capturing state.
    pub fn is_talking(self) -> bool {
        !matches!(self, CaptureState::Idle)
    }
}

/// Why a capture stopped (the [`PushToTalk::on_stop`] argument).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The gesture ended it: key released (Hold) or pressed again
    /// (Latch).
    Released,
    /// The terminal lost focus — the mandatory mic-privacy stop.
    FocusLost,
    /// A job-control suspend is stopping the process (cycle-2 review
    /// I-2) — a stopped process cannot observe the release that would
    /// end a hold, so capture fails toward not-recording BEFORE the
    /// stop. Like focus loss, capture never auto-restarts on resume.
    Suspended,
    /// [`PushToTalk::cancel`] was called.
    Cancelled,
}

/// Which gesture the current wire honestly supports.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PttMode {
    /// Press starts, release stops (kitty releases live).
    Hold,
    /// Press toggles (legacy wire — releases unknowable).
    Latch,
}

type StartSlot = Rc<RefCell<Option<Box<dyn FnMut()>>>>;
type StopSlot = Rc<RefCell<Option<Box<dyn FnMut(StopReason)>>>>;

/// The push-to-talk binding. Cheap to clone; every clone shares the
/// same state signal and callbacks.
///
/// ```ignore
/// let ptt = PushToTalk::bind(cx, KeyChord::plain(Key::Char(' ')))
///     .on_start(|| recorder.start())
///     .on_stop(|reason| recorder.stop(reason));
/// let state = ptt.state();          // Signal<CaptureState> for the meter
/// let hint = ptt.gesture_label();   // "hold Space" / "press Space to start/stop"
/// ```
#[derive(Clone)]
pub struct PushToTalk {
    chord: KeyChord,
    keys: KeyState,
    state: Signal<CaptureState>,
    /// The chord-press level seen by the PREVIOUS machine step. The
    /// machine acts on press RISING edges only: the effect re-runs on
    /// every key-state change (including a mid-turn fidelity flip or an
    /// unrelated key in the same burst), and re-processing a stale
    /// press edge would double-toggle a latch.
    prev_pressed: Rc<std::cell::Cell<bool>>,
    on_start: StartSlot,
    on_stop: StopSlot,
}

impl PushToTalk {
    /// Bind `chord` as the capture gesture. Arms the key-state service
    /// and wires an effect (owned by `cx` — the binding dies with the
    /// component) that steps the state machine on every key edge.
    pub fn bind(cx: Scope, chord: KeyChord) -> PushToTalk {
        let keys = use_key_state(cx);
        let state = cx.signal(CaptureState::default());
        let on_start: StartSlot = Rc::new(RefCell::new(None));
        let on_stop: StopSlot = Rc::new(RefCell::new(None));

        let ptt = PushToTalk {
            chord,
            keys,
            state,
            prev_pressed: Rc::new(std::cell::Cell::new(false)),
            on_start,
            on_stop,
        };
        let machine = ptt.clone();
        let mut primed = false;
        cx.effect_labeled("push-to-talk", move || {
            machine.step(primed);
            primed = true;
        });
        ptt
    }

    /// One state-machine step, run by the binding's effect on every
    /// key-state change (tracked reads subscribe it). The bind-time run
    /// (`primed == false`) only records edge memory: an edge already in
    /// flight when the binding is created must not act — a capture
    /// gesture is a deliberate act against an EXISTING binding.
    fn step(&self, primed: bool) {
        // Tracked reads FIRST, unconditionally: the effect's
        // subscriptions must not depend on the branch taken.
        let fidelity = self.keys.fidelity();
        let focus_cleared = self.keys.focus_cleared();
        let suspend_cleared = self.keys.suspend_cleared();
        let pressed_now = self.keys.pressed_chord(self.chord);
        let released = self.keys.released(self.chord.key);
        let down = self.keys.is_down(self.chord.key);

        // Act on press RISING edges only: the effect re-runs on every
        // key-state change (an unrelated key in the burst, a mid-turn
        // 0293 fidelity flip), and re-processing the same still-sealed
        // press edge would double-act (live case: the fidelity upgrade
        // landing in the press's own turn used to toggle a fresh latch
        // straight off).
        let pressed = primed && pressed_now && !self.prev_pressed.get();
        self.prev_pressed.set(pressed_now);
        if !primed {
            return;
        }

        let current = self.state.get_untracked();
        // Mic privacy first: focus loss stops capture in EVERY mode.
        if focus_cleared && current.is_talking() {
            self.stop(StopReason::FocusLost);
            return;
        }
        // Suspend stops capture in every mode too (cycle-2 review
        // I-2): the process is about to be STOPPED — it cannot observe
        // a release, and an external recorder would keep recording
        // through the stop. Latch mode included (no held key needed).
        if suspend_cleared && current.is_talking() {
            self.stop(StopReason::Suspended);
            return;
        }
        match fidelity {
            KeyFidelity::Full => {
                if current == CaptureState::Idle && pressed {
                    self.start(CaptureState::Held);
                }
                match self.state.get_untracked() {
                    // Release edge — or the key provably no longer down
                    // (belt for a missed edge) — ends a hold. A press +
                    // release landing in one turn is a tap: start and
                    // stop both fire, in order.
                    CaptureState::Held if released || !down => {
                        self.stop(StopReason::Released);
                    }
                    // A latch that predates a mid-session fidelity
                    // upgrade (0293) still toggles off on press.
                    CaptureState::Latched if pressed => {
                        self.stop(StopReason::Released);
                    }
                    _ => {}
                }
            }
            KeyFidelity::Degraded => {
                if pressed {
                    match current {
                        CaptureState::Idle => self.start(CaptureState::Latched),
                        _ => self.stop(StopReason::Released),
                    }
                }
            }
        }
    }

    fn start(&self, next: CaptureState) {
        self.state.set(next);
        // Take/restore so a callback touching this binding again (or a
        // panic-free re-entry) can never hit a borrowed slot.
        let cb = self.on_start.borrow_mut().take();
        if let Some(mut f) = cb {
            f();
            let mut slot = self.on_start.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
        }
    }

    fn stop(&self, reason: StopReason) {
        self.state.set(CaptureState::Idle);
        let cb = self.on_stop.borrow_mut().take();
        if let Some(mut f) = cb {
            f(reason);
            let mut slot = self.on_stop.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
        }
    }

    /// Capture-start callback (builder style; replaces any previous).
    pub fn on_start(self, f: impl FnMut() + 'static) -> PushToTalk {
        *self.on_start.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Capture-stop callback with the reason (builder style).
    pub fn on_stop(self, f: impl FnMut(StopReason) + 'static) -> PushToTalk {
        *self.on_stop.borrow_mut() = Some(Box::new(f));
        self
    }

    /// The one truth for dependent UI (meter animation, recording
    /// badges, transcription feeds).
    pub fn state(&self) -> Signal<CaptureState> {
        self.state
    }

    /// Tracked convenience read of [`PushToTalk::state`].
    pub fn is_talking(&self) -> bool {
        self.state.get().is_talking()
    }

    /// The gesture the current wire honestly supports (tracked — flips
    /// Latch→Hold live when the 0293 probe upgrade lands).
    pub fn mode(&self) -> PttMode {
        match self.keys.fidelity() {
            KeyFidelity::Full => PttMode::Hold,
            KeyFidelity::Degraded => PttMode::Latch,
        }
    }

    /// Truthful gesture wording for hint lines (tracked, like
    /// [`PushToTalk::mode`]): "hold Space" vs "press Space to
    /// start/stop".
    pub fn gesture_label(&self) -> String {
        hold_gesture_label(self.keys.fidelity(), self.chord)
    }

    /// The bound chord.
    pub fn chord(&self) -> KeyChord {
        self.chord
    }

    /// Programmatic stop: if capturing, go idle and fire
    /// [`PushToTalk::on_stop`] with [`StopReason::Cancelled`].
    pub fn cancel(&self) {
        if self.state.get_untracked().is_talking() {
            self.stop(StopReason::Cancelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::keys;
    use crate::input::{KeyCode, KeyEvent as InKeyEvent, KeyEventKind};
    use crate::reactive::{create_root, flush_effects};
    use crate::ui::Key;

    fn press(code: KeyCode) -> InKeyEvent {
        InKeyEvent::plain(code)
    }

    fn release(code: KeyCode) -> InKeyEvent {
        InKeyEvent::plain(code).with_kind(KeyEventKind::Release)
    }

    /// Shared log the callbacks write into.
    type Log = Rc<RefCell<Vec<String>>>;

    fn bind_logged(cx: Scope, chord: KeyChord) -> (PushToTalk, Log) {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let l1 = log.clone();
        let l2 = log.clone();
        let ptt = PushToTalk::bind(cx, chord)
            .on_start(move || l1.borrow_mut().push("start".into()))
            .on_stop(move |r| l2.borrow_mut().push(format!("stop:{r:?}")));
        (ptt, log)
    }

    fn reset_keys(full: bool) {
        // Fresh frame per test-thread store; fidelity per scenario.
        let _ = keys::key_state();
        keys::begin_turn();
        keys::publish_fidelity(full);
        keys::begin_turn();
    }

    #[test]
    fn hold_mode_start_on_press_stop_on_release() {
        reset_keys(true);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        assert_eq!(ptt.mode(), PttMode::Hold);
        assert_eq!(ptt.gesture_label(), "hold Space");

        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Held);

        // Repeats while held must not restart or stop anything.
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')).with_kind(KeyEventKind::Repeat));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Held);

        keys::begin_turn();
        keys::on_key_event(&release(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(
            log.borrow().as_slice(),
            ["start", "stop:Released"],
            "one start, one stop"
        );
    }

    #[test]
    fn hold_mode_release_with_stale_mods_still_stops() {
        // Ctrl+Space to talk; user releases Ctrl first, then Space —
        // the Space release (now mod-less) must still stop capture.
        reset_keys(true);
        let (_root, (ptt, log)) = create_root(|cx| bind_logged(cx, KeyChord::ctrl(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&InKeyEvent::new(
            KeyCode::Char(' '),
            crate::input::Mods::CTRL,
        ));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Held);
        keys::begin_turn();
        keys::on_key_event(&release(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().last().unwrap(), "stop:Released");
    }

    #[test]
    fn tap_in_one_turn_fires_start_then_stop() {
        reset_keys(true);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        keys::on_key_event(&release(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().as_slice(), ["start", "stop:Released"]);
    }

    #[test]
    fn focus_lost_stops_with_the_privacy_reason_and_never_restarts() {
        reset_keys(true);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        keys::begin_turn();
        keys::on_focus_lost();
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().last().unwrap(), "stop:FocusLost");
        // Focus returns while the key is STILL physically held: repeats
        // re-prove the hold but capture must not auto-restart (a fresh
        // press is required — the safe direction for a mic).
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')).with_kind(KeyEventKind::Repeat));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().len(), 2, "no third event");
    }

    #[test]
    fn latch_mode_toggles_and_labels_truthfully() {
        reset_keys(false);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        assert_eq!(ptt.mode(), PttMode::Latch);
        assert_eq!(ptt.gesture_label(), "press Space to start/stop");

        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Latched);
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().as_slice(), ["start", "stop:Released"]);
    }

    #[test]
    fn latch_mode_stops_on_focus_loss_too() {
        reset_keys(false);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert!(ptt.is_talking());
        keys::begin_turn();
        keys::on_focus_lost();
        flush_effects();
        assert!(!ptt.state().get_untracked().is_talking());
        assert_eq!(log.borrow().last().unwrap(), "stop:FocusLost");
    }

    #[test]
    fn suspend_stops_a_hold_with_the_truthful_reason() {
        // Cycle-2 review I-2: the suspend drain must not read as a
        // user release — the reason names the suspend, and capture
        // never auto-restarts on resume (repeat re-proves the hold
        // without starting a fresh capture, the focus-return rule).
        reset_keys(true);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Held);

        keys::begin_turn();
        keys::on_suspend();
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().last().unwrap(), "stop:Suspended");
        // Resume with the key STILL physically held: the first repeat
        // re-proves the hold but capture must not auto-restart.
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')).with_kind(KeyEventKind::Repeat));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().len(), 2, "no third event");
    }

    #[test]
    fn suspend_stops_a_latch_on_a_degraded_wire_too() {
        // The latch holds no key (Degraded wires cannot), so the drain
        // synthesizes no releases — the suspend_cleared FLAG is what
        // stops it. Privacy rule: every capture mode stops on suspend.
        reset_keys(false);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Latched);

        keys::begin_turn();
        keys::on_suspend();
        flush_effects();
        assert!(!ptt.state().get_untracked().is_talking());
        assert_eq!(log.borrow().last().unwrap(), "stop:Suspended");
    }

    #[test]
    fn cancel_is_programmatic_and_idempotent() {
        reset_keys(false);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        ptt.cancel(); // idle: no-op
        assert!(log.borrow().is_empty());
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        ptt.cancel();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().last().unwrap(), "stop:Cancelled");
        ptt.cancel(); // already idle: no second event
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn fidelity_upgrade_mid_latch_keeps_capture_until_toggled_off() {
        // 0293 shape: the session starts Degraded, the probe proves the
        // protocol within the first frames.
        reset_keys(false);
        let (_root, (ptt, log)) =
            create_root(|cx| bind_logged(cx, KeyChord::plain(Key::Char(' '))));
        flush_effects();
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Latched);

        keys::publish_fidelity(true); // the upgrade lands mid-latch
        flush_effects();
        assert_eq!(ptt.mode(), PttMode::Hold, "label flips live");
        assert!(
            ptt.is_talking(),
            "an upgrade must not silently kill a running capture"
        );
        keys::begin_turn();
        keys::on_key_event(&press(KeyCode::Char(' ')));
        flush_effects();
        assert_eq!(ptt.state().get_untracked(), CaptureState::Idle);
        assert_eq!(log.borrow().last().unwrap(), "stop:Released");
        keys::publish_fidelity(false); // tidy the thread store
    }
}
