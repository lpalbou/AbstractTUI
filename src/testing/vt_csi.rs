//! CSI dispatch for [`super::vt::VtScreen`]: cursor motion, erase, SGR
//! (incl. truecolor / 256-color / basic-16), DECSET/DECRST mode flags and
//! kitty keyboard push/pop framing.
//!
//! OWNER: REDTEAM.
//!
//! Param parsing follows ECMA-48: `;` separates params, an empty param is
//! "default", `:` sub-parameters are honored where SGR 38/48 use them and
//! consumed (counted per-item, not per-sequence) elsewhere. Unknown finals
//! and unknown SGR items bump the unknown counter — the presenter must
//! stay inside this modeled set to pass a zero-unknowns run.

use super::grid::Attrs;
use super::palette::xterm_256;
use super::vt::VtScreen;
use crate::base::Rgba;

/// One parsed CSI parameter with its (optional) colon sub-parameters.
#[derive(Debug, Default, Clone)]
struct Param {
    value: Option<u32>,
    subs: Vec<Option<u32>>,
}

impl Param {
    fn or(&self, default: u32) -> u32 {
        self.value.unwrap_or(default)
    }

    /// Motion/count params: default AND explicit 0 both mean 1 (xterm
    /// normalizes `CSI 0 A` to one row).
    fn count(&self) -> u32 {
        self.or(1).max(1)
    }
}

/// xterm caps numeric params at 65535; adopting the same cap here kills
/// the whole "giant param" class (and keeps `as i32` casts safe).
const PARAM_CAP: u32 = 65535;

fn parse_params(body: &[u8]) -> Vec<Param> {
    let mut params = vec![Param::default()];
    let mut in_sub = false;
    for &b in body {
        match b {
            b'0'..=b'9' => {
                let p = params.last_mut().expect("params never empty");
                let slot = if in_sub {
                    p.subs.last_mut().expect("sub pushed on ':'")
                } else {
                    &mut p.value
                };
                let d = (b - b'0') as u32;
                *slot = Some(
                    slot.unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(d)
                        .min(PARAM_CAP),
                );
            }
            b';' => {
                params.push(Param::default());
                in_sub = false;
            }
            b':' => {
                params
                    .last_mut()
                    .expect("params never empty")
                    .subs
                    .push(None);
                in_sub = true;
            }
            _ => {} // intermediates handled by the caller's prefix check
        }
    }
    params
}

impl VtScreen {
    /// Dispatch a complete CSI sequence: `body` is everything between
    /// `ESC [` and the final byte.
    pub(super) fn dispatch_csi(&mut self, body: &[u8], final_byte: u8) {
        // Private-parameter prefix (`?`, `>`, `<`, `=`) changes the family.
        let (prefix, rest) = match body.first() {
            Some(&b @ (b'?' | b'>' | b'<' | b'=')) => (Some(b), &body[1..]),
            _ => (None, body),
        };
        // Intermediate bytes (0x20-0x2f) before the final: only `$` (DECRQM)
        // is recognized; anything else is out of the modeled set.
        let has_dollar = rest.contains(&b'$');
        let params = parse_params(rest);

        match (prefix, final_byte) {
            (None, b'A') => self.move_rel(0, -(params[0].count() as i32)),
            (None, b'B') => self.move_rel(0, params[0].count() as i32),
            (None, b'C') => self.move_rel(params[0].count() as i32, 0),
            (None, b'D') => self.move_rel(-(params[0].count() as i32), 0),
            (None, b'E') => {
                // CNL: next line, column 0.
                let p = self.cursor();
                self.set_cursor_clamped(0, p.y + params[0].count() as i32);
            }
            (None, b'F') => {
                let p = self.cursor();
                self.set_cursor_clamped(0, p.y - params[0].count() as i32);
            }
            (None, b'G') => {
                // CHA: column absolute (1-based).
                let p = self.cursor();
                self.set_cursor_clamped(params[0].or(1) as i32 - 1, p.y);
            }
            (None, b'd') => {
                // VPA: row absolute (1-based).
                let p = self.cursor();
                self.set_cursor_clamped(p.x, params[0].or(1) as i32 - 1);
            }
            (None, b'H') | (None, b'f') => {
                let row = params[0].or(1) as i32 - 1;
                let col = params.get(1).map(|p| p.or(1)).unwrap_or(1) as i32 - 1;
                self.set_cursor_clamped(col, row);
            }
            (None, b'J') => self.erase_display(params[0].or(0)),
            (None, b'K') => self.erase_line(params[0].or(0)),
            (None, b'X') => {
                // ECH: erase n chars at the cursor, no cursor motion.
                let n = params[0].count() as i32;
                let p = self.cursor();
                let erase = self.current_paint().erase_paint();
                self.grid_mut().erase_row_range(p.y, p.x, p.x + n, erase);
                self.clear_wrap_pending();
            }
            (None, b'S') => {
                // SU scrolls the DECSTBM region (xterm scopes it).
                let erase = self.current_paint().erase_paint();
                let (top, bottom) = self.scroll_span();
                self.grid_mut()
                    .scroll_up_region(top, bottom, params[0].count() as i32, erase);
            }
            (None, b'T') => {
                let erase = self.current_paint().erase_paint();
                let (top, bottom) = self.scroll_span();
                self.grid_mut()
                    .scroll_down_region(top, bottom, params[0].count() as i32, erase);
            }
            (None, b'L') | (None, b'M') => {
                // IL/DL: only act with the cursor inside the region; the
                // shift is bounded by the bottom margin; the cursor moves
                // to column 0 (VT102 + xterm).
                let n = params[0].count() as i32;
                let p = self.cursor();
                let (top, bottom) = self.scroll_span();
                if p.y >= top && p.y <= bottom {
                    let erase = self.current_paint().erase_paint();
                    if final_byte == b'L' {
                        self.grid_mut().insert_lines(p.y, bottom, n, erase);
                    } else {
                        self.grid_mut().delete_lines(p.y, bottom, n, erase);
                    }
                    self.set_cursor_clamped(0, p.y);
                }
            }
            (None, b'r') if !has_dollar => {
                // DECSTBM: 1-based inclusive margins; defaults = full
                // screen; bottom must exceed top or the sequence is
                // ignored (xterm). Cursor homes (origin mode off ->
                // absolute home). Full-screen margins reset to None.
                let h = self.grid_ref().h;
                let top = params[0].or(1) as i32 - 1;
                let bottom = params.get(1).map(|p| p.or(0)).unwrap_or(0) as i32;
                let bottom = if bottom == 0 { h - 1 } else { bottom - 1 };
                let top = top.clamp(0, h - 1);
                let bottom = bottom.clamp(0, h - 1);
                if bottom > top {
                    if top == 0 && bottom == h - 1 {
                        self.set_margins(None);
                    } else {
                        self.set_margins(Some((top, bottom)));
                    }
                    self.home_cursor();
                }
                // Invalid (bottom <= top): ignored entirely, per xterm.
            }
            (None, b'q') if rest.contains(&b' ') => {
                // DECSCUSR (`CSI Ps SP q`): cursor style. Tracked state.
                self.set_cursor_style(params[0].or(0));
            }
            (None, b'm') => self.dispatch_sgr(&params),
            (Some(b'?'), b'h') if !has_dollar => {
                for p in &params {
                    if let Some(mode) = p.value {
                        self.apply_private_mode(mode, true);
                    }
                }
            }
            (Some(b'?'), b'l') if !has_dollar => {
                for p in &params {
                    if let Some(mode) = p.value {
                        self.apply_private_mode(mode, false);
                    }
                }
            }
            // Kitty keyboard framing: push (`CSI > flags u`) / pop
            // (`CSI < n u`). Tracked as a depth so leave-balance is testable.
            (Some(b'>'), b'u') => self.counters_mut().kitty_push_depth += 1,
            (Some(b'<'), b'u') => {
                let c = self.counters_mut();
                c.kitty_push_depth = c.kitty_push_depth.saturating_sub(1);
            }
            // Queries (DECRQM `CSI ? .. $ p`, DA1 `CSI c`, DSR `CSI n`,
            // XTVERSION `CSI > q`...) are legal app->terminal traffic that
            // paints nothing. Consumed silently: a presenter may probe.
            (Some(b'?'), b'p') if has_dollar => {}
            (None, b'c') | (Some(b'>'), b'c') | (None, b'n') | (Some(b'>'), b'q') => {}
            // KERNEL's capability probes (probe.rs): kitty keyboard query
            // `CSI ? u`, XTSMGRAPHICS read `CSI ? 1;1;0 S` (params = a
            // read request, never a scroll), XTWINOPS `CSI Ps t` (16 =
            // cell pixel size query; 22/23 = title stack push/pop).
            // Query traffic paints nothing; tracked as legal.
            (Some(b'?'), b'u') => {}
            (Some(b'?'), b'S') => {}
            (None, b't') => {}
            _ => self.note_unknown(&format!(
                "CSI {}{} final '{}'",
                prefix
                    .map(|p| p as char)
                    .map(String::from)
                    .unwrap_or_default(),
                String::from_utf8_lossy(rest),
                final_byte as char
            )),
        }
    }

    fn move_rel(&mut self, dx: i32, dy: i32) {
        let p = self.cursor();
        self.set_cursor_clamped(p.x + dx, p.y + dy);
    }

    fn erase_display(&mut self, mode: u32) {
        let erase = self.current_paint().erase_paint();
        let p = self.cursor();
        let (w, h) = {
            let g = self.grid_ref();
            (g.w, g.h)
        };
        match mode {
            0 => {
                // Cursor to end of screen.
                self.grid_mut().erase_row_range(p.y, p.x, w, erase);
                for y in (p.y + 1)..h {
                    self.grid_mut().erase_row_range(y, 0, w, erase);
                }
            }
            1 => {
                // Start of screen to cursor (inclusive).
                for y in 0..p.y {
                    self.grid_mut().erase_row_range(y, 0, w, erase);
                }
                self.grid_mut().erase_row_range(p.y, 0, p.x + 1, erase);
            }
            2 | 3 => self.grid_mut().clear_all(erase),
            _ => self.note_unknown(&format!("ED mode {mode}")),
        }
        self.clear_wrap_pending();
    }

    fn erase_line(&mut self, mode: u32) {
        let erase = self.current_paint().erase_paint();
        let p = self.cursor();
        let w = self.grid_ref().w;
        match mode {
            0 => self.grid_mut().erase_row_range(p.y, p.x, w, erase),
            1 => self.grid_mut().erase_row_range(p.y, 0, p.x + 1, erase),
            2 => self.grid_mut().erase_row_range(p.y, 0, w, erase),
            _ => self.note_unknown(&format!("EL mode {mode}")),
        }
        self.clear_wrap_pending();
    }

    fn apply_private_mode(&mut self, mode: u32, on: bool) {
        // All DECSET/DECRST arrivals are tracked as flags; the ones with
        // model-visible behavior get it. 1049 clears the alt screen on
        // entry (xterm: save cursor, switch, clear).
        if on {
            self.modes_mut().insert(mode);
        } else {
            self.modes_mut().remove(mode);
        }
        match mode {
            1049 if on => {
                let erase = self.current_paint().erase_paint();
                self.grid_mut().clear_all(erase);
                self.home_cursor();
            }
            2026 => {
                if on {
                    self.counters_mut().sync_begins += 1;
                } else {
                    self.counters_mut().sync_ends += 1;
                }
            }
            _ => {}
        }
    }

    // ---- SGR ---------------------------------------------------------------

    fn dispatch_sgr(&mut self, params: &[Param]) {
        let mut i = 0;
        while i < params.len() {
            let p = &params[i];
            let n = p.or(0); // empty SGR param means 0 (reset)
            match n {
                0 => {
                    let link = self.paint_mut().link; // OSC 8 owns links, not SGR
                    *self.paint_mut() = super::grid::Paint {
                        link,
                        ..Default::default()
                    };
                }
                1 => self.paint_mut().attrs.set(Attrs::BOLD, true),
                2 => self.paint_mut().attrs.set(Attrs::DIM, true),
                3 => self.paint_mut().attrs.set(Attrs::ITALIC, true),
                4 => {
                    // Underline styles via colon sub-param (kitty/xterm):
                    // 4 / 4:1 / 4:2 plain-ish underline, 4:3 undercurl,
                    // 4:0 none. The presenter emits 4 and 4:3 only.
                    match p.subs.first().copied().flatten() {
                        None | Some(1) | Some(2) => {
                            self.paint_mut().attrs.set(Attrs::UNDERLINE, true)
                        }
                        Some(0) => {
                            self.paint_mut().attrs.set(Attrs::UNDERLINE, false);
                            self.paint_mut().attrs.set(Attrs::UNDERCURL, false);
                        }
                        Some(3) => self.paint_mut().attrs.set(Attrs::UNDERCURL, true),
                        Some(other) => self.note_unknown(&format!("SGR 4:{other}")),
                    }
                }
                5 => self.paint_mut().attrs.set(Attrs::BLINK, true),
                7 => self.paint_mut().attrs.set(Attrs::REVERSE, true),
                8 => self.paint_mut().attrs.set(Attrs::HIDDEN, true),
                9 => self.paint_mut().attrs.set(Attrs::STRIKE, true),
                22 => {
                    self.paint_mut().attrs.set(Attrs::BOLD, false);
                    self.paint_mut().attrs.set(Attrs::DIM, false);
                }
                23 => self.paint_mut().attrs.set(Attrs::ITALIC, false),
                24 => {
                    // Shared reset: clears underline AND undercurl (the
                    // presenter relies on this for its re-add sequences).
                    self.paint_mut().attrs.set(Attrs::UNDERLINE, false);
                    self.paint_mut().attrs.set(Attrs::UNDERCURL, false);
                }
                25 => self.paint_mut().attrs.set(Attrs::BLINK, false),
                27 => self.paint_mut().attrs.set(Attrs::REVERSE, false),
                28 => self.paint_mut().attrs.set(Attrs::HIDDEN, false),
                29 => self.paint_mut().attrs.set(Attrs::STRIKE, false),
                30..=37 => self.paint_mut().fg = Some(xterm_256((n - 30) as u8)),
                40..=47 => self.paint_mut().bg = Some(xterm_256((n - 40) as u8)),
                90..=97 => self.paint_mut().fg = Some(xterm_256((n - 90 + 8) as u8)),
                100..=107 => self.paint_mut().bg = Some(xterm_256((n - 100 + 8) as u8)),
                39 => self.paint_mut().fg = None,
                49 => self.paint_mut().bg = None,
                59 => self.paint_mut().ul = None,
                38 | 48 | 58 => {
                    let (color, consumed) = self.parse_extended_color(params, i);
                    match color {
                        Some(c) => {
                            match n {
                                38 => self.paint_mut().fg = Some(c),
                                48 => self.paint_mut().bg = Some(c),
                                _ => self.paint_mut().ul = Some(c),
                            }
                            i += consumed;
                        }
                        None => {
                            // Malformed extended color: the remaining params
                            // are unattributable (ECMA's semicolon form is
                            // ambiguous), so abort the whole sequence rather
                            // than misread color args as attribute codes.
                            self.note_unknown(&format!("SGR {n} malformed color"));
                            return;
                        }
                    }
                    continue;
                }
                _ => self.note_unknown(&format!("SGR {n}")),
            }
            i += 1;
        }
    }

    /// Parse SGR 38/48 in both spellings: semicolon (`38;2;r;g;b`,
    /// `38;5;n`) and colon sub-params (`38:2:r:g:b`, `38:2::r:g:b`,
    /// `38:5:n`). Returns (color, params consumed incl. the 38/48 itself).
    fn parse_extended_color(&mut self, params: &[Param], i: usize) -> (Option<Rgba>, usize) {
        let p = &params[i];
        if !p.subs.is_empty() {
            // Colon form: everything rides in one Param.
            let s = &p.subs;
            let mode = s.first().copied().flatten().unwrap_or(0);
            let color = match (mode, s.len()) {
                (5, 2..) => s[1].map(|n| xterm_256(n.min(255) as u8)),
                // 38:2:r:g:b (3 args) or 38:2::r:g:b (colorspace slot).
                (2, 4) => rgb(s[1], s[2], s[3]),
                (2, 5..) => rgb(s[2], s[3], s[4]),
                _ => None,
            };
            return (color, 1);
        }
        // Semicolon form: mode and args are separate Params.
        let mode = params.get(i + 1).and_then(|p| p.value);
        match mode {
            Some(5) => {
                let c = params
                    .get(i + 2)
                    .and_then(|p| p.value)
                    .map(|n| xterm_256(n.min(255) as u8));
                (c, if c.is_some() { 3 } else { 2 })
            }
            Some(2) => {
                let c = (|| {
                    rgb(
                        params.get(i + 2)?.value,
                        params.get(i + 3)?.value,
                        params.get(i + 4)?.value,
                    )
                })();
                (c, if c.is_some() { 5 } else { 2 })
            }
            _ => (None, 2),
        }
    }
}

fn rgb(r: Option<u32>, g: Option<u32>, b: Option<u32>) -> Option<Rgba> {
    Some(Rgba::rgb(
        r?.min(255) as u8,
        g?.min(255) as u8,
        b?.min(255) as u8,
    ))
}
