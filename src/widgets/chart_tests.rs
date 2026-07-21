//! Chart widget tests, split out to keep `chart.rs` within the
//! file-size budget; deterministic fixed-series pins.

use super::*;
use crate::base::Size;
use crate::theme::default_theme;
use crate::widgets::test_util::{draw_into, row};

#[test]
fn braille_bits_match_unicode_dot_order() {
    // Dots 1..8 in the standard layout — the whole grid rides this.
    assert_eq!(braille_bit(0, 0), 0x01);
    assert_eq!(braille_bit(0, 3), 0x40);
    assert_eq!(braille_bit(1, 0), 0x08);
    assert_eq!(braille_bit(1, 3), 0x80);
}

#[test]
fn sparkline_renders_deterministic_ramp() {
    let t = default_theme().tokens;
    // 4 samples across 2 cells (4 dot columns): rows 3,2,1,0.
    let el = Sparkline::new(vec![0.0, 1.0, 2.0, 3.0]).element(&t);
    let c = draw_into(el, Size::new(2, 1));
    assert_eq!(row(&c, 0), "⡠⠊");
    // Determinism: same input, same cells.
    let el = Sparkline::new(vec![0.0, 1.0, 2.0, 3.0]).element(&t);
    let c2 = draw_into(el, Size::new(2, 1));
    assert_eq!(row(&c2, 0), row(&c, 0));
    // Line color is the requested ramp slot.
    let el = Sparkline::new(vec![0.0, 1.0, 2.0, 3.0]).slot(3).element(&t);
    let c = draw_into(el, Size::new(2, 1));
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.chart(3));
}

#[test]
fn sparkline_flat_series_centers_and_gaps_skip() {
    let t = default_theme().tokens;
    let el = Sparkline::new(vec![5.0, 5.0, 5.0, 5.0]).element(&t);
    let c = draw_into(el, Size::new(2, 1));
    // Flat: mid-range row (row 2 of 0..=3 -> dots 0x04/0x20 = '⠤').
    assert_eq!(row(&c, 0), "⠤⠤");

    let el = Sparkline::new(vec![f32::NAN, f32::NAN]).element(&t);
    let c = draw_into(el, Size::new(2, 1));
    assert_eq!(row(&c, 0).trim(), "", "all-NaN series draws nothing");
}

#[test]
fn line_chart_axes_labels_and_series_colors() {
    let t = default_theme().tokens;
    let el = LineChart::new(vec![vec![0.0, 10.0], vec![10.0, 0.0]])
        .range(0.0, 10.0)
        .element(&t);
    let c = draw_into(el, Size::new(12, 4));
    // Gutter labels: max top-left, min above the x-axis.
    assert!(row(&c, 0).starts_with("10"), "{:?}", row(&c, 0));
    assert!(row(&c, 2).starts_with('0'), "{:?}", row(&c, 2));
    // Axis glyphs present: corner sits after the "10 " gutter
    // (label width 2 + pad 1 = column 3).
    assert_eq!(c.cell(Point::new(3, 3)).unwrap().0, '└');
    // Both series drew in their ramp slots somewhere in the plot.
    let mut seen = [false, false];
    for y in 0..4 {
        for x in 0..12 {
            let (_, fg, _) = c.cell(Point::new(x, y)).unwrap();
            if fg == t.chart(0) {
                seen[0] = true;
            }
            if fg == t.chart(1) {
                seen[1] = true;
            }
        }
    }
    assert!(seen[0] && seen[1], "both series colored");
}

#[test]
fn line_chart_degrades_without_room_and_never_panics() {
    let t = default_theme().tokens;
    for size in [
        Size::new(0, 0),
        Size::new(1, 1),
        Size::new(3, 1),
        Size::new(5, 2),
    ] {
        let el = LineChart::new(vec![vec![1.0, 2.0, 3.0]]).element(&t);
        let _ = draw_into(el, size);
    }
    // Empty series set: nothing drawn, no range, no panic.
    let el = LineChart::new(vec![]).element(&t);
    let c = draw_into(el, Size::new(6, 3));
    assert_eq!(row(&c, 1).trim(), "");
}

#[test]
fn bar_chart_eighth_precision_and_cycling_colors() {
    let t = default_theme().tokens;
    // h=2 cells = 16 eighths. 0.5 -> 8 (one full cell); 9/16 -> full
    // + one eighth above.
    let el = BarChart::new(vec![0.5, 9.0 / 16.0])
        .range(0.0, 1.0)
        .bar(1, 1)
        .element(&t);
    let c = draw_into(el, Size::new(3, 2));
    assert_eq!(c.cell(Point::new(0, 1)).unwrap().0, '█');
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().0, ' ');
    assert_eq!(c.cell(Point::new(2, 1)).unwrap().0, '█');
    assert_eq!(c.cell(Point::new(2, 0)).unwrap().0, '▁');
    // Default coloring cycles the ramp per bar.
    assert_eq!(c.cell(Point::new(0, 1)).unwrap().1, t.chart(0));
    assert_eq!(c.cell(Point::new(2, 1)).unwrap().1, t.chart(1));
    // Single-slot override pins every bar.
    let el = BarChart::new(vec![1.0, 1.0]).slot(4).bar(1, 0).element(&t);
    let c = draw_into(el, Size::new(2, 1));
    assert_eq!(c.cell(Point::new(0, 0)).unwrap().1, t.chart(4));
    assert_eq!(c.cell(Point::new(1, 0)).unwrap().1, t.chart(4));
}

#[test]
fn bar_chart_edge_cases() {
    let t = default_theme().tokens;
    // Zero/NaN/overflow values and zero-size rects are all safe.
    let el = BarChart::new(vec![0.0, f32::NAN, 99.0])
        .range(0.0, 1.0)
        .element(&t);
    let c = draw_into(el, Size::new(8, 2));
    // Over-range clamps to a full column, NaN leaves its slot empty.
    assert_eq!(c.cell(Point::new(6, 0)).unwrap().0, '█');
    assert_eq!(c.cell(Point::new(3, 1)).unwrap().0, ' ');
    let el = BarChart::new(vec![]).element(&t);
    let _ = draw_into(el, Size::new(0, 0));
}
