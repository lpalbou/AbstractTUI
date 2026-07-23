//! Translation from KERNEL's parsed [`crate::input::Event`]s to the ui
//! routing vocabulary, plus the default quit policy.
//!
//! The two vocabularies stay separate on purpose: `input` speaks the
//! terminal's full richness (kitty release/repeat events, lock-key
//! modifiers, caps replies), `ui::event` speaks what ROUTING needs
//! (identity + modifiers + position). This is the lossy, documented seam
//! — everything dropped here is listed on `convert_event`.

use crate::input::{Event, KeyCode, KeyEventKind, Mods as InMods, MouseKind as InMouseKind};
use crate::ui::{Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, UiEvent};

/// Kernel modifier bits -> ui modifier bits. The two layouts differ
/// (kernel mirrors the kitty wire encoding; ui packs for chord tables),
/// so this must map field-by-field, never transmute. Lock keys are
/// dropped: Ctrl+S must match whether or not caps lock is latched.
pub(crate) fn convert_mods(m: InMods) -> Mods {
    let mut out = Mods::NONE;
    if m.contains(InMods::SHIFT) {
        out = out | Mods::SHIFT;
    }
    if m.contains(InMods::CTRL) {
        out = out | Mods::CTRL;
    }
    if m.contains(InMods::ALT) {
        out = out | Mods::ALT;
    }
    if m.contains(InMods::SUPER) {
        out = out | Mods::SUPER;
    }
    out
}

/// Key-identity conversion, shared with the key-state service
/// (`app::keys` taps the pre-conversion stream and must speak the same
/// vocabulary routing does).
pub(crate) fn convert_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Esc => Key::Escape,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::F(n) => Key::F(n),
        // Bare modifiers, lock keys, media keys, unidentified: routing has
        // no binding vocabulary for these yet (ui::Key grows when a widget
        // needs one — extension, not re-parse).
        KeyCode::CapsLock
        | KeyCode::ScrollLock
        | KeyCode::NumLock
        | KeyCode::PrintScreen
        | KeyCode::Pause
        | KeyCode::Menu
        | KeyCode::Modifier(_)
        | KeyCode::Functional(_)
        | KeyCode::Unidentified => return None,
    })
}

/// Convert a kernel event into a routable ui event.
///
/// Returns `None` for events routing cannot use yet (documented drops):
/// key RELEASE events (kitty; press/repeat dispatch), bare-modifier and
/// unnamed functional keys, wheel-left/right, mouse buttons past the
/// classic three, paste (cycle-3 text-input work), terminal focus
/// in/out (distinct from widget focus — wiring them to hover/focus
/// policy is a widgets decision), caps replies (handled by the driver's
/// probe), resize (handled by the driver as geometry, not routing), and
/// `Unknown` (deliberately swallowed garbage).
pub(crate) fn convert_event(event: &Event) -> Option<UiEvent> {
    match event {
        Event::Key(k) => {
            if k.kind == KeyEventKind::Release {
                return None;
            }
            let key = convert_key(k.code)?;
            Some(UiEvent::Key(KeyEvent::new(
                key,
                convert_mods(k.mods.without_locks()),
            )))
        }
        Event::Mouse(m) => {
            let button = |b: crate::input::MouseButton| -> Option<MouseButton> {
                match b {
                    crate::input::MouseButton::Left => Some(MouseButton::Left),
                    crate::input::MouseButton::Middle => Some(MouseButton::Middle),
                    crate::input::MouseButton::Right => Some(MouseButton::Right),
                    _ => None,
                }
            };
            let kind = match m.kind {
                InMouseKind::Down => MouseKind::Down(button(m.button)?),
                InMouseKind::Up => MouseKind::Up(button(m.button)?),
                InMouseKind::Drag => MouseKind::Drag(button(m.button)?),
                InMouseKind::Move => MouseKind::Move,
                InMouseKind::WheelUp => MouseKind::ScrollUp,
                InMouseKind::WheelDown => MouseKind::ScrollDown,
                // Horizontal wheels are real on macOS trackpads (KERNEL
                // trap 3): routed since cycle 3.
                InMouseKind::WheelLeft => MouseKind::ScrollLeft,
                InMouseKind::WheelRight => MouseKind::ScrollRight,
            };
            Some(UiEvent::Mouse(MouseEvent {
                pos: m.pos,
                kind,
                mods: convert_mods(m.mods),
            }))
        }
        // Paste routes WHOLE to the focused widget (KERNEL trap 4: never
        // synthesized into per-char keys — that would reintroduce the
        // injection attack bracketed paste exists to prevent).
        Event::Paste(s) => Some(UiEvent::Paste(s.clone())),
        Event::FocusGained
        | Event::FocusLost
        | Event::Resize(_)
        | Event::CapsReply(_)
        | Event::Unknown(_) => None,
    }
}

/// Default quit chord: Ctrl+C. Raw mode clears ISIG, so Ctrl+C arrives as
/// an ordinary key event and quit policy belongs here (KERNEL request 3).
/// An app overrides by CONSUMING the event: any handler/shortcut that
/// handles Ctrl+C (returns consumed from dispatch) suppresses the default.
pub(crate) fn is_default_quit(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::Key(k) if k.key == Key::Char('c') && k.mods == Mods::CTRL
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::KeyEvent as InKeyEvent;

    #[test]
    fn keys_and_mods_map_field_by_field() {
        // Kernel ALT=2/CTRL=4 vs ui CTRL=2/ALT=4: transmuting would swap
        // them — this is the regression the test pins.
        let ev = Event::Key(InKeyEvent::char('s').with_mods(InMods::CTRL | InMods::ALT));
        let Some(UiEvent::Key(k)) = convert_event(&ev) else {
            panic!("must convert")
        };
        assert_eq!(k.key, Key::Char('s'));
        assert!(k.mods.contains(Mods::CTRL) && k.mods.contains(Mods::ALT));
        assert!(!k.mods.contains(Mods::SHIFT));
    }

    #[test]
    fn locks_strip_and_releases_drop() {
        let ev = Event::Key(InKeyEvent::char('s').with_mods(InMods::CTRL | InMods::CAPS_LOCK));
        let Some(UiEvent::Key(k)) = convert_event(&ev) else {
            panic!()
        };
        assert_eq!(k.mods, Mods::CTRL, "caps lock must not break chords");
        let mut rel = InKeyEvent::char('a');
        rel.kind = KeyEventKind::Release;
        assert_eq!(convert_event(&Event::Key(rel)), None);
    }

    #[test]
    fn ctrl_c_is_the_default_quit() {
        let ev = Event::Key(InKeyEvent::char('c').with_mods(InMods::CTRL));
        let ui = convert_event(&ev).expect("converts");
        assert!(is_default_quit(&ui));
        let plain = convert_event(&Event::Key(InKeyEvent::plain(KeyCode::Char('c')))).unwrap();
        assert!(!is_default_quit(&plain));
    }
}
