//! Input pipeline: raw terminal bytes -> structured [`Event`]s.
//!
//! OWNER: KERNEL. Design + protocol citations: `docs/design/term-input.md` §3.
//!
//! The core is [`Parser`], a resumable state machine that accepts arbitrary
//! byte chunks (split anywhere, including mid-UTF-8 or mid-escape) and never
//! panics on any input. [`EventReader`] glues a `term::Terminal` to the
//! parser and owns the ESC-disambiguation deadlines. Terminal query replies
//! surface as [`Event::CapsReply`] so capability probing rides the same
//! stream as keystrokes and can never desynchronize it.
//!
//! # Bytes in, events out
//!
//! ```
//! use abstracttui::input::{Event, KeyCode, KeyEvent, Mods, Parser};
//!
//! let mut parser = Parser::new();
//! let mut events = Vec::new();
//!
//! // A keystroke burst: 'h', 'i', Ctrl+Right (word jump), Up — split
//! // anywhere, even mid-sequence, and the decode is identical.
//! parser.feed(b"hi\x1b[1;5C\x1b", &mut events);
//! parser.feed(b"[A", &mut events); // the torn arrow completes
//!
//! assert_eq!(events[0], Event::Key(KeyEvent::char('h')));
//! assert_eq!(events[1], Event::Key(KeyEvent::char('i')));
//! assert_eq!(
//!     events[2],
//!     Event::Key(KeyEvent::new(KeyCode::Right, Mods::CTRL))
//! );
//! assert_eq!(events[3], Event::Key(KeyEvent::plain(KeyCode::Up)));
//! ```
#![warn(missing_docs)]

pub mod parser;
pub mod reader;

mod kitty;
mod legacy;
mod mouse;
mod params;

#[cfg(test)]
mod editor_matrix_tests;
#[cfg(test)]
mod parser_tests;

pub use parser::{Parser, Pending};
pub use reader::{probe_active, EventReader};

use crate::base::{Point, Size};
use crate::term::caps::CapsReply;

/// Modifier bitset. Hand-rolled (no external bitflags): the bit layout
/// mirrors the kitty keyboard protocol's modifier encoding so decoding is a
/// subtraction, and legacy encodings map into the same bits.
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct Mods(pub u8);

impl Mods {
    /// No modifiers held.
    pub const NONE: Mods = Mods(0);
    /// Shift (kitty bit 1).
    pub const SHIFT: Mods = Mods(1);
    /// Alt / Option (kitty bit 2; legacy ESC prefix).
    pub const ALT: Mods = Mods(2);
    /// Control (kitty bit 4).
    pub const CTRL: Mods = Mods(4);
    /// Super / Command / Windows key (kitty bit 8).
    pub const SUPER: Mods = Mods(8);
    /// Hyper (kitty bit 16; rare, X11 lineage).
    pub const HYPER: Mods = Mods(16);
    /// Meta (kitty bit 32; rare, distinct from Alt on some layouts).
    pub const META: Mods = Mods(32);
    /// Caps-lock LATCH state (kitty bit 64) — not a held key. Strip with
    /// [`Mods::without_locks`] before shortcut matching.
    pub const CAPS_LOCK: Mods = Mods(64);
    /// Num-lock latch state (kitty bit 128); see [`Mods::CAPS_LOCK`].
    pub const NUM_LOCK: Mods = Mods(128);

    /// True when every bit of `other` is set in `self`.
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    /// Bitwise union (also available as the `|` operator).
    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    /// True when no modifier bit is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Ignore lock keys when matching shortcuts: Ctrl+S must fire whether
    /// or not caps lock is latched.
    pub const fn without_locks(self) -> Mods {
        Mods(self.0 & !(Self::CAPS_LOCK.0 | Self::NUM_LOCK.0))
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        self.union(rhs)
    }
}

impl std::fmt::Debug for Mods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return write!(f, "Mods(none)");
        }
        let names = [
            (Self::SHIFT, "shift"),
            (Self::ALT, "alt"),
            (Self::CTRL, "ctrl"),
            (Self::SUPER, "super"),
            (Self::HYPER, "hyper"),
            (Self::META, "meta"),
            (Self::CAPS_LOCK, "caps"),
            (Self::NUM_LOCK, "num"),
        ];
        let mut first = true;
        write!(f, "Mods(")?;
        for (m, n) in names {
            if self.contains(m) {
                if !first {
                    write!(f, "+")?;
                }
                write!(f, "{n}")?;
                first = false;
            }
        }
        write!(f, ")")
    }
}

/// A named modifier key (kitty reports bare modifier presses when event
/// reporting is on).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // self-describing physical-key names
pub enum ModifierKey {
    LeftShift,
    LeftCtrl,
    LeftAlt,
    LeftSuper,
    LeftHyper,
    LeftMeta,
    RightShift,
    RightCtrl,
    RightAlt,
    RightSuper,
    RightHyper,
    RightMeta,
    /// AltGr on ISO layouts (kitty 57453).
    IsoLevel3Shift,
    /// The rarer level-5 shift (kitty 57454).
    IsoLevel5Shift,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Key identity, protocol-neutral: legacy escape decoding and the kitty
/// protocol land in the same vocabulary so bindings never care which
/// wire delivered the key.
pub enum KeyCode {
    /// A text-producing key, carrying its UNSHIFTED identity under the
    /// kitty protocol ('a' even when Shift is held; produced text rides
    /// [`KeyEvent::text`] when it differs).
    Char(char),
    /// Return / keypad Enter (keypad origin flagged by [`KeyEvent::keypad`]).
    Enter,
    /// Tab; Shift+Tab arrives as `Tab` + [`Mods::SHIFT`] (CSI Z decoded).
    Tab,
    /// Backspace (wire 0x7F; 0x08 decodes as Ctrl+H — see the editor
    /// matrix notes on the legacy ambiguity).
    Backspace,
    /// The Escape key (only after the reader's ESC-disambiguation window,
    /// or unambiguously under the kitty protocol).
    Esc,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Home (also legacy `CSI 1~`/`7~`).
    Home,
    /// End (also legacy `CSI 4~`/`8~`).
    End,
    /// Page up (`CSI 5~`).
    PageUp,
    /// Page down (`CSI 6~`).
    PageDown,
    /// Insert (`CSI 2~`).
    Insert,
    /// Forward delete (`CSI 3~`), NOT Backspace.
    Delete,
    /// F1..=F35 (kitty extends past the classic 12; legacy tilde codes
    /// cover F1-F20, kitty-only above).
    F(u8),
    /// Caps Lock as a KEY event (kitty 57358) — the latch STATE rides
    /// [`Mods::CAPS_LOCK`].
    CapsLock,
    /// Scroll Lock (kitty 57359).
    ScrollLock,
    /// Num Lock as a key event (kitty 57360); latch state in mods.
    NumLock,
    /// Print Screen (kitty 57361).
    PrintScreen,
    /// Pause/Break (kitty 57362).
    Pause,
    /// The menu/application key (kitty 57363).
    Menu,
    /// A bare modifier key event (kitty protocol).
    Modifier(ModifierKey),
    /// A kitty functional code we do not name yet (media keys etc.).
    /// Preserved so apps can bind it; naming can grow without re-parsing.
    Functional(u32),
    /// Decoded but unnameable (e.g. `CSI E` keypad-begin in legacy mode).
    Unidentified,
}

/// Press / repeat / release. Legacy terminals only ever produce
/// [`KeyEventKind::Press`]; repeat and release require the kitty
/// protocol's event-type reporting. Shortcut code should usually match
/// on [`KeyEvent::is_down`] (press OR repeat), not raw kind.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyEventKind {
    /// Initial key-down (the only kind legacy terminals report).
    #[default]
    Press,
    /// Held-key auto-repeat (kitty event type 2).
    Repeat,
    /// Key-up (kitty event type 3); never fires shortcuts.
    Release,
}

/// One decoded keyboard event.
///
/// Non-exhaustive (cycle 8, completing the announced migration): the
/// struct gains fields as protocols grow, so downstream crates —
/// including this repo's own `tests/`/`benches/`/`examples/` — must
/// construct through [`KeyEvent::new`]/[`KeyEvent::char`]/
/// [`KeyEvent::key`] + builders and destructure with `..`. Full literals
/// AND functional-update syntax are compile errors downstream; that is
/// the point (silent drift became an explicit error).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// The key's identity (unshifted under kitty; see [`KeyCode::Char`]).
    pub code: KeyCode,
    /// Held modifiers (+ lock latches; strip via [`Mods::without_locks`]).
    pub mods: Mods,
    /// Press / repeat / release (see [`KeyEventKind`]).
    pub kind: KeyEventKind,
    /// Text this key produced when it differs from what `code` implies —
    /// only the kitty protocol reports it. `None` for plain typing keeps
    /// the hot path allocation-free (the char lives in `code`).
    pub text: Option<String>,
    /// The key lives on the numeric keypad. Only protocols that can tell
    /// set it (kitty's dedicated keypad range; SS3 application-keypad
    /// forms); the identity in `code` stays the MAIN-key equivalent
    /// (`Char('5')`, `Enter`, `Home`…) so bindings work unchanged —
    /// `chord_matches` deliberately ignores this flag, and apps that bind
    /// keypad keys distinctly read it explicitly.
    pub keypad: bool,
}

impl KeyEvent {
    // CONSTRUCTION CONTRACT (cycle 6, after a downstream breakage): this
    // struct gains fields as protocols grow — construct through these
    // functions (or pattern-match with `..`), never with full literals.
    // Integration tests/benches/examples are DOWNSTREAM crates: a full
    // literal there breaks on every field addition. `#[non_exhaustive]`
    // will enforce this at compile time from cycle 7 (deferred one cycle
    // so existing downstream literals convert first — flipping it now
    // would break a just-fixed test file mid-cycle).

    /// A press of `code` with `mods` (the two-argument constructor).
    pub fn new(code: KeyCode, mods: Mods) -> Self {
        KeyEvent {
            code,
            mods,
            kind: KeyEventKind::Press,
            text: None,
            keypad: false,
        }
    }

    /// A press of `code` with no modifiers.
    pub fn plain(code: KeyCode) -> Self {
        Self::new(code, Mods::NONE)
    }

    /// A plain character press — the most common event in any test.
    pub fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }

    /// Alias of [`Self::plain`] that reads better beside `char(..)`.
    pub fn key(code: KeyCode) -> Self {
        Self::plain(code)
    }

    /// Builder-style modifier set (consuming, chainable).
    pub fn with_mods(mut self, mods: Mods) -> Self {
        self.mods = mods;
        self
    }

    /// Builder-style event kind (press/repeat/release).
    pub fn with_kind(mut self, kind: KeyEventKind) -> Self {
        self.kind = kind;
        self
    }

    /// Builder-style associated text (kitty report-text form).
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Builder-style keypad location flag.
    pub fn on_keypad(mut self) -> Self {
        self.keypad = true;
        self
    }

    /// The initial press only (not a held-key repeat).
    pub fn is_press(&self) -> bool {
        self.kind == KeyEventKind::Press
    }

    /// Held-key auto-repeat (kitty event reporting only).
    pub fn is_repeat(&self) -> bool {
        self.kind == KeyEventKind::Repeat
    }

    /// Only ever true when the kitty protocol reports event types;
    /// legacy terminals never produce releases.
    pub fn is_release(&self) -> bool {
        self.kind == KeyEventKind::Release
    }

    /// Press OR repeat — what shortcut/navigation handling usually wants
    /// (holding an arrow key should keep scrolling).
    pub fn is_down(&self) -> bool {
        matches!(self.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    }

    /// Shortcut-chord equality: a down event with this identity and these
    /// modifiers, ignoring lock keys on both sides (Ctrl+S must fire with
    /// caps lock latched) and ignoring release events (a shortcut firing
    /// twice per keystroke under the kitty protocol is the trap this
    /// helper exists to close).
    pub fn chord_matches(&self, code: KeyCode, mods: Mods) -> bool {
        self.is_down() && self.code == code && self.mods.without_locks() == mods.without_locks()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Which physical mouse button an event names.
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Middle button / wheel click.
    Middle,
    /// Secondary button.
    Right,
    /// Button 8 (browser-style back).
    Back,
    /// Button 9 (browser-style forward).
    Forward,
    /// Motion with no button held, or an encoding we cannot name.
    None,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// What a mouse event reports.
pub enum MouseKind {
    /// Button pressed.
    Down,
    /// Button released (SGR reports the REAL button, unlike legacy X10).
    Up,
    /// Motion while `button` is held.
    Drag,
    /// Motion with no button (mode 1003 only).
    Move,
    /// Wheel/scroll up (never paired with an Up event).
    WheelUp,
    /// Wheel/scroll down.
    WheelDown,
    /// Horizontal wheel left (trackpads; no ui-level equivalent yet).
    WheelLeft,
    /// Horizontal wheel right.
    WheelRight,
}

// Non-exhaustive as announced in cycle 6: downstream crates (including
// this repo's tests/benches/examples) must construct via `MouseEvent::new`
// + builders and destructure with `..` — full literals and FRU would
// break on every field addition, and this attribute turns that drift
// into an immediate, explicit compile error instead. (KeyEvent's flip is
// held: one downstream FRU construction site remains — see
// reviews/cycle7/kernel-requests.md.)
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// One decoded mouse event (SGR 1006/1016 wire).
pub struct MouseEvent {
    /// What happened (down/up/drag/move/wheel).
    pub kind: MouseKind,
    /// Which button (or [`MouseButton::None`] for pure motion/wheel).
    pub button: MouseButton,
    /// Cell position, 0-based (the wire format is 1-based). ALWAYS cells:
    /// under SGR-Pixels (1016) the reader divides by the configured cell
    /// geometry before events surface — raw pixels never pose as cells.
    pub pos: Point,
    /// Raw pixel position when SGR-Pixels reporting is active (smooth
    /// image drags read this); `None` in ordinary cell reporting.
    pub pixel: Option<Point>,
    /// Held modifiers (shift/alt/ctrl ride SGR button bits).
    pub mods: Mods,
}

impl MouseEvent {
    // Same construction contract as [`KeyEvent`]: functions, not full
    // literals — `#[non_exhaustive]` enforces from cycle 7.

    /// A cell-coordinate mouse event (`pixel` starts `None`).
    pub fn new(kind: MouseKind, button: MouseButton, pos: Point, mods: Mods) -> Self {
        MouseEvent {
            kind,
            button,
            pos,
            pixel: None,
            mods,
        }
    }

    /// Builder-style raw pixel position (SGR-Pixels lane).
    pub fn with_pixel(mut self, pixel: Point) -> Self {
        self.pixel = Some(pixel);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Everything the input pipeline delivers to the application, one
/// unified stream: decoded wire events, platform events (resize), and
/// terminal query replies.
pub enum Event {
    /// A keyboard event.
    Key(KeyEvent),
    /// A mouse event.
    Mouse(MouseEvent),
    /// Bracketed paste content. Pastes larger than the internal cap arrive
    /// as several consecutive `Paste` events (bounded memory beats a single
    /// unbounded string; see design doc §3.1).
    Paste(String),
    /// The terminal window gained focus (DEC 1004).
    FocusGained,
    /// The terminal window lost focus (DEC 1004).
    FocusLost,
    /// Window geometry changed. Produced by the platform layer (`term`),
    /// not parsed from bytes; unified here so apps consume one stream.
    Resize(Size),
    /// A terminal query reply, routed to `term::caps`.
    CapsReply(CapsReply),
    /// A syntactically-valid-but-unrecognized (or aborted) sequence,
    /// swallowed so it can never leak into the text stream as fake
    /// keystrokes. Raw bytes are capped at [`parser::UNKNOWN_CAP`].
    Unknown(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_ops() {
        let m = Mods::CTRL | Mods::SHIFT | Mods::CAPS_LOCK;
        assert!(m.contains(Mods::CTRL) && m.contains(Mods::SHIFT));
        assert!(!m.contains(Mods::ALT));
        assert_eq!(m.without_locks(), Mods::CTRL | Mods::SHIFT);
        assert_eq!(format!("{:?}", Mods::CTRL | Mods::ALT), "Mods(alt+ctrl)");
        assert_eq!(format!("{:?}", Mods::NONE), "Mods(none)");
    }

    #[test]
    fn key_event_kind_helpers() {
        let mut k = KeyEvent::new(KeyCode::Char('s'), Mods::CTRL);
        assert!(k.is_press() && k.is_down() && !k.is_repeat() && !k.is_release());
        k.kind = KeyEventKind::Repeat;
        assert!(!k.is_press() && k.is_down() && k.is_repeat());
        k.kind = KeyEventKind::Release;
        assert!(!k.is_down() && k.is_release());
    }

    #[test]
    fn chord_matching_ignores_locks_and_releases() {
        let mut k = KeyEvent::new(KeyCode::Char('s'), Mods::CTRL | Mods::CAPS_LOCK);
        assert!(k.chord_matches(KeyCode::Char('s'), Mods::CTRL));
        assert!(!k.chord_matches(KeyCode::Char('s'), Mods::CTRL | Mods::ALT));
        assert!(!k.chord_matches(KeyCode::Char('x'), Mods::CTRL));
        // Repeats fire shortcuts; releases never do.
        k.kind = KeyEventKind::Repeat;
        assert!(k.chord_matches(KeyCode::Char('s'), Mods::CTRL));
        k.kind = KeyEventKind::Release;
        assert!(!k.chord_matches(KeyCode::Char('s'), Mods::CTRL));
    }
}
