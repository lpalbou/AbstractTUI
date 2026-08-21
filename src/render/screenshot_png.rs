//! PNG export for [`Screenshot`] — the FAITHFUL capture artifact.
//!
//! Why a second image format beside `to_svg`: an SVG of a terminal is
//! a promise the writer cannot keep. It names a font family and hopes
//! the viewer has one whose advance width matches the grid; when it
//! does not, every glyph is stretched to fit its column, box-drawing
//! strokes stop meeting, and the picture lies about how the app looks.
//! A PNG carries no such hope. Every pixel here is decided by this
//! code: text is drawn from an embedded 8x16 bitmap
//! ([`screenshot_font_data`](super::screenshot_font_data)), and the
//! ranges that must TILE — box drawing, block elements, braille,
//! sextants — are drawn GEOMETRICALLY, so their strokes meet exactly
//! at every cell boundary on every machine that ever opens the file.
//!
//! Determinism is the contract: the same capture produces byte-
//! identical PNG bytes on any platform (integer math, no map
//! iteration, no floats).
//!
//! Coverage is bounded and HONEST: a character outside the bitmap
//! table and the geometric ranges draws a hollow placeholder box, not
//! a wrong glyph and not a blank. CJK, emoji, and other scripts fall
//! there today — see `docs/api.md` § "Stability and limits".

use crate::base::{Rect, Rgba};
use crate::gfx::{png_encode, Bitmap};

use super::cell::Attrs;
use super::screenshot::{Screenshot, ShotCell};
use super::screenshot_font_data::{GLYPHS, GLYPH_H, GLYPH_W};

/// Knobs for [`Screenshot::to_png_with`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PngOpts {
    /// Integer pixel multiplier. `1` = one cell per 8x16 pixels (a
    /// 90x30 capture lands at 720x480); `2` doubles it for a display
    /// that wants the extra density. Clamped to 1..=8.
    pub scale: u32,
    /// Color for cells carrying "terminal default" foreground.
    pub default_fg: Rgba,
    /// Color for cells carrying "terminal default" background.
    pub default_bg: Rgba,
}

impl Default for PngOpts {
    fn default() -> Self {
        PngOpts {
            scale: 1,
            // The same classic light-on-dark defaults `to_svg` uses, so
            // the two artifacts of one capture agree.
            default_fg: Rgba::rgb(0xEE, 0xEE, 0xEE),
            default_bg: Rgba::rgb(0x00, 0x00, 0x00),
        }
    }
}

impl Screenshot {
    /// Render the capture to PNG bytes with the default options.
    ///
    /// ```
    /// use abstracttui::base::Size;
    /// use abstracttui::render::style::Style;
    /// use abstracttui::render::{Cell, Screenshot, Surface};
    ///
    /// let mut surface = Surface::new(Size::new(8, 2), Cell::EMPTY);
    /// surface.draw_text(0, 0, "hello", Style::new());
    /// let png = Screenshot::from_surface(&surface).to_png();
    /// assert_eq!(&png[1..4], b"PNG", "a real PNG stream");
    /// ```
    pub fn to_png(&self) -> Vec<u8> {
        self.to_png_with(PngOpts::default())
    }

    /// Render the capture to PNG bytes.
    pub fn to_png_with(&self, opts: PngOpts) -> Vec<u8> {
        png_encode::encode(&self.to_bitmap(opts))
    }

    /// Render the capture to an RGBA bitmap — the PNG path's own
    /// output, exposed because a caller compositing several captures
    /// (a contact sheet, a diff strip) should not have to round-trip
    /// through the encoder.
    pub fn to_bitmap(&self, opts: PngOpts) -> Bitmap {
        let scale = opts.scale.clamp(1, 8);
        let (cols, rows) = (self.width().max(0) as u32, self.height().max(0) as u32);
        let (cw, ch) = (GLYPH_W * scale, GLYPH_H * scale);
        let mut img = Bitmap::new((cols * cw).max(1), (rows * ch).max(1), opts.default_bg);

        for y in 0..rows {
            for x in 0..cols {
                let Some(cell) = self.cell(x as i32, y as i32) else {
                    continue;
                };
                // A continuation cell is the second half of a wide
                // glyph: the lead cell already painted it.
                if cell.is_continuation() {
                    continue;
                }
                paint_cell(&mut img, cell, x * cw, y * ch, cw, ch, scale, &opts);
            }
        }
        // Protocol-image regions are NOT in the capture (the cells
        // beneath a kitty/sixel placement are not the picture): veil
        // them, exactly as the SVG writer does, so the artifact says
        // what it does not know.
        for region in self.pixel_regions() {
            veil(&mut img, *region, cw, ch, scale);
        }
        img
    }

    /// Write the capture to `path` as a PNG.
    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_png())
    }
}

/// Paint one cell: background, glyph, then the line decorations.
#[allow(clippy::too_many_arguments)]
fn paint_cell(
    img: &mut Bitmap,
    cell: &ShotCell,
    ox: u32,
    oy: u32,
    cw: u32,
    ch: u32,
    scale: u32,
    opts: &PngOpts,
) {
    let attrs = cell.attrs();
    let (mut fg, mut bg) = (
        cell.fg().unwrap_or(opts.default_fg),
        cell.bg().unwrap_or(opts.default_bg),
    );
    if attrs.contains(Attrs::REVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if attrs.contains(Attrs::DIM) {
        fg = mix(fg, bg, 2, 3); // two parts ink, one part paper
    }
    // A wide glyph owns two cells' worth of pixels.
    let span = cw * (cell.width().max(1) as u32);
    fill(img, ox, oy, span, ch, bg);

    if !attrs.contains(Attrs::HIDDEN) {
        if let Some(g) = cell.text().chars().next() {
            draw_glyph(img, g, ox, oy, span, ch, scale, fg, attrs);
        }
    }

    // Decorations ride on top of the glyph, in the underline color
    // when the capture carries one.
    let ul = cell.ul().unwrap_or(fg);
    if attrs.contains(Attrs::UNDERLINE) || attrs.contains(Attrs::UNDERCURL) {
        // Undercurl draws as a straight underline — labeled downlevel
        // in the docs, same call the SVG writer makes.
        let y = oy + ch - 2 * scale;
        fill(img, ox, y, span, scale, ul);
    }
    if attrs.contains(Attrs::STRIKE) {
        let y = oy + ch / 2;
        fill(img, ox, y, span, scale, ul);
    }
}

/// Draw one character: geometry first (the tiling ranges), then the
/// bitmap table, then the honest placeholder.
#[allow(clippy::too_many_arguments)]
fn draw_glyph(
    img: &mut Bitmap,
    g: char,
    ox: u32,
    oy: u32,
    cw: u32,
    ch: u32,
    scale: u32,
    fg: Rgba,
    attrs: Attrs,
) {
    if g == ' ' || g == '\u{a0}' {
        return;
    }
    if geometry::draw(img, g, ox, oy, cw, ch, scale, fg) {
        return;
    }
    let Ok(idx) = GLYPHS.binary_search_by_key(&g, |(c, _)| *c) else {
        placeholder(img, ox, oy, cw, ch, scale, fg);
        return;
    };
    let rows = &GLYPHS[idx].1;
    for (row, bits) in rows.iter().enumerate() {
        for col in 0..GLYPH_W {
            if bits & (0x80 >> col) == 0 {
                continue;
            }
            let px = ox + col * scale;
            let py = oy + row as u32 * scale;
            fill(img, px, py, scale, scale, fg);
            // Bold is a one-pixel horizontal smear — the bitmap-font
            // convention, and the only weight this table can offer.
            if attrs.contains(Attrs::BOLD) {
                fill(img, px + scale, py, scale, scale, fg);
            }
        }
    }
}

/// A character this build cannot draw: a hollow box, so the reader
/// sees "something was here and the writer knows it did not draw it".
fn placeholder(img: &mut Bitmap, ox: u32, oy: u32, cw: u32, ch: u32, scale: u32, fg: Rgba) {
    let (x0, y0) = (ox + scale, oy + 2 * scale);
    let (w, h) = (cw.saturating_sub(2 * scale), ch.saturating_sub(4 * scale));
    if w == 0 || h == 0 {
        return;
    }
    fill(img, x0, y0, w, scale, fg);
    fill(img, x0, y0 + h - scale, w, scale, fg);
    fill(img, x0, y0, scale, h, fg);
    fill(img, x0 + w - scale, y0, scale, h, fg);
}

/// Label the cells under a pixel-protocol placement: a diagonal hatch,
/// so nobody mistakes the veil for content.
fn veil(img: &mut Bitmap, region: Rect, cw: u32, ch: u32, scale: u32) {
    let ink = Rgba::rgb(0x44, 0x44, 0x55);
    let (x0, y0) = (region.x.max(0) as u32 * cw, region.y.max(0) as u32 * ch);
    let (w, h) = (region.w.max(0) as u32 * cw, region.h.max(0) as u32 * ch);
    for y in 0..h {
        for x in 0..w {
            if (x / scale + y / scale) % 6 < 2 {
                fill(img, x0 + x, y0 + y, 1, 1, ink);
            }
        }
    }
}

fn fill(img: &mut Bitmap, x: u32, y: u32, w: u32, h: u32, color: Rgba) {
    for py in y..y.saturating_add(h) {
        for px in x..x.saturating_add(w) {
            img.set(px, py, color);
        }
    }
}

/// Straight-alpha-free blend of two opaque colors: `num/den` of `a`.
fn mix(a: Rgba, b: Rgba, num: u32, den: u32) -> Rgba {
    let c = |x: u8, y: u8| ((x as u32 * num + y as u32 * (den - num)) / den) as u8;
    Rgba::rgb(c(a.r, b.r), c(a.g, b.g), c(a.b, b.b))
}

/// The glyph ranges a terminal UI is actually built from — drawn as
/// GEOMETRY, never as font bitmaps, so every stroke meets its
/// neighbour exactly at the cell boundary.
mod geometry {
    use super::fill;
    use crate::base::Rgba;
    use crate::gfx::Bitmap;

    /// Returns true when `g` was drawn here.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        img: &mut Bitmap,
        g: char,
        ox: u32,
        oy: u32,
        cw: u32,
        ch: u32,
        scale: u32,
        fg: Rgba,
    ) -> bool {
        match g as u32 {
            0x2500..=0x257F => box_drawing(img, g, ox, oy, cw, ch, scale, fg),
            0x2580..=0x259F => block(img, g, ox, oy, cw, ch, fg),
            0x2800..=0x28FF => braille(img, g, ox, oy, cw, ch, scale, fg),
            0x1FB00..=0x1FB3B => sextant(img, g, ox, oy, cw, ch, fg),
            _ => false,
        }
    }

    /// Stroke weights per side, in the order up, right, down, left.
    /// 0 = none, 1 = light, 2 = heavy, 3 = double.
    fn sides(g: char) -> Option<([u8; 4], bool)> {
        // (up, right, down, left), rounded corner
        let s = match g {
            '─' => ([0, 1, 0, 1], false),
            '━' => ([0, 2, 0, 2], false),
            '│' => ([1, 0, 1, 0], false),
            '┃' => ([2, 0, 2, 0], false),
            '┌' => ([0, 1, 1, 0], false),
            '┍' => ([0, 2, 1, 0], false),
            '┎' => ([0, 1, 2, 0], false),
            '┏' => ([0, 2, 2, 0], false),
            '┐' => ([0, 0, 1, 1], false),
            '┑' => ([0, 0, 1, 2], false),
            '┒' => ([0, 0, 2, 1], false),
            '┓' => ([0, 0, 2, 2], false),
            '└' => ([1, 1, 0, 0], false),
            '┕' => ([1, 2, 0, 0], false),
            '┖' => ([2, 1, 0, 0], false),
            '┗' => ([2, 2, 0, 0], false),
            '┘' => ([1, 0, 0, 1], false),
            '┙' => ([1, 0, 0, 2], false),
            '┚' => ([2, 0, 0, 1], false),
            '┛' => ([2, 0, 0, 2], false),
            '├' => ([1, 1, 1, 0], false),
            '┤' => ([1, 0, 1, 1], false),
            '┬' => ([0, 1, 1, 1], false),
            '┴' => ([1, 1, 0, 1], false),
            '┼' => ([1, 1, 1, 1], false),
            '┝' => ([1, 2, 1, 0], false),
            '┥' => ([1, 0, 1, 2], false),
            '┯' => ([0, 2, 1, 2], false),
            '┷' => ([1, 2, 0, 2], false),
            '┿' => ([1, 2, 1, 2], false),
            '╂' => ([2, 1, 2, 1], false),
            '╋' => ([2, 2, 2, 2], false),
            '┠' => ([2, 1, 2, 0], false),
            '┨' => ([2, 0, 2, 1], false),
            '┳' => ([0, 2, 2, 2], false),
            '┻' => ([2, 2, 0, 2], false),
            '═' => ([0, 3, 0, 3], false),
            '║' => ([3, 0, 3, 0], false),
            '╔' => ([0, 3, 3, 0], false),
            '╗' => ([0, 0, 3, 3], false),
            '╚' => ([3, 3, 0, 0], false),
            '╝' => ([3, 0, 0, 3], false),
            '╠' => ([3, 3, 3, 0], false),
            '╣' => ([3, 0, 3, 3], false),
            '╦' => ([0, 3, 3, 3], false),
            '╩' => ([3, 3, 0, 3], false),
            '╬' => ([3, 3, 3, 3], false),
            '╭' => ([0, 1, 1, 0], true),
            '╮' => ([0, 0, 1, 1], true),
            '╯' => ([1, 0, 0, 1], true),
            '╰' => ([1, 1, 0, 0], true),
            // Dashed families draw as their solid parent: the dash
            // pattern is decoration, the CONNECTION is the meaning.
            '┄' | '┈' | '╌' => ([0, 1, 0, 1], false),
            '┅' | '┉' | '╍' => ([0, 2, 0, 2], false),
            '┆' | '┊' | '╎' => ([1, 0, 1, 0], false),
            '┇' | '┋' | '╏' => ([2, 0, 2, 0], false),
            '╴' => ([0, 0, 0, 1], false),
            '╵' => ([1, 0, 0, 0], false),
            '╶' => ([0, 1, 0, 0], false),
            '╷' => ([0, 0, 1, 0], false),
            _ => return None,
        };
        Some(s)
    }

    #[allow(clippy::too_many_arguments)]
    fn box_drawing(
        img: &mut Bitmap,
        g: char,
        ox: u32,
        oy: u32,
        cw: u32,
        ch: u32,
        scale: u32,
        fg: Rgba,
    ) -> bool {
        let Some((w, _rounded)) = sides(g) else {
            return false;
        };
        // Rounded corners (╭╮╯╰) draw as square ones: at an 8x16 cell
        // the arc and the corner occupy the same pixels, so rounding
        // would be a lie about resolution rather than a detail.
        // The centre line sits on the cell's middle track; a light
        // stroke is one scaled pixel, heavy two, double is two rails.
        let (mx, my) = (ox + cw / 2 - scale / 2, oy + ch / 2 - scale / 2);
        let thick = |weight: u8| -> u32 {
            match weight {
                2 => 2 * scale,
                _ => scale,
            }
        };
        for (i, weight) in w.iter().enumerate() {
            if *weight == 0 {
                continue;
            }
            let t = thick(*weight);
            // Double lines are two parallel rails straddling the centre.
            let rails: &[i32] = if *weight == 3 { &[-1, 1] } else { &[0] };
            for rail in rails {
                let off = rail * scale as i32;
                match i {
                    // up
                    0 => {
                        let x = (mx as i32 + off).max(0) as u32;
                        fill(img, x, oy, t, (my + t).saturating_sub(oy), fg);
                    }
                    // right
                    1 => {
                        let y = (my as i32 + off).max(0) as u32;
                        fill(img, mx, y, ox + cw - mx, t, fg);
                    }
                    // down
                    2 => {
                        let x = (mx as i32 + off).max(0) as u32;
                        fill(img, x, my, t, oy + ch - my, fg);
                    }
                    // left
                    _ => {
                        let y = (my as i32 + off).max(0) as u32;
                        fill(img, ox, y, mx + t - ox, t, fg);
                    }
                }
            }
        }
        true
    }

    /// Block elements: eighth bars, quadrants, shades — all rectangles
    /// on the cell's own grid, so they abut with no seam.
    fn block(img: &mut Bitmap, g: char, ox: u32, oy: u32, cw: u32, ch: u32, fg: Rgba) -> bool {
        let eighth_h = |n: u32| (ch * n).div_ceil(8);
        let eighth_w = |n: u32| (cw * n).div_ceil(8);
        match g {
            // Lower eighths ▁..█ and upper half ▀.
            '▀' => fill(img, ox, oy, cw, ch / 2, fg),
            '▁'..='▇' => {
                let n = g as u32 - '▁' as u32 + 1;
                let h = eighth_h(n);
                fill(img, ox, oy + ch - h, cw, h, fg);
            }
            '█' => fill(img, ox, oy, cw, ch, fg),
            // Left eighths ▉..▏ (8/8 down to 1/8).
            // U+2589..U+258F run from seven eighths down to one:
            // eighths = 8 - (offset from the FULL block).
            '▉'..='▏' => {
                let eighths = 8 - (g as u32 - '█' as u32);
                fill(img, ox, oy, eighth_w(eighths), ch, fg);
            }
            '▐' => fill(img, ox + cw / 2, oy, cw - cw / 2, ch, fg),
            '▔' => fill(img, ox, oy, cw, eighth_h(1), fg),
            '▕' => fill(img, ox + cw - eighth_w(1), oy, eighth_w(1), ch, fg),
            // Shades: a dither at cell resolution, not a tint — the
            // capture must survive being viewed at 1:1.
            '░' | '▒' | '▓' => {
                let period = match g {
                    '░' => 4,
                    '▒' => 2,
                    _ => 4,
                };
                let keep = if g == '▓' { 3 } else { 1 };
                for y in 0..ch {
                    for x in 0..cw {
                        if (x + y) % period < keep {
                            fill(img, ox + x, oy + y, 1, 1, fg);
                        }
                    }
                }
            }
            // Quadrants: bit per corner (UL, UR, LL, LR).
            '▖' | '▗' | '▘' | '▙' | '▚' | '▛' | '▜' | '▝' | '▞' | '▟' => {
                let bits = match g {
                    '▘' => 0b0001,
                    '▝' => 0b0010,
                    '▖' => 0b0100,
                    '▗' => 0b1000,
                    '▚' => 0b1001,
                    '▞' => 0b0110,
                    '▙' => 0b1101,
                    '▛' => 0b0111,
                    '▜' => 0b1011,
                    _ => 0b1110, // ▟
                };
                let (hw, hh) = (cw / 2, ch / 2);
                for (i, (dx, dy)) in [(0, 0), (hw, 0), (0, hh), (hw, hh)].iter().enumerate() {
                    if bits & (1 << i) != 0 {
                        fill(img, ox + dx, oy + dy, cw - hw, ch - hh, fg);
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// Braille: the codepoint's low 8 bits ARE the dot pattern
    /// (2 columns x 4 rows, the Unicode dot order).
    #[allow(clippy::too_many_arguments)]
    fn braille(
        img: &mut Bitmap,
        g: char,
        ox: u32,
        oy: u32,
        cw: u32,
        ch: u32,
        scale: u32,
        fg: Rgba,
    ) -> bool {
        let bits = g as u32 - 0x2800;
        // Unicode braille bit order: 0,1,2 = left column rows 0..2;
        // 3,4,5 = right column rows 0..2; 6 = left row 3; 7 = right row 3.
        let dots = [
            (0u32, 0u32, 0),
            (0, 1, 1),
            (0, 2, 2),
            (1, 0, 3),
            (1, 1, 4),
            (1, 2, 5),
            (0, 3, 6),
            (1, 3, 7),
        ];
        let (dw, dh) = (cw / 2, ch / 4);
        // Dot size is what makes braille read as INK rather than as
        // punctuation: terminal fonts draw fat dots, and this range is
        // how the engine's mosaic renderer paints pictures. Three
        // quarters of the sub-cell matches what a terminal shows;
        // anything thinner turns a braille image into a grey wash.
        let (rw, rh) = ((dw * 3 / 4).max(1), (dh * 3 / 4).max(1));
        for (col, row, bit) in dots {
            if bits & (1 << bit) == 0 {
                continue;
            }
            let cx = ox + col * dw + (dw - rw) / 2;
            let cy = oy + row * dh + (dh - rh) / 2;
            fill(img, cx, cy, rw, rh, fg);
        }
        let _ = scale;
        true
    }

    /// Sextants (U+1FB00..): a 2x3 lattice, the index's bits in
    /// row-major order with the three legacy-block patterns skipped.
    fn sextant(img: &mut Bitmap, g: char, ox: u32, oy: u32, cw: u32, ch: u32, fg: Rgba) -> bool {
        let raw = g as u32 - 0x1FB00 + 1;
        // U+1FB00.. skips patterns 21 (▌), 42 (▐) and 63 (█).
        let pattern = match raw {
            0..=20 => raw,
            21..=40 => raw + 1,
            41..=60 => raw + 2,
            _ => return false,
        };
        let (hw, th) = (cw / 2, ch / 3);
        for bit in 0..6u32 {
            if pattern & (1 << bit) == 0 {
                continue;
            }
            let (col, row) = (bit % 2, bit / 2);
            let x = ox + col * hw;
            let y = oy + row * th;
            let w = if col == 1 { cw - hw } else { hw };
            let h = if row == 2 { ch - 2 * th } else { th };
            fill(img, x, y, w, h, fg);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::render::style::Style;
    use crate::render::{Cell, Surface};

    fn shot(text: &str, w: i32, h: i32) -> Screenshot {
        let mut s = Surface::new(Size::new(w, h), Cell::EMPTY);
        for y in 0..h {
            s.draw_text(
                0,
                y,
                text,
                Style::new()
                    .fg(Rgba::rgb(255, 255, 255))
                    .bg(Rgba::rgb(0, 0, 0)),
            );
        }
        Screenshot::from_surface(&s)
    }

    fn bitmap(text: &str, w: i32, h: i32) -> Bitmap {
        shot(text, w, h).to_bitmap(PngOpts {
            scale: 2,
            ..Default::default()
        })
    }

    fn ink(b: &Bitmap, x: u32, y: u32) -> bool {
        b.get(x, y).is_some_and(|p| p.r > 128)
    }

    /// THE reason this writer exists: box-drawing strokes must meet
    /// across cell boundaries. A font cannot promise that; geometry
    /// can, so the promise is a test.
    #[test]
    fn box_strokes_tile_across_cell_boundaries() {
        // Three stacked verticals: the ink column is unbroken top to
        // bottom, including at both seams.
        let b = bitmap("│", 1, 3);
        for y in 0..b.height() {
            assert!(ink(&b, 7, y), "vertical broken at y={y}");
        }
        // Three side-by-side horizontals: unbroken left to right.
        let b = bitmap("───", 3, 1);
        for x in 0..b.width() {
            assert!(ink(&b, x, 16), "horizontal broken at x={x}");
        }
        // A corner pair meets: ┌ turns down-and-right, ┐ turns
        // down-and-left, and the run between them is continuous.
        let b = bitmap("┌─┐", 3, 1);
        for x in 7..(b.width() - 7) {
            assert!(ink(&b, x, 16), "corner run broken at x={x}");
        }
    }

    /// Weight is the other thing a font would decide for us.
    #[test]
    fn stroke_weights_are_distinct_and_centered() {
        let across = |ch: &str| -> usize {
            let b = bitmap(ch, 1, 1);
            (0..b.width()).filter(|&x| ink(&b, x, 16)).count()
        };
        assert_eq!(across("│"), 2, "light");
        assert_eq!(across("┃"), 4, "heavy is twice light");
        assert_eq!(across("║"), 4, "double is two light rails");
        // The double's rails have a gap between them; the heavy does not.
        let b = bitmap("║", 1, 1);
        let lit: Vec<u32> = (0..b.width()).filter(|&x| ink(&b, x, 16)).collect();
        assert!(
            lit.last().unwrap() - lit.first().unwrap() > 3,
            "double rails must straddle the centre: {lit:?}"
        );
    }

    /// Blocks are the mosaic vocabulary: they must fill their exact
    /// fraction of the cell, edge to edge, or an image drawn with them
    /// grows seams.
    #[test]
    fn block_elements_fill_exact_fractions() {
        let coverage = |ch: &str| -> f32 {
            let b = bitmap(ch, 1, 1);
            let lit = (0..b.height())
                .flat_map(|y| (0..b.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| ink(&b, x, y))
                .count();
            lit as f32 / (b.width() * b.height()) as f32
        };
        assert_eq!(coverage("█"), 1.0, "full block");
        assert!((coverage("▀") - 0.5).abs() < 0.01, "upper half");
        assert!((coverage("▌") - 0.5).abs() < 0.01, "left half");
        assert!((coverage("▄") - 0.5).abs() < 0.01, "lower half");
        assert!((coverage("▁") - 0.125).abs() < 0.02, "one eighth");
        assert!((coverage(" ") - 0.0).abs() < 0.01, "space is paper");
    }

    /// Braille carries pictures in this engine (the mosaic renderer's
    /// densest mode). Dots must read as ink, not as punctuation.
    #[test]
    fn braille_dots_read_as_ink() {
        let b = bitmap("⣿", 1, 1);
        let lit = (0..b.height())
            .flat_map(|y| (0..b.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| ink(&b, x, y))
            .count() as f32
            / (b.width() * b.height()) as f32;
        assert!(
            (0.4..0.75).contains(&lit),
            "all-dots braille covered {lit:.2} of the cell"
        );
        // An empty braille cell is blank, and a single dot is one dot.
        let empty = bitmap("⠀", 1, 1);
        assert!(
            (0..empty.height()).all(|y| (0..empty.width()).all(|x| !ink(&empty, x, y))),
            "U+2800 is blank"
        );
    }

    /// A character this build cannot draw gets a labeled placeholder —
    /// never a wrong glyph, never a silent blank — and a WIDE one
    /// covers both of its cells.
    #[test]
    fn unsupported_glyphs_draw_a_placeholder_spanning_their_width() {
        let b = bitmap("日", 2, 1);
        let lit: Vec<u32> = (0..b.width()).filter(|&x| ink(&b, x, 8)).collect();
        let (first, last) = (*lit.first().unwrap(), *lit.last().unwrap());
        assert!(first < 4, "placeholder starts in the first cell: {first}");
        assert!(
            last > 24,
            "placeholder must span BOTH cells of a wide glyph: {last}"
        );
        // It is a hollow box: the middle of the cell is paper.
        assert!(!ink(&b, 16, 16), "placeholder is hollow");
    }

    /// The artifact must be reproducible: same capture, same bytes, on
    /// any machine. (Integer math only, no map iteration, no floats.)
    #[test]
    fn output_is_deterministic_and_a_real_png() {
        let s = shot("determinism ┼ ⣿ █", 20, 2);
        let a = s.to_png();
        let b = s.to_png();
        assert_eq!(a, b, "same capture must produce identical bytes");
        assert_eq!(&a[1..4], b"PNG", "PNG signature");
        // The decoder round-trips its own writer.
        let img = crate::gfx::decode_image(&a).expect("our own PNG must decode");
        assert_eq!(img.width(), 20 * GLYPH_W);
        assert_eq!(img.height(), 2 * GLYPH_H);
    }

    /// Scale is an integer multiplier, clamped, and the geometry holds
    /// at every step.
    #[test]
    fn scale_multiplies_without_breaking_tiling() {
        for scale in [1u32, 2, 3, 8] {
            let b = shot("││", 2, 1).to_bitmap(PngOpts {
                scale,
                ..Default::default()
            });
            assert_eq!(b.width(), 2 * GLYPH_W * scale);
            assert_eq!(b.height(), GLYPH_H * scale);
        }
        // Out-of-range scales clamp instead of panicking or exploding.
        let huge = shot("x", 1, 1).to_bitmap(PngOpts {
            scale: 99,
            ..Default::default()
        });
        assert_eq!(huge.width(), GLYPH_W * 8);
        let zero = shot("x", 1, 1).to_bitmap(PngOpts {
            scale: 0,
            ..Default::default()
        });
        assert_eq!(zero.width(), GLYPH_W);
    }

    /// Attributes the terminal shows, the picture must show.
    #[test]
    fn attributes_change_the_pixels() {
        let plain = bitmap("x", 1, 1);
        let count = |b: &Bitmap| {
            (0..b.height())
                .flat_map(|y| (0..b.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| ink(b, x, y))
                .count()
        };
        let styled = |style: Style| {
            let mut s = Surface::new(Size::new(1, 1), Cell::EMPTY);
            s.draw_text(0, 0, "x", style);
            Screenshot::from_surface(&s).to_bitmap(PngOpts {
                scale: 2,
                ..Default::default()
            })
        };
        let white = Style::new()
            .fg(Rgba::rgb(255, 255, 255))
            .bg(Rgba::rgb(0, 0, 0));
        assert!(
            count(&styled(white.bold())) > count(&plain),
            "bold smears one pixel wider"
        );
        assert!(
            count(&styled(white.underline())) > count(&plain),
            "underline adds a rule"
        );
        assert!(
            count(&styled(white.strike())) > count(&plain),
            "strike adds a rule"
        );
        // Reverse swaps ink and paper: the cell becomes mostly ink.
        let rev = styled(white.reverse());
        assert!(count(&rev) > (rev.width() * rev.height()) as usize / 2);
    }

    /// Cells under a pixel-protocol placement are NOT the picture: the
    /// artifact says so rather than showing whatever text was beneath.
    #[test]
    fn protocol_regions_are_veiled_not_faked() {
        let mut s = shot("secret", 6, 1);
        let clean = s.to_bitmap(PngOpts {
            scale: 2,
            ..Default::default()
        });
        s.add_pixel_region(crate::base::Rect::new(0, 0, 6, 1));
        let veiled = s.to_bitmap(PngOpts {
            scale: 2,
            ..Default::default()
        });
        assert_ne!(clean.pixels(), veiled.pixels(), "the veil must be visible");
    }
}
