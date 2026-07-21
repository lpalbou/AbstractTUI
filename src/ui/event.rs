//! UI event model: routing phases, key/mouse events, keymaps.
//!
//! CONTRACT(KERNEL): `input::Event` (the parser's output) should either
//! reuse these types or provide a lossless conversion; the routing
//! contract below is what the app loop feeds. Filed in
//! reviews/cycle1/react-requests.md. Kept minimal on purpose: kitty
//! keyboard richness (repeat/release, text-with-modifiers) extends
//! `KeyEvent` without changing routing.

use crate::base::Point;

/// Modifier set. Hand-rolled bitflags (dependency policy).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(pub u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const SHIFT: Mods = Mods(1);
    pub const CTRL: Mods = Mods(2);
    pub const ALT: Mods = Mods(4);
    pub const SUPER: Mods = Mods(8);

    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        self.union(rhs)
    }
}

/// Key identity. `Char` carries the *unmodified* character where the
/// terminal reports one.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
}

impl KeyEvent {
    pub const fn new(key: Key, mods: Mods) -> Self {
        KeyEvent { key, mods }
    }

    pub const fn plain(key: Key) -> Self {
        KeyEvent {
            key,
            mods: Mods::NONE,
        }
    }

    pub fn chord(self) -> KeyChord {
        KeyChord {
            key: self.key,
            mods: self.mods,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Down(MouseButton),
    Up(MouseButton),
    Move,
    Drag(MouseButton),
    ScrollUp,
    ScrollDown,
    /// Horizontal wheel (macOS trackpads are real; KERNEL trap 3).
    ScrollLeft,
    ScrollRight,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub pos: Point,
    pub kind: MouseKind,
    pub mods: Mods,
}

/// Everything the ui tree routes.
///
/// Synthesized-by-the-tree events (never fed from outside):
/// - `FocusIn`/`FocusOut`: widget focus transitions, delivered to the
///   two affected nodes only.
/// - `MouseEnter`/`MouseLeave`: PER-NODE hover transitions (DOM
///   `mouseenter` semantics): when the hovered path changes, every node
///   leaving the path gets `MouseLeave` (deepest first) and every node
///   entering gets `MouseEnter` (outermost first). An ancestor counts as
///   hovered while the pointer is anywhere in its subtree — no bubbling
///   needed, which is exactly what a `hover_signal` wants.
///
/// `Paste` arrives whole and is routed to the FOCUSED widget — never
/// synthesized into per-char key events (that would reintroduce the
/// paste-injection attack bracketed paste exists to prevent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    FocusIn,
    FocusOut,
    MouseEnter,
    MouseLeave,
    Paste(String),
}

/// Routing phases, W3C order: root walks DOWN to the target (capture),
/// hits the target, then bubbles UP. Capture lets containers intercept
/// (e.g. a modal swallowing clicks); bubble is the default listening
/// phase.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Capture,
    Target,
    Bubble,
}

/// A shortcut chord: modifiers + key, e.g. Ctrl+S.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub mods: Mods,
}

impl KeyChord {
    pub const fn new(mods: Mods, key: Key) -> Self {
        KeyChord { key, mods }
    }

    /// Human-readable chord ("Ctrl+Shift+S", "F5", "Space") for keymap
    /// help and palettes.
    pub fn display(&self) -> String {
        let mut out = String::new();
        if self.mods.contains(Mods::CTRL) {
            out.push_str("Ctrl+");
        }
        if self.mods.contains(Mods::ALT) {
            out.push_str("Alt+");
        }
        if self.mods.contains(Mods::SHIFT) {
            out.push_str("Shift+");
        }
        match self.key {
            Key::Char(' ') => out.push_str("Space"),
            Key::Char(c) => out.extend(c.to_uppercase()),
            Key::F(n) => out.push_str(&format!("F{n}")),
            k => out.push_str(&format!("{k:?}")),
        }
        out
    }

    pub const fn plain(key: Key) -> Self {
        KeyChord {
            key,
            mods: Mods::NONE,
        }
    }

    pub const fn ctrl(key: Key) -> Self {
        KeyChord {
            key,
            mods: Mods::CTRL,
        }
    }
}

/// Handler control surface. Handlers receive `&mut EventCtx` and the
/// event; mutations are COMMANDS applied by the tree after dispatch —
/// handlers never hold a mutable borrow of the tree itself (they are
/// invoked while the tree is traversed).
#[derive(Default)]
pub struct EventCtx {
    pub(crate) stopped: bool,
    pub(crate) focus_request: Option<super::tree::ViewId>,
    pub(crate) damage_all: bool,
    /// Some(Some(id)) = capture to id; Some(None) = release.
    pub(crate) capture_request: Option<Option<super::tree::ViewId>>,
    /// The event target's solved rect, set by the tree before routing —
    /// handlers convert positions to local coordinates with it (a list
    /// mapping a click to a row index, a scrollbar mapping drag to
    /// offset) without holding any tree borrow.
    pub(crate) target_rect: crate::base::Rect,
    /// The target instance itself (capture requests, identity checks).
    pub(crate) target: Option<super::tree::ViewId>,
    /// The node whose handler is CURRENTLY running (updated per routing
    /// step) and its rect — an ancestor handling a bubbled event gets
    /// its OWN geometry here (RT3-4: a scroll container clamping against
    /// the deep hit target's rect scrolled by zero).
    pub(crate) current: Option<super::tree::ViewId>,
    pub(crate) current_rect: crate::base::Rect,
}

impl EventCtx {
    /// Stop routing after the current phase step (like stopPropagation).
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }

    /// Ask the tree to move focus to a specific view after dispatch.
    pub fn request_focus(&mut self, view: super::tree::ViewId) {
        self.focus_request = Some(view);
    }

    /// Route every subsequent mouse event to `view` until release —
    /// sliders/scrollbars keep receiving drags outside their rect.
    /// (Mouse DOWN captures its target automatically; this is the
    /// explicit form for custom gestures.)
    pub fn capture_pointer(&mut self, view: super::tree::ViewId) {
        self.capture_request = Some(Some(view));
    }

    pub fn release_pointer(&mut self) {
        self.capture_request = Some(None);
    }

    /// The event target's solved rect (screen coordinates).
    pub fn target_rect(&self) -> crate::base::Rect {
        self.target_rect
    }

    /// The event's target instance.
    pub fn target(&self) -> Option<super::tree::ViewId> {
        self.target
    }

    /// The solved rect of the node whose handler is running right now.
    /// THE rect for a widget's own geometry math (row under the pointer,
    /// scrollbar proportions, page size): under bubbling or capture the
    /// TARGET can be a deep descendant — or, mid-drag, the captured node
    /// — while this is always yours.
    pub fn current_rect(&self) -> crate::base::Rect {
        self.current_rect
    }

    /// The node whose handler is running right now.
    pub fn current(&self) -> Option<super::tree::ViewId> {
        self.current
    }

    /// Blunt damage hint (fine-grained damage comes from `Dyn`
    /// re-renders; handlers that mutate visual state directly can use
    /// this until widget-level damage lands).
    pub fn request_repaint(&mut self) {
        self.damage_all = true;
    }
}
