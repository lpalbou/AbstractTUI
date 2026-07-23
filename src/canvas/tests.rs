//! Canvas layer tests: dot-order pins, Bresenham/bezier/arc goldens
//! (deterministic — same inputs, same dots), the cell-color rule,
//! clip composition and the eighth-block fill vocabulary. The chart
//! refactor proof lives in `widgets/chart_tests.rs` (byte-identical
//! goldens on the shipped widgets).

use super::*;
use crate::base::{Rect, Size};
use crate::ui::{BufferCanvas, ClippedCanvas};

fn dots(dc: &DotCanvas) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..dc.dots_h() {
        for x in 0..dc.dots_w() {
            if dc.get(x, y) {
                out.push((x, y));
            }
        }
    }
    out
}

fn lit_near(dc: &DotCanvas, x: i32, y: i32, radius: i32) -> bool {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dc.get(x + dx, y + dy) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Vocabulary pins
// ---------------------------------------------------------------------------

#[test]
fn braille_bits_cover_unicode_dot_order() {
    // Dots 1..8 in the standard layout (the four-value pin from the
    // chart tests, extended to the full table).
    assert_eq!(braille_bit(0, 0), 0x01);
    assert_eq!(braille_bit(0, 1), 0x02);
    assert_eq!(braille_bit(0, 2), 0x04);
    assert_eq!(braille_bit(0, 3), 0x40);
    assert_eq!(braille_bit(1, 0), 0x08);
    assert_eq!(braille_bit(1, 1), 0x10);
    assert_eq!(braille_bit(1, 2), 0x20);
    assert_eq!(braille_bit(1, 3), 0x80);
    // All 8 bits, each exactly once.
    let mut all = 0u32;
    for row in 0..4 {
        for col in 0..2 {
            let b = u32::from(braille_bit(col, row));
            assert_eq!(all & b, 0, "duplicate bit at ({col},{row})");
            all |= b;
        }
    }
    assert_eq!(all, 0xFF);
}

#[test]
fn quadrant_and_eighth_ramps_are_pinned() {
    assert_eq!(QUADRANT_CHARS[0], ' ');
    assert_eq!(QUADRANT_CHARS[0b0011], '\u{2580}'); // upper half = UL|UR
    assert_eq!(QUADRANT_CHARS[0b0101], '\u{258C}'); // left half = UL|LL
    assert_eq!(QUADRANT_CHARS[0b1111], '\u{2588}');
    let uniq: std::collections::HashSet<char> = QUADRANT_CHARS.iter().copied().collect();
    assert_eq!(uniq.len(), 16);

    assert_eq!(V_EIGHTHS, ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']);
    assert_eq!(H_EIGHTHS, ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█']);
}

// ---------------------------------------------------------------------------
// Dots, clipping, modes
// ---------------------------------------------------------------------------

#[test]
fn set_clear_get_clip_at_grid_edges() {
    let mut dc = DotCanvas::braille(2, 1); // 4x4 dots
    for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 4), (i32::MAX, i32::MIN)] {
        dc.set(x, y); // clipped, never panics
        dc.clear(x, y);
        assert!(!dc.get(x, y));
    }
    dc.set(3, 3);
    assert!(dc.get(3, 3));
    dc.clear(3, 3);
    assert!(!dc.get(3, 3));
    dc.set(0, 0);
    dc.set(3, 3);
    dc.clear_all();
    assert!(dots(&dc).is_empty(), "clear_all unlights everything");

    // Degenerate grids accept every call and draw nothing.
    let mut empty = DotCanvas::new(DotMode::Braille, -3, 0);
    empty.set(0, 0);
    empty.line((0, 0), (10, 10));
    assert_eq!(empty.cell_char(0, 0), None);
    let mut out = BufferCanvas::new(Size::new(4, 2));
    empty.blit(&mut out, Point::new(0, 0), Rgba::WHITE);
    assert_eq!(out.row_text(0), "    ");
}

#[test]
fn quadrant_mode_maps_bits_to_block_glyphs() {
    let mut dc = DotCanvas::quadrant(2, 1); // 4x2 dots
    dc.set(0, 0); // UL of cell 0
    assert_eq!(dc.cell_char(0, 0), Some('\u{2598}'));
    dc.set(1, 1); // + LR -> diagonal
    assert_eq!(dc.cell_char(0, 0), Some('\u{259A}'));
    // All four dots of cell 1 -> full block.
    for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
        dc.set(x, y);
    }
    assert_eq!(dc.cell_char(1, 0), Some('\u{2588}'));
    assert_eq!(dc.dots_h(), 2, "quadrant cells are 2 dots tall");
}

#[test]
fn line_clips_offgrid_and_polyline_chains() {
    let mut dc = DotCanvas::braille(3, 1); // 6x4 dots
    dc.line((-5, -5), (5, 5)); // enters the grid at (0,0)
    assert!(dc.get(0, 0));
    assert!(dc.get(3, 3));
    // A line entirely outside lights nothing new below the diagonal.
    let before = dots(&dc);
    dc.line((-9, 2), (-2, 2));
    assert_eq!(dots(&dc), before);

    let mut poly = DotCanvas::braille(3, 1);
    poly.polyline(&[]);
    assert!(dots(&poly).is_empty());
    poly.polyline(&[(2, 2)]);
    assert_eq!(dots(&poly), vec![(2, 2)], "single point lights one dot");
    poly.clear_all();
    poly.polyline(&[(0, 3), (2, 0), (5, 3)]);
    assert!(poly.get(0, 3) && poly.get(2, 0) && poly.get(5, 3));
}

/// Cross-pin against the shipped chart behavior: the sparkline ramp
/// golden ("⡠⠊", chart_tests.rs) drawn through the promoted API —
/// byte-for-byte the cells `BrailleGrid` produced before promotion.
#[test]
fn bresenham_matches_the_shipped_chart_ramp() {
    let mut dc = DotCanvas::braille(2, 1);
    dc.set(0, 3);
    dc.line((0, 3), (1, 2));
    dc.line((1, 2), (2, 1));
    dc.line((2, 1), (3, 0));
    let mut out = BufferCanvas::new(Size::new(2, 1));
    dc.blit(&mut out, Point::new(0, 0), Rgba::WHITE);
    assert_eq!(out.row_text(0), "⡠⠊");
}

// ---------------------------------------------------------------------------
// Curves: determinism + curve tracking (bounded flattening)
// ---------------------------------------------------------------------------

fn quad_at(p0: (f32, f32), c: (f32, f32), p1: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
        u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
    )
}

fn cubic_at(p0: (f32, f32), c0: (f32, f32), c1: (f32, f32), p1: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * c0.0 + 3.0 * u * t * t * c1.0 + t * t * t * p1.0,
        u * u * u * p0.1 + 3.0 * u * u * t * c0.1 + 3.0 * u * t * t * c1.1 + t * t * t * p1.1,
    )
}

#[test]
fn bezier_quad_is_deterministic_and_tracks_the_curve() {
    let (p0, c, p1) = ((1.0, 14.0), (16.0, -8.0), (30.0, 14.0));
    let mut a = DotCanvas::braille(16, 4); // 32x16 dots
    a.bezier_quad(p0, c, p1, 0.25);
    let mut b = DotCanvas::braille(16, 4);
    b.bezier_quad(p0, c, p1, 0.25);
    assert_eq!(dots(&a), dots(&b), "same inputs, same dots");
    assert!(!dots(&a).is_empty());

    // Endpoints land exactly (rounded to dots).
    assert!(a.get(1, 14) && a.get(30, 14));
    // Every finely-sampled curve point has a lit dot nearby, and every
    // lit dot sits near the curve (flattening tracked the geometry,
    // not an artifact of subdivision).
    for i in 0..=512 {
        let (x, y) = quad_at(p0, c, p1, i as f32 / 512.0);
        assert!(
            lit_near(&a, x.round() as i32, y.round() as i32, 2),
            "curve point ({x:.1},{y:.1}) has no lit dot within 2"
        );
    }
    for (x, y) in dots(&a) {
        let near = (0..=512).any(|i| {
            let (cx, cy) = quad_at(p0, c, p1, i as f32 / 512.0);
            (cx - x as f32).abs() <= 2.0 && (cy - y as f32).abs() <= 2.0
        });
        assert!(near, "lit dot ({x},{y}) is far from the curve");
    }
}

#[test]
fn bezier_cubic_is_deterministic_and_tracks_the_curve() {
    let (p0, c0, c1, p1) = ((0.0, 2.0), (12.0, 18.0), (20.0, -4.0), (31.0, 12.0));
    let mut a = DotCanvas::braille(16, 4);
    a.bezier_cubic(p0, c0, c1, p1, 0.25);
    let mut b = DotCanvas::braille(16, 4);
    b.bezier_cubic(p0, c0, c1, p1, 0.25);
    assert_eq!(dots(&a), dots(&b));
    assert!(a.get(0, 2) && a.get(31, 12), "endpoints lit");
    for i in 0..=512 {
        let (x, y) = cubic_at(p0, c0, c1, p1, i as f32 / 512.0);
        assert!(
            lit_near(&a, x.round() as i32, y.round() as i32, 2),
            "curve point ({x:.1},{y:.1}) has no lit dot within 2"
        );
    }
}

#[test]
fn bezier_flattening_is_bounded_and_rejects_non_finite() {
    let mut dc = DotCanvas::braille(4, 2);
    // Pathological control points + degenerate tolerance: terminates
    // (depth cap) and stays clipped to the grid.
    dc.bezier_cubic(
        (0.0, 0.0),
        (1.0e9, -1.0e9),
        (-1.0e9, 1.0e9),
        (7.0, 7.0),
        0.0,
    );
    let _ = dots(&dc); // reachable, bounded

    // Non-finite inputs draw nothing (the chart sample-skip contract).
    let mut clean = DotCanvas::braille(4, 2);
    clean.bezier_quad((f32::NAN, 0.0), (2.0, 2.0), (7.0, 7.0), 0.25);
    clean.bezier_cubic(
        (0.0, 0.0),
        (f32::INFINITY, 0.0),
        (2.0, 2.0),
        (7.0, 7.0),
        0.25,
    );
    clean.ellipse_arc((4.0, 4.0), f32::NAN, 2.0, 0.0, 1.0);
    assert!(dots(&clean).is_empty());
}

#[test]
fn ellipse_arc_is_deterministic_symmetric_and_on_radius() {
    let mut a = DotCanvas::braille(16, 8); // 32x32 dots
    a.ellipse_arc((16.0, 16.0), 10.0, 10.0, 0.0, std::f32::consts::TAU);
    let mut b = DotCanvas::braille(16, 8);
    b.ellipse_arc((16.0, 16.0), 10.0, 10.0, 0.0, std::f32::consts::TAU);
    assert_eq!(dots(&a), dots(&b), "same inputs, same dots");

    let lit = dots(&a);
    assert!(lit.len() > 40, "a r=10 circle lights a full ring");
    for &(x, y) in &lit {
        // On the circle within rasterization slack.
        let d2 = f64::from((x - 16).pow(2) + (y - 16).pow(2)) / 100.0;
        assert!(
            (0.72..=1.32).contains(&d2),
            "dot ({x},{y}) is off the circle: r²-ratio {d2:.2}"
        );
        // Mirror-symmetric within one dot (Bresenham chords are not
        // exactly reversal-symmetric, so exact set equality is not
        // the contract — proximity is).
        assert!(lit_near(&a, 32 - x, y, 1), "x-mirror of ({x},{y}) unlit");
        assert!(lit_near(&a, x, 32 - y, 1), "y-mirror of ({x},{y}) unlit");
    }

    // A quarter arc stays in its quadrant (screen-space angles:
    // 0 -> +x, sweep towards +y).
    let mut q = DotCanvas::braille(16, 8);
    q.ellipse_arc((16.0, 16.0), 10.0, 10.0, 0.0, std::f32::consts::FRAC_PI_2);
    for (x, y) in dots(&q) {
        assert!(
            x >= 16 && y >= 16,
            "quarter-arc dot ({x},{y}) left its quadrant"
        );
    }
}

// ---------------------------------------------------------------------------
// Blit: the cell-color rule, clip composition, styled strokes
// ---------------------------------------------------------------------------

#[test]
fn blit_color_rule_later_grids_win_overlapping_cells() {
    let red = Rgba::rgb(200, 40, 40);
    let blue = Rgba::rgb(40, 40, 200);
    // Grid A: horizontal line over cells 0..=3; grid B: vertical line
    // in cell 2. Overlap cell (2, 0); cell (4, 0) is empty in both.
    let mut a = DotCanvas::braille(5, 1);
    a.line((0, 1), (7, 1));
    let mut b = DotCanvas::braille(5, 1);
    b.line((5, 0), (5, 3));

    let mut out = BufferCanvas::new(Size::new(5, 1));
    a.blit(&mut out, Point::new(0, 0), red);
    b.blit(&mut out, Point::new(0, 0), blue);

    // A-only cells keep A's color; the overlap took B's glyph AND
    // color wholesale (one fg per cell — dots never merge).
    let (ch_a, fg_a, _) = out.cell(Point::new(0, 0)).unwrap();
    assert_eq!((ch_a, fg_a), (a.cell_char(0, 0).unwrap(), red));
    let (ch_mid, fg_mid, _) = out.cell(Point::new(2, 0)).unwrap();
    assert_eq!((ch_mid, fg_mid), (b.cell_char(2, 0).unwrap(), blue));
    assert_ne!(
        b.cell_char(2, 0),
        a.cell_char(2, 0),
        "precondition: the two grids disagree on the overlap cell"
    );
    // Cells empty in both grids stayed untouched (transparent
    // composition — blit skips them instead of painting spaces).
    assert_eq!(a.cell_char(4, 0), None);
    assert_eq!(b.cell_char(4, 0), None);
    assert_eq!(out.cell(Point::new(4, 0)).unwrap().0, ' ');
}

/// Far-off-grid segments are pre-clipped: the walk is O(grid), the
/// deltas cannot overflow, and the visible run still rasters.
#[test]
fn line_walk_is_bounded_for_far_segments() {
    let mut dc = DotCanvas::braille(4, 1); // 8x4 dots
    dc.line((-1_000_000, 2), (1_000_000, 2));
    for x in 0..8 {
        assert!(dc.get(x, 2), "visible run missing dot ({x},2)");
    }
    // Extreme endpoints (would overflow `x1 - x0` unclipped).
    let mut ext = DotCanvas::braille(4, 1);
    ext.line((i32::MIN, i32::MIN), (i32::MAX, i32::MAX));
    let _ = dots(&ext); // completed, bounded, no panic
                        // A far segment that misses the grid entirely draws nothing.
    let mut miss = DotCanvas::braille(4, 1);
    miss.line((-1_000_000, 50), (1_000_000, 50));
    assert!(dots(&miss).is_empty());
}

#[test]
fn blit_composes_with_clipped_canvas() {
    let mut dc = DotCanvas::braille(8, 4);
    dc.line((0, 0), (15, 15));
    dc.line((0, 15), (15, 0));

    let mut out = BufferCanvas::new(Size::new(8, 4));
    let clip = Rect::new(2, 1, 4, 2);
    {
        let mut clipped = ClippedCanvas::new(&mut out, clip);
        dc.blit(&mut clipped, Point::new(0, 0), Rgba::WHITE);
    }
    for y in 0..4 {
        for x in 0..8 {
            let (ch, _, _) = out.cell(Point::new(x, y)).unwrap();
            if clip.contains(Point::new(x, y)) {
                assert_eq!(
                    ch,
                    dc.cell_char(x, y).unwrap_or(' '),
                    "inside clip at ({x},{y})"
                );
            } else {
                assert_eq!(ch, ' ', "write leaked outside the clip at ({x},{y})");
            }
        }
    }
}

#[test]
fn blit_styled_carries_attributes() {
    let mut dc = DotCanvas::braille(3, 1);
    dc.line((0, 3), (5, 0));
    let mut out = BufferCanvas::new(Size::new(3, 1));
    let style = crate::render::Style::new().fg(Rgba::rgb(9, 9, 9)).bold();
    dc.blit_styled(&mut out, Point::new(0, 0), &style);
    for cx in 0..3 {
        if dc.cell_char(cx, 0).is_some() {
            assert!(
                out.attrs_at(Point::new(cx, 0))
                    .contains(crate::render::Attrs::BOLD),
                "stroke cell {cx} lost its attributes"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Eighth-block fills
// ---------------------------------------------------------------------------

#[test]
fn fill_v_matches_the_bar_vocabulary() {
    let fg = Rgba::rgb(1, 2, 3);
    let mut out = BufferCanvas::new(Size::new(2, 2));
    // 9/16 of a 2-cell column = one full cell + one eighth above it.
    fill_v(
        &mut out,
        Rect::new(0, 0, 2, 2),
        9.0 / 16.0,
        fg,
        Rgba::TRANSPARENT,
    );
    assert_eq!(out.row_text(1), "██");
    assert_eq!(out.row_text(0), "▁▁");
    assert_eq!(out.cell(Point::new(0, 1)).unwrap().1, fg);

    let mut none = BufferCanvas::new(Size::new(2, 2));
    fill_v(&mut none, Rect::new(0, 0, 2, 2), 0.0, fg, Rgba::TRANSPARENT);
    fill_v(
        &mut none,
        Rect::new(0, 0, 2, 2),
        f32::NAN,
        fg,
        Rgba::TRANSPARENT,
    );
    assert_eq!(none.row_text(0), "  ");
    assert_eq!(none.row_text(1), "  ");

    let mut all = BufferCanvas::new(Size::new(2, 2));
    fill_v(&mut all, Rect::new(0, 0, 2, 2), 7.0, fg, Rgba::TRANSPARENT); // clamps
    assert_eq!(all.row_text(0), "██");
    assert_eq!(all.row_text(1), "██");
}

#[test]
fn fill_h_matches_the_progress_vocabulary() {
    let fg = Rgba::rgb(1, 2, 3);
    let bg = Rgba::rgb(7, 7, 7);
    let mut out = BufferCanvas::new(Size::new(10, 1));
    // 0.56 of 10 cells = 44.8 eighths -> 5 full + a 5/8 block (the
    // shipped Progress golden).
    fill_h(&mut out, Rect::new(0, 0, 10, 1), 0.56, fg, bg);
    assert_eq!(out.row_text(0), "█████▋    ");
    assert_eq!(
        out.cell(Point::new(5, 0)).unwrap().2,
        bg,
        "bg rides the fill"
    );
    assert_eq!(out.cell(Point::new(0, 0)).unwrap().1, fg);
}
