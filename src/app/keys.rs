//! Key press/release state (backlog games/0700): held keys as a
//! first-class input fact, with CAPABILITY HONESTY.
//!
//! The parser decodes press/repeat/release (`input::KeyEventKind`), but
//! the routing seam drops releases and erases kinds — correctly, for
//! shortcut dispatch. Real-time surfaces (games' move-while-held,
//! voice push-to-talk) need key STATE over time, so the driver taps its
//! PRE-conversion input stream into this service before routing.
//!
//! ## Fidelity is the contract
//!
//! [`KeyFidelity::Full`] means release events are actually live on this
//! session: the terminal speaks the kitty keyboard protocol AND the
//! driver currently has the `REPORT_EVENT_TYPES` flags pushed (at enter
//! on env-claimed terminals; mid-session after the probe proves the
//! protocol — backlog 0293). Everything else is
//! [`KeyFidelity::Degraded`]: a legacy wire only ever reports presses,
//! so "held" is UNKNOWABLE there — this service says so instead of
//! guessing. Deliberately NO repeat-timeout approximation exists: OS
//! auto-repeat cadence cannot distinguish "held" from "tapping fast",
//! and a dropped repeat would fabricate a release mid-hold (for
//! push-to-talk capture that is a privacy bug, media-av/0610). Apps on
//! Degraded wires fall back to latch/tap semantics and label the
//! gesture truthfully ([`hold_gesture_label`]).
//!
//! What each read is worth per fidelity:
//!
//! | read                    | Full            | Degraded            |
//! |-------------------------|-----------------|---------------------|
//! | `pressed`/`pressed_chord` | press edges   | press edges (honest on both wires) |
//! | `is_down`/`keys_down`   | true key state  | always empty (never fakes a hold) |
//! | `released`              | release edges   | never fires         |
//!
//! ## Frame semantics
//!
//! Edge sets (pressed/released/focus-cleared) SEAL per driver turn: the
//! driver clears them at the top of phase U, then folds that turn's
//! events in. They stay readable through the frame's later phases and
//! any idle time until the next turn — "this frame's edges" in the
//! game-loop sense. The down-set persists across turns until a release
//! (or focus loss) removes the key.
//!
//! Focus hygiene: `FocusLost` clears the whole down-set and synthesizes
//! release edges for every key it held — a key released while the
//! terminal was unfocused must not stick down forever, and failing
//! toward "not held" is the safe direction (movement stops, mics stop).
//! [`KeyState::focus_cleared`] distinguishes those synthesized edges
//! from wire releases.
//!
//! Suspend hygiene, the same rule beside focus loss (cycle-2 review
//! I-2): a job-control suspend
//! ([`Terminal::suspend`](crate::term::Terminal::suspend), driven
//! through [`Driver::suspend`](super::Driver::suspend)) stops the
//! process —
//! releases during the stop are unobservable and no repeat ever
//! corrects a stale hold on resume (Ctrl+Z keeps the window focused,
//! so no `FocusLost` arrives to cover it). The suspend seam drains
//! the down-set into synthesized release edges BEFORE the stop and
//! flags the frame ([`KeyState::suspend_cleared`]) so capture
//! surfaces stop with a truthful reason. Same safe direction: a hold
//! that survives the suspend re-proves itself through its first kitty
//! repeat after resume.
//!
//! ## Cost contract
//!
//! Zero cost while no consumer exists: the store arms on the first
//! [`use_key_state`]/[`key_state`] call; until then every driver hook
//! is one thread-local flag read. Armed but quiet costs the same — the
//! generation signal bumps only on real edges (and once more on the
//! turn that clears them), so idle turns stay allocation- and
//! signal-free (the alloc-budget pin covers this).
//!
//! OWNER: INPUTAV (wave 3). Spec: docs/backlog/completed/games/0700.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::input::{KeyEvent as InKeyEvent, KeyEventKind};
use crate::reactive::{create_root, Scope, Signal};
use crate::term::{Capabilities, KittyFlags};
use crate::ui::{Key, KeyChord, Mods};

use super::events::{convert_key, convert_mods};

/// How much the key-state service can honestly know on this session.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum KeyFidelity {
    /// Kitty release events are live: `is_down`/`released` carry true
    /// key state (the protocol is spoken AND the event-type flags are
    /// currently pushed).
    Full,
    /// Legacy wire: presses only. Press edges stay honest; the down-set
    /// stays empty and releases never fire — apps use latch/tap
    /// semantics and say so ([`hold_gesture_label`]).
    #[default]
    Degraded,
}

/// Per-frame + persistent key state (interior data behind [`KeyState`]).
#[derive(Default)]
struct Frame {
    /// Keys currently held (Full fidelity only; identity vocabulary,
    /// same conversion rules as routing — locks stripped, bare
    /// modifiers excluded).
    down: Vec<Key>,
    /// Press edges observed this turn, with their (lock-stripped) mods
    /// for chord matching.
    pressed: Vec<(Key, Mods)>,
    /// Release edges observed this turn (wire releases on Full, plus
    /// focus-loss/suspend synthesized releases — see `focus_cleared` /
    /// `suspend_cleared`).
    released: Vec<Key>,
    /// The terminal lost focus this turn (the down-set was cleared and
    /// its keys synthesized into `released`).
    focus_cleared: bool,
    /// A job-control suspend cleared the down-set this turn (same
    /// synthesized-release shape as focus loss, without claiming a
    /// focus event — cycle-2 review I-2).
    suspend_cleared: bool,
}

struct Store {
    /// Armed by the first consumer; every driver hook gates on this.
    enabled: Cell<bool>,
    /// Bumped on every real state change; consumers subscribe through
    /// it (tracked reads), so one signal fans out to all readers.
    gen: Signal<u64>,
    fidelity: Signal<KeyFidelity>,
    frame: RefCell<Frame>,
}

thread_local! {
    static STORE: RefCell<Option<Rc<Store>>> = const { RefCell::new(None) };
}

/// Same immortal-root pattern as the caps/viewport/theme signals: one
/// store per thread, deliberately leaked (disposing it would invalidate
/// every captured handle).
fn store() -> Rc<Store> {
    STORE.with(|slot| {
        if let Some(s) = slot.borrow().as_ref() {
            return s.clone();
        }
        let (root, (gen, fidelity)) =
            create_root(|cx| (cx.signal(0u64), cx.signal(KeyFidelity::default())));
        std::mem::forget(root);
        let s = Rc::new(Store {
            enabled: Cell::new(false),
            gen,
            fidelity,
            frame: RefCell::new(Frame::default()),
        });
        *slot.borrow_mut() = Some(s.clone());
        s
    })
}

/// Cheap `Copy` handle to the thread's key-state service. All reads are
/// TRACKED (they subscribe the running computation through the
/// generation signal), so `dyn_view`s and effects re-run on key edges —
/// read it where you read any signal (builders/effects), never inside
/// draw closures.
#[derive(Copy, Clone)]
pub struct KeyState {
    gen: Signal<u64>,
    fidelity: Signal<KeyFidelity>,
}

impl KeyState {
    /// What the session can honestly report (tracked — the 0293 probe
    /// upgrade can flip Degraded→Full within the first frames, and a
    /// hint line reading this re-renders at that moment).
    pub fn fidelity(self) -> KeyFidelity {
        self.fidelity.get()
    }

    /// Untracked fidelity snapshot (plumbing, diagnostics).
    pub fn fidelity_untracked(self) -> KeyFidelity {
        self.fidelity.get_untracked()
    }

    /// True while `key` is held — Full fidelity only ever answers true;
    /// on Degraded wires this is always false (the honest answer is
    /// "unknowable", and the service never fakes a hold).
    pub fn is_down(self, key: Key) -> bool {
        let _ = self.gen.get();
        with_frame(|f| f.down.contains(&key))
    }

    /// Snapshot of every currently-held key (Full fidelity; empty on
    /// Degraded).
    pub fn keys_down(self) -> Vec<Key> {
        let _ = self.gen.get();
        with_frame(|f| f.down.clone())
    }

    /// True when at least one key is held (Full fidelity).
    pub fn any_down(self) -> bool {
        let _ = self.gen.get();
        with_frame(|f| !f.down.is_empty())
    }

    /// `key` saw a press edge this turn, any modifiers (honest on both
    /// fidelities — a press event is a press event on every wire).
    pub fn pressed(self, key: Key) -> bool {
        let _ = self.gen.get();
        with_frame(|f| f.pressed.iter().any(|(k, _)| *k == key))
    }

    /// Exact chord press edge this turn (mods lock-stripped, same
    /// matching rules as shortcuts).
    pub fn pressed_chord(self, chord: KeyChord) -> bool {
        let _ = self.gen.get();
        with_frame(|f| {
            f.pressed
                .iter()
                .any(|(k, m)| *k == chord.key && *m == chord.mods)
        })
    }

    /// `key` saw a release edge this turn. Wire releases arrive on Full
    /// fidelity only; focus-loss synthesizes release edges for held
    /// keys on any fidelity (check [`KeyState::focus_cleared`] to tell
    /// them apart).
    pub fn released(self, key: Key) -> bool {
        let _ = self.gen.get();
        with_frame(|f| f.released.contains(&key))
    }

    /// The terminal lost focus this turn: the down-set was cleared and
    /// its keys appear in `released`. Capture surfaces stop on this
    /// (the media-av/0610 mic rule).
    pub fn focus_cleared(self) -> bool {
        let _ = self.gen.get();
        with_frame(|f| f.focus_cleared)
    }

    /// A job-control suspend cleared the down-set this turn (cycle-2
    /// review I-2): its held keys appear in `released` as synthesized
    /// edges — the focus-loss rule without claiming a focus event.
    /// Capture surfaces stop on this too (a stopped process cannot
    /// observe the release that would end a hold).
    pub fn suspend_cleared(self) -> bool {
        let _ = self.gen.get();
        with_frame(|f| f.suspend_cleared)
    }
}

fn with_frame<R>(f: impl FnOnce(&Frame) -> R) -> R {
    let s = store();
    let frame = s.frame.borrow();
    f(&frame)
}

/// The key-state service for component code — arming it is what turns
/// the driver's tap on (before the first call, key events cost the
/// service nothing). The `cx` parameter mirrors the other `use_*`
/// surfaces (the store itself is thread-immortal).
///
/// ```ignore
/// let keys = use_key_state(cx);
/// dyn_view(LayoutStyle::line(1), move || {
///     text(match keys.fidelity() {
///         KeyFidelity::Full => "hold-to-act available (kitty releases live)",
///         KeyFidelity::Degraded => "legacy wire: press-only — latch semantics",
///     })
/// })
/// ```
pub fn use_key_state(_cx: Scope) -> KeyState {
    key_state()
}

/// Non-component access to the same service (helpers, tests). Also arms
/// the driver's tap.
pub fn key_state() -> KeyState {
    let s = store();
    s.enabled.set(true);
    KeyState {
        gen: s.gen,
        fidelity: s.fidelity,
    }
}

/// Truthful gesture wording for a hold-to-act binding: what a hint line
/// should print for `chord` under `fidelity`. The generalized form of
/// push-to-talk's label — any hold-to-confirm/scrub-while-held UI wants
/// the same honesty ("hold Space" only where a release can end it).
pub fn hold_gesture_label(fidelity: KeyFidelity, chord: KeyChord) -> String {
    match fidelity {
        KeyFidelity::Full => format!("hold {}", chord.display()),
        KeyFidelity::Degraded => format!("press {} to start/stop", chord.display()),
    }
}

// ---------------------------------------------------------------------------
// Driver hooks (pub(crate)): the pre-conversion tap.
// ---------------------------------------------------------------------------

/// True when kitty release events are actually live for a session with
/// `flags` currently pushed on a `caps` terminal — the one fidelity
/// rule, shared by the driver's enter and upgrade publish points.
pub(crate) fn release_events_live(caps: &Capabilities, flags: KittyFlags) -> bool {
    caps.kitty_keyboard && (flags.0 & KittyFlags::REPORT_EVENT_TYPES) != 0
}

/// Driver publish point (enter + 0293 caps upgrade). Cheap and
/// unconditional: the equality cut-off keeps no-op publishes free.
///
/// A DOWNGRADE (Full → Degraded) drains the down-set into synthesized
/// release edges first — the focus-loss rule generalized: a Degraded
/// wire can never deliver the releases that would empty it, so a held
/// key would read "down" forever (for push-to-talk capture that is the
/// stuck-mic privacy class). Unreachable from today's driver (probes
/// only ever upgrade), but the contract table's "Degraded ⇒ down-set
/// empty" row is enforced structurally rather than incidentally
/// (cycle-2 review: an invariant held in one lane and violated by a
/// later lane is the recurring defect class).
pub(crate) fn publish_fidelity(full: bool) {
    let s = store();
    let fid = if full {
        KeyFidelity::Full
    } else {
        KeyFidelity::Degraded
    };
    if s.fidelity.get_untracked() != fid {
        if fid == KeyFidelity::Degraded {
            let drained = {
                let mut f = s.frame.borrow_mut();
                let had = !f.down.is_empty();
                while let Some(key) = f.down.pop() {
                    f.released.push(key);
                }
                had
            };
            if drained {
                bump(&s);
            }
        }
        s.fidelity.set(fid);
    }
}

/// Top of the driver's phase U: seal the previous turn's edges away.
/// Bumps the generation only when edges existed (the falling edge of
/// the pulse) — quiet turns touch nothing.
pub(crate) fn begin_turn() {
    let s = store();
    if !s.enabled.get() {
        return;
    }
    let had_edges = {
        let mut f = s.frame.borrow_mut();
        let had =
            !f.pressed.is_empty() || !f.released.is_empty() || f.focus_cleared || f.suspend_cleared;
        f.pressed.clear();
        f.released.clear();
        f.focus_cleared = false;
        f.suspend_cleared = false;
        had
    };
    if had_edges {
        bump(&s);
    }
}

/// Pre-conversion key tap (driver `handle_event`, before any routing —
/// key STATE is a physical fact, observed even for keys a modal or the
/// selection layer consumes).
pub(crate) fn on_key_event(ev: &InKeyEvent) {
    let s = store();
    if !s.enabled.get() {
        return;
    }
    // Same identity rules as routing: unconvertible keys (bare
    // modifiers, lock keys, unnamed functionals) stay outside the
    // vocabulary; lock latches strip from mods.
    let Some(key) = convert_key(ev.code) else {
        return;
    };
    let full = s.fidelity.get_untracked() == KeyFidelity::Full;
    let changed = {
        let mut f = s.frame.borrow_mut();
        match ev.kind {
            KeyEventKind::Press => {
                f.pressed.push((key, convert_mods(ev.mods.without_locks())));
                if full && !f.down.contains(&key) {
                    f.down.push(key);
                }
                true
            }
            KeyEventKind::Repeat => {
                // Not a press edge — but on Full fidelity a repeat is
                // PROOF the key is down now (covers a press that
                // predates arming or a focus-cleared hold resuming
                // visibility; deliberately without a new press edge, so
                // capture surfaces never auto-restart).
                if full && !f.down.contains(&key) {
                    f.down.push(key);
                    true
                } else {
                    false
                }
            }
            KeyEventKind::Release => {
                // Releases only carry state on Full fidelity; an
                // unmatched release is a no-op, never a panic.
                if full {
                    f.released.push(key);
                    if let Some(at) = f.down.iter().position(|k| *k == key) {
                        f.down.swap_remove(at);
                    }
                    true
                } else {
                    false
                }
            }
        }
    };
    if changed {
        bump(&s);
    }
}

/// Focus-loss hygiene: clear the down-set, synthesize release edges for
/// everything it held, and flag the frame so consumers can tell
/// synthesized releases from wire releases.
pub(crate) fn on_focus_lost() {
    let s = store();
    if !s.enabled.get() {
        return;
    }
    {
        let mut f = s.frame.borrow_mut();
        f.focus_cleared = true;
        while let Some(key) = f.down.pop() {
            f.released.push(key);
        }
    }
    bump(&s);
}

/// Suspend hygiene (cycle-2 review I-2): the focus-loss drain without
/// claiming a focus event — clear the down-set, synthesize release
/// edges, flag the frame (`suspend_cleared`). Called by the suspend
/// orchestration ([`Driver::suspend`](super::Driver::suspend)) BEFORE
/// the process stops, so capture stop-callbacks run pre-stop. The flag
/// is set even with an empty down-set: a Degraded-wire LATCHED capture
/// holds no key, and it must stop on suspend too.
pub(crate) fn on_suspend() {
    let s = store();
    if !s.enabled.get() {
        return;
    }
    {
        let mut f = s.frame.borrow_mut();
        f.suspend_cleared = true;
        while let Some(key) = f.down.pop() {
            f.released.push(key);
        }
    }
    bump(&s);
}

fn bump(s: &Store) {
    s.gen.update(|g| *g = g.wrapping_add(1));
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
