//! The resumable input state machine: arbitrary byte chunks in, events out.
//!
//! OWNER: KERNEL. Grammar notes: `docs/design/term-input.md` §3.
//!
//! Invariants this file defends (REDTEAM fuzzes them):
//! - never panics, for any byte sequence, split at any boundary;
//! - bounded memory: escape sequences cap at [`SEQ_CAP`], string frames at
//!   [`STR_CAP`], pastes flush in [`PASTE_FLUSH`] chunks, `Unknown` events
//!   carry at most [`UNKNOWN_CAP`] bytes;
//! - garbage never leaks into the text stream as fake keystrokes: whatever
//!   is not understood is swallowed as [`Event::Unknown`].

use super::params::CsiParams;
use super::{kitty, legacy, mouse};
use super::{Event, KeyCode, KeyEvent, Mods};
use crate::term::caps::CapsReply;

/// Max raw length of a CSI/SS3 sequence before it is declared garbage.
pub const SEQ_CAP: usize = 256;
/// Max payload of OSC/DCS/APC string frames; excess is dropped (frame sync
/// is kept — we still hunt the real terminator).
pub const STR_CAP: usize = 4096;
/// Max raw bytes carried by an `Event::Unknown`.
pub const UNKNOWN_CAP: usize = 64;
/// Paste content flushes as a `Paste` event every this many bytes, so a
/// hostile never-terminated paste cannot grow memory without bound.
pub const PASTE_FLUSH: usize = 64 * 1024;

const NEEDLE_PASTE_END: &[u8; 6] = b"\x1b[201~";

/// What the parser is holding between chunks (drives the reader deadlines).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Nothing deadline-worthy buffered.
    None,
    /// A lone ESC: after a short deadline it becomes the Esc key.
    BareEsc,
    /// A partial escape sequence: after a long deadline it flushes as
    /// `Unknown` (or Alt+intro when it is just two bytes).
    Sequence,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum StrKind {
    Osc,
    Dcs,
    Apc,
    /// PM / SOS: legal frames nothing speaks to apps with; swallowed.
    Other(u8),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    /// A CSI that overflowed [`SEQ_CAP`]: silently consume param bytes
    /// until the final byte so the oversized tail can never replay as fake
    /// keystrokes (the head already went out as `Unknown`).
    CsiDiscard,
    Ss3,
    Str(StrKind),
    Paste,
    /// Legacy X10 mouse (`CSI M` + 3 raw bytes): consumed so the payload
    /// can never replay as fake keystrokes. We only ever enable SGR mouse,
    /// so this fires on foreign traffic only.
    X10 {
        left: u8,
    },
}

/// The resumable input state machine: feed arbitrary byte chunks, take
/// decoded [`Event`]s. Never panics, never allocates unboundedly, never
/// leaks garbage into the text stream (module docs carry the fuzz
/// invariants).
pub struct Parser {
    state: State,
    // Incremental UTF-8 (Ground only; escape grammar is pure ASCII).
    u_buf: [u8; 4],
    u_len: u8,
    u_need: u8,
    /// An ESC immediately preceded this text byte: apply Alt to the next
    /// completed key.
    alt_pending: bool,
    /// Raw bytes of the escape sequence being accumulated (for `Unknown`).
    seq: Vec<u8>,
    str_payload: Vec<u8>,
    str_esc: bool,
    paste_buf: Vec<u8>,
    paste_match: u8,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// A parser in ground state.
    pub fn new() -> Self {
        Parser {
            state: State::Ground,
            u_buf: [0; 4],
            u_len: 0,
            u_need: 0,
            alt_pending: false,
            seq: Vec::with_capacity(SEQ_CAP),
            str_payload: Vec::with_capacity(256),
            str_esc: false,
            paste_buf: Vec::new(),
            paste_match: 0,
        }
    }

    /// Feed one chunk; decoded events append to `out` (reused by callers to
    /// stay allocation-light).
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Event>) {
        for &b in bytes {
            self.step(b, out);
        }
    }

    /// What incomplete input is buffered (drives the reader's
    /// ESC-disambiguation deadlines; see [`Pending`]).
    pub fn pending(&self) -> Pending {
        match self.state {
            State::Ground | State::Paste => Pending::None,
            State::Esc => Pending::BareEsc,
            _ => Pending::Sequence,
        }
    }

    /// Resolve buffered ambiguity after a deadline (owned by `EventReader`):
    /// a bare ESC becomes the Esc key; a two-byte `ESC x` becomes Alt+x; a
    /// longer torn sequence flushes as `Unknown`.
    pub fn flush_pending(&mut self, out: &mut Vec<Event>) {
        match self.state {
            State::Ground | State::Paste => {}
            State::Esc => {
                out.push(Event::Key(KeyEvent::plain(KeyCode::Esc)));
                self.reset_to_ground();
            }
            State::CsiDiscard => self.reset_to_ground(),
            State::Csi | State::Ss3 => {
                if self.seq.len() == 2 {
                    // A dangling "ESC [" / "ESC O" is exactly what Alt+[ /
                    // Alt+O produce; a longer tail is torn garbage.
                    let c = self.seq[1] as char;
                    out.push(Event::Key(KeyEvent::new(KeyCode::Char(c), Mods::ALT)));
                } else {
                    push_unknown(out, &self.seq);
                }
                self.reset_to_ground();
            }
            State::Str(kind) => {
                let mut raw = vec![0x1b, str_intro(kind)];
                raw.extend_from_slice(&self.str_payload);
                push_unknown(out, &raw);
                self.reset_to_ground();
            }
            State::X10 { .. } => {
                push_unknown(out, &self.seq);
                self.reset_to_ground();
            }
        }
    }

    /// End-of-stream: also resolves UTF-8 partials (as U+FFFD) and emits
    /// any buffered paste content — nothing silently disappears.
    pub fn finish(&mut self, out: &mut Vec<Event>) {
        if self.u_need > 0 {
            self.u_len = 0;
            self.u_need = 0;
            self.emit_char(char::REPLACEMENT_CHARACTER, out);
        }
        if self.state == State::Paste {
            // A half-matched terminator prefix is content at end of stream.
            self.paste_buf
                .extend_from_slice(&NEEDLE_PASTE_END[..self.paste_match as usize]);
            self.paste_match = 0;
            self.end_paste(out);
            self.state = State::Ground;
        }
        self.flush_pending(out);
    }

    fn reset_to_ground(&mut self) {
        self.state = State::Ground;
        self.seq.clear();
        self.str_payload.clear();
        self.str_esc = false;
    }

    fn step(&mut self, b: u8, out: &mut Vec<Event>) {
        match self.state {
            State::Ground => self.step_ground(b, out),
            State::Esc => self.step_esc(b, out),
            State::Csi => self.step_csi(b, out),
            State::CsiDiscard => match b {
                0x20..=0x3f => {} // still inside the oversized sequence
                _ => {
                    // Final byte (or anything illegal) ends the discard;
                    // an ESC starts over, everything else is dropped with
                    // the sequence it belonged to.
                    self.reset_to_ground();
                    if b == 0x1b {
                        self.step(b, out);
                    }
                }
            },
            State::Ss3 => self.step_ss3(b, out),
            State::Str(kind) => self.step_str(kind, b, out),
            State::Paste => self.step_paste(b, out),
            State::X10 { left } => {
                self.seq.push(b);
                if left <= 1 {
                    push_unknown(out, &self.seq);
                    self.reset_to_ground();
                } else {
                    self.state = State::X10 { left: left - 1 };
                }
            }
        }
    }

    fn step_ground(&mut self, b: u8, out: &mut Vec<Event>) {
        if self.u_need > 0 {
            self.utf8_continue(b, out);
            return;
        }
        match b {
            0x1b => {
                self.seq.clear();
                self.seq.push(0x1b);
                self.state = State::Esc;
            }
            0x00..=0x1f | 0x7f => self.emit_control(b, out),
            0x20..=0x7e => self.emit_char(b as char, out),
            0x80..=0xff => self.utf8_lead(b, out),
        }
    }

    fn step_esc(&mut self, b: u8, out: &mut Vec<Event>) {
        match b {
            b'[' => {
                self.seq.push(b);
                self.state = State::Csi;
            }
            b'O' => {
                self.seq.push(b);
                self.state = State::Ss3;
            }
            b']' => self.start_str(StrKind::Osc),
            b'P' => self.start_str(StrKind::Dcs),
            b'_' => self.start_str(StrKind::Apc),
            b'^' | b'X' => self.start_str(StrKind::Other(b)),
            0x1b => {
                // ESC ESC: the first is a real Esc key, the second starts a
                // fresh escape (whatever follows decides what it is).
                out.push(Event::Key(KeyEvent::plain(KeyCode::Esc)));
                // state stays Esc; seq stays [ESC]
            }
            0x00..=0x1a | 0x1c..=0x1f | 0x7f => {
                // Alt + control key (e.g. Alt+Enter, Alt+Backspace).
                self.state = State::Ground;
                self.seq.clear();
                self.alt_pending = true;
                self.emit_control(b, out);
            }
            0x20..=0x7e => {
                self.state = State::Ground;
                self.seq.clear();
                self.alt_pending = true;
                self.emit_char(b as char, out);
            }
            0x80..=0xff => {
                // Alt + non-ASCII char: the char completes through the
                // ordinary UTF-8 path with alt applied at emission.
                self.state = State::Ground;
                self.seq.clear();
                self.alt_pending = true;
                self.utf8_lead(b, out);
            }
        }
    }

    fn start_str(&mut self, kind: StrKind) {
        self.seq.clear();
        self.str_payload.clear();
        self.str_esc = false;
        self.state = State::Str(kind);
    }

    fn step_csi(&mut self, b: u8, out: &mut Vec<Event>) {
        if self.seq.len() >= SEQ_CAP {
            push_unknown(out, &self.seq);
            self.seq.clear();
            self.state = State::CsiDiscard;
            self.step(b, out);
            return;
        }
        self.seq.push(b);
        match b {
            0x20..=0x3f => {} // params + intermediates accumulate
            0x40..=0x7e => {
                self.dispatch_csi(b, out);
                if self.state == State::Csi {
                    self.reset_to_ground();
                }
            }
            0x1b => {
                // A torn sequence must never eat the next one.
                self.seq.pop();
                push_unknown(out, &self.seq);
                self.seq.clear();
                self.seq.push(0x1b);
                self.str_payload.clear();
                self.str_esc = false;
                self.state = State::Esc;
            }
            _ => {
                // C0 control or DEL inside a CSI: declare garbage (design
                // doc §3.1 — aborting beats silently executing controls).
                push_unknown(out, &self.seq);
                self.reset_to_ground();
            }
        }
    }

    fn step_ss3(&mut self, b: u8, out: &mut Vec<Event>) {
        if b == 0x1b {
            push_unknown(out, &self.seq);
            self.seq.clear();
            self.seq.push(0x1b);
            self.state = State::Esc;
            return;
        }
        self.seq.push(b);
        match legacy::decode_ss3(b) {
            Some(ev) => out.push(ev),
            None => push_unknown(out, &self.seq),
        }
        self.reset_to_ground();
    }

    fn step_str(&mut self, kind: StrKind, b: u8, out: &mut Vec<Event>) {
        if self.str_esc {
            self.str_esc = false;
            match b {
                b'\\' => {
                    // ST (ESC \) terminates the frame.
                    self.dispatch_str(kind, out);
                    self.reset_to_ground();
                }
                0x1b => {
                    // The previous ESC was content; this one is the new
                    // terminator candidate.
                    self.push_str_payload(0x1b);
                    self.str_esc = true;
                }
                _ => {
                    self.push_str_payload(0x1b);
                    self.push_str_payload(b);
                }
            }
            return;
        }
        match b {
            0x1b => self.str_esc = true,
            0x07 if kind == StrKind::Osc => {
                // BEL is a legal OSC terminator (xterm tradition).
                self.dispatch_str(kind, out);
                self.reset_to_ground();
            }
            _ => self.push_str_payload(b),
        }
    }

    fn push_str_payload(&mut self, b: u8) {
        // Overflow drops content but keeps hunting the real terminator, so
        // one giant foreign frame cannot desynchronize everything after it.
        if self.str_payload.len() < STR_CAP {
            self.str_payload.push(b);
        }
    }

    fn dispatch_str(&mut self, kind: StrKind, out: &mut Vec<Event>) {
        let payload = std::mem::take(&mut self.str_payload);
        match kind {
            StrKind::Osc => out.push(Event::CapsReply(CapsReply::Osc { raw: payload })),
            StrKind::Dcs => {
                if let Some(rest) = payload.strip_prefix(b">|") {
                    out.push(Event::CapsReply(CapsReply::XtVersion {
                        text: String::from_utf8_lossy(rest).trim().to_string(),
                    }));
                } else if payload.starts_with(b"1+r") || payload.starts_with(b"0+r") {
                    out.push(Event::CapsReply(CapsReply::XtGetTcap { raw: payload }));
                } else {
                    let mut raw = vec![0x1b, b'P'];
                    raw.extend_from_slice(&payload);
                    push_unknown(out, &raw);
                }
            }
            StrKind::Apc => {
                if payload.first() == Some(&b'G') {
                    out.push(Event::CapsReply(CapsReply::KittyGraphics { raw: payload }));
                } else {
                    let mut raw = vec![0x1b, b'_'];
                    raw.extend_from_slice(&payload);
                    push_unknown(out, &raw);
                }
            }
            StrKind::Other(intro) => {
                let mut raw = vec![0x1b, intro];
                raw.extend_from_slice(&payload);
                push_unknown(out, &raw);
            }
        }
    }

    fn dispatch_csi(&mut self, final_byte: u8, out: &mut Vec<Event>) {
        let inner = &self.seq[2..self.seq.len().saturating_sub(1).max(2)];
        let p = CsiParams::parse(inner);
        match (p.private, final_byte) {
            (b'<', b'M') | (b'<', b'm') => match mouse::decode_sgr(&p, final_byte) {
                Some(ev) => out.push(ev),
                None => push_unknown(out, &self.seq),
            },
            (b'?', b'u') => out.push(Event::CapsReply(CapsReply::KittyKeyboard {
                flags: p.get_or(0, 0),
            })),
            (b'?', b'c') => out.push(Event::CapsReply(CapsReply::PrimaryDa { params: p.list() })),
            (b'?', b'y') if p.intermediate == b'$' => {
                out.push(Event::CapsReply(CapsReply::DecMode {
                    mode: p.get_or(0, 0),
                    status: p.get_or(1, 0).min(255) as u8,
                }));
            }
            (b'?', b'S') => {
                // XTSMGRAPHICS report: CSI ? item ; status ; value S.
                out.push(Event::CapsReply(CapsReply::XtSmGraphics {
                    item: p.get_or(0, 0),
                    status: p.get_or(1, 0),
                    value: p.get_or(2, 0),
                }));
            }
            (0, b'u') => match kitty::decode_csi_u(&p) {
                Some(ev) => out.push(ev),
                None => push_unknown(out, &self.seq),
            },
            (0, b'~') => {
                let code = p.get_or(0, 0);
                if code == 200 {
                    self.paste_buf.clear();
                    self.paste_match = 0;
                    self.state = State::Paste;
                    self.seq.clear();
                } else if code == 201 {
                    // Stray paste-end with no paste open: foreign traffic.
                    push_unknown(out, &self.seq);
                } else {
                    match legacy::decode_tilde(&p) {
                        Some(ev) => out.push(ev),
                        None => push_unknown(out, &self.seq),
                    }
                }
            }
            (0, b'R') if p.len() >= 2 && p.get_or(0, 1) != 1 => {
                // CSI row;col R is a cursor position report; CSI 1;mods R
                // is legacy F3 (the collision kitty's protocol eliminates).
                out.push(Event::CapsReply(CapsReply::CursorPos {
                    row: p.get_or(0, 1),
                    col: p.get_or(1, 1),
                }));
            }
            (0, b'I') => out.push(Event::FocusGained),
            (0, b'O') => out.push(Event::FocusLost),
            (0, b't') if p.get_or(0, 0) != 0 => {
                // XTWINOPS report (op 6 = cell pixel size, 4 = text area
                // pixels, 8 = chars...). Routed to caps — window geometry
                // reports are terminal answers, never keystrokes.
                out.push(Event::CapsReply(CapsReply::WindowOp {
                    op: p.get_or(0, 0),
                    a: p.get_or(1, 0),
                    b: p.get_or(2, 0),
                }));
            }
            (0, b'M') if p.len() < 3 => {
                // Legacy X10 mouse: exactly 3 raw payload bytes follow;
                // seq keeps accumulating for the Unknown emission.
                // (With >= 3 params this is urxvt 1015 encoding instead —
                // self-delimiting, handled by the fallthrough as Unknown.)
                self.state = State::X10 { left: 3 };
            }
            (0, f) => match legacy::decode_named(f, &p) {
                Some(ev) => out.push(ev),
                None => push_unknown(out, &self.seq),
            },
            _ => push_unknown(out, &self.seq),
        }
    }

    fn step_paste(&mut self, b: u8, out: &mut Vec<Event>) {
        let m = self.paste_match as usize;
        if b == NEEDLE_PASTE_END[m] {
            self.paste_match += 1;
            if self.paste_match as usize == NEEDLE_PASTE_END.len() {
                self.paste_match = 0;
                self.end_paste(out);
                self.reset_to_ground();
            }
            return;
        }
        // Mismatch: matched-so-far bytes were real content. A new match can
        // only restart at an ESC — the needle's interior contains none, so
        // this replay is exact, not heuristic.
        self.paste_buf.extend_from_slice(&NEEDLE_PASTE_END[..m]);
        self.paste_match = 0;
        if b == NEEDLE_PASTE_END[0] {
            self.paste_match = 1;
        } else {
            self.paste_buf.push(b);
        }
        if self.paste_buf.len() >= PASTE_FLUSH {
            self.flush_paste_chunk(out);
        }
    }

    /// Mid-paste flush: emit the buffer UP TO the last complete UTF-8
    /// boundary, holding back an incomplete trailing sequence (≤ 3 bytes)
    /// for the next chunk. Editors reassemble pastes by concatenation, so
    /// a chunk seam through a multibyte character must never turn it into
    /// replacement chars — the terminator/finish path emits everything
    /// (a genuinely truncated final sequence is honestly U+FFFD there).
    fn flush_paste_chunk(&mut self, out: &mut Vec<Event>) {
        let boundary = last_utf8_boundary(&self.paste_buf);
        if boundary == 0 {
            // All-continuation garbage (not real UTF-8): lossy as-is.
            self.end_paste(out);
            return;
        }
        let tail = self.paste_buf.split_off(boundary);
        let head = std::mem::replace(&mut self.paste_buf, tail);
        out.push(Event::Paste(String::from_utf8_lossy(&head).into_owned()));
    }

    /// Emit buffered paste content. Deliberately does NOT touch
    /// `paste_match`: the mid-paste flush (PASTE_FLUSH) can fire while a
    /// terminator prefix is half-matched, and resetting the match there
    /// would drop those bytes and miss the real terminator.
    fn end_paste(&mut self, out: &mut Vec<Event>) {
        let content = std::mem::take(&mut self.paste_buf);
        // Invalid UTF-8 inside a paste becomes U+FFFD, same as typed input.
        out.push(Event::Paste(String::from_utf8_lossy(&content).into_owned()));
    }

    // ---- UTF-8 (incremental, never panics) ----

    fn utf8_lead(&mut self, b: u8, out: &mut Vec<Event>) {
        let need = match b {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            // 0x80..=0xC1 (stray continuation / overlong) and 0xF5..=0xFF.
            _ => {
                self.emit_char(char::REPLACEMENT_CHARACTER, out);
                return;
            }
        };
        self.u_buf[0] = b;
        self.u_len = 1;
        self.u_need = need;
    }

    fn utf8_continue(&mut self, b: u8, out: &mut Vec<Event>) {
        if (0x80..=0xbf).contains(&b) {
            self.u_buf[self.u_len as usize] = b;
            self.u_len += 1;
            if self.u_len == self.u_need {
                let complete = &self.u_buf[..self.u_len as usize];
                // from_utf8 rejects overlong forms and surrogates for us.
                let c = std::str::from_utf8(complete)
                    .ok()
                    .and_then(|s| s.chars().next())
                    .unwrap_or(char::REPLACEMENT_CHARACTER);
                self.u_len = 0;
                self.u_need = 0;
                self.emit_char(c, out);
            }
            return;
        }
        // Broken continuation: one U+FFFD for the partial, then reprocess
        // this byte with clean state (bounded recursion: depth one).
        self.u_len = 0;
        self.u_need = 0;
        self.emit_char(char::REPLACEMENT_CHARACTER, out);
        self.step(b, out);
    }

    // ---- key emission ----

    fn take_alt(&mut self) -> Mods {
        if self.alt_pending {
            self.alt_pending = false;
            Mods::ALT
        } else {
            Mods::NONE
        }
    }

    fn emit_char(&mut self, c: char, out: &mut Vec<Event>) {
        let mods = self.take_alt();
        out.push(Event::Key(KeyEvent::new(KeyCode::Char(c), mods)));
    }

    fn emit_control(&mut self, b: u8, out: &mut Vec<Event>) {
        let mods = self.take_alt();
        let mut ev = legacy::control_key(b);
        ev.mods = ev.mods | mods;
        out.push(Event::Key(ev));
    }
}

pub(crate) fn push_unknown(out: &mut Vec<Event>, bytes: &[u8]) {
    let cap = bytes.len().min(UNKNOWN_CAP);
    out.push(Event::Unknown(bytes[..cap].to_vec()));
}

/// Index of the byte after the last COMPLETE UTF-8 sequence: `len` when
/// the buffer ends on a boundary, else the start of the trailing partial
/// sequence. Returns 0 only when the whole (short) buffer is one partial
/// sequence or leading continuation garbage.
fn last_utf8_boundary(buf: &[u8]) -> usize {
    let len = buf.len();
    // A lead byte sits within the last 4 positions or the tail is garbage.
    let scan_from = len.saturating_sub(4);
    for i in (scan_from..len).rev() {
        let b = buf[i];
        if (0x80..=0xbf).contains(&b) {
            continue; // continuation: keep scanning back for its lead
        }
        let width = match b {
            0x00..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1, // invalid lead: nothing after it can complete
        };
        return if i + width <= len { len } else { i };
    }
    // No lead byte in the window: either continuation garbage (emit all)
    // or a buffer shorter than one sequence (hold everything).
    if len >= 4 {
        len
    } else {
        0
    }
}

fn str_intro(kind: StrKind) -> u8 {
    match kind {
        StrKind::Osc => b']',
        StrKind::Dcs => b'P',
        StrKind::Apc => b'_',
        StrKind::Other(b) => b,
    }
}
