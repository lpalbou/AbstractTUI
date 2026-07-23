//! Shared sub-cell glyph vocabularies: braille dot bits, quadrant
//! blocks and the two eighth-block ramps. ONE authority — the widget
//! layer and `gfx::mosaic_fit`'s image fitters consume these same
//! tables (backlog 0420: the dedup covers tables, not fitters; the
//! sextant table stays in `mosaic_fit` because no stroke mode uses it).
//!
//! OWNER: CANVAS (extensions wave).

/// Braille dot bit for (col in 0..2, row in 0..4) — Unicode braille
/// bit order (dots 1-8). Dots are numbered column-first (1-3,7 left;
/// 4-6,8 right) with the bottom row split out historically, hence the
/// irregular mapping. `0x2800 + OR of lit bits` is the codepoint.
pub const fn braille_bit(col: i32, row: i32) -> u8 {
    match (col, row) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, 3) => 0x40,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        _ => 0x80, // (1, 3)
    }
}

/// Quadrant glyphs indexed by fg pattern bits (bit i = subpixel i is
/// fg; subpixels row-major: 0=UL, 1=UR, 2=LL, 3=LR). Index 0 is a
/// space (empty cell), index 15 the full block.
pub const QUADRANT_CHARS: [char; 16] = [
    ' ', '\u{2598}', '\u{259D}', '\u{2580}', // -, UL, UR, upper half
    '\u{2596}', '\u{258C}', '\u{259E}', '\u{259B}', // LL, left half, anti-diag, no-LR
    '\u{2597}', '\u{259A}', '\u{2590}', '\u{259C}', // LR, diag, right half, no-LL
    '\u{2584}', '\u{2599}', '\u{259F}', '\u{2588}', // lower half, no-UR, no-UL, full
];

/// Vertical eighth blocks rising from the cell bottom, index =
/// eighths filled minus one (1..=8): `V_EIGHTHS[7]` is the full block.
pub const V_EIGHTHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Horizontal eighth blocks anchored at the cell's left edge, same
/// indexing: `H_EIGHTHS[7]` is the full block.
pub const H_EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
