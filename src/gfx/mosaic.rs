//! Unicode mosaic rendering: approximate an RGBA bitmap with one glyph +
//! two colors per terminal cell. The universal fallback of the gfx
//! ladder (kitty > iterm2 > sixel > mosaic) and the cheap channel for
//! animated content.
//!
//! This file owns the grid orchestration (scaling, cell walking, buffer
//! reuse); the per-cell glyph/color selection math lives in
//! `mosaic_fit` (see that module for the 2-color fit derivation).
//!
//! Pure module: no terminal I/O, no dependency on `render::Cell`. The
//! output is our own `MosaicCell`/`CellPatch`; the integrator bridges it
//! into RENDER's surface types (decoupling requested by both sides so
//! either can refactor without silently breaking the other).

use crate::base::{Point, Rgba};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::mosaic_fit::{
    fit_braille, fit_half_block, fit_two_color, QUADRANT_CHARS, SEXTANT_CHARS,
};

/// How many source pixels a cell represents, and which glyph family.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MosaicMode {
    /// 1x2 px/cell using `▀`: top pixel -> fg, bottom -> bg. Exact.
    HalfBlock,
    /// 2x2 px/cell, 16-glyph quadrant set, 2-color best fit.
    Quadrant,
    /// 2x3 px/cell, 64-pattern sextant set (U+1FB00 legacy computing).
    Sextant,
    /// 2x4 px/cell, braille dots by luminance threshold (monochrome-ish).
    Braille,
}

impl MosaicMode {
    /// Subpixels per cell as (width, height).
    /// Pick the best mosaic mode for a probed terminal, with the
    /// REASON for the choice (label it in diagnostics — the pick is a
    /// heuristic, not a measurement):
    ///
    /// - locale not UTF-8 → `HalfBlock`: U+2580 is the only mosaic
    ///   glyph that survives most legacy codepages (CP437 included).
    ///   Still not ASCII — there is no lower rung, so the label says
    ///   so (`#FALLBACK`).
    /// - UTF-8 but monochrome-class (neither truecolor nor 256-color)
    ///   → `Braille`: the luminance fit reads best when color cannot
    ///   carry the image.
    /// - UTF-8 + color → `Quadrant`: 2-color fit with UNIVERSAL glyph
    ///   coverage (Unicode 3.2 block elements). `Sextant` is denser
    ///   but its U+1FB00 glyphs (Unicode 13, 2020) still miss from
    ///   enough terminal fonts that tofu is a real risk — no font
    ///   probe exists, so the denser mode stays an explicit opt-in.
    pub fn auto(caps: &crate::term::Capabilities) -> (MosaicMode, &'static str) {
        if !caps.unicode_ok {
            return (
                MosaicMode::HalfBlock,
                "#FALLBACK locale not UTF-8: half-block only (U+2580 survives legacy codepages)",
            );
        }
        if !caps.truecolor && !caps.colors_256 {
            return (
                MosaicMode::Braille,
                "monochrome-class terminal: braille luminance fit (color cannot carry the image)",
            );
        }
        (
            MosaicMode::Quadrant,
            "color terminal: quadrant 2-color fit (universal glyphs; sextant is opt-in — U+1FB00 needs a recent font)",
        )
    }

    pub const fn cell_pixels(self) -> (u32, u32) {
        match self {
            MosaicMode::HalfBlock => (1, 2),
            MosaicMode::Quadrant => (2, 2),
            MosaicMode::Sextant => (2, 3),
            MosaicMode::Braille => (2, 4),
        }
    }
}

/// One rendered cell. `fg`/`bg` are straight-alpha RGBA; a fully
/// transparent side means "nothing of the image there" and the
/// compositor may show whatever is beneath.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MosaicCell {
    pub ch: char,
    pub fg: Rgba,
    pub bg: Rgba,
}

impl MosaicCell {
    pub const EMPTY: MosaicCell = MosaicCell {
        ch: ' ',
        fg: Rgba::TRANSPARENT,
        bg: Rgba::TRANSPARENT,
    };
}

/// Row-major grid of rendered cells.
#[derive(Clone, Debug, Default)]
pub struct MosaicGrid {
    cols: u32,
    rows: u32,
    cells: Vec<MosaicCell>,
}

impl MosaicGrid {
    pub fn cols(&self) -> u32 {
        self.cols
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    pub fn cells(&self) -> &[MosaicCell] {
        &self.cells
    }

    pub fn get(&self, col: u32, row: u32) -> Option<&MosaicCell> {
        if col < self.cols && row < self.rows {
            self.cells.get((row * self.cols + col) as usize)
        } else {
            None
        }
    }

    /// Iterate cells as `(screen point, char, fg, bg)` with the grid
    /// placed at `origin` — exactly the shape RENDER's
    /// `Surface::blit_mosaic` consumes (integrator contract, cycle 2).
    /// Every cell is yielded, including fully transparent ones: "image
    /// empty here" is information (stale content may need clearing).
    pub fn cell_patches(
        &self,
        origin: Point,
    ) -> impl Iterator<Item = (Point, char, Rgba, Rgba)> + '_ {
        let cols = self.cols;
        self.cells.iter().enumerate().map(move |(i, c)| {
            let col = (i as u32) % cols.max(1);
            let row = (i as u32) / cols.max(1);
            (
                Point::new(origin.x + col as i32, origin.y + row as i32),
                c.ch,
                c.fg,
                c.bg,
            )
        })
    }
}

/// Cell-space patch: what the integrator writes into RENDER's surface.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CellPatch {
    pub pos: Point,
    pub ch: char,
    pub fg: Rgba,
    pub bg: Rgba,
}

/// Flatten a grid into patches at `origin` (allocating convenience).
/// One-call "picture to cells": pick the best mosaic mode for the
/// probed terminal ([`MosaicMode::auto`]), fit the bitmap to the cell
/// rect, and return ready-to-blit patches. The convenience over
/// building a [`MosaicRenderer`] when you draw a picture once; keep a
/// renderer (or an `ImageSession`) for animation/reuse — this
/// allocates its scratch per call.
///
/// ```
/// use abstracttui::base::{Rect, Rgba};
/// use abstracttui::gfx::{render_to_cells, Bitmap};
/// use abstracttui::term::Capabilities;
///
/// let img = Bitmap::new(16, 8, Rgba::rgb(180, 90, 30));
/// let cells = render_to_cells(&img, Rect::new(2, 1, 8, 4), &Capabilities::default());
/// assert_eq!(cells.len(), 8 * 4);
/// assert_eq!(cells[0].pos.x, 2);
/// ```
pub fn render_to_cells(
    img: &Bitmap,
    rect: crate::base::Rect,
    caps: &crate::term::Capabilities,
) -> Vec<CellPatch> {
    if rect.w <= 0 || rect.h <= 0 {
        return Vec::new();
    }
    let (mode, _reason) = MosaicMode::auto(caps);
    let mut renderer = MosaicRenderer::new();
    let grid = renderer.render(img, rect.w as u32, rect.h as u32, mode);
    blit_to_cells(grid, rect.origin())
}

pub fn blit_to_cells(grid: &MosaicGrid, origin: Point) -> Vec<CellPatch> {
    let mut out = Vec::with_capacity(grid.cells.len());
    blit_into(grid, origin, &mut out);
    out
}

/// Flatten into a caller-owned Vec (cleared first) so frame loops reuse
/// the allocation. Every cell is emitted, including fully transparent
/// ones — "image has nothing here" is information the compositor needs
/// (it may have stale content to clear).
pub fn blit_into(grid: &MosaicGrid, origin: Point, out: &mut Vec<CellPatch>) {
    out.clear();
    out.reserve(grid.cells.len());
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let c = grid.cells[(row * grid.cols + col) as usize];
            out.push(CellPatch {
                pos: Point::new(origin.x + col as i32, origin.y + row as i32),
                ch: c.ch,
                fg: c.fg,
                bg: c.bg,
            });
        }
    }
}

/// One-shot render (allocates a renderer; fine outside frame loops).
pub fn render(src: &Bitmap, cols: u32, rows: u32, mode: MosaicMode) -> MosaicGrid {
    let mut r = MosaicRenderer::new();
    r.render(src, cols, rows, mode);
    r.take_grid()
}

/// Upper bound on each grid dimension. Real terminals top out around a
/// few hundred cells per axis; the bound exists so a corrupted size
/// upstream degrades to a clipped render instead of a multi-gigabyte
/// allocation (the scratch bitmap scales with cols*rows*subpixels).
pub const MAX_GRID_DIM: u32 = 4096;

/// Reusable renderer: owns the scaled scratch bitmap and the output
/// grid so steady-state animation does zero heap allocation (buffers
/// only reallocate when the target size changes).
///
/// ```
/// use abstracttui::base::Rgba;
/// use abstracttui::gfx::{Bitmap, MosaicMode, MosaicRenderer};
/// use abstracttui::term::Capabilities;
///
/// // Pick the best mode for the terminal (labeled heuristic):
/// let (mode, reason) = MosaicMode::auto(&Capabilities::default());
/// assert!(!reason.is_empty());
///
/// let img = Bitmap::new(8, 8, Rgba::rgb(200, 120, 40));
/// let mut renderer = MosaicRenderer::new();
/// // 4x2 CELLS; the renderer scales the bitmap to the mode's
/// // subpixel grid (half-block: 1x2 px per cell).
/// let grid = renderer.render(&img, 4, 2, MosaicMode::HalfBlock);
/// assert_eq!(grid.cells().len(), 4 * 2);
/// ```
#[derive(Default)]
pub struct MosaicRenderer {
    scratch: Bitmap,
    grid: MosaicGrid,
}

impl MosaicRenderer {
    pub fn new() -> MosaicRenderer {
        MosaicRenderer::default()
    }

    /// Render `src` into a cols x rows cell grid. The source is scaled
    /// (bilinear) to cols*subw x rows*subh unless it already has exactly
    /// that size — the 3D viewport renders cell-exact frames and skips
    /// the resample entirely.
    pub fn render(&mut self, src: &Bitmap, cols: u32, rows: u32, mode: MosaicMode) -> &MosaicGrid {
        let cols = cols.min(MAX_GRID_DIM);
        let rows = rows.min(MAX_GRID_DIM);
        let (subw, subh) = mode.cell_pixels();
        let (pw, ph) = (cols * subw, rows * subh);

        self.grid.cols = cols;
        self.grid.rows = rows;
        self.grid.cells.clear();
        self.grid
            .cells
            .resize((cols as usize) * (rows as usize), MosaicCell::EMPTY);
        if cols == 0 || rows == 0 {
            return &self.grid;
        }

        let source: &Bitmap = if src.width() == pw && src.height() == ph {
            src
        } else {
            if self.scratch.width() != pw || self.scratch.height() != ph {
                self.scratch = Bitmap::new(pw, ph, Rgba::TRANSPARENT);
            }
            if src.is_empty() {
                self.scratch.fill(Rgba::TRANSPARENT);
            } else {
                src.resize_bilinear_into(&mut self.scratch);
            }
            &self.scratch
        };

        // Per-cell scratch lives on the stack: max 8 subpixels.
        let n = (subw * subh) as usize;
        let mut sub = [Rgba::TRANSPARENT; 8];
        for row in 0..rows {
            for col in 0..cols {
                for (i, s) in sub.iter_mut().enumerate().take(n) {
                    let sx = col * subw + (i as u32) % subw;
                    let sy = row * subh + (i as u32) / subw;
                    // In-bounds by construction (source is pw x ph).
                    *s = source.get(sx, sy).unwrap_or(Rgba::TRANSPARENT);
                }
                let cell = match mode {
                    MosaicMode::HalfBlock => fit_half_block(sub[0], sub[1]),
                    MosaicMode::Quadrant => fit_two_color(&sub[..4], &QUADRANT_CHARS),
                    MosaicMode::Sextant => fit_two_color(&sub[..6], &SEXTANT_CHARS),
                    MosaicMode::Braille => fit_braille(&sub[..8]),
                };
                self.grid.cells[(row * cols + col) as usize] = cell;
            }
        }
        &self.grid
    }

    pub fn grid(&self) -> &MosaicGrid {
        &self.grid
    }

    /// Move the grid out (one-shot use); the renderer stays reusable.
    pub fn take_grid(&mut self) -> MosaicGrid {
        std::mem::take(&mut self.grid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mosaic fit cost at a large viewport (200x60 cells), per mode —
    /// the cycle-7 budget numbers for docs/design/gfx-three.md.
    /// `cargo test --release -- --ignored perf_mosaic_200x60 --nocapture`
    #[test]
    #[ignore = "perf report; run explicitly in release"]
    fn perf_mosaic_200x60() {
        // Photo-ish noise (xorshift): the WORST case for the 2-color
        // fit — no uniform-cell early outs, every pattern scored.
        let mut state = 0x9E3779B97F4A7C15u64;
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let (cols, rows) = (200u32, 60u32);
        for mode in [
            MosaicMode::HalfBlock,
            MosaicMode::Quadrant,
            MosaicMode::Sextant,
            MosaicMode::Braille,
        ] {
            let (sw, sh) = mode.cell_pixels();
            let (w, h) = (cols * sw, rows * sh);
            let mut px = Vec::with_capacity((w * h) as usize);
            for _ in 0..w * h {
                let v = rng();
                px.push(crate::base::Rgba::rgb(
                    v as u8,
                    (v >> 8) as u8,
                    (v >> 16) as u8,
                ));
            }
            let bmp = Bitmap::from_pixels(w, h, px).unwrap();
            let mut r = MosaicRenderer::new();
            let name = format!("mosaic_{mode:?}_200x60");
            let m = crate::testing::bench::time_median(&name, 3, 5, 20, |_| {
                let grid = r.render(&bmp, cols, rows, mode);
                crate::testing::bench::sink(grid.cells().len());
            });
            eprintln!("{}", m.report());
        }
    }

    #[test]
    fn mosaic_auto_picks_by_capability_with_labeled_reason() {
        use crate::term::Capabilities;
        let mut caps = Capabilities {
            unicode_ok: false,
            truecolor: true,
            ..Capabilities::default()
        };
        let (m, why) = MosaicMode::auto(&caps);
        assert_eq!(m, MosaicMode::HalfBlock);
        assert!(why.contains("#FALLBACK"), "{why}");

        caps.unicode_ok = true;
        caps.truecolor = false;
        caps.colors_256 = false;
        let (m, why) = MosaicMode::auto(&caps);
        assert_eq!(m, MosaicMode::Braille);
        assert!(why.contains("monochrome"), "{why}");

        caps.colors_256 = true;
        let (m, why) = MosaicMode::auto(&caps);
        assert_eq!(m, MosaicMode::Quadrant);
        assert!(why.to_lowercase().contains("quadrant"), "{why}");

        caps.truecolor = true;
        assert_eq!(MosaicMode::auto(&caps).0, MosaicMode::Quadrant);
    }

    const RED: Rgba = Rgba::rgb(255, 0, 0);
    const GREEN: Rgba = Rgba::rgb(0, 255, 0);
    const BLUE: Rgba = Rgba::rgb(0, 0, 255);

    #[test]
    fn half_block_colors_exact() {
        let src = Bitmap::from_fn(1, 2, |_, y| if y == 0 { RED } else { BLUE });
        let g = render(&src, 1, 1, MosaicMode::HalfBlock);
        let c = g.get(0, 0).unwrap();
        assert_eq!((c.ch, c.fg, c.bg), ('\u{2580}', RED, BLUE));
    }

    #[test]
    fn half_block_uniform_is_space() {
        let src = Bitmap::new(1, 2, GREEN);
        let c = *render(&src, 1, 1, MosaicMode::HalfBlock).get(0, 0).unwrap();
        assert_eq!(c.ch, ' ');
        assert_eq!(c.bg, GREEN);
    }

    #[test]
    fn quadrant_clean_partitions() {
        // Left column red, right column blue: partition {UL,LL} vs
        // {UR,LR}. Canonical winner has UL in bg -> pattern UR|LR =
        // 0b1010 = right half block, fg = blue, bg = red.
        let src = Bitmap::from_fn(2, 2, |x, _| if x == 0 { RED } else { BLUE });
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2590}');
        assert_eq!((c.fg, c.bg), (BLUE, RED));

        // Top row white, bottom black -> lower-half glyph (UL in bg).
        let src = Bitmap::from_fn(2, 2, |_, y| if y == 0 { Rgba::WHITE } else { Rgba::BLACK });
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2584}');
        assert_eq!((c.fg, c.bg), (Rgba::BLACK, Rgba::WHITE));

        // Single distinct corner: LR green on red -> '▗' fg=green.
        let src = Bitmap::from_fn(2, 2, |x, y| if (x, y) == (1, 1) { GREEN } else { RED });
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2597}');
        assert_eq!((c.fg, c.bg), (GREEN, RED));
    }

    #[test]
    fn quadrant_uniform_cell_is_space_plus_bg() {
        let src = Bitmap::new(2, 2, BLUE);
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        assert_eq!((c.ch, c.bg), (' ', BLUE));
    }

    #[test]
    fn quadrant_two_color_fit_noisy() {
        // Noisy but separable: dark-ish top pair vs bright bottom pair.
        // The fit must pick the horizontal split and average each side.
        let top0 = Rgba::rgb(10, 12, 8);
        let top1 = Rgba::rgb(14, 10, 12);
        let bot0 = Rgba::rgb(240, 250, 246);
        let bot1 = Rgba::rgb(250, 244, 240);
        let src = Bitmap::from_pixels(2, 2, vec![top0, top1, bot0, bot1]).unwrap();
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2584}', "expected lower-half split, got {:?}", c);
        assert_eq!(c.fg, Rgba::rgb(245, 247, 243)); // mean of bottoms
        assert_eq!(c.bg, Rgba::rgb(12, 11, 10)); // mean of tops
    }

    #[test]
    fn quadrant_transparent_subpixels_dont_vote() {
        // UL opaque red, rest fully transparent green: green must not
        // influence colors; the visible pixel lands in one side and the
        // empty side reports transparent.
        let t = Rgba::new(0, 255, 0, 0);
        let src = Bitmap::from_pixels(2, 2, vec![RED, t, t, t]).unwrap();
        let c = *render(&src, 1, 1, MosaicMode::Quadrant).get(0, 0).unwrap();
        // All partitions where red is alone on its side score 0; ties
        // resolve to the lowest canonical pattern = 0 (space + bg=red
        // means, since bg holds subpixel 0).
        assert_eq!(c.ch, ' ');
        assert_eq!((c.bg.r, c.bg.g, c.bg.b), (255, 0, 0));
        assert_eq!(c.fg, Rgba::TRANSPARENT);
        // Cell-mean alpha of the bg side: 4 subpixels, one opaque.
        assert_eq!(c.bg.a, 64);
    }

    #[test]
    fn sextant_patterns() {
        // Left column red, right blue over 2x3 -> right-half glyph '▐'
        // (pattern bits 1,3,5 = 0b101010 = 42), fg blue bg red.
        let src = Bitmap::from_fn(2, 3, |x, _| if x == 0 { RED } else { BLUE });
        let c = *render(&src, 1, 1, MosaicMode::Sextant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2590}');
        assert_eq!((c.fg, c.bg), (BLUE, RED));

        // Only the UR subpixel differs -> BLOCK SEXTANT-2 (bit 1, index 2).
        let src = Bitmap::from_fn(2, 3, |x, y| if (x, y) == (1, 0) { GREEN } else { RED });
        let c = *render(&src, 1, 1, MosaicMode::Sextant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{1FB01}');
        assert_eq!((c.fg, c.bg), (GREEN, RED));

        // Bottom row only -> cells 5,6 = bits 4,5 = index 48 = '🬭'.
        let src = Bitmap::from_fn(2, 3, |_, y| if y == 2 { Rgba::WHITE } else { Rgba::BLACK });
        let c = *render(&src, 1, 1, MosaicMode::Sextant).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{1FB2D}');
        assert_eq!((c.fg, c.bg), (Rgba::WHITE, Rgba::BLACK));
    }

    #[test]
    fn braille_dot_bits() {
        // Light only the top-left pixel: brighter than the mean of the
        // rest -> dot 1 -> U+2801. fg = white, bg = mean of the black.
        let src = Bitmap::from_fn(2, 4, |x, y| {
            if (x, y) == (0, 0) {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        });
        let c = *render(&src, 1, 1, MosaicMode::Braille).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2801}');
        assert_eq!(c.fg, Rgba::WHITE);
        assert_eq!(c.bg, Rgba::BLACK);

        // Bottom-right pixel -> dot 8 -> U+2880.
        let src = Bitmap::from_fn(2, 4, |x, y| {
            if (x, y) == (1, 3) {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        });
        let c = *render(&src, 1, 1, MosaicMode::Braille).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2880}');

        // Left column bright -> dots 1,2,3,7 -> 0x01|0x02|0x04|0x40.
        let src = Bitmap::from_fn(2, 4, |x, _| if x == 0 { Rgba::WHITE } else { Rgba::BLACK });
        let c = *render(&src, 1, 1, MosaicMode::Braille).get(0, 0).unwrap();
        assert_eq!(c.ch, char::from_u32(0x2800 + 0x47).unwrap());
    }

    #[test]
    fn braille_uniform_cell_blank() {
        let src = Bitmap::new(2, 4, GREEN);
        let c = *render(&src, 1, 1, MosaicMode::Braille).get(0, 0).unwrap();
        assert_eq!(c.ch, '\u{2800}', "uniform cell lights no dots");
        assert_eq!(c.bg, GREEN);
        assert_eq!(c.fg, Rgba::TRANSPARENT);
    }

    #[test]
    fn fully_transparent_cell_is_empty() {
        let src = Bitmap::new(2, 3, Rgba::TRANSPARENT);
        for mode in [
            MosaicMode::HalfBlock,
            MosaicMode::Quadrant,
            MosaicMode::Sextant,
            MosaicMode::Braille,
        ] {
            let c = *render(&src, 1, 1, mode).get(0, 0).unwrap();
            if mode == MosaicMode::HalfBlock {
                // HalfBlock canonicalizes uniform (incl. transparent).
                assert_eq!(c.ch, ' ');
                assert_eq!(c.bg, Rgba::TRANSPARENT);
            } else {
                assert_eq!(c, MosaicCell::EMPTY, "mode {mode:?}");
            }
        }
    }

    #[test]
    fn grid_geometry_and_resize_path() {
        // 4x4 source into 2x1 quadrant cells: forces a bilinear resample
        // to 4x2 then two cells. Just shape + determinism here.
        let src = Bitmap::from_fn(4, 4, |x, y| {
            Rgba::new((x * 60) as u8, (y * 60) as u8, 0, 255)
        });
        let mut r = MosaicRenderer::new();
        let a = r.render(&src, 2, 1, MosaicMode::Quadrant).clone();
        assert_eq!((a.cols(), a.rows()), (2, 1));
        assert_eq!(a.cells().len(), 2);
        let b = r.render(&src, 2, 1, MosaicMode::Quadrant).clone();
        assert_eq!(a.cells(), b.cells(), "renderer reuse must be deterministic");
    }

    #[test]
    fn renderer_survives_mode_and_size_changes() {
        // The reused scratch bitmap must resize correctly when the mode
        // (subpixel geometry) or the target size changes between calls.
        let src = Bitmap::from_fn(8, 8, |x, y| {
            Rgba::new((x * 30) as u8, (y * 30) as u8, 0, 255)
        });
        let mut r = MosaicRenderer::new();
        r.render(&src, 4, 4, MosaicMode::HalfBlock);
        assert_eq!(r.grid().cells().len(), 16);
        r.render(&src, 2, 2, MosaicMode::Braille);
        assert_eq!(r.grid().cells().len(), 4);
        r.render(&src, 3, 1, MosaicMode::Sextant);
        assert_eq!(r.grid().cells().len(), 3);
    }

    #[test]
    fn blit_positions() {
        let src = Bitmap::from_fn(2, 2, |x, _| if x == 0 { RED } else { BLUE });
        let g = render(&src, 2, 1, MosaicMode::HalfBlock);
        let patches = blit_to_cells(&g, Point::new(10, 5));
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].pos, Point::new(10, 5));
        assert_eq!(patches[1].pos, Point::new(11, 5));
        let mut reuse = Vec::new();
        blit_into(&g, Point::ZERO, &mut reuse);
        assert_eq!(reuse.len(), 2);
    }

    #[test]
    fn oversized_grid_is_clamped() {
        // A corrupted upstream size degrades to a clipped render, not
        // a multi-gigabyte allocation.
        let src = Bitmap::new(2, 2, RED);
        let g = render(&src, MAX_GRID_DIM + 500, 1, MosaicMode::HalfBlock);
        assert_eq!(g.cols(), MAX_GRID_DIM);
        assert_eq!(g.rows(), 1);
    }

    #[test]
    fn zero_sized_targets_are_safe() {
        let src = Bitmap::new(4, 4, RED);
        let g = render(&src, 0, 3, MosaicMode::Quadrant);
        assert_eq!(g.cells().len(), 0);
        let g = render(&Bitmap::new(0, 0, RED), 2, 2, MosaicMode::Sextant);
        assert_eq!(g.cells().len(), 4, "empty source renders empty cells");
        assert!(g.cells().iter().all(|c| *c == MosaicCell::EMPTY));
    }
}
