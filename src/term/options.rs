//! Session options and the ANSI byte sequences that realize them.
//!
//! OWNER: KERNEL.
//!
//! Both platform backends share these builders so enter/leave emissions can
//! never drift apart. Leave is generated in exact reverse order of enter:
//! the kitty keyboard pop must happen while the alternate screen still
//! absorbs any in-flight replies, and the alternate-screen exit must be last
//! so the primary screen is never polluted by teardown traffic.

/// Mouse tracking granularity. SGR encoding (1006) is always requested with
/// tracking because the legacy X10 byte encoding cannot express coordinates
/// past column 223 and is ambiguous in UTF-8 streams.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MouseMode {
    /// No mouse reports.
    #[default]
    Off,
    /// Presses/releases + wheel + drag while a button is held (mode 1002).
    ButtonDrag,
    /// All motion, even with no button held (mode 1003). Heavier traffic;
    /// hover-driven UIs opt in explicitly.
    AnyMotion,
}

impl MouseMode {
    /// Bytes arming this mode's tracking (SGR encoding rides along).
    /// Shared by `EnterOptions::enter_bytes` and the runtime
    /// suspend/resume verb (`Terminal::set_mouse_reporting`) so the
    /// arm/disarm pairs can never drift apart.
    pub(crate) const fn arm_bytes(self) -> &'static [u8] {
        match self {
            MouseMode::Off => b"",
            MouseMode::ButtonDrag => b"\x1b[?1002h\x1b[?1006h",
            MouseMode::AnyMotion => b"\x1b[?1003h\x1b[?1006h",
        }
    }

    /// Bytes disarming this mode, exact reverse order of `arm_bytes`.
    pub(crate) const fn disarm_bytes(self) -> &'static [u8] {
        match self {
            MouseMode::Off => b"",
            MouseMode::ButtonDrag => b"\x1b[?1006l\x1b[?1002l",
            MouseMode::AnyMotion => b"\x1b[?1006l\x1b[?1003l",
        }
    }
}

/// Kitty keyboard protocol progressive-enhancement flags
/// (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct KittyFlags(pub u8);

impl KittyFlags {
    /// Unambiguous Esc/Ctrl encodings (bit 1) — the flag that retires the
    /// ESC-disambiguation deadline.
    pub const DISAMBIGUATE: u8 = 1;
    /// Press/repeat/release visibility (bit 2).
    pub const REPORT_EVENT_TYPES: u8 = 2;
    /// Shifted/base-layout alternate key codes (bit 4).
    pub const REPORT_ALTERNATE: u8 = 4;
    /// Even plain text arrives as CSI-u escapes (bit 8; costs allocation
    /// on the hot path — off in [`KittyFlags::standard`]).
    pub const REPORT_ALL_AS_ESCAPES: u8 = 8;
    /// Associated text on key events (bit 16).
    pub const REPORT_TEXT: u8 = 16;

    /// The set a full-screen app wants: unambiguous Esc/Ctrl keys plus
    /// repeat/release visibility. "Report all keys as escapes" is left off:
    /// plain text arriving as text keeps the hot path allocation-free.
    pub const fn standard() -> Self {
        KittyFlags(Self::DISAMBIGUATE | Self::REPORT_EVENT_TYPES)
    }

    /// True when no enhancement is requested (nothing pushed on enter).
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The `CSI > flags u` push realizing these flags. Shared by
    /// [`EnterOptions::enter_bytes`] and the runtime upgrade verb
    /// (`Terminal::set_kitty_keyboard`) so the two emission points can
    /// never drift apart (the `MouseMode::arm_bytes` rule).
    pub(crate) fn push_bytes(self) -> Vec<u8> {
        format!("\x1b[>{}u", self.0).into_bytes()
    }

    /// The `CSI < u` pop undoing one push — the exact bytes
    /// [`EnterOptions::leave_bytes`] emits for its own entry.
    pub(crate) const POP_BYTES: &'static [u8] = b"\x1b[<u";
}

/// What `Terminal::enter` should switch on. Defaults are the full-screen
/// app posture; embedders (inline REPL widgets) can turn pieces off.
// NOTE: pixel-unit mouse reporting (DEC 1016) is deliberately NOT an
// EnterOptions field: it is a mid-session toggle (apps flip it while a
// pointer hovers an image), so it ships as the `Terminal::set_pixel_mouse`
// verb with the same latch-and-restore machinery as the cursor style.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EnterOptions {
    /// Switch to the alternate screen (DEC 1049) and back on leave.
    pub alternate_screen: bool,
    /// Hide the cursor (DEC 25) for compositor-driven cursor drawing.
    pub hide_cursor: bool,
    /// Mouse tracking granularity (SGR encoding always rides along).
    pub mouse: MouseMode,
    /// Bracketed paste (DEC 2004) — the ONLY paste path (see verbs on
    /// why OSC 52 reads are forbidden).
    pub bracketed_paste: bool,
    /// Focus in/out reporting (DEC 1004).
    pub focus_events: bool,
    /// Push these kitty keyboard flags on enter (pop on leave). Callers
    /// should gate this on `Capabilities::kitty_keyboard`; pushing at an
    /// unsupporting terminal is harmless (ignored) but pointless.
    pub kitty_keyboard: KittyFlags,
}

impl Default for EnterOptions {
    fn default() -> Self {
        EnterOptions {
            alternate_screen: true,
            hide_cursor: true,
            mouse: MouseMode::ButtonDrag,
            bracketed_paste: true,
            focus_events: true,
            kitty_keyboard: KittyFlags(0),
        }
    }
}

impl EnterOptions {
    /// Bytes that realize these options, written once on enter.
    pub fn enter_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        if self.alternate_screen {
            b.extend_from_slice(b"\x1b[?1049h");
        }
        if self.hide_cursor {
            b.extend_from_slice(b"\x1b[?25l");
        }
        b.extend_from_slice(self.mouse.arm_bytes());
        if self.bracketed_paste {
            b.extend_from_slice(b"\x1b[?2004h");
        }
        if self.focus_events {
            b.extend_from_slice(b"\x1b[?1004h");
        }
        if !self.kitty_keyboard.is_empty() {
            // Push (>) rather than set (=): leave pops our entry, restoring
            // whatever the terminal had, instead of clobbering the outer
            // program's flags (matters under nested screens like ssh+kitty).
            b.extend_from_slice(&self.kitty_keyboard.push_bytes());
        }
        b
    }

    /// Bytes that undo `enter_bytes`, in reverse order, plus defensive
    /// resets (SGR, synchronized-output) that cost nothing when already off.
    pub fn leave_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        if !self.kitty_keyboard.is_empty() {
            b.extend_from_slice(KittyFlags::POP_BYTES);
        }
        if self.focus_events {
            b.extend_from_slice(b"\x1b[?1004l");
        }
        if self.bracketed_paste {
            b.extend_from_slice(b"\x1b[?2004l");
        }
        b.extend_from_slice(self.mouse.disarm_bytes());
        // Defensive: a crashed frame may have left synchronized output open
        // or attributes set; both resets are no-ops otherwise.
        b.extend_from_slice(b"\x1b[?2026l\x1b[0m");
        if self.hide_cursor {
            b.extend_from_slice(b"\x1b[?25h");
        }
        if self.alternate_screen {
            b.extend_from_slice(b"\x1b[?1049l");
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_leave_mirror_each_other() {
        let opts = EnterOptions {
            kitty_keyboard: KittyFlags::standard(),
            ..EnterOptions::default()
        };
        let enter = String::from_utf8(opts.enter_bytes()).unwrap();
        let leave = String::from_utf8(opts.leave_bytes()).unwrap();
        assert!(enter.starts_with("\x1b[?1049h"));
        assert!(enter.ends_with("\x1b[>3u"));
        assert!(leave.starts_with("\x1b[<u"));
        assert!(leave.ends_with("\x1b[?1049l"));
        // Every mode set on enter is reset on leave.
        for mode in ["1002", "1006", "2004", "1004", "25", "1049"] {
            assert!(enter.contains(&format!("[?{mode}h")) || mode == "25");
            assert!(leave.contains(&format!("[?{mode}l")) || mode == "25");
        }
        assert!(enter.contains("[?25l") && leave.contains("[?25h"));
    }

    #[test]
    fn minimal_options_emit_minimal_bytes() {
        let opts = EnterOptions {
            alternate_screen: false,
            hide_cursor: false,
            mouse: MouseMode::Off,
            bracketed_paste: false,
            focus_events: false,
            kitty_keyboard: KittyFlags(0),
        };
        assert!(opts.enter_bytes().is_empty());
        // Leave still carries the defensive resets.
        assert_eq!(opts.leave_bytes(), b"\x1b[?2026l\x1b[0m".to_vec());
    }

    #[test]
    fn any_motion_uses_1003() {
        let opts = EnterOptions {
            mouse: MouseMode::AnyMotion,
            ..EnterOptions::default()
        };
        let enter = String::from_utf8(opts.enter_bytes()).unwrap();
        assert!(enter.contains("[?1003h"));
        assert!(!enter.contains("[?1002h"));
    }
}
