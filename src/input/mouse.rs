//! SGR mouse (DEC 1006) decoding: `CSI < b ; x ; y M|m`.
//!
//! OWNER: KERNEL. SGR is the only encoding we ever enable: the legacy X10
//! byte encoding saturates at column 223 and is ambiguous inside UTF-8
//! streams. Button bits: 0-2 button id, +4 shift, +8 alt, +16 ctrl,
//! +32 motion, +64 wheel, +128 extra buttons (8-11). Unlike X10, SGR
//! reports the *real* button on release (`m` final).

use super::params::CsiParams;
use super::{Event, Mods, MouseButton, MouseEvent, MouseKind};
use crate::base::Point;

pub(crate) fn decode_sgr(p: &CsiParams, final_byte: u8) -> Option<Event> {
    let b = p.get_or(0, 0);
    let x = p.get_or(1, 1);
    let y = p.get_or(2, 1);
    // Wire coordinates are 1-based; clamp defensively (a zero would mean a
    // broken emitter — better a clamped event than a lost one).
    let pos = Point::new(x.max(1) as i32 - 1, y.max(1) as i32 - 1);

    let mut mods = Mods::NONE;
    if b & 4 != 0 {
        mods = mods | Mods::SHIFT;
    }
    if b & 8 != 0 {
        mods = mods | Mods::ALT;
    }
    if b & 16 != 0 {
        mods = mods | Mods::CTRL;
    }

    let motion = b & 32 != 0;
    let wheel = b & 64 != 0;
    let extra = b & 128 != 0;
    let low = b & 3;

    let (kind, button) = if wheel {
        let kind = match low {
            0 => MouseKind::WheelUp,
            1 => MouseKind::WheelDown,
            2 => MouseKind::WheelLeft,
            _ => MouseKind::WheelRight,
        };
        (kind, MouseButton::None)
    } else {
        let button = match (extra, low) {
            (false, 0) => MouseButton::Left,
            (false, 1) => MouseButton::Middle,
            (false, 2) => MouseButton::Right,
            (false, _) => MouseButton::None, // 3 = no button (motion)
            (true, 0) => MouseButton::Back,
            (true, 1) => MouseButton::Forward,
            (true, _) => MouseButton::None, // buttons 10/11: unnamed
        };
        let kind = if motion {
            if button == MouseButton::None {
                MouseKind::Move
            } else {
                MouseKind::Drag
            }
        } else if final_byte == b'm' {
            MouseKind::Up
        } else {
            MouseKind::Down
        };
        (kind, button)
    };

    // The 1006 and 1016 wire grammars are IDENTICAL — only the active
    // mode decides the unit, which the parser cannot see. The reader owns
    // the pixel->cell conversion and fills `pixel` there.
    Some(Event::Mouse(MouseEvent::new(kind, button, pos, mods)))
}

#[cfg(test)]
mod tests {
    use super::super::Parser;
    use super::*;

    fn mouse(bytes: &[u8]) -> MouseEvent {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(bytes, &mut out);
        assert_eq!(out.len(), 1, "one event expected: {out:?}");
        match out.remove(0) {
            Event::Mouse(m) => m,
            other => panic!("expected mouse, got {other:?}"),
        }
    }

    #[test]
    fn press_release_have_real_buttons() {
        let m = mouse(b"\x1b[<0;1;1M");
        assert_eq!((m.kind, m.button), (MouseKind::Down, MouseButton::Left));
        assert_eq!(m.pos, Point::ZERO); // 1-based wire -> 0-based cells
        let m = mouse(b"\x1b[<0;1;1m");
        assert_eq!((m.kind, m.button), (MouseKind::Up, MouseButton::Left));
        let m = mouse(b"\x1b[<2;80;24M");
        assert_eq!((m.kind, m.button), (MouseKind::Down, MouseButton::Right));
        assert_eq!(m.pos, Point::new(79, 23));
        let m = mouse(b"\x1b[<1;5;5m");
        assert_eq!((m.kind, m.button), (MouseKind::Up, MouseButton::Middle));
    }

    #[test]
    fn drag_and_motion() {
        let m = mouse(b"\x1b[<32;10;5M"); // left held + motion
        assert_eq!((m.kind, m.button), (MouseKind::Drag, MouseButton::Left));
        let m = mouse(b"\x1b[<35;10;5M"); // motion, no button (1003)
        assert_eq!((m.kind, m.button), (MouseKind::Move, MouseButton::None));
    }

    #[test]
    fn wheel_all_directions() {
        assert_eq!(mouse(b"\x1b[<64;1;1M").kind, MouseKind::WheelUp);
        assert_eq!(mouse(b"\x1b[<65;1;1M").kind, MouseKind::WheelDown);
        assert_eq!(mouse(b"\x1b[<66;1;1M").kind, MouseKind::WheelLeft);
        assert_eq!(mouse(b"\x1b[<67;1;1M").kind, MouseKind::WheelRight);
    }

    #[test]
    fn modifiers_ride_along() {
        let m = mouse(b"\x1b[<16;3;4M"); // ctrl+left press
        assert_eq!(m.mods, Mods::CTRL);
        assert_eq!(m.button, MouseButton::Left);
        let m = mouse(b"\x1b[<28;3;4M"); // shift+alt+ctrl left
        assert_eq!(m.mods, Mods::SHIFT | Mods::ALT | Mods::CTRL);
        let m = mouse(b"\x1b[<69;2;2M"); // shift+wheel down
        assert_eq!((m.kind, m.mods), (MouseKind::WheelDown, Mods::SHIFT));
    }

    #[test]
    fn back_forward_buttons() {
        let m = mouse(b"\x1b[<128;1;1M");
        assert_eq!((m.kind, m.button), (MouseKind::Down, MouseButton::Back));
        let m = mouse(b"\x1b[<129;1;1m");
        assert_eq!((m.kind, m.button), (MouseKind::Up, MouseButton::Forward));
    }

    #[test]
    fn zero_coordinates_clamp() {
        let m = mouse(b"\x1b[<0;0;0M"); // broken emitter
        assert_eq!(m.pos, Point::ZERO);
    }
}
