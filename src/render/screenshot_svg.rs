//! SVG export for [`Screenshot`] — the documentation/report artifact
//! (GitHub renders SVG in READMEs). Split from `screenshot.rs` for the
//! file-size budget; same type, sibling impl.
//!
//! Deterministic by construction: pure function of the capture, integer
//! cell metrics, no floats, no map iteration. Layout: one merged `<rect>`
//! per same-background run, one `<text>` per same-style run pinned to its
//! columns with `textLength` (font metric drift cannot shear the grid),
//! explicit decoration rects (underline / strike; undercurl draws as a
//! straight underline — labeled downlevel), and labeled placeholder
//! veils over [`Screenshot::pixel_regions`] (the cells beneath a
//! protocol image are not the picture — the veil says so instead of
//! letting the capture lie).

use std::fmt::Write as _;

use crate::base::Rgba;

use super::cell::Attrs;
use super::screenshot::{Screenshot, ShotCell};

/// Cell box in SVG user units. ~1:2 aspect, the terminal cell shape.
const CELL_W: i32 = 9;
const CELL_H: i32 = 18;
/// Font size + baseline offset tuned for the 9x18 box.
const FONT_SIZE: i32 = 15;
const BASELINE: i32 = 14;
/// Ink/paper for cells carrying "terminal default" colors — classic
/// light-on-dark terminal defaults, NOT theme tokens (the render layer
/// resolves no themes; pass explicit colors via [`Screenshot::to_svg_with`]).
const DEFAULT_FG: Rgba = Rgba::rgb(0xd0, 0xd0, 0xd0);
const DEFAULT_BG: Rgba = Rgba::rgb(0x00, 0x00, 0x00);

const FONT_STACK: &str =
    "ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace";

impl Screenshot {
    /// SVG export with the built-in default ink/paper (see
    /// [`Screenshot::to_svg_with`] for the full contract).
    pub fn to_svg(&self) -> String {
        self.to_svg_with(DEFAULT_FG, DEFAULT_BG)
    }

    /// SVG export, choosing what "terminal default" foreground/background
    /// mean (cells captured from a themed app carry concrete colors, so
    /// these only show where nothing was ever painted).
    ///
    /// Visual attribute mapping: bold -> `font-weight:700`, italic ->
    /// `font-style:italic`, dim -> `fill-opacity:0.6`, reverse -> fg/bg
    /// swapped at paint time, hidden -> background only, underline +
    /// undercurl -> a 1-unit underline rect (colored by the cell's
    /// underline color when set), strike -> a 1-unit rect through the
    /// x-height, blink -> static (an SVG still cannot blink honestly).
    pub fn to_svg_with(&self, default_fg: Rgba, default_bg: Rgba) -> String {
        let (w, h) = (self.width(), self.height());
        let (px_w, px_h) = (w.max(0) * CELL_W, h.max(0) * CELL_H);
        let mut out = String::with_capacity(1024 + (w * h).max(0) as usize * 8);
        let _ = writeln!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {px_w} {px_h}\" \
             width=\"{px_w}\" height=\"{px_h}\" font-family=\"{FONT_STACK}\" \
             font-size=\"{FONT_SIZE}\" xml:space=\"preserve\">"
        );
        let _ = writeln!(
            out,
            "<rect width=\"{px_w}\" height=\"{px_h}\" fill=\"{}\"/>",
            hex(default_bg)
        );

        self.svg_backgrounds(&mut out, default_fg, default_bg);
        self.svg_text_runs(&mut out, default_fg, default_bg);
        self.svg_decorations(&mut out, default_fg, default_bg);
        self.svg_pixel_veils(&mut out);

        out.push_str("</svg>\n");
        out
    }

    /// Merged background rects, one per same-color run; runs matching the
    /// page background are skipped (already painted).
    fn svg_backgrounds(&self, out: &mut String, default_fg: Rgba, default_bg: Rgba) {
        out.push_str("<g shape-rendering=\"crispEdges\">\n");
        for y in 0..self.height() {
            let mut x = 0;
            while x < self.width() {
                let bg = self
                    .cell(x, y)
                    .map(|c| resolve(c, default_fg, default_bg).1)
                    .unwrap_or(default_bg);
                let start = x;
                x += 1;
                while x < self.width() {
                    let next = self
                        .cell(x, y)
                        .map(|c| resolve(c, default_fg, default_bg).1)
                        .unwrap_or(default_bg);
                    if next != bg {
                        break;
                    }
                    x += 1;
                }
                if bg != default_bg {
                    let _ = writeln!(
                        out,
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{CELL_H}\" fill=\"{}\"/>",
                        start * CELL_W,
                        y * CELL_H,
                        (x - start) * CELL_W,
                        hex(bg)
                    );
                }
            }
        }
        out.push_str("</g>\n");
    }

    /// Text runs: consecutive leader cells sharing (resolved fg, bold,
    /// dim, italic) merge into one column-pinned `<text>`; wide glyphs
    /// run alone so a fallback font cannot shear the columns after them;
    /// hidden cells and all-space runs paint nothing.
    fn svg_text_runs(&self, out: &mut String, default_fg: Rgba, default_bg: Rgba) {
        let mut run = String::new();
        for y in 0..self.height() {
            let mut x = 0;
            while x < self.width() {
                let Some(cell) = self.cell(x, y) else {
                    x += 1;
                    continue;
                };
                if cell.is_continuation()
                    || cell.attrs().contains(Attrs::HIDDEN)
                    || cell.width() == 2
                {
                    // Wide glyph: emit as its own pinned run below.
                    if cell.width() == 2 && !cell.attrs().contains(Attrs::HIDDEN) {
                        run.clear();
                        run.push_str(cell.text());
                        emit_text_run(out, x, y, 2, cell, &run, default_fg, default_bg);
                    }
                    x += cell.width().max(1);
                    continue;
                }
                // Narrow run: extend while style matches and cells stay
                // narrow and visible.
                let key = style_key(cell, default_fg, default_bg);
                let start = x;
                run.clear();
                run.push_str(cell.text());
                x += 1;
                while x < self.width() {
                    let Some(next) = self.cell(x, y) else { break };
                    if next.width() != 1
                        || next.attrs().contains(Attrs::HIDDEN)
                        || style_key(next, default_fg, default_bg) != key
                    {
                        break;
                    }
                    run.push_str(next.text());
                    x += 1;
                }
                if run.bytes().any(|b| b != b' ') {
                    emit_text_run(out, start, y, x - start, cell, &run, default_fg, default_bg);
                }
            }
        }
    }

    /// Underline (incl. undercurl, downleveled straight) and strike
    /// rects, merged per same-color run.
    fn svg_decorations(&self, out: &mut String, default_fg: Rgba, default_bg: Rgba) {
        // (attr mask picking the decoration, y offset inside the cell box)
        const DECOS: [(u16, i32); 2] = [(UNDERLINE_MASK, 15), (STRIKE_MASK, 9)];
        for (mask_bits, dy) in DECOS {
            let mask = Attrs::from_bits_truncate(mask_bits);
            for y in 0..self.height() {
                let mut x = 0;
                while x < self.width() {
                    let color = self
                        .cell(x, y)
                        .filter(|c| c.attrs().intersects(mask))
                        .map(|c| deco_color(c, mask, default_fg, default_bg));
                    let Some(color) = color else {
                        x += 1;
                        continue;
                    };
                    let start = x;
                    x += 1;
                    while x < self.width() {
                        let same = self
                            .cell(x, y)
                            .filter(|c| c.attrs().intersects(mask))
                            .map(|c| deco_color(c, mask, default_fg, default_bg))
                            == Some(color);
                        if !same {
                            break;
                        }
                        x += 1;
                    }
                    let _ = writeln!(
                        out,
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"1\" fill=\"{}\"/>",
                        start * CELL_W,
                        y * CELL_H + dy,
                        (x - start) * CELL_W,
                        hex(color)
                    );
                }
            }
        }
    }

    /// Labeled veils over byte-channel image placements: the honest
    /// "pixels live here, cells cannot show them" marker.
    fn svg_pixel_veils(&self, out: &mut String) {
        for r in self.pixel_regions() {
            let (x, y) = (r.x * CELL_W, r.y * CELL_H);
            let (w, h) = (r.w * CELL_W, r.h * CELL_H);
            let _ = writeln!(
                out,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#7f7f7f\" \
                 fill-opacity=\"0.25\" stroke=\"#ffffff\" stroke-opacity=\"0.5\" \
                 stroke-dasharray=\"4 3\"/>"
            );
            let _ = writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\" \
                 fill=\"#ffffff\" fill-opacity=\"0.8\">image (pixels)</text>",
                x + w / 2,
                y + h / 2 + 4
            );
        }
    }
}

const UNDERLINE_MASK: u16 = Attrs::UNDERLINE.bits() | Attrs::UNDERCURL.bits();
const STRIKE_MASK: u16 = Attrs::STRIKE.bits();

/// (fg, bg) with defaults substituted and REVERSE applied — what the
/// viewer's eye receives.
fn resolve(c: &ShotCell, default_fg: Rgba, default_bg: Rgba) -> (Rgba, Rgba) {
    let fg = c.fg().unwrap_or(default_fg);
    let bg = c.bg().unwrap_or(default_bg);
    if c.attrs().contains(Attrs::REVERSE) {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

/// Underline color follows SGR 58 when set and the ink otherwise;
/// strike always follows the ink.
fn deco_color(c: &ShotCell, mask: Attrs, default_fg: Rgba, default_bg: Rgba) -> Rgba {
    let ink = resolve(c, default_fg, default_bg).0;
    if mask.intersects(Attrs::UNDERLINE | Attrs::UNDERCURL) {
        c.ul().unwrap_or(ink)
    } else {
        ink
    }
}

/// Everything that changes how a narrow glyph paints (bg is handled by
/// the background pass).
fn style_key(c: &ShotCell, default_fg: Rgba, default_bg: Rgba) -> (Rgba, bool, bool, bool) {
    let ink = resolve(c, default_fg, default_bg).0;
    (
        ink,
        c.attrs().contains(Attrs::BOLD),
        c.attrs().contains(Attrs::DIM),
        c.attrs().contains(Attrs::ITALIC),
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_text_run(
    out: &mut String,
    x: i32,
    y: i32,
    cols: i32,
    style_of: &ShotCell,
    run: &str,
    default_fg: Rgba,
    default_bg: Rgba,
) {
    let ink = resolve(style_of, default_fg, default_bg).0;
    let _ = write!(
        out,
        "<text x=\"{}\" y=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\"",
        x * CELL_W,
        y * CELL_H + BASELINE,
        cols * CELL_W,
        hex(ink)
    );
    let attrs = style_of.attrs();
    if attrs.contains(Attrs::BOLD) {
        out.push_str(" font-weight=\"700\"");
    }
    if attrs.contains(Attrs::ITALIC) {
        out.push_str(" font-style=\"italic\"");
    }
    if attrs.contains(Attrs::DIM) {
        out.push_str(" fill-opacity=\"0.6\"");
    }
    out.push('>');
    push_escaped(out, run);
    out.push_str("</text>\n");
}

fn hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// XML text-content escaping (the five entities — attribute-safe too, so
/// one rule serves the whole exporter).
fn push_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
}
