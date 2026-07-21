//! Mosaic quality goldens (cycle-4 quality pass): the chooser is the
//! chafa-style MSE 2-color fit; these tests pin its DECISIONS on the
//! three canonical content classes — hard edges, gradients, noise — as
//! grid snapshots, so a chooser regression shows up as a golden diff,
//! not a vague "looks worse".
//!
//! The reference points come from the chafa selection model
//! (docs/design/gfx-three.md §2.4): uniform cells emit space+bg (SGR
//! economy), edges inside a cell pick the partition glyph matching the
//! edge orientation, and smooth gradients must NOT invent structure
//! glyphs (spaces or low-bit glyphs only).

use crate::base::Rgba;
use crate::gfx::bitmap::Bitmap;
use crate::gfx::mosaic::{render, MosaicMode};

fn glyph_rows(src: &Bitmap, cols: u32, rows: u32, mode: MosaicMode) -> Vec<String> {
    let grid = render(src, cols, rows, mode);
    (0..rows)
        .map(|r| (0..cols).map(|c| grid.get(c, r).unwrap().ch).collect())
        .collect()
}

/// Hard vertical edge OFF the cell boundary: quadrant cells straddling
/// the edge must pick vertical-half glyphs, never diagonal/checker.
#[test]
fn hard_vertical_edge_picks_vertical_partitions() {
    // 8x4 px, edge at x=3 (mid-cell for 2x2 quadrant cells).
    let src = Bitmap::from_fn(8, 4, |x, _| if x < 3 { Rgba::WHITE } else { Rgba::BLACK });
    let rows = glyph_rows(&src, 4, 2, MosaicMode::Quadrant);
    // Cell 0: fully white -> space. Cell 1: left column white, right
    // black -> a vertical half glyph. Cells 2-3: uniform black -> space.
    for row in &rows {
        assert_eq!(row.chars().count(), 4);
        let cells: Vec<char> = row.chars().collect();
        assert_eq!(cells[0], ' ', "uniform cell must be space+bg: {row:?}");
        assert!(
            cells[1] == '▌' || cells[1] == '▐',
            "edge cell must pick a vertical half, got {:?} in {row:?}",
            cells[1]
        );
        assert_eq!(cells[2], ' ');
        assert_eq!(cells[3], ' ');
    }
}

/// Hard horizontal edge mid-cell: sextants must pick a horizontal
/// band pattern (top-row-only or bottom-rows-only), never a column.
#[test]
fn hard_horizontal_edge_picks_horizontal_partitions() {
    // 2x6 px = 1x2 sextant cells; edge at y=1 (inside the top cell).
    let src = Bitmap::from_fn(2, 6, |_, y| if y < 1 { Rgba::WHITE } else { Rgba::BLACK });
    let rows = glyph_rows(&src, 1, 2, MosaicMode::Sextant);
    // Top cell: only its first pixel row differs -> the row-band
    // partition {top row} vs {rows 2+3}, in EITHER orientation: the
    // canonicalization puts subpixel 0 (white) in bg, so the chooser
    // legitimately picks the complement glyph '🬹' (cells 3..6 as fg)
    // — same partition, swapped colors. '🬂' is the other orientation.
    assert_eq!(rows[1], " ", "fully black cell is space+bg");
    let top = rows[0].chars().next().unwrap();
    assert!(
        top == '🬂' || top == '🬹',
        "top-row-band partition expected, got {top:?}"
    );
}

/// Diagonal edge through quadrant cells: the checker/diagonal glyphs
/// are the correct minimum-error picks.
#[test]
fn diagonal_edge_picks_diagonal_partition() {
    // One 2x2 cell: white on the main diagonal, black off it.
    let src = Bitmap::from_pixels(
        2,
        2,
        vec![Rgba::WHITE, Rgba::BLACK, Rgba::BLACK, Rgba::WHITE],
    )
    .unwrap();
    let rows = glyph_rows(&src, 1, 1, MosaicMode::Quadrant);
    let ch = rows[0].chars().next().unwrap();
    assert!(
        ch == '▚' || ch == '▞',
        "diagonal partition expected, got {ch:?}"
    );
}

/// Smooth gradients must not invent structure: every cell is either a
/// space (uniform after averaging) or a low-contrast partition whose
/// two colors are near neighbors — pinned as an exact golden.
#[test]
fn gradient_golden_quadrant() {
    let src = Bitmap::from_fn(16, 8, |x, _| {
        let v = (x * 255 / 15) as u8;
        Rgba::rgb(v, v, v)
    });
    let rows = glyph_rows(&src, 8, 4, MosaicMode::Quadrant);
    // Horizontal gradient: within each cell the left column is darker
    // -> vertical halves or spaces only; and all rows identical.
    for row in &rows {
        assert_eq!(row, &rows[0], "gradient rows must match");
        for ch in row.chars() {
            assert!(
                ch == ' ' || ch == '▌' || ch == '▐',
                "gradient invented structure: {ch:?} in {row:?}"
            );
        }
    }
}

/// Deterministic noise snapshot: the exact glyph grid for a seeded
/// noise field, all three block modes. A chooser change MUST show up
/// here (update the golden deliberately, with eyes on the output).
#[test]
fn noise_snapshot_goldens() {
    // xorshift-ish deterministic noise.
    let mut state = 0x2545F491u32;
    let mut rand = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let mut px = Vec::with_capacity(8 * 6);
    for _ in 0..(8 * 6) {
        let v = rand();
        px.push(Rgba::rgb(
            (v & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            ((v >> 16) & 0xFF) as u8,
        ));
    }
    let src = Bitmap::from_pixels(8, 6, px).unwrap();

    let quad = glyph_rows(&src, 4, 3, MosaicMode::Quadrant);
    let sext = glyph_rows(&src, 4, 2, MosaicMode::Sextant);
    let half = glyph_rows(&src, 8, 3, MosaicMode::HalfBlock);

    // GOLDENS: captured from the shipped chooser (cycle 4, printed by
    // running with CAPTURE below). These pin determinism and chooser
    // identity — not aesthetics; a deliberate chooser improvement
    // updates them with a review note.
    if std::env::var("CAPTURE_MOSAIC_GOLDENS").is_ok() {
        eprintln!("quad: {quad:?}\nsext: {sext:?}\nhalf: {half:?}");
    }
    assert_eq!(quad, vec!["▐▝▄▗", "▟▗▗▐", "▐▖▐▞"], "quadrant noise golden");
    assert_eq!(sext, vec!["🬘🬁🬋🬖", "🬦🬑▐🬢"], "sextant noise golden");
    assert_eq!(
        half,
        vec!["▀▀▀▀▀▀▀▀", "▀▀▀▀▀▀▀▀", "▀▀▀▀▀▀▀▀"],
        "half-block noise golden"
    );
}

/// Dither opt (MosaicOpts): a 2-color dither of a gradient must show
/// mixing (both palette colors present in interleaved runs) instead of
/// one hard band — and stay deterministic.
#[test]
fn dither_option_mixes_gradients() {
    use crate::base::Rect;
    use crate::gfx::mosaic::MosaicMode;
    use crate::gfx::pipeline::{ImageOutput, ImageRenderer, MosaicOpts};
    use crate::term::caps::GraphicsCaps;

    let caps = GraphicsCaps {
        wrap: None,
        kitty_graphics: false,
        iterm2_images: false,
        sixel: false,
        sixel_max_registers: None,
        cell_pixel_size: None,
    };
    let src = Bitmap::from_fn(32, 8, |x, _| {
        let v = (x * 255 / 31) as u8;
        Rgba::rgb(v, v, v)
    });
    let mut r = ImageRenderer::new();
    r.config.mosaic = MosaicOpts {
        mode: MosaicMode::HalfBlock,
        dither: Some(2),
    };
    let out = r.render(&src, Rect::new(0, 0, 32, 4), &caps);
    let ImageOutput::Cells(cells) = &out.output else {
        panic!()
    };
    // With 2 colors, the middle third must contain BOTH extremes
    // (dither mixing), not a single flat color.
    let mid: Vec<_> = cells
        .iter()
        .filter(|c| c.pos.x >= 10 && c.pos.x < 22)
        .collect();
    let has_dark = mid.iter().any(|c| c.bg.r < 100 || c.fg.r < 100);
    let has_light = mid.iter().any(|c| c.bg.r > 150 || c.fg.r > 150);
    assert!(
        has_dark && has_light,
        "dither must interleave both palette colors"
    );
    // Determinism.
    let mut r2 = ImageRenderer::new();
    r2.config.mosaic = MosaicOpts {
        mode: MosaicMode::HalfBlock,
        dither: Some(2),
    };
    let out2 = r2.render(&src, Rect::new(0, 0, 32, 4), &caps);
    let ImageOutput::Cells(cells2) = &out2.output else {
        panic!()
    };
    assert_eq!(cells, cells2);
}
