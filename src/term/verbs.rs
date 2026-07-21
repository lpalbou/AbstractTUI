//! Session verbs: the byte builders behind the `Terminal` convenience
//! methods (cursor style, title, clipboard copy, bell/notify) and the tmux
//! passthrough wrapper.
//!
//! OWNER: KERNEL. Rationale + citations: `docs/design/term-input.md` §1.7
//! (verbs) and §1.8 (tmux passthrough).
//!
//! Everything here is a pure bytes-in/bytes-out builder so the trait's
//! default methods stay stateless (scripted terminals inherit them and
//! capture the bytes), while the platform backends latch "was this verb
//! used" to append the matching restore on leave.

/// DECSCUSR cursor styles (`CSI Ps SP q`, vt510 + xterm). `Default` is the
/// terminal's own configured cursor (Ps = 0), which is also what leave
/// restores after any style change.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CursorStyle {
    /// Ps 0: whatever the user configured their terminal to show.
    #[default]
    Default,
    /// Ps 1: blinking block.
    BlinkingBlock,
    /// Ps 2: steady block.
    SteadyBlock,
    /// Ps 3: blinking underline.
    BlinkingUnderline,
    /// Ps 4: steady underline.
    SteadyUnderline,
    /// Ps 5: blinking bar (xterm extension, universal in modern emulators).
    BlinkingBar,
    /// Ps 6: steady bar.
    SteadyBar,
}

impl CursorStyle {
    pub(crate) const fn param(self) -> u8 {
        match self {
            CursorStyle::Default => 0,
            CursorStyle::BlinkingBlock => 1,
            CursorStyle::SteadyBlock => 2,
            CursorStyle::BlinkingUnderline => 3,
            CursorStyle::SteadyUnderline => 4,
            CursorStyle::BlinkingBar => 5,
            CursorStyle::SteadyBar => 6,
        }
    }
}

/// `CSI Ps SP q` — the space intermediate is what distinguishes DECSCUSR
/// from DECSCA/XTVERSION finals.
pub(crate) fn cursor_style_bytes(style: CursorStyle) -> Vec<u8> {
    format!("\x1b[{} q", style.param()).into_bytes()
}

/// Reset emitted on leave when a style was set: Ps 0 = the user's own
/// configured cursor, not a hardcoded block.
pub(crate) const CURSOR_STYLE_RESET: &[u8] = b"\x1b[0 q";

/// Strip bytes that would break out of an OSC string frame (all C0
/// controls + DEL). Titles and notification texts are app data — a
/// filename containing ESC must not become an escape injection.
fn sanitize_osc_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// `OSC 0 ; title ST` — sets icon name + window title together (the form
/// interactive apps conventionally use; OSC 2 would set the window title
/// only).
pub(crate) fn set_title_bytes(title: &str) -> Vec<u8> {
    format!("\x1b]0;{}\x1b\\", sanitize_osc_text(title)).into_bytes()
}

/// XTWINOPS title-stack push/pop (`CSI 22;0t` / `CSI 23;0t`): saves the
/// user's title before our first set and restores it on leave. Terminals
/// without the stack ignore both — best effort by design.
pub(crate) const TITLE_PUSH: &[u8] = b"\x1b[22;0t";
pub(crate) const TITLE_POP: &[u8] = b"\x1b[23;0t";

/// `OSC 52 ; c ; base64 ST` — write `text` to the system clipboard through
/// the terminal. The `c` selection is the clipboard proper (not primary).
///
/// Deliberately WRITE-ONLY: the read form (`OSC 52 ; c ; ?`) asks the
/// terminal to type the clipboard back into the input stream, which is a
/// data-exfiltration vector (any app that can write bytes could steal the
/// clipboard); most terminals refuse or prompt, and this engine simply
/// never asks. Paste arrives through bracketed paste instead.
pub(crate) fn clipboard_copy_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + text.len() * 4 / 3 + 4);
    out.extend_from_slice(b"\x1b]52;c;");
    base64_into(text.as_bytes(), &mut out);
    out.extend_from_slice(b"\x1b\\");
    out
}

/// RFC 4648 standard base64 with padding, appended to `out`. Hand-rolled
/// per the dependency policy; `gfx` keeps its own decoder — the layers may
/// not import each other and 20 lines does not justify a base type.
fn base64_into(data: &[u8], out: &mut Vec<u8>) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63]);
        out.push(TABLE[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63]
        } else {
            b'='
        });
    }
}

/// SGR-Pixels mouse reporting (DEC 1016): pixel-unit coordinates on the
/// active SGR mouse encoding. A mid-session toggle (image hover), not an
/// EnterOptions bit. Terminals without 1016 ignore it and keep reporting
/// 1006 cells — consumers must key their unit interpretation on
/// `Capabilities::sgr_pixel_mouse`, never on having sent this.
pub(crate) const PIXEL_MOUSE_ON: &[u8] = b"\x1b[?1016h";
pub(crate) const PIXEL_MOUSE_OFF: &[u8] = b"\x1b[?1016l";

/// The audible/attention bell: a bare BEL. Universally safe; terminals map
/// it to sound/flash/urgency per user config.
pub(crate) const BELL: &[u8] = b"\x07";

/// Which wire carries a desktop notification. Terminals split into two
/// dialects with no overlap-free superset: emitting BOTH frames would
/// double-notify on terminals speaking both (ghostty), so the channel is
/// chosen per capabilities (`Capabilities::notify_channel`) rather than
/// sprayed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NotifyChannel {
    /// `OSC 9` — the iTerm2 convention (iTerm2, WezTerm, ghostty).
    Osc9,
    /// `OSC 99` — kitty's desktop-notifications protocol (kitty).
    Osc99,
    /// No notification dialect: degrade to the BEL attention bell.
    BellOnly,
}

/// `OSC 9 ; message ST` — desktop notification (iTerm2 convention, adopted
/// by WezTerm/ghostty). Callers gate via `Capabilities::notify_channel`;
/// unsupporting terminals ignore the frame.
pub(crate) fn notify_bytes(message: &str) -> Vec<u8> {
    format!("\x1b]9;{}\x1b\\", sanitize_osc_text(message)).into_bytes()
}

/// `OSC 99 ; ; body ST` — kitty desktop notification, basic form: empty
/// metadata section = defaults (single complete notification, payload is
/// the body). The richer protocol (ids, actions, icons) waits for a
/// consumer; the basic form is forward-compatible with it.
pub(crate) fn notify_bytes_osc99(message: &str) -> Vec<u8> {
    format!("\x1b]99;;{}\x1b\\", sanitize_osc_text(message)).into_bytes()
}

/// Wrap a payload for tmux passthrough: `ESC P tmux ; payload ESC \` with
/// every ESC in the payload doubled (tmux(1) manual, `allow-passthrough`).
/// The wrapper exists for graphics/OSC consumers once passthrough is
/// verified; the caps env pass still disables graphics under tmux because
/// `allow-passthrough` (off by default since tmux 3.3a) is invisible from
/// the environment — see `Capabilities::needs_tmux_passthrough`.
pub fn tmux_wrap(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + payload.len() / 8 + 16);
    out.extend_from_slice(b"\x1bPtmux;");
    for &b in payload {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_style_sequences() {
        assert_eq!(cursor_style_bytes(CursorStyle::Default), b"\x1b[0 q");
        assert_eq!(cursor_style_bytes(CursorStyle::BlinkingBlock), b"\x1b[1 q");
        assert_eq!(cursor_style_bytes(CursorStyle::SteadyBlock), b"\x1b[2 q");
        assert_eq!(
            cursor_style_bytes(CursorStyle::BlinkingUnderline),
            b"\x1b[3 q"
        );
        assert_eq!(
            cursor_style_bytes(CursorStyle::SteadyUnderline),
            b"\x1b[4 q"
        );
        assert_eq!(cursor_style_bytes(CursorStyle::BlinkingBar), b"\x1b[5 q");
        assert_eq!(cursor_style_bytes(CursorStyle::SteadyBar), b"\x1b[6 q");
        assert_eq!(CURSOR_STYLE_RESET, b"\x1b[0 q");
    }

    #[test]
    fn title_bytes_and_injection_defense() {
        assert_eq!(set_title_bytes("hello"), b"\x1b]0;hello\x1b\\");
        // Control bytes in app data must not break the OSC frame: an ESC
        // or BEL inside a filename is stripped, multi-byte text survives.
        assert_eq!(
            set_title_bytes("evil\x1b]710;x\x07name"),
            b"\x1b]0;evil]710;xname\x1b\\".to_vec()
        );
        assert_eq!(
            set_title_bytes("héllo — ✓"),
            "\x1b]0;héllo — ✓\x1b\\".as_bytes()
        );
    }

    #[test]
    fn base64_rfc4648_vectors() {
        fn enc(s: &str) -> String {
            let mut v = Vec::new();
            base64_into(s.as_bytes(), &mut v);
            String::from_utf8(v).unwrap()
        }
        assert_eq!(enc(""), "");
        assert_eq!(enc("f"), "Zg==");
        assert_eq!(enc("fo"), "Zm8=");
        assert_eq!(enc("foo"), "Zm9v");
        assert_eq!(enc("foob"), "Zm9vYg==");
        assert_eq!(enc("fooba"), "Zm9vYmE=");
        assert_eq!(enc("foobar"), "Zm9vYmFy");
    }

    #[test]
    fn clipboard_frame_shape() {
        assert_eq!(clipboard_copy_bytes("hello"), b"\x1b]52;c;aGVsbG8=\x1b\\");
        // Empty text is the documented "clear clipboard" form.
        assert_eq!(clipboard_copy_bytes(""), b"\x1b]52;c;\x1b\\");
        // Payload is base64: hostile text cannot escape the frame even
        // without sanitization.
        let bytes = clipboard_copy_bytes("\x1b]52;c;evil\x07");
        let inner = &bytes[b"\x1b]52;c;".len()..bytes.len() - 2];
        assert!(inner
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')));
    }

    #[test]
    fn notify_and_bell() {
        assert_eq!(notify_bytes("build done"), b"\x1b]9;build done\x1b\\");
        assert_eq!(notify_bytes("a\x1bb"), b"\x1b]9;ab\x1b\\".to_vec());
        assert_eq!(BELL, b"\x07");
        // kitty OSC 99 basic form: empty metadata, sanitized body.
        assert_eq!(
            notify_bytes_osc99("build done"),
            b"\x1b]99;;build done\x1b\\"
        );
        assert_eq!(
            notify_bytes_osc99("a\x1b]99;evil\x07b"),
            b"\x1b]99;;a]99;evilb\x1b\\".to_vec()
        );
    }

    #[test]
    fn tmux_wrap_doubles_escapes() {
        // A kitty APC inside the wrapper: every payload ESC doubled, frame
        // intact around it.
        let wrapped = tmux_wrap(b"\x1b_Ga=q;AAAA\x1b\\");
        assert_eq!(
            wrapped,
            b"\x1bPtmux;\x1b\x1b_Ga=q;AAAA\x1b\x1b\\\x1b\\".to_vec()
        );
        // No ESC in payload -> byte-identical passthrough inside the frame.
        assert_eq!(tmux_wrap(b"plain"), b"\x1bPtmux;plain\x1b\\".to_vec());
    }
}
