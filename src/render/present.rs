//! Presenter: diff runs -> minimal, deterministic ANSI byte stream.
//!
//! Determinism is a hard contract (REDTEAM snapshots exact bytes): every
//! emitted byte is a pure function of (presenter state, runs, next frame,
//! caps), and presenter state across frames is deliberately tiny — the
//! virtual cursor and "pen is at defaults" knowledge. Each frame trailer
//! closes any hyperlink, resets SGR and parks the cursor, so SGR/link
//! state can never drift across frames or across foreign writers (the gfx
//! layer emitting image protocols between frames).
//!
//! Cursor motion economy: within a row, `CR` (1 byte) to column 0, else
//! CUF/CUB (3+ bytes); same column across rows, CUD/CUU; anything else —
//! including an unknown cursor — absolute CUP. LF is never used for
//! motion: its behavior depends on ONLCR/scroll state the presenter does
//! not own.
//!
//! The last-column wrap hazard: writing the bottom-right cell arms xterm's
//! deferred autowrap (the wrap happens when the *next* glyph prints, which
//! would scroll the screen). Strategy: the presenter does write last-column
//! cells, then invalidates its virtual cursor, so the next emission starts
//! with an absolute CUP — cursor motion clears the pending-wrap flag and no
//! glyph is ever printed while a wrap is pending. Wide glyphs cannot start
//! in the last column (surface invariant), so only width-1 writes reach it.

use super::cell::Cell;
use super::diff::Run;
use super::scroll::{ScrolledRuns, Shift};
use super::sgr::{build_incremental, build_reset, csi_n, cup, push_u32, resolve_pen, Pen};
use super::surface::Surface;

/// Presenter feature switches. `scroll_optimization` gates the
/// DECSTBM+SU/SD path (docs/design/render.md §2.7): **ON by default**
/// since cycle 5 — the testing VT model replays scroll regions
/// byte-level and the diff/present property holds with it engaged. The
/// byte-win guard lives in detection (full-width band ≥ 8 rows, ≥ 4 rows
/// made diff-clean), so enabling it can only reduce bytes. Callers
/// consult this flag to pick `compute` vs `compute_scrolled`; the
/// [`ScrolledRuns`] token makes wrong pairing unrepresentable.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PresenterOpts {
    /// Gate for the DECSTBM+SU/SD scroll path (ON by default; see the
    /// type docs — turning it off makes `emit_scrolled` treat every
    /// token as plain runs).
    pub scroll_optimization: bool,
}

impl Default for PresenterOpts {
    fn default() -> Self {
        PresenterOpts {
            scroll_optimization: true,
        }
    }
}

/// What the presenter needs to know about the terminal. KERNEL's richer
/// `Capabilities` should provide a conversion into this (requested in
/// reviews/cycle1/render-requests.md); the render layer stays decoupled
/// from detection.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PresentCaps {
    /// Deepest color form the terminal accepts; deeper cell colors
    /// downlevel (nearest xterm-256 / ANSI-16, pair-contrast preserved).
    pub color: ColorDepth,
    /// DEC 2026 synchronized output: bracket frames so the terminal
    /// presents them atomically.
    pub sync_output_2026: bool,
    /// OSC 8 hyperlinks.
    pub hyperlinks: bool,
    /// SGR 4:3 curly underline; without it UNDERCURL degrades to UNDERLINE.
    pub undercurl: bool,
    /// SGR 58/59 underline color; without it the color is dropped (the
    /// underline itself stays — labeled downlevel, DESIGN request 2).
    pub underline_color: bool,
}

impl PresentCaps {
    /// Conservative baseline: plain 16-color, no extensions. Anything on
    /// top must be detected, never assumed.
    pub const BASELINE: PresentCaps = PresentCaps {
        color: ColorDepth::Ansi16,
        sync_output_2026: false,
        hyperlinks: false,
        undercurl: false,
        underline_color: false,
    };

    /// Everything on: what a modern terminal (kitty, WezTerm, recent
    /// iTerm2) advertises. Tests and demos use it; real apps detect.
    pub const FULL: PresentCaps = PresentCaps {
        color: ColorDepth::TrueColor,
        sync_output_2026: true,
        hyperlinks: true,
        undercurl: true,
        underline_color: true,
    };
}

/// The color vocabulary the terminal accepts (emission form, not
/// storage — cells always store RGBA).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    /// SGR 38;2;r;g;b — exact colors.
    TrueColor,
    /// SGR 38;5;n — nearest of the xterm-256 palette.
    Xterm256,
    /// SGR 30-37/90-97 — nearest of the classic 16.
    Ansi16,
}

/// Turns diff runs into minimal, deterministic ANSI bytes. Owns the
/// virtual cursor/pen state and reusable scratch — keep ONE per terminal
/// across frames (a fresh presenter re-syncs from scratch).
pub struct Presenter {
    opts: PresenterOpts,
    /// Virtual cursor in cell coordinates; `None` = unknown (start of
    /// life, after `invalidate`, or after a last-column write armed the
    /// wrap hazard).
    cursor: Option<(i32, i32)>,
    /// `Some(Pen::DEFAULT)` between frames (the trailer resets); `None`
    /// after `invalidate` forces a reset-based first SGR.
    pen: Option<Pen>,
    /// Hyperlink currently open *within* a frame (always closed by the
    /// trailer; never carries across frames).
    open_link: String,
    /// Scratch for candidate SGR parameter strings (incremental vs reset).
    seq_inc: Vec<u8>,
    seq_reset: Vec<u8>,
}

impl Default for Presenter {
    fn default() -> Self {
        Presenter::new()
    }
}

impl Presenter {
    /// A presenter with default options (scroll optimization ON).
    pub fn new() -> Presenter {
        Presenter::with_opts(PresenterOpts::default())
    }

    /// A presenter with explicit [`PresenterOpts`].
    pub fn with_opts(opts: PresenterOpts) -> Presenter {
        Presenter {
            opts,
            cursor: None,
            pen: None,
            open_link: String::new(),
            seq_inc: Vec::new(),
            seq_reset: Vec::new(),
        }
    }

    /// The feature switches this presenter was built with — the app
    /// consults `opts().scroll_optimization` to pick `compute` vs
    /// `compute_scrolled` (the pairing contract on [`PresenterOpts`]).
    pub fn opts(&self) -> PresenterOpts {
        self.opts
    }

    /// Declares the on-screen state unknown (external writer touched the
    /// terminal, resize, resume from suspend). The next frame re-syncs
    /// cursor and pen unconditionally.
    pub fn invalidate(&mut self) {
        self.cursor = None;
        self.pen = None;
        self.open_link.clear();
    }

    /// Presenter custody of foreign bytes (damage contract §6): pixel
    /// protocol payloads — kitty APC, iTerm2 OSC 1337, sixel DCS — reach
    /// the terminal ONLY through this bracket, never via a raw terminal
    /// write around the presenter.
    ///
    /// Sequence: (a) flush pending presenter state — close any open
    /// hyperlink and reset SGR, so the payload is not interpreted under
    /// text attributes; (b) absolute CUP to `at` (relative motion would
    /// trust a virtual cursor that may already be stale mid-composition);
    /// (c) the payload verbatim; (d) invalidate — protocol payloads move
    /// the real cursor in protocol-specific ways, so every assumption
    /// (cursor, pen, link) is forgotten and the next emission re-syncs
    /// from absolute state.
    ///
    /// The caller appends this into the same per-frame buffer as
    /// [`Presenter::emit`] output; the one-flush-per-frame contract
    /// (docs/design/render.md §2.6) is unchanged.
    pub fn external_write(&mut self, out: &mut Vec<u8>, bytes: &[u8], at: crate::base::Point) {
        if !self.open_link.is_empty() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
            self.open_link.clear();
        }
        out.extend_from_slice(b"\x1b[0m");
        cup(out, at.x, at.y);
        out.extend_from_slice(bytes);
        self.invalidate();
    }

    /// Emits the byte stream that turns the previous frame into `next`,
    /// given `runs` from [`super::diff::FrameDiff`]. Appends to `out`
    /// (callers reuse the buffer). Zero runs emit zero bytes.
    ///
    /// One-flush rule: the presenter only APPENDS — the app writes the
    /// buffer to the terminal exactly once per frame (partial flushes
    /// tear frames on terminals without DEC 2026, and even with it the
    /// sync bracket must reach the tty in one piece).
    pub fn emit(&mut self, runs: &[Run], next: &Surface, caps: &PresentCaps, out: &mut Vec<u8>) {
        self.emit_scrolled(ScrolledRuns::plain(runs), next, caps, out);
    }

    /// [`Presenter::emit`] for a [`ScrolledRuns`] token: the token's
    /// scroll (if any) executes FIRST at the terminal, then its runs
    /// paint the residual differences against the shifted state. The
    /// token is the ONLY way to emit a shift — shift-relative runs cannot
    /// reach the plain path (the cycle-4 wrong-pairing hazard is
    /// unrepresentable). A shift with zero runs is a legal frame (a pure
    /// scroll).
    pub fn emit_scrolled(
        &mut self,
        frame: ScrolledRuns<'_>,
        next: &Surface,
        caps: &PresentCaps,
        out: &mut Vec<u8>,
    ) {
        let (shift, runs) = (frame.shift(), frame.runs());
        if frame.is_empty() {
            return;
        }
        if caps.sync_output_2026 {
            out.extend_from_slice(b"\x1b[?2026h");
        }
        if let Some(shift) = shift {
            self.emit_shift(&shift, caps, out);
        }

        for run in runs {
            self.emit_run(run, next, caps, out);
        }

        // Frame trailer: no state leaks into the next frame.
        self.close_link(caps, out);
        out.extend_from_slice(b"\x1b[0m");
        self.pen = Some(Pen::DEFAULT);
        self.park_cursor(next, out);
        if caps.sync_output_2026 {
            out.extend_from_slice(b"\x1b[?2026l");
        }
    }

    /// The scroll prelude: `SGR 0` (BCE fill = default ground — vacated
    /// rows must never flash a stale bg), DECSTBM region, SU/SD, region
    /// reset. DECSTBM and its reset both HOME the cursor (absolute
    /// addressing — the engine never enables origin mode), so the virtual
    /// cursor lands at (0,0), a known state.
    fn emit_shift(&mut self, shift: &Shift, caps: &PresentCaps, out: &mut Vec<u8>) {
        self.close_link(caps, out);
        out.extend_from_slice(b"\x1b[0m");
        self.pen = Some(Pen::DEFAULT);
        // CSI top;bottom r — 1-based inclusive.
        out.extend_from_slice(b"\x1b[");
        push_u32(out, (shift.top + 1) as u32);
        out.push(b';');
        push_u32(out, shift.bottom as u32);
        out.push(b'r');
        csi_n(out, shift.n as u32, if shift.up { b'S' } else { b'T' });
        out.extend_from_slice(b"\x1b[r");
        self.cursor = Some((0, 0));
    }

    fn emit_run(&mut self, run: &Run, next: &Surface, caps: &PresentCaps, out: &mut Vec<u8>) {
        let y = run.y;
        let mut x = run.x;
        let end = run.end();

        // A run must never begin mid-glyph. The diff widens runs to
        // include leaders, so a leading continuation here means its leader
        // is unchanged and already covers this column: skip.
        while x < end && next.get(x, y).is_some_and(Cell::is_continuation) {
            x += 1;
        }
        if x >= end {
            return;
        }
        self.move_cursor(x, y, out);

        while x < end {
            let Some(&cell) = next.get(x, y) else {
                break;
            };
            if cell.is_continuation() {
                // Interior continuation: the leader's emission advanced the
                // cursor past this column already.
                x += 1;
                continue;
            }
            // A risky cluster earlier in this run invalidated the cursor
            // (see below): re-anchor absolutely before the next glyph.
            if self.cursor.is_none() {
                self.move_cursor(x, y, out);
            }
            self.set_link(cell.link, next, caps, out);
            self.set_pen(&cell, caps, out);

            let width = cell.glyph.width().max(1);
            if cell.is_wide_leader() && x + 1 >= next.width() {
                // Defense in depth: a wide leader in the last column
                // violates the surface invariant; printing it would arm
                // the wrap hazard mid-frame. Substitute a space.
                out.push(b' ');
                self.advance_cursor(1, next.width());
                x += 1;
                continue;
            }
            let s = next.glyph_str(&cell);
            if s.is_empty() {
                out.push(b' '); // EMPTY renders as a styled blank
                self.advance_cursor(width, next.width());
            } else {
                out.extend_from_slice(s.as_bytes());
                if crate::text::is_risky_cluster(s) {
                    // RT1-7: terminals disagree about the width of VS16 /
                    // ZWJ / ambiguous-width clusters. Our width opinion
                    // decides the CELL layout; the terminal's decides
                    // where the physical cursor lands. Forgetting the
                    // virtual cursor confines any disagreement to this
                    // cluster instead of smearing the rest of the run.
                    self.cursor = None;
                } else {
                    self.advance_cursor(width, next.width());
                }
            }
            x += width;
        }
    }

    // -- cursor ------------------------------------------------------------

    fn advance_cursor(&mut self, width: i32, surface_width: i32) {
        if let Some((cx, cy)) = self.cursor {
            let nx = cx + width;
            // Reaching the right edge arms deferred autowrap; forget the
            // cursor so the next motion is absolute (clears the pending
            // wrap before anything prints).
            self.cursor = if nx >= surface_width {
                None
            } else {
                Some((nx, cy))
            };
        }
    }

    fn move_cursor(&mut self, x: i32, y: i32, out: &mut Vec<u8>) {
        match self.cursor {
            Some((cx, cy)) if cx == x && cy == y => {}
            Some((cx, cy)) if cy == y => {
                if x == 0 {
                    out.push(b'\r');
                } else if x > cx {
                    csi_n(out, (x - cx) as u32, b'C');
                } else {
                    csi_n(out, (cx - x) as u32, b'D');
                }
            }
            Some((cx, cy)) if cx == x => {
                if y > cy {
                    csi_n(out, (y - cy) as u32, b'B');
                } else {
                    csi_n(out, (cy - y) as u32, b'A');
                }
            }
            _ => cup(out, x, y),
        }
        self.cursor = Some((x, y));
    }

    /// Bottom-left park: harmless to scrolling (unlike bottom-right, which
    /// would arm the wrap hazard) and a sane place for a crash to leave
    /// the real cursor.
    fn park_cursor(&mut self, next: &Surface, out: &mut Vec<u8>) {
        let y = (next.height() - 1).max(0);
        self.move_cursor(0, y, out);
    }

    // -- hyperlinks ----------------------------------------------------------

    fn set_link(&mut self, link: u16, next: &Surface, caps: &PresentCaps, out: &mut Vec<u8>) {
        if !caps.hyperlinks {
            return;
        }
        let uri = next.link_uri(link).unwrap_or("");
        if uri == self.open_link {
            return;
        }
        if !self.open_link.is_empty() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
            self.open_link.clear();
        }
        if !uri.is_empty() {
            out.extend_from_slice(b"\x1b]8;id=");
            push_u32(out, link as u32);
            out.push(b';');
            out.extend_from_slice(uri.as_bytes());
            out.extend_from_slice(b"\x1b\\");
            self.open_link.push_str(uri);
        }
    }

    fn close_link(&mut self, caps: &PresentCaps, out: &mut Vec<u8>) {
        if caps.hyperlinks && !self.open_link.is_empty() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
            self.open_link.clear();
        }
    }

    // -- SGR -----------------------------------------------------------------

    fn set_pen(&mut self, cell: &Cell, caps: &PresentCaps, out: &mut Vec<u8>) {
        let target = resolve_pen(cell, caps);
        match self.pen {
            Some(cur) if cur == target => return,
            Some(cur) => {
                build_incremental(&cur, &target, &mut self.seq_inc);
                build_reset(&target, &mut self.seq_reset);
                // Shorter wins; ties go incremental (avoids the flash-prone
                // full reset on terminals that repaint per-SGR).
                let params: &[u8] = if self.seq_reset.len() < self.seq_inc.len() {
                    &self.seq_reset
                } else {
                    &self.seq_inc
                };
                out.extend_from_slice(b"\x1b[");
                out.extend_from_slice(params);
                out.push(b'm');
            }
            None => {
                build_reset(&target, &mut self.seq_reset);
                out.extend_from_slice(b"\x1b[");
                out.extend_from_slice(&self.seq_reset);
                out.push(b'm');
            }
        }
        self.pen = Some(target);
    }
}

// Byte-snapshot tests live beside this file to keep it within the size
// budget; they are ordinary unit tests of the public API.
#[cfg(test)]
#[path = "present_tests.rs"]
mod tests;
