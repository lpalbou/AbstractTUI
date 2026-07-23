//! Unit tests for the Screenshot capture value + exporters. The
//! cross-source roundtrip / golden / perf evidence lives in
//! `tests/wave_screenshot.rs` (it needs the testing rig and the driver).

use super::*;
use crate::base::{Rect, Rgba, Size};
use crate::render::style::Style;
use crate::render::Cell;

fn surface(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

#[test]
fn captures_text_colors_attrs_and_wide_pairs() {
    let mut s = surface(10, 2);
    s.draw_text(
        0,
        0,
        "err",
        Style::new()
            .fg(Rgba::rgb(255, 0, 0))
            .bg(Rgba::rgb(0, 0, 40))
            .bold(),
    );
    s.draw_text(4, 0, "世", Style::new());
    let shot = Screenshot::from_surface(&s);
    assert_eq!(shot.size(), Size::new(10, 2));

    let e = shot.cell(0, 0).unwrap();
    assert_eq!(e.text(), "e");
    assert_eq!(e.fg(), Some(Rgba::rgb(255, 0, 0)));
    assert_eq!(e.bg(), Some(Rgba::rgb(0, 0, 40)));
    assert!(e.attrs().contains(Attrs::BOLD));

    let leader = shot.cell(4, 0).unwrap();
    assert_eq!(leader.text(), "世");
    assert_eq!(leader.width(), 2);
    let cont = shot.cell(5, 0).unwrap();
    assert!(cont.is_continuation());
    assert_eq!(cont.text(), "");

    // Untouched ground: default blank (space, no colors).
    let blank = shot.cell(0, 1).unwrap();
    assert_eq!(blank.text(), " ");
    assert_eq!(blank.fg(), None);
    assert_eq!(blank.bg(), None);
}

#[test]
fn blank_and_printed_space_capture_identically() {
    // The two are visually and wire-identical; captures from the frame
    // and from a byte replay must compare equal (the roundtrip law).
    let mut a = surface(3, 1);
    a.draw_text(0, 0, " x", Style::new());
    let b = {
        let mut s = surface(3, 1);
        s.draw_text(1, 0, "x", Style::new());
        s
    };
    let (sa, sb) = (Screenshot::from_surface(&a), Screenshot::from_surface(&b));
    assert_eq!(sa.cell(0, 0), sb.cell(0, 0), "space == untouched blank");
    assert_eq!(sa.cell(1, 0), sb.cell(1, 0));
}

#[test]
fn alpha_normalizes_to_wire_truth() {
    // Alpha 0 = terminal default; any other alpha is told to the
    // terminal as its opaque RGB — the capture stores exactly that.
    let mut s = surface(2, 1);
    s.draw_text(0, 0, "x", Style::new().bg(Rgba::new(10, 20, 30, 128)));
    let shot = Screenshot::from_surface(&s);
    assert_eq!(shot.cell(0, 0).unwrap().bg(), Some(Rgba::rgb(10, 20, 30)));
    assert_eq!(shot.cell(1, 0).unwrap().bg(), None);
}

#[test]
fn to_text_trims_trailing_blanks_and_renders_wide_once() {
    let mut s = surface(8, 2);
    s.draw_text(0, 0, "hi 世界", Style::new());
    let shot = Screenshot::from_surface(&s);
    assert_eq!(shot.to_text(), "hi 世界\n\n");
}

#[test]
fn to_ansi_emits_minimal_sgr_runs() {
    let mut s = surface(8, 1);
    let red = Style::new().fg(Rgba::rgb(255, 0, 0));
    s.draw_text(0, 0, "aa", red);
    s.draw_text(2, 0, "bb", red); // same style: same run, no new SGR
    s.draw_text(4, 0, "c", Style::new().fg(Rgba::rgb(0, 255, 0)));
    let ansi = Screenshot::from_surface(&s).to_ansi();
    assert_eq!(
        ansi, "\x1b[0m\x1b[38;2;255;0;0maabb\x1b[38;2;0;255;0mc\x1b[0m",
        "one SGR per style change, reset at row end, no trailing newline"
    );
}

#[test]
fn to_ansi_keeps_colored_trailing_run_and_separates_rows() {
    let mut s = surface(4, 2);
    // Row 0 ends in a colored-bg blank (a visible bar): must NOT trim.
    s.fill_rect(
        Rect::new(2, 0, 2, 1),
        Cell::EMPTY.with_bg(Rgba::rgb(0, 0, 200)),
    );
    s.draw_text(0, 1, "x", Style::new());
    let ansi = Screenshot::from_surface(&s).to_ansi();
    assert_eq!(ansi, "\x1b[0m  \x1b[48;2;0;0;200m  \x1b[0m\r\nx");
}

#[test]
fn empty_and_one_by_one_grids_export_without_panic() {
    let zero = Screenshot::from_surface(&surface(0, 0));
    assert_eq!(zero.to_text(), "");
    assert_eq!(zero.to_ansi(), "\x1b[0m");
    assert!(zero.to_svg().starts_with("<svg "));
    assert!(zero.to_svg().ends_with("</svg>\n"));

    let mut s = surface(1, 1);
    s.draw_text(0, 0, "q", Style::new());
    let one = Screenshot::from_surface(&s);
    assert_eq!(one.to_text(), "q\n");
    assert_eq!(one.to_ansi(), "\x1b[0mq");
    assert!(one.to_svg().contains(">q</text>"));
}

#[test]
fn svg_escapes_markup_glyphs_everywhere() {
    let mut s = surface(8, 1);
    s.draw_text(0, 0, "<&>\"'", Style::new());
    let svg = Screenshot::from_surface(&s).to_svg();
    assert!(svg.contains("&lt;&amp;&gt;&quot;&apos;"), "{svg}");
    assert!(!svg.contains("<&"), "raw markup must never survive: {svg}");
}

#[test]
fn svg_maps_attrs_reverse_and_decorations() {
    let mut s = surface(12, 2);
    s.draw_text(
        0,
        0,
        "bold",
        Style::new().fg(Rgba::rgb(10, 10, 10)).bold().italic(),
    );
    s.draw_text(
        0,
        1,
        "rev",
        Style::new()
            .fg(Rgba::rgb(1, 2, 3))
            .bg(Rgba::rgb(9, 9, 9))
            .reverse(),
    );
    s.draw_text(
        4,
        1,
        "ul",
        Style::new()
            .underline()
            .underline_color(Rgba::rgb(0, 0, 255)),
    );
    s.draw_text(7, 1, "hid", Style::new().attrs(Attrs::HIDDEN));
    let svg = Screenshot::from_surface(&s).to_svg();
    assert!(svg.contains("font-weight=\"700\""));
    assert!(svg.contains("font-style=\"italic\""));
    // Reverse: ink paints with the cell's bg, ground with the cell's fg.
    assert!(svg.contains("fill=\"#090909\">rev</text>"), "{svg}");
    assert!(
        svg.contains("fill=\"#010203\"/>"),
        "reversed bg rect: {svg}"
    );
    // Underline decoration rect carries the SGR 58 color.
    assert!(svg.contains("height=\"1\" fill=\"#0000ff\""), "{svg}");
    // Hidden ink paints nothing.
    assert!(!svg.contains("hid</text>"), "{svg}");
}

#[test]
fn svg_pins_wide_glyphs_to_their_columns() {
    let mut s = surface(6, 1);
    s.draw_text(0, 0, "a世b", Style::new());
    let svg = Screenshot::from_surface(&s).to_svg();
    // The wide glyph is its own run, 2 columns wide at column 1.
    assert!(
        svg.contains("<text x=\"9\" y=\"14\" textLength=\"18\""),
        "{svg}"
    );
    assert!(svg.contains(">世</text>"), "{svg}");
}

#[test]
fn pixel_regions_clip_and_render_as_labeled_veils() {
    let mut shot = Screenshot::from_surface(&surface(10, 4));
    shot.add_pixel_region(Rect::new(8, 2, 10, 10)); // clips to 2x2
    shot.add_pixel_region(Rect::new(-5, -5, 2, 2)); // fully outside: dropped
    assert_eq!(shot.pixel_regions(), &[Rect::new(8, 2, 2, 2)]);
    let svg = shot.to_svg();
    assert!(svg.contains("image (pixels)"), "{svg}");
    assert!(svg.contains("stroke-dasharray"), "{svg}");
}

#[test]
fn exports_are_deterministic() {
    let mut s = surface(20, 3);
    s.draw_text(0, 0, "同 text ⚡", Style::new().fg(Rgba::rgb(9, 9, 9)));
    let shot = Screenshot::from_surface(&s);
    assert_eq!(shot.to_text(), shot.to_text());
    assert_eq!(shot.to_ansi(), shot.to_ansi());
    assert_eq!(shot.to_svg(), shot.to_svg());
    let again = Screenshot::from_surface(&s);
    assert_eq!(shot, again, "same screen, same capture");
}
