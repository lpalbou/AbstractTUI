//! Canvas cross-review (extensions wave, cycle 2): the graph/mermaid
//! consumer probing the 0420 vector layer's documented contracts —
//! (a) clipped-line boundary, (b) arc sufficiency for loopbacks,
//! (c) deterministic-trig drift at raster scale, (d) flattening
//! density in Quadrant mode, (e) the cell-color rule, (f) eighth-fill
//! boundaries. Each probe records its disposition for the wave report.

use std::collections::BTreeSet;

use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::canvas::{fill_h, fill_v, DotCanvas, H_EIGHTHS, V_EIGHTHS};
use abstracttui::ui::{BufferCanvas, Canvas};

type Dots = BTreeSet<(i32, i32)>;

fn dots(grid: &DotCanvas) -> Dots {
    let mut out = BTreeSet::new();
    for y in 0..grid.dots_h() {
        for x in 0..grid.dots_w() {
            if grid.get(x, y) {
                out.insert((x, y));
            }
        }
    }
    out
}

/// Reference Bresenham (i64 — safe for far endpoints), collecting the
/// dots that land inside `w x h`.
fn reference_walk(a: (i64, i64), b: (i64, i64), w: i64, h: i64) -> Dots {
    let (mut x0, mut y0) = a;
    let (x1, y1) = b;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i64 = if x0 < x1 { 1 } else { -1 };
    let sy: i64 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = BTreeSet::new();
    loop {
        if (0..w).contains(&x0) && (0..h).contains(&y0) {
            out.insert((x0 as i32, y0 as i32));
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

/// (a) Clipped-line boundary contract.
///
/// DISPOSITION: holds as documented. In-box segments raster
/// byte-identical to the unclipped walk (the chart byte-identity
/// gate); far-endpoint segments differ from the ideal walk only
/// within one dot of the clip boundary. GRAPH-LANE VERDICT: the lane
/// is not exposed at all — GraphView/mermaid stroke into a
/// full-content-size grid (endpoints always in-box, the exact path),
/// and panning clips at CELL blit level after rasterization, so a
/// panned edge can never raster differently at the viewport boundary.
#[test]
fn a_clipped_line_boundary_holds_and_graph_lane_is_unexposed() {
    // In-box: exact equality with the reference walk.
    let mut grid = DotCanvas::braille(10, 5); // 20x20 dots
    grid.line((1, 1), (18, 17));
    assert_eq!(
        dots(&grid),
        reference_walk((1, 1), (18, 17), 20, 20),
        "in-box segments walk exactly as unclipped"
    );

    // Far endpoints: the visible raster may differ from the ideal
    // walk by at most one dot, and only adjacent to the boundary.
    let mut clipped = DotCanvas::braille(10, 5);
    clipped.line((-100_000, -99_983), (100_000, 100_017)); // y = x + 17
    let got = dots(&clipped);
    assert!(!got.is_empty(), "the visible portion drew");
    let ideal = reference_walk((-100_000, -99_983), (100_000, 100_017), 20, 20);
    for d in got.symmetric_difference(&ideal) {
        let near_edge = d.0 <= 1 || d.1 <= 1 || d.0 >= 18 || d.1 >= 18;
        assert!(near_edge, "difference {d:?} must sit at the clip boundary");
        let other = if got.contains(d) { &ideal } else { &got };
        assert!(
            other
                .iter()
                .any(|o| (o.0 - d.0).abs() <= 1 && (o.1 - d.1).abs() <= 1),
            "difference {d:?} exceeds the one-dot contract"
        );
    }

    // Determinism of the clipped path.
    let mut again = DotCanvas::braille(10, 5);
    again.line((-100_000, -99_983), (100_000, 100_017));
    assert_eq!(got, dots(&again));
}

/// (b) Arc API sufficiency for arrowheads/loopbacks.
///
/// DISPOSITION: sufficient. `ellipse_arc(center, rx, ry, start,
/// sweep)` draws partial arcs (loopback lobes, rounded corners) and
/// full circles; arrowheads never needed arcs (cell glyphs in the
/// view, or two short strokes at dot scale). Missing by design: SVG
/// endpoint-parameterized arcs — at cell resolution center/radii/
/// angles are directly computable, so no gap for the diagram lane.
#[test]
fn b_arc_api_covers_loopbacks_and_arrowheads() {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    // A 3/4 loopback lobe anchored right of a card edge.
    let mut lobe = DotCanvas::braille(12, 4); // 24x16 dots
    lobe.ellipse_arc((12.0, 8.0), 6.0, 5.0, -FRAC_PI_2, PI * 1.5);
    let lit = dots(&lobe);
    assert!(lit.len() > 10, "lobe drew a real arc");
    assert!(
        lit.iter().any(|d| d.0 >= 17),
        "lobe reaches rx right of center"
    );
    let mut again = DotCanvas::braille(12, 4);
    again.ellipse_arc((12.0, 8.0), 6.0, 5.0, -FRAC_PI_2, PI * 1.5);
    assert_eq!(lit, dots(&again), "deterministic");

    // Full circle passes through its parametric start point.
    let mut circle = DotCanvas::braille(10, 5);
    circle.ellipse_arc((10.0, 10.0), 8.0, 8.0, 0.0, TAU);
    assert!(circle.get(18, 10), "closes at (cx + r, cy)");

    // Arrowhead at dot scale: two short strokes, no arc required.
    let mut head = DotCanvas::braille(4, 2);
    head.line((6, 4), (2, 2));
    head.line((6, 4), (2, 6));
    assert!(head.get(6, 4) && head.get(2, 2) && head.get(2, 6));
}

/// (c) Deterministic-trig drift, observed at raster scale.
///
/// DISPOSITION: doc-state confirmed by probe. `det_sin_cos` is
/// (rightly) private, so the drift is pinned through its only
/// consumer: every lit dot of a large circle sits within 1.3 dots of
/// the ideal radius (rounding accounts for ~0.71; chord sag at the
/// 2048-segment cap ~0.0005; the Taylor kernels' 1e-9 relative error
/// contributes ~4e-7 dots at r=400 — invisible, as documented).
#[test]
fn c_det_trig_drift_invisible_at_raster_scale() {
    let r = 400.0f32;
    let mut g = DotCanvas::braille(420, 210); // 840x840 dots
    g.ellipse_arc((420.0, 420.0), r, r, 0.0, std::f32::consts::TAU);
    let lit = dots(&g);
    assert!(lit.len() > 1500, "a real circle: {} dots", lit.len());
    let mut worst = 0.0f64;
    for (x, y) in &lit {
        let (dx, dy) = (f64::from(*x) - 420.0, f64::from(*y) - 420.0);
        let dev = ((dx * dx + dy * dy).sqrt() - 400.0).abs();
        worst = worst.max(dev);
    }
    println!("det-trig circle: worst radial deviation {worst:.4} dots");
    assert!(
        worst <= 1.3,
        "radial deviation {worst} exceeds raster bound"
    );
}

/// (d) Flatness tolerance density in Quadrant mode.
///
/// DISPOSITION: no gaps. The 0.25-dot tolerance is applied in DOT
/// space, so the coarser quadrant grid subdivides just as finely
/// relative to its cells; a shallow wide quadratic lights every
/// column it crosses in BOTH modes (continuity, no holes).
#[test]
fn d_flattening_density_leaves_no_column_gaps_in_either_mode() {
    let check = |mut grid: DotCanvas, tag: &str| {
        grid.bezier_quad((2.0, 10.0), (30.0, -8.0), (58.0, 10.0), 0.25);
        let lit = dots(&grid);
        for x in 2..=58 {
            assert!(lit.iter().any(|d| d.0 == x), "{tag}: column {x} has a hole");
        }
    };
    check(DotCanvas::quadrant(30, 8), "quadrant"); // 60x16 dots
    check(DotCanvas::braille(30, 4), "braille"); // 60x16 dots
}

/// (e) The cell-color rule: three grids, one shared cell — the LAST
/// blit wins glyph AND color (dots never merge across grids), empty
/// cells stay transparent, and origins translate exactly.
///
/// DISPOSITION: holds as documented; the "one grid per ink" recipe
/// the graph view uses (normal vs broken strokes) is sound.
#[test]
fn e_cell_color_rule_last_blit_wins_whole_cells() {
    let (red, green, blue) = (
        Rgba::rgb(255, 0, 0),
        Rgba::rgb(0, 255, 0),
        Rgba::rgb(0, 0, 255),
    );
    // Three grids, each lighting a DIFFERENT dot of cell (3, 1).
    let mut a = DotCanvas::braille(6, 3);
    a.set(6, 4); // braille bit (0,0) -> U+2801
    let mut b = DotCanvas::braille(6, 3);
    b.set(7, 5); // bit (1,1) -> U+2810
    let mut c = DotCanvas::braille(6, 3);
    c.set(6, 7); // bit (0,3) -> U+2840

    let mut out = BufferCanvas::new(Size::new(12, 6));
    let origin = Point::new(2, 1);
    a.blit(&mut out, origin, red);
    b.blit(&mut out, origin, green);
    c.blit(&mut out, origin, blue);
    let cell = out.cell(Point::new(5, 2)).unwrap();
    assert_eq!(
        cell.0, '\u{2840}',
        "last grid's GLYPH wins — dots never merge"
    );
    assert_eq!(cell.1, blue, "last grid's COLOR wins");

    // Transparent composition: an empty grid blits nothing over it.
    let empty = DotCanvas::braille(6, 3);
    empty.blit(&mut out, origin, red);
    assert_eq!(out.cell(Point::new(5, 2)).unwrap().0, '\u{2840}');

    // Offset-origin: the same grid lands where the origin says.
    let mut out2 = BufferCanvas::new(Size::new(16, 8));
    a.blit(&mut out2, Point::new(0, 0), red);
    a.blit(&mut out2, Point::new(5, 2), green);
    assert_eq!(out2.cell(Point::new(3, 1)).unwrap().0, '\u{2801}');
    assert_eq!(out2.cell(Point::new(8, 3)).unwrap().0, '\u{2801}');
    assert_eq!(out2.cell(Point::new(8, 3)).unwrap().1, green);
}

/// (f) Eighth-fill boundaries: exact halves, rounding, untouched
/// remainders, full and non-finite fractions.
///
/// DISPOSITION: holds; `0.5` on odd extents produces the exact half
/// glyph, even extents produce clean full runs, cells beyond the run
/// are untouched (transparent-track contract), and round-half-away
/// applies at the eighth boundary.
#[test]
fn f_fill_half_boundaries_are_exact() {
    let marker = |canvas: &mut BufferCanvas, rect: Rect| {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                canvas.put(Point::new(x, y), '.', Rgba::WHITE, Rgba::TRANSPARENT);
            }
        }
    };
    let ink = Rgba::rgb(200, 200, 0);

    // Vertical, h = 1, exact half -> the 4/8 block.
    let mut c1 = BufferCanvas::new(Size::new(4, 4));
    let r1 = Rect::new(0, 0, 1, 1);
    marker(&mut c1, r1);
    fill_v(&mut c1, r1, 0.5, ink, Rgba::TRANSPARENT);
    assert_eq!(c1.cell(Point::new(0, 0)).unwrap().0, V_EIGHTHS[3]);

    // Vertical, h = 2, exact half -> one clean full row, top UNTOUCHED.
    let mut c2 = BufferCanvas::new(Size::new(4, 4));
    let r2 = Rect::new(0, 0, 1, 2);
    marker(&mut c2, r2);
    fill_v(&mut c2, r2, 0.5, ink, Rgba::TRANSPARENT);
    assert_eq!(c2.cell(Point::new(0, 1)).unwrap().0, '█');
    assert_eq!(c2.cell(Point::new(0, 0)).unwrap().0, '.', "track untouched");

    // Vertical, h = 3, half -> full bottom, half middle, untouched top.
    let mut c3 = BufferCanvas::new(Size::new(4, 4));
    let r3 = Rect::new(0, 0, 1, 3);
    marker(&mut c3, r3);
    fill_v(&mut c3, r3, 0.5, ink, Rgba::TRANSPARENT);
    assert_eq!(c3.cell(Point::new(0, 2)).unwrap().0, '█');
    assert_eq!(c3.cell(Point::new(0, 1)).unwrap().0, V_EIGHTHS[3]);
    assert_eq!(c3.cell(Point::new(0, 0)).unwrap().0, '.');

    // Round-half-away at the eighth boundary: 3.5 eighths -> 4.
    let mut c4 = BufferCanvas::new(Size::new(4, 4));
    let r4 = Rect::new(0, 0, 1, 1);
    fill_v(&mut c4, r4, 0.4375, ink, Rgba::TRANSPARENT);
    assert_eq!(c4.cell(Point::new(0, 0)).unwrap().0, V_EIGHTHS[3]);

    // Full and non-finite.
    let mut c5 = BufferCanvas::new(Size::new(4, 4));
    marker(&mut c5, r3);
    fill_v(&mut c5, r3, 1.0, ink, Rgba::TRANSPARENT);
    for y in 0..3 {
        assert_eq!(c5.cell(Point::new(0, y)).unwrap().0, '█');
    }
    let mut c6 = BufferCanvas::new(Size::new(4, 4));
    marker(&mut c6, r3);
    fill_v(&mut c6, r3, f32::NAN, ink, Rgba::TRANSPARENT);
    for y in 0..3 {
        assert_eq!(
            c6.cell(Point::new(0, y)).unwrap().0,
            '.',
            "NaN draws nothing"
        );
    }

    // Horizontal twins: w = 1 half, w = 3 half.
    let mut h1 = BufferCanvas::new(Size::new(4, 4));
    fill_h(&mut h1, Rect::new(0, 0, 1, 1), 0.5, ink, Rgba::TRANSPARENT);
    assert_eq!(h1.cell(Point::new(0, 0)).unwrap().0, H_EIGHTHS[3]);
    let mut h3 = BufferCanvas::new(Size::new(4, 4));
    let rh = Rect::new(0, 0, 3, 1);
    marker(&mut h3, rh);
    fill_h(&mut h3, rh, 0.5, ink, Rgba::TRANSPARENT);
    assert_eq!(h3.cell(Point::new(0, 0)).unwrap().0, '█');
    assert_eq!(h3.cell(Point::new(1, 0)).unwrap().0, H_EIGHTHS[3]);
    assert_eq!(h3.cell(Point::new(2, 0)).unwrap().0, '.');
}
