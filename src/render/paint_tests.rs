//! Paint tests: gradient math (stops, angles, radial, dither), the
//! axis convenience, and the drop-shadow layer recipe.

use super::*;
use crate::base::Size;
use crate::render::compositor::Compositor;
use crate::render::style::Style;

fn surf(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

#[test]
fn axis_convenience_interpolates_endpoints_exactly() {
    let mut s = surf(5, 2);
    let b = s.bounds();
    fill_gradient_axis(
        &mut s,
        b,
        Rgba::rgb(0, 0, 0),
        Rgba::rgb(200, 100, 0),
        Axis::Horizontal,
    );
    // Cell centers at t=0.1..0.9: endpoints are NEAR the stop colors
    // (center sampling, undithered): first cell 10% in, last 90%.
    let first = s.get(0, 0).unwrap().bg;
    let last = s.get(4, 0).unwrap().bg;
    assert!(first.r <= 30, "{first:?}");
    assert!(last.r >= 170, "{last:?}");
    assert_eq!(
        s.get(2, 0).unwrap().bg,
        Rgba::rgb(100, 50, 0),
        "midpoint exact"
    );
    assert_eq!(
        s.get(2, 1).unwrap().bg,
        s.get(2, 0).unwrap().bg,
        "rows equal"
    );
}

#[test]
fn multi_stop_and_vertical_angle() {
    let mut s = surf(1, 9);
    let spec = GradientSpec::linear(
        90.0,
        vec![
            (0.0, Rgba::rgb(0, 0, 0)),
            (0.5, Rgba::rgb(255, 0, 0)),
            (1.0, Rgba::rgb(255, 255, 255)),
        ],
    )
    .without_dither();
    let b = s.bounds();
    fill_gradient(&mut s, b, &spec);
    // Middle row sits at the middle stop.
    assert_eq!(s.get(0, 4).unwrap().bg, Rgba::rgb(255, 0, 0));
    // Above middle: black->red leg; below: red->white leg.
    let above = s.get(0, 1).unwrap().bg;
    assert!(above.r > 0 && above.g == 0 && above.b == 0, "{above:?}");
    let below = s.get(0, 7).unwrap().bg;
    assert!(
        below.g > 0,
        "past the middle stop climbs toward white: {below:?}"
    );
    // Unsorted stop lists behave identically (sorted at fill).
    let mut s2 = surf(1, 9);
    let spec2 = GradientSpec {
        stops: vec![
            (1.0, Rgba::rgb(255, 255, 255)),
            (0.0, Rgba::rgb(0, 0, 0)),
            (0.5, Rgba::rgb(255, 0, 0)),
        ],
        kind: GradientKind::Linear { angle_deg: 90.0 },
        dither: false,
    };
    let b2 = s2.bounds();
    fill_gradient(&mut s2, b2, &spec2);
    for y in 0..9 {
        assert_eq!(s2.get(0, y).unwrap().bg, s.get(0, y).unwrap().bg, "row {y}");
    }
}

#[test]
fn diagonal_angle_orders_corners() {
    // 45° visual: top-left darkest, bottom-right lightest, and the two
    // off-diagonal corners in between.
    let mut s = surf(12, 6);
    let spec = GradientSpec::two(
        Rgba::rgb(0, 0, 0),
        Rgba::rgb(240, 240, 240),
        GradientKind::Linear { angle_deg: 45.0 },
    )
    .without_dither();
    let b = s.bounds();
    fill_gradient(&mut s, b, &spec);
    let tl = s.get(0, 0).unwrap().bg.r;
    let br = s.get(11, 5).unwrap().bg.r;
    let tr = s.get(11, 0).unwrap().bg.r;
    let bl = s.get(0, 5).unwrap().bg.r;
    assert!(tl < tr && tl < bl, "top-left darkest: {tl} {tr} {bl}");
    assert!(br > tr && br > bl, "bottom-right lightest: {br} {tr} {bl}");
}

#[test]
fn radial_center_dark_edges_light() {
    let mut s = surf(11, 5);
    let spec = GradientSpec::radial(
        (0.5, 0.5),
        vec![(0.0, Rgba::rgb(0, 0, 0)), (1.0, Rgba::rgb(200, 200, 200))],
    )
    .without_dither();
    let b = s.bounds();
    fill_gradient(&mut s, b, &spec);
    let center = s.get(5, 2).unwrap().bg.r;
    let corner = s.get(0, 0).unwrap().bg.r;
    let edge_mid = s.get(0, 2).unwrap().bg.r;
    assert!(center < 40, "center near the inner stop: {center}");
    assert!(corner > 150, "corner near the outer stop: {corner}");
    assert!(edge_mid > center && edge_mid < corner, "monotone outward");
}

#[test]
fn dither_breaks_bands_but_stays_within_one_step() {
    // A wide rect with a tiny delta: undithered it bands into 2 solid
    // halves; dithered, transition columns mix the two levels.
    let from = Rgba::rgb(100, 100, 100);
    let to = Rgba::rgb(102, 102, 102);
    let mut plain = surf(64, 4);
    let b = plain.bounds();
    fill_gradient(
        &mut plain,
        b,
        &GradientSpec::two(from, to, GradientKind::Linear { angle_deg: 0.0 }).without_dither(),
    );
    let mut dithered = surf(64, 4);
    let b2 = dithered.bounds();
    fill_gradient(
        &mut dithered,
        b2,
        &GradientSpec::two(from, to, GradientKind::Linear { angle_deg: 0.0 }),
    );
    // Every dithered value stays inside the gradient's color range.
    let mut mixed_columns = 0;
    for x in 0..64 {
        let mut seen = std::collections::BTreeSet::new();
        for y in 0..4 {
            let c = dithered.get(x, y).unwrap().bg;
            assert!(
                (100..=102).contains(&c.r),
                "dither may only pick adjacent gradient levels: {c:?}"
            );
            seen.insert(c.r);
        }
        if seen.len() > 1 {
            mixed_columns += 1;
        }
    }
    assert!(
        mixed_columns > 4,
        "dither must actually mix near band edges: {mixed_columns}"
    );
    // Determinism: same input, same pixels.
    let mut again = surf(64, 4);
    let b3 = again.bounds();
    fill_gradient(
        &mut again,
        b3,
        &GradientSpec::two(from, to, GradientKind::Linear { angle_deg: 0.0 }),
    );
    for y in 0..4 {
        for x in 0..64 {
            assert_eq!(again.get(x, y).unwrap().bg, dithered.get(x, y).unwrap().bg);
        }
    }
}

#[test]
fn gradient_preserves_glyphs_pairs_and_damages_once() {
    let mut s = surf(8, 2);
    s.draw_text(0, 0, "a世b", Style::new());
    let mut sink = Vec::new();
    s.take_damage(&mut sink);
    sink.clear();
    let b = s.bounds();
    fill_gradient_axis(
        &mut s,
        b,
        Rgba::rgb(10, 0, 0),
        Rgba::rgb(60, 0, 0),
        Axis::Horizontal,
    );
    assert_eq!(s.glyph_str(s.get(1, 0).unwrap()), "世", "glyphs survive");
    s.debug_validate().unwrap();
    let leader = s.get(1, 0).unwrap();
    let cont = s.get(2, 0).unwrap();
    assert_eq!(leader.bg, cont.bg, "pair carries one bg");
    s.take_damage(&mut sink);
    assert!(!sink.is_empty(), "fill damages");
    sink.clear();
    s.take_damage(&mut sink);
    assert!(sink.is_empty(), "…exactly once (one-time paint)");
}

#[test]
fn drop_shadow_layer_recipe_composes() {
    let mut frame = surf(20, 8);
    let mut comp = Compositor::new();
    comp.set_ground(Some(Rgba::rgb(30, 30, 40)));
    let panel = Rect::new(4, 2, 8, 3);
    // Recipe: shadow below (z 4), panel above (z 5).
    let shadow = drop_shadow(panel, Point::new(1, 1), 2, Rgba::new(0, 0, 0, 160), 4);
    let mut panel_surface = Surface::new(panel.size(), Cell::EMPTY);
    panel_surface.fill_rect(
        Rect::from_size(panel.size()),
        Cell::EMPTY.with_bg(Rgba::rgb(90, 90, 120)),
    );
    let panel_layer = Layer::new(panel_surface, panel.origin(), 5);
    let mut layers = vec![shadow, panel_layer];
    comp.flatten(&mut frame, &mut layers);

    // Panel cells are the panel color (shadow beneath is hidden).
    assert_eq!(frame.get(5, 3).unwrap().bg, Rgba::rgb(90, 90, 120));
    // Just outside the panel toward the offset: ground darkened by the
    // shadow, darker close to the panel than farther away.
    let ground = Rgba::rgb(30, 30, 40);
    let near = frame.get(12, 5).unwrap().bg; // 1 cell past the panel edge
    let far = frame.get(14, 7).unwrap().bg; // feather edge
    assert!(
        near.r < ground.r,
        "near shadow darkens the ground: {near:?}"
    );
    assert!(
        far.r >= near.r,
        "shadow feathers out: near {near:?} far {far:?}"
    );
    // Well away from the shadow: untouched terminal-default.
    assert!(frame.get(0, 0).unwrap().bg.is_transparent());
}

#[test]
fn degenerate_specs_are_inert() {
    let mut s = surf(4, 2);
    let b = s.bounds();
    fill_gradient(
        &mut s,
        b,
        &GradientSpec {
            stops: vec![],
            kind: GradientKind::Linear { angle_deg: 0.0 },
            dither: true,
        },
    );
    assert!(
        s.get(0, 0).unwrap().bg.is_transparent(),
        "no stops, no paint"
    );
    // Single stop = flat fill.
    fill_gradient(
        &mut s,
        b,
        &GradientSpec {
            stops: vec![(0.3, Rgba::rgb(7, 7, 7))],
            kind: GradientKind::Radial { center: (0.5, 0.5) },
            dither: true,
        },
    );
    assert_eq!(s.get(3, 1).unwrap().bg, Rgba::rgb(7, 7, 7));
    // Zero-feather shadow is a hard offset silhouette, no panic.
    let l = drop_shadow(
        Rect::new(0, 0, 3, 2),
        Point::new(1, 1),
        0,
        Rgba::new(0, 0, 0, 120),
        0,
    );
    assert_eq!(l.bounds().size(), Size::new(3, 2));
}
