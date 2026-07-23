//! Screenshots: a captured screen as a plain value, with deterministic
//! exporters.
//!
//! [`Screenshot`] is a grid of `{glyph, fg, bg, ul, attrs}` cells — the
//! debugging/documentation/test-artifact currency the bordered
//! [`snapshot`](super::snapshot::snapshot) dumps are too informal for.
//! It is capturable from every truth surface the engine has:
//!
//! - the **live composed frame** — `Driver::screenshot()` /
//!   `app::request_screenshot(..)` read what was last PRESENTED (no
//!   re-render, no damage side effects);
//! - the **testing rig** — `testing::VtScreen::screenshot()` converts the
//!   VT model's grid (what the emitted bytes actually produced);
//! - any [`Surface`] directly — [`Screenshot::from_surface`].
//!
//! Exporters are pure functions of the value, byte-deterministic:
//! [`Screenshot::to_text`] (plain UTF-8 lines), [`Screenshot::to_ansi`]
//! (SGR-styled, replayable with `cat`), and `to_svg` (the
//! GitHub-renderable artifact; sibling file `screenshot_svg.rs`).
//!
//! Honesty notes (the full story is in docs/api.md):
//! - Colors are what the terminal is told: `None` = terminal default,
//!   `Some` = the exact opaque RGB. Compositing alpha is already resolved
//!   by the time a frame is presentable, so a capture normalizes alpha
//!   away exactly like the presenter does.
//! - Hyperlink ids are NOT captured (a visual capture has no click
//!   surface); the styled debug dumps show them.
//! - Cells under a kitty/iTerm2/sixel image hold the CELL PLANE, not the
//!   picture. The driver stamps such placements as [`Screenshot::pixel_regions`];
//!   `to_svg` renders them as labeled placeholders, text/ANSI exports
//!   stay cell-plane-verbatim.

use std::path::Path;

use crate::base::{Rect, Rgba, Size};

use super::cell::Attrs;
use super::sgr::{build_incremental, build_reset, csi_n, ColorRepr, Pen};
use super::surface::Surface;

/// One captured cell. Continuations of wide glyphs are kept explicitly
/// (`width() == 0`) so the grid stays column-addressable; exporters skip
/// them (the leader renders both columns).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShotCell {
    /// Canonical storage: `""` with width 1 means a blank/space (the two
    /// are visually and wire-identical; storing them one way keeps
    /// captures comparable across sources and avoids one allocation per
    /// empty cell). `""` with width 0 is a continuation.
    text: String,
    /// Display columns: 0 (continuation), 1, or 2.
    width: u8,
    fg: Option<Rgba>,
    bg: Option<Rgba>,
    ul: Option<Rgba>,
    attrs: Attrs,
}

impl ShotCell {
    const BLANK: ShotCell = ShotCell {
        text: String::new(),
        width: 1,
        fg: None,
        bg: None,
        ul: None,
        attrs: Attrs::NONE,
    };

    /// The displayed grapheme cluster: `" "` for blanks, `""` for the
    /// continuation half of a wide glyph.
    pub fn text(&self) -> &str {
        if self.text.is_empty() && self.width == 1 {
            " "
        } else {
            &self.text
        }
    }

    /// Display columns: 0 for a continuation, 1 or 2 otherwise.
    pub fn width(&self) -> i32 {
        self.width as i32
    }

    /// True for the trailing half of a wide glyph.
    pub fn is_continuation(&self) -> bool {
        self.width == 0
    }

    /// Ink color; `None` = terminal default foreground.
    pub fn fg(&self) -> Option<Rgba> {
        self.fg
    }

    /// Ground color; `None` = terminal default background.
    pub fn bg(&self) -> Option<Rgba> {
        self.bg
    }

    /// Underline color; `None` = follow the foreground (SGR 59 state).
    pub fn ul(&self) -> Option<Rgba> {
        self.ul
    }

    /// Text attributes (bold, dim, italic, underline, undercurl, blink,
    /// reverse, hidden, strike — the engine's full [`Attrs`] set).
    pub fn attrs(&self) -> Attrs {
        self.attrs
    }

    /// Everything a default blank has: no text, no colors, no attrs.
    fn is_default_blank(&self) -> bool {
        *self == ShotCell::BLANK
    }

    fn new(
        text: &str,
        width: u8,
        fg: Option<Rgba>,
        bg: Option<Rgba>,
        ul: Option<Rgba>,
        attrs: Attrs,
    ) -> ShotCell {
        ShotCell {
            // Canonicalize the two space spellings (see the field doc).
            text: if text == " " {
                String::new()
            } else {
                text.to_string()
            },
            width,
            fg,
            bg,
            ul,
            attrs,
        }
    }
}

/// A captured screen: a row-major cell grid plus the byte-channel image
/// placements the cell plane cannot see. Plain value semantics —
/// `Clone`, `PartialEq` (goldens and roundtrip proofs compare whole
/// captures).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screenshot {
    w: i32,
    h: i32,
    cells: Vec<ShotCell>,
    pixel_regions: Vec<Rect>,
}

impl Screenshot {
    /// Capture a [`Surface`] verbatim — typically the compositor's
    /// composed frame (the driver's capture surface reads exactly that).
    ///
    /// Normalizations, all forced by the capture vocabulary and matching
    /// the presenter's own emission semantics: alpha-0 colors become
    /// `None` (that IS "terminal default"), other alphas drop to the
    /// opaque RGB the terminal would be told, the see-through
    /// [`super::cell::Glyph::EMPTY`] becomes a space (the presenter
    /// prints one), and hyperlink ids are dropped.
    pub fn from_surface(s: &Surface) -> Screenshot {
        let (w, h) = (s.width(), s.height());
        let mut cells = Vec::with_capacity((w * h).max(0) as usize);
        for y in 0..h {
            for x in 0..w {
                let Some(cell) = s.get(x, y) else {
                    cells.push(ShotCell::BLANK);
                    continue;
                };
                if cell.is_continuation() {
                    cells.push(ShotCell::new(
                        "",
                        0,
                        opaque(cell.fg),
                        opaque(cell.bg),
                        opaque(cell.ul),
                        cell.attrs,
                    ));
                    continue;
                }
                let text = s.glyph_str(cell);
                let width = cell.glyph.width().clamp(1, 2) as u8;
                cells.push(ShotCell::new(
                    if text.is_empty() { " " } else { text },
                    width,
                    opaque(cell.fg),
                    opaque(cell.bg),
                    opaque(cell.ul),
                    cell.attrs,
                ));
            }
        }
        Screenshot {
            w,
            h,
            cells,
            pixel_regions: Vec::new(),
        }
    }

    /// Assemble from pre-built cells (crate-internal: the testing rig's
    /// `VtScreen::screenshot()` bridge builds through this).
    pub(crate) fn from_cells(size: Size, cells: Vec<ShotCell>) -> Screenshot {
        debug_assert_eq!(cells.len(), (size.w * size.h).max(0) as usize);
        Screenshot {
            w: size.w.max(0),
            h: size.h.max(0),
            cells,
            pixel_regions: Vec::new(),
        }
    }

    /// Crate-internal cell builder for the [`Screenshot::from_cells`] path.
    pub(crate) fn make_cell(
        text: &str,
        width: u8,
        fg: Option<Rgba>,
        bg: Option<Rgba>,
        ul: Option<Rgba>,
        attrs: Attrs,
    ) -> ShotCell {
        ShotCell::new(text, width, fg, bg, ul, attrs)
    }

    /// Grid dimensions in cells.
    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    /// Columns.
    pub fn width(&self) -> i32 {
        self.w
    }

    /// Rows.
    pub fn height(&self) -> i32 {
        self.h
    }

    /// The cell at (x, y), or `None` outside the grid.
    pub fn cell(&self, x: i32, y: i32) -> Option<&ShotCell> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        self.cells.get((y * self.w + x) as usize)
    }

    /// Regions where a byte-channel protocol image (kitty / iTerm2 /
    /// sixel) is placed: the terminal shows PIXELS there, the cell grid
    /// holds whatever sits beneath. Stamped by `Driver::screenshot()`
    /// from the live placement bookkeeping; VT-model captures carry none
    /// (the rig consumes protocol payloads as counted, unmodeled frames).
    pub fn pixel_regions(&self) -> &[Rect] {
        &self.pixel_regions
    }

    /// Record a pixel-image placement (clipped to the grid; empty results
    /// are dropped). Public so embedders compositing their own image
    /// lanes can label regions the cell plane cannot represent.
    pub fn add_pixel_region(&mut self, rect: Rect) {
        let clipped = rect.intersect(Rect::from_size(self.size()));
        if !clipped.is_empty() {
            self.pixel_regions.push(clipped);
        }
    }

    // ---- exporters --------------------------------------------------------

    /// Plain UTF-8 text: one line per row, trailing whitespace trimmed,
    /// `\n` after every row. Identical to `VtScreen::to_text` for the
    /// same screen — the two capture sources agree on this view.
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity((self.w as usize + 1) * self.h as usize);
        let mut row = String::with_capacity(self.w.max(0) as usize * 4);
        for y in 0..self.h {
            row.clear();
            for x in 0..self.w {
                let Some(c) = self.cell(x, y) else { continue };
                if c.is_continuation() {
                    continue; // the leader renders both columns
                }
                row.push_str(c.text());
            }
            out.push_str(row.trim_end());
            out.push('\n');
        }
        out
    }

    /// SGR-styled ANSI text — `cat` it into any truecolor terminal to
    /// replay the capture. Deterministic and minimal: one SGR transition
    /// per style change (the presenter's own shorter-of incremental/reset
    /// builders), rows separated by `SGR 0` + CRLF, trailing fully-default
    /// blanks trimmed per row, no trailing newline (the exact grid,
    /// nothing more).
    ///
    /// Fidelity: replaying the bytes through the testing rig's `VtScreen`
    /// reproduces the capture exactly (test-pinned), including the
    /// cluster-fusion hazards: after a cluster that can arm terminal
    /// join state (ZWJ/VS16/ambiguous-width — the presenter's "risky"
    /// set — or a trailing regional indicator), the export re-anchors
    /// the COLUMN with `CHA` — the escape breaks pending join state
    /// exactly like the presenter's re-anchor, and column-absolute
    /// addressing bounds a live terminal's divergent width opinion
    /// (the engine-wide RT1-7 hazard) to that cluster. `CHA` rather
    /// than the presenter's `CUP` deliberately: rows here FLOW (the
    /// output must replay from any scrollback position), so the row
    /// stays relative and only the column is pinned. The one honest
    /// residue: a standalone skin-tone modifier cell adjacent to an
    /// emoji fuses on real terminals by their own rules — such content
    /// is unrepresentable in cells, capture or no capture.
    pub fn to_ansi(&self) -> String {
        let mut out: Vec<u8> = Vec::with_capacity((self.w * self.h).max(16) as usize * 2);
        let mut pen = Pen::DEFAULT;
        let mut seq_inc: Vec<u8> = Vec::new();
        let mut seq_reset: Vec<u8> = Vec::new();
        out.extend_from_slice(b"\x1b[0m"); // known state, even mid-scrollback
        for y in 0..self.h {
            // Trim trailing cells indistinguishable from an untouched
            // terminal cell (default blank); a colored trailing run stays.
            let mut end = self.w;
            while end > 0 {
                match self.cell(end - 1, y) {
                    Some(c) if c.is_default_blank() => end -= 1,
                    _ => break,
                }
            }
            let mut anchor_needed = false;
            for x in 0..end {
                let Some(c) = self.cell(x, y) else { continue };
                if c.is_continuation() {
                    continue;
                }
                if anchor_needed {
                    // Re-anchor after a fusion-arming cluster (see the
                    // method docs): the escape breaks pending ZWJ /
                    // regional-pair state in every VT, and the absolute
                    // COLUMN bounds real-terminal width drift — row
                    // stays relative so scrollback replay works. The
                    // flag is re-derived from THIS cluster below.
                    csi_n(&mut out, (x + 1) as u32, b'G');
                }
                let want = Pen {
                    attrs: c.attrs,
                    fg: repr(c.fg),
                    bg: repr(c.bg),
                    ul: repr(c.ul),
                };
                if want != pen {
                    build_incremental(&pen, &want, &mut seq_inc);
                    build_reset(&want, &mut seq_reset);
                    let params: &[u8] = if seq_reset.len() < seq_inc.len() {
                        &seq_reset
                    } else {
                        &seq_inc
                    };
                    out.extend_from_slice(b"\x1b[");
                    out.extend_from_slice(params);
                    out.push(b'm');
                    pen = want;
                }
                let text = c.text();
                out.extend_from_slice(text.as_bytes());
                anchor_needed = arms_fusion(text);
            }
            if pen != Pen::DEFAULT {
                out.extend_from_slice(b"\x1b[0m");
                pen = Pen::DEFAULT;
            }
            if y + 1 < self.h {
                out.extend_from_slice(b"\r\n");
            }
        }
        // Every byte pushed is ASCII escape machinery or a cluster str.
        String::from_utf8(out).expect("ANSI export is valid UTF-8 by construction")
    }

    // `to_svg` / `to_svg_with` live in screenshot_svg.rs (file-size budget).

    // ---- file conveniences -------------------------------------------------

    /// [`Screenshot::to_text`] to a file.
    pub fn write_text(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_text())
    }

    /// [`Screenshot::to_ansi`] to a file (view with `cat`).
    pub fn write_ansi(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_ansi())
    }

    /// [`Screenshot::to_svg`] to a file (GitHub renders it in READMEs).
    pub fn write_svg(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_svg())
    }
}

/// A cluster after which flowing straight into the next glyph could FUSE
/// the two cells: the presenter's risky set (ZWJ / VS16 /
/// ambiguous-width — terminals disagree or join across them) plus a
/// trailing regional indicator (the next indicator would complete a
/// flag pair). The exporter re-anchors the column with `CHA` after these.
fn arms_fusion(cluster: &str) -> bool {
    if crate::text::is_risky_cluster(cluster) {
        return true;
    }
    cluster
        .chars()
        .next_back()
        .is_some_and(|c| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c))
}

/// Alpha-0 = terminal default; anything else is told to the terminal as
/// its opaque RGB (exactly `sgr::repr_rgb`'s reading of a cell color).
fn opaque(c: Rgba) -> Option<Rgba> {
    if c.is_transparent() {
        None
    } else {
        Some(Rgba::rgb(c.r, c.g, c.b))
    }
}

fn repr(c: Option<Rgba>) -> ColorRepr {
    match c {
        None => ColorRepr::Default,
        Some(c) => ColorRepr::Rgb(c.r, c.g, c.b),
    }
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
