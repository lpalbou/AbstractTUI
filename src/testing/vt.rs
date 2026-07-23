//! `VtScreen`: a small VT100/xterm interpreter over a model screen grid.
//! This is the ground truth for the diff/present correctness property:
//! bytes emitted by the presenter, applied here over the previous frame,
//! must reproduce the intended frame exactly.
//!
//! OWNER: REDTEAM.
//!
//! Scope: exactly what our presenter is allowed to emit (cursor motion,
//! CR/LF, ED/EL/ECH, SGR incl. truecolor/256/16, DECSET/DECRST mode
//! flags, OSC 8 hyperlinks, kitty keyboard push/pop framing) plus enough
//! parsing armor (string frames, CAN/SUB aborts, caps) that arbitrary
//! byte soup never panics. Anything outside the modeled set is consumed
//! safely and COUNTED — `unknown_seq_count() == 0` is the presenter's
//! cleanliness assertion. CSI/SGR dispatch lives in `vt_csi.rs`.

use crate::base::{Point, Size};

use super::grid::{Grid, Paint, VtCell};

/// Parser cap: a CSI sequence longer than this is hostile or corrupt.
const MAX_CSI_LEN: usize = 256;
/// Parser cap: OSC/DCS/APC payloads beyond this are discarded (frame sync
/// is kept — we still hunt the terminator).
const MAX_STRING_LEN: usize = 4096;
/// How many unknown-sequence samples to keep for diagnostics.
const MAX_UNKNOWN_SAMPLES: usize = 32;

#[derive(Debug, Default)]
enum State {
    #[default]
    Ground,
    Esc,
    /// One-byte charset designation argument (`ESC ( B` etc.).
    Charset,
    Csi {
        buf: Vec<u8>,
        overflow: bool,
    },
    Osc {
        buf: Vec<u8>,
        /// Saw ESC, waiting to see if it is ST (`ESC \`).
        esc: bool,
    },
    /// DCS / APC / PM / SOS frames: consumed until ST, counted, unmodeled.
    StringFrame {
        kind: u8,
        len: usize,
        esc: bool,
    },
}

pub use super::vt_state::{Modes, VtCounters};

pub struct VtScreen {
    grid: Grid,
    cursor: Point,
    /// Deferred autowrap: a glyph was just written into the last column;
    /// the NEXT printable wraps first (xterm semantics).
    wrap_pending: bool,
    paint: Paint,
    modes: Modes,
    state: State,
    /// Incremental UTF-8: pending lead + continuation bytes (max 3 pending).
    utf8: Vec<u8>,
    /// Where the last glyph was written (combining marks attach here).
    last_write: Option<Point>,
    /// A ZWJ was appended to the last cluster: the NEXT printable joins
    /// that cluster instead of printing (the engine-wide cluster-width
    /// convention — render.md §2.5; joined sequences are one ≤2-wide
    /// cell). Cleared by any control/motion byte.
    pending_zwj: bool,
    /// The last written glyph was a LONE regional-indicator scalar: the
    /// NEXT regional indicator fuses with it into one flag cluster
    /// (width 2), matching the grapheme segmentation the render layer
    /// uses. Any non-indicator write clears it (the flag says so by
    /// setting it false on every normal glyph).
    pending_regional: bool,
    /// Saved cursor for DECSC/DECRC (ESC 7 / ESC 8).
    saved_cursor: Option<(Point, Paint)>,
    /// DECSTBM margins: (top, bottom), 0-based inclusive. None = full
    /// screen. LF/RI/SU/SD/IL/DL scroll within these; ED/EL ignore them
    /// (xterm semantics).
    margins: Option<(i32, i32)>,
    /// DECSCUSR cursor style (Ps of `CSI Ps SP q`; 0/1 blinking block
    /// default). Tracked state, not rendered.
    cursor_style: u32,
    /// Last OSC 52 clipboard write: (selection params, base64 payload).
    /// Tracked so KERNEL's clipboard emission is assertable and never
    /// counts as unknown traffic.
    clipboard: Option<(String, String)>,
    /// OSC 9 notifications (iTerm2-style), in arrival order.
    notifications: Vec<String>,
    /// OSC 8 hyperlink registry: id -> (params, uri). Cell paints carry ids.
    links: Vec<(String, String)>,
    counters: VtCounters,
    unknown_samples: Vec<String>,
    /// Window title via OSC 0/2 (tracked; not part of styled dumps).
    title: Option<String>,
}

impl VtScreen {
    pub fn new(size: Size) -> VtScreen {
        let mut modes = Modes::default();
        // Terminal boot posture: cursor visible, autowrap on.
        modes.insert(25);
        modes.insert(7);
        VtScreen {
            grid: Grid::new(size.w, size.h),
            cursor: Point::ZERO,
            wrap_pending: false,
            paint: Paint::default(),
            modes,
            state: State::Ground,
            utf8: Vec::new(),
            last_write: None,
            pending_zwj: false,
            pending_regional: false,
            saved_cursor: None,
            margins: None,
            cursor_style: 0,
            clipboard: None,
            notifications: Vec::new(),
            links: Vec::new(),
            counters: VtCounters::default(),
            unknown_samples: Vec::new(),
            title: None,
        }
    }

    pub fn size(&self) -> Size {
        Size::new(self.grid.w, self.grid.h)
    }
    pub fn cursor(&self) -> Point {
        self.cursor
    }
    pub fn modes(&self) -> &Modes {
        &self.modes
    }
    pub fn counters(&self) -> &VtCounters {
        &self.counters
    }
    pub fn unknown_seq_count(&self) -> u64 {
        self.counters.unknown
    }
    /// First few unknown sequences, human-readable — put this in assert
    /// messages so a presenter regression names itself.
    pub fn unknown_samples(&self) -> &[String] {
        &self.unknown_samples
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn cell(&self, x: i32, y: i32) -> Option<&VtCell> {
        self.grid.cell(x, y)
    }
    /// Resolve a cell's hyperlink id to its (params, uri).
    pub fn link_target(&self, id: u32) -> Option<(&str, &str)> {
        self.links
            .get(id as usize)
            .map(|(p, u)| (p.as_str(), u.as_str()))
    }
    /// The paint that would be applied to the next printable (test hook).
    pub fn current_paint(&self) -> Paint {
        self.paint
    }

    // ---- byte pump ------------------------------------------------------

    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.feed_byte(b);
        }
    }

    fn feed_byte(&mut self, b: u8) {
        // CAN/SUB abort any escape sequence in progress (xterm behavior).
        if (b == 0x18 || b == 0x1a) && !matches!(self.state, State::Ground) {
            self.note_unknown("sequence aborted by CAN/SUB");
            self.state = State::Ground;
            return;
        }
        match std::mem::take(&mut self.state) {
            State::Ground => self.ground_byte(b),
            State::Esc => self.esc_byte(b),
            State::Charset => { /* one-byte charset arg consumed; not modeled */ }
            State::Csi { buf, overflow } => self.csi_byte(b, buf, overflow),
            State::Osc { buf, esc } => self.osc_byte(b, buf, esc),
            State::StringFrame { kind, len, esc } => self.string_byte(b, kind, len, esc),
        }
    }

    fn ground_byte(&mut self, b: u8) {
        match b {
            0x1b => {
                self.flush_partial_utf8();
                self.state = State::Esc;
            }
            0x0d => {
                self.flush_partial_utf8();
                self.cursor.x = 0;
                self.wrap_pending = false;
            }
            0x0a..=0x0c => {
                self.flush_partial_utf8();
                self.line_feed();
            }
            0x08 => {
                self.flush_partial_utf8();
                self.cursor.x = (self.cursor.x - 1).max(0);
                self.wrap_pending = false;
            }
            0x09 => {
                self.flush_partial_utf8();
                // Tab stops every 8 columns, clamped to the last column.
                let next = ((self.cursor.x / 8) + 1) * 8;
                self.cursor.x = next.min(self.grid.w - 1);
                self.wrap_pending = false;
            }
            0x00..=0x1f | 0x7f => {
                // Unmodeled C0 control: a clean presenter never emits these.
                self.flush_partial_utf8();
                self.note_unknown(&format!("C0 control 0x{b:02x}"));
            }
            _ => self.utf8_byte(b),
        }
    }

    // ---- UTF-8 + printing ------------------------------------------------

    fn utf8_byte(&mut self, b: u8) {
        if self.utf8.is_empty() && b < 0x80 {
            self.print_char(b as char);
            return;
        }
        self.utf8.push(b);
        // Decode as soon as the buffer is a complete (or broken) sequence.
        let need = match self.utf8[0] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => {
                // Stray continuation or invalid lead byte.
                self.utf8.clear();
                self.counters.utf8_errors += 1;
                self.print_char('\u{fffd}');
                return;
            }
        };
        // A non-continuation byte before completion breaks the sequence.
        if self.utf8.len() > 1 && (b & 0xc0) != 0x80 {
            self.utf8.clear();
            self.counters.utf8_errors += 1;
            self.print_char('\u{fffd}');
            // Re-process the breaking byte from Ground (it may start
            // something valid: ASCII, a new lead, or ESC).
            self.feed_byte(b);
            return;
        }
        if self.utf8.len() == need {
            let taken = std::mem::take(&mut self.utf8);
            match std::str::from_utf8(&taken) {
                Ok(s) => {
                    if let Some(c) = s.chars().next() {
                        self.print_char(c);
                    }
                }
                Err(_) => {
                    // Overlong / surrogate / out-of-range.
                    self.counters.utf8_errors += 1;
                    self.print_char('\u{fffd}');
                }
            }
        }
    }

    /// A partial UTF-8 sequence interrupted by a control byte is an error.
    /// Any control also breaks a pending ZWJ join (clusters cannot span
    /// escapes — the cluster-width policy in render.md §2.5 applies to
    /// contiguous text only).
    fn flush_partial_utf8(&mut self) {
        self.pending_zwj = false;
        self.pending_regional = false;
        if !self.utf8.is_empty() {
            self.utf8.clear();
            self.counters.utf8_errors += 1;
            self.print_char('\u{fffd}');
        }
    }

    // The print plane (print_char .. line_feed) lives in a `#[path]`
    // child (file-size split): see vt_print.rs — same impl.
}

#[path = "vt_print.rs"]
mod print;

impl VtScreen {
    // ---- ESC dispatch ----------------------------------------------------

    fn esc_byte(&mut self, b: u8) {
        match b {
            b'[' => {
                self.state = State::Csi {
                    buf: Vec::new(),
                    overflow: false,
                }
            }
            b']' => {
                self.state = State::Osc {
                    buf: Vec::new(),
                    esc: false,
                }
            }
            b'P' | b'_' | b'^' | b'X' => {
                self.state = State::StringFrame {
                    kind: b,
                    len: 0,
                    esc: false,
                }
            }
            b'(' | b')' | b'*' | b'+' => self.state = State::Charset,
            b'7' => self.saved_cursor = Some((self.cursor, self.paint)),
            b'8' => {
                if let Some((p, paint)) = self.saved_cursor {
                    self.cursor =
                        Point::new(p.x.clamp(0, self.grid.w - 1), p.y.clamp(0, self.grid.h - 1));
                    self.paint = paint;
                    self.wrap_pending = false;
                }
            }
            b'M' => {
                // Reverse index: up one row, scrolling the region down at
                // its top margin.
                let (top, bottom) = self.scroll_span();
                if self.cursor.y == top {
                    self.grid
                        .scroll_down_region(top, bottom, 1, self.paint.erase_paint());
                    self.last_write = None;
                } else if self.cursor.y > 0 {
                    self.cursor.y -= 1;
                }
                self.wrap_pending = false;
            }
            b'D' => self.line_feed(), // Index
            b'E' => {
                self.cursor.x = 0;
                self.line_feed();
            }
            b'c' => self.full_reset(),
            b'\\' => { /* stray ST: harmless */ }
            0x1b => {
                // ESC ESC: first escape was torn. Count it, restart.
                self.note_unknown("ESC ESC");
                self.state = State::Esc;
            }
            _ => self.note_unknown(&format!("ESC 0x{b:02x} ({})", printable(b))),
        }
    }

    fn full_reset(&mut self) {
        let size = self.size();
        *self = VtScreen::new(size);
    }

    // ---- CSI framing (dispatch in vt_csi.rs) ------------------------------

    fn csi_byte(&mut self, b: u8, mut buf: Vec<u8>, overflow: bool) {
        match b {
            0x1b => {
                // ESC aborts the sequence and starts a new one.
                self.note_unknown(&format!("CSI aborted by ESC after {:?}", sample(&buf)));
                self.state = State::Esc;
            }
            0x40..=0x7e => {
                if overflow {
                    self.note_unknown("CSI overflow (>256 bytes)");
                } else {
                    self.dispatch_csi(&buf, b);
                }
            }
            0x20..=0x3f => {
                if buf.len() >= MAX_CSI_LEN {
                    self.state = State::Csi {
                        buf,
                        overflow: true,
                    };
                } else {
                    if !overflow {
                        buf.push(b);
                    }
                    self.state = State::Csi { buf, overflow };
                }
            }
            _ => {
                // C0 inside CSI (other than the aborts handled in feed_byte):
                // xterm executes some; the model treats it as dirt.
                self.note_unknown(&format!("byte 0x{b:02x} inside CSI"));
            }
        }
    }

    // ---- OSC -------------------------------------------------------------

    fn osc_byte(&mut self, b: u8, mut buf: Vec<u8>, esc: bool) {
        if esc {
            if b == b'\\' {
                self.dispatch_osc(&buf);
            } else {
                self.note_unknown("OSC torn by ESC (no ST)");
                // The ESC began something new; reprocess.
                self.state = State::Esc;
                self.feed_byte(b);
            }
            return;
        }
        match b {
            0x07 => self.dispatch_osc(&buf),
            0x1b => self.state = State::Osc { buf, esc: true },
            _ => {
                if buf.len() < MAX_STRING_LEN {
                    buf.push(b);
                }
                self.state = State::Osc { buf, esc: false };
            }
        }
    }

    fn dispatch_osc(&mut self, buf: &[u8]) {
        let s = String::from_utf8_lossy(buf);
        let (code, rest) = match s.split_once(';') {
            Some((c, r)) => (c, r),
            None => (s.as_ref(), ""),
        };
        match code {
            "8" => {
                let (params, uri) = rest.split_once(';').unwrap_or(("", rest));
                if uri.is_empty() {
                    self.paint.link = None;
                } else {
                    let key = (params.to_string(), uri.to_string());
                    let id = match self.links.iter().position(|k| *k == key) {
                        Some(i) => i as u32,
                        None => {
                            self.links.push(key);
                            (self.links.len() - 1) as u32
                        }
                    };
                    self.paint.link = Some(id);
                }
            }
            "0" | "2" => self.title = Some(rest.to_string()),
            "52" => {
                // OSC 52 clipboard write: `52;<selection>;<base64>`.
                // Tracked state (KERNEL emits it); a query (`?` payload)
                // stays legal traffic too.
                let (sel, payload) = rest.split_once(';').unwrap_or((rest, ""));
                self.clipboard = Some((sel.to_string(), payload.to_string()));
            }
            "9" => {
                // OSC 9 notification (iTerm2 convention; KERNEL's notify
                // verb). Tracked, never unknown.
                self.notifications.push(rest.to_string());
            }
            "99" => {
                // OSC 99 kitty notification, basic form `99;meta;body`.
                // The body follows the (possibly empty) metadata section.
                let body = rest.split_once(';').map(|(_, b)| b).unwrap_or(rest);
                self.notifications.push(body.to_string());
            }
            _ => self.note_unknown(&format!("OSC {code}")),
        }
    }

    // ---- DCS/APC/PM/SOS ----------------------------------------------------

    fn string_byte(&mut self, b: u8, kind: u8, len: usize, esc: bool) {
        if esc {
            if b == b'\\' {
                self.counters.string_frames += 1;
            } else {
                self.note_unknown("string frame torn by ESC");
                self.state = State::Esc;
                self.feed_byte(b);
            }
            return;
        }
        match b {
            0x1b => {
                self.state = State::StringFrame {
                    kind,
                    len,
                    esc: true,
                }
            }
            0x07 if kind == b'P' || kind == b'_' => {
                // BEL-terminated DCS/APC: nonstandard but seen in the wild.
                self.counters.string_frames += 1;
            }
            _ => {
                self.state = State::StringFrame {
                    kind,
                    len: (len + 1).min(MAX_STRING_LEN),
                    esc: false,
                }
            }
        }
    }

    // ---- shared helpers for vt_csi.rs -------------------------------------

    pub(super) fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }
    pub(super) fn grid_ref(&self) -> &Grid {
        &self.grid
    }
    pub(super) fn set_cursor_clamped(&mut self, x: i32, y: i32) {
        self.cursor = Point::new(x.clamp(0, self.grid.w - 1), y.clamp(0, self.grid.h - 1));
        self.wrap_pending = false;
    }
    pub(super) fn paint_mut(&mut self) -> &mut Paint {
        &mut self.paint
    }
    pub(super) fn modes_mut(&mut self) -> &mut Modes {
        &mut self.modes
    }
    pub(super) fn counters_mut(&mut self) -> &mut VtCounters {
        &mut self.counters
    }
    pub(super) fn clear_wrap_pending(&mut self) {
        self.wrap_pending = false;
    }
    pub(super) fn home_cursor(&mut self) {
        self.cursor = Point::ZERO;
        self.wrap_pending = false;
    }

    pub(super) fn note_unknown(&mut self, what: &str) {
        self.counters.unknown += 1;
        if self.unknown_samples.len() < MAX_UNKNOWN_SAMPLES {
            self.unknown_samples.push(what.to_string());
        }
    }

    /// Whether a wrap is pending (glyph written into the last column;
    /// the next printable wraps first). Exposed for dumps and tests.
    pub fn wrap_is_pending(&self) -> bool {
        self.wrap_pending
    }

    /// Current DECSTBM margins, 0-based inclusive; None = full screen.
    pub fn margins(&self) -> Option<(i32, i32)> {
        self.margins
    }

    /// DECSCUSR style (0 = default).
    pub fn cursor_style(&self) -> u32 {
        self.cursor_style
    }

    /// Last OSC 52 clipboard write as (selection, base64 payload).
    pub fn clipboard(&self) -> Option<(&str, &str)> {
        self.clipboard
            .as_ref()
            .map(|(s, p)| (s.as_str(), p.as_str()))
    }

    /// OSC 9 notifications in arrival order.
    pub fn notifications(&self) -> &[String] {
        &self.notifications
    }

    /// The scroll span LF/RI/SU/SD operate in: margins or full screen.
    pub(super) fn scroll_span(&self) -> (i32, i32) {
        self.margins.unwrap_or((0, self.grid.h - 1))
    }

    pub(super) fn set_margins(&mut self, m: Option<(i32, i32)>) {
        self.margins = m;
    }

    pub(super) fn set_cursor_style(&mut self, style: u32) {
        self.cursor_style = style;
    }
}

fn printable(b: u8) -> char {
    if (0x20..0x7f).contains(&b) {
        b as char
    } else {
        '?'
    }
}

fn sample(buf: &[u8]) -> String {
    String::from_utf8_lossy(&buf[..buf.len().min(24)]).into_owned()
}
