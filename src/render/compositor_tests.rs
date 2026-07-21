//! Compositor tests (cycle-2 behavior pins + cycle-3 blend/grade/shader).

use super::*;
use crate::base::{Point, Rgba, Size};
use crate::render::style::Style;

fn surface_with_text(w: i32, h: i32, text: &str) -> Surface {
    let mut s = Surface::new(Size::new(w, h), Cell::EMPTY);
    s.draw_text(0, 0, text, Style::new());
    s
}

fn frame(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

fn glyph_at(s: &Surface, x: i32, y: i32) -> String {
    s.glyph_str(s.get(x, y).unwrap()).to_string()
}

#[test]
fn z_order_and_glyph_replacement() {
    let mut f = frame(10, 2);
    let mut comp = Compositor::new();
    let mut layers = vec![
        Layer::new(surface_with_text(10, 2, "bottom"), Point::ZERO, 0),
        Layer::new(surface_with_text(3, 1, "top"), Point::ZERO, 5),
    ];
    let damage = comp.flatten(&mut f, &mut layers).to_vec();
    assert!(!damage.is_empty());
    assert_eq!(glyph_at(&f, 0, 0), "t");
    assert_eq!(glyph_at(&f, 3, 0), "t"); // "bottom"[3], top layer ends at 3
    assert_eq!(glyph_at(&f, 4, 0), "o");
}

#[test]
fn empty_glyph_is_see_through_space_erases() {
    let mut f = frame(4, 1);
    let mut comp = Compositor::new();
    let mut under = Surface::new(Size::new(4, 1), Cell::EMPTY);
    under.draw_text(0, 0, "abcd", Style::new());
    let mut over = Surface::new(Size::new(4, 1), Cell::EMPTY);
    // Column 0 stays EMPTY (see-through); column 1 gets a real space.
    over.draw_text(1, 0, " ", Style::new());
    let mut layers = vec![
        Layer::new(under, Point::ZERO, 0),
        Layer::new(over, Point::ZERO, 1),
    ];
    comp.flatten(&mut f, &mut layers);
    assert_eq!(glyph_at(&f, 0, 0), "a", "EMPTY lets lower glyph through");
    assert_eq!(glyph_at(&f, 1, 0), " ", "space erases lower glyph");
}

#[test]
fn translucent_background_blends_and_veils() {
    let mut f = frame(2, 1);
    let mut comp = Compositor::new();
    let mut under = Surface::new(Size::new(2, 1), Cell::EMPTY);
    under.fill_rect(under.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(0, 0, 0)));
    under.draw_text(0, 0, "x", Style::new().fg(Rgba::WHITE));
    let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
    over.fill_rect(
        over.bounds(),
        Cell::EMPTY.with_bg(Rgba::new(255, 0, 0, 128)),
    );
    let mut layers = vec![
        Layer::new(under, Point::ZERO, 0),
        Layer::new(over, Point::ZERO, 1),
    ];
    comp.flatten(&mut f, &mut layers);
    let cell = f.get(0, 0).unwrap();
    assert_eq!(f.glyph_str(cell), "x", "glyph survives a veil");
    assert!(cell.bg.r > 100, "bg blended toward red: {:?}", cell.bg);
    assert!(
        cell.fg.r > 200 && cell.fg.g < 200,
        "white fg veiled pink: {:?}",
        cell.fg
    );
}

#[test]
fn opacity_scales_the_whole_layer() {
    let mut f = frame(2, 1);
    let mut comp = Compositor::new();
    let mut under = Surface::new(Size::new(2, 1), Cell::EMPTY);
    under.fill_rect(under.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(0, 0, 255)));
    let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
    over.fill_rect(over.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(255, 0, 0)));
    let mut layers = vec![
        Layer::new(under, Point::ZERO, 0),
        Layer::new(over, Point::ZERO, 1),
    ];
    layers[1].set_opacity(0.5);
    comp.flatten(&mut f, &mut layers);
    let bg = f.get(0, 0).unwrap().bg;
    assert!(bg.r > 80 && bg.b > 80, "half red over blue mixes: {bg:?}");
}

#[test]
fn moving_a_layer_damages_old_and_new_bounds() {
    let mut f = frame(20, 3);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(surface_with_text(3, 1, "box"), Point::ZERO, 0)];
    comp.flatten(&mut f, &mut layers);
    assert_eq!(glyph_at(&f, 0, 0), "b");

    layers[0].set_origin(Point::new(10, 1));
    let damage = comp.flatten(&mut f, &mut layers).to_vec();
    // Old position reverted to background, new position shows content.
    assert_eq!(glyph_at(&f, 0, 0), "");
    assert_eq!(glyph_at(&f, 10, 1), "b");
    let covers = |p: Point| damage.iter().any(|r| r.contains(p));
    assert!(covers(Point::new(0, 0)), "old bounds damaged");
    assert!(covers(Point::new(10, 1)), "new bounds damaged");
}

#[test]
fn hide_reveals_content_below() {
    let mut f = frame(6, 1);
    let mut comp = Compositor::new();
    let mut layers = vec![
        Layer::new(surface_with_text(6, 1, "under"), Point::ZERO, 0),
        Layer::new(surface_with_text(6, 1, "OVER!"), Point::ZERO, 1),
    ];
    comp.flatten(&mut f, &mut layers);
    assert_eq!(glyph_at(&f, 0, 0), "O");
    layers[1].set_visible(false);
    comp.flatten(&mut f, &mut layers);
    assert_eq!(glyph_at(&f, 0, 0), "u");
}

#[test]
fn idle_frame_produces_no_damage() {
    let mut f = frame(10, 2);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(surface_with_text(10, 2, "hi"), Point::ZERO, 0)];
    comp.flatten(&mut f, &mut layers);
    assert!(!Compositor::any_dirty(&layers));
    let damage = comp.flatten(&mut f, &mut layers);
    assert!(damage.is_empty(), "second flatten with no writes is free");
}

#[test]
fn wide_pair_sliced_by_layer_edge_is_repaired() {
    let mut f = frame(6, 1);
    let mut comp = Compositor::new();
    // Upper layer 2 columns wide, holding one CJK pair, overlapping a
    // lower text layer so its edges cut through lower pairs.
    let mut layers = vec![
        Layer::new(surface_with_text(6, 1, "世界人"), Point::ZERO, 0),
        Layer::new(surface_with_text(2, 1, "中"), Point::new(1, 0), 1),
    ];
    comp.flatten(&mut f, &mut layers);
    // Frame: col0 = orphan of 世 (blanked), col1-2 = 中 pair,
    // col3 = orphan of 界 (blanked), col4-5 = 人 pair.
    assert_eq!(glyph_at(&f, 0, 0), " ");
    assert_eq!(glyph_at(&f, 1, 0), "中");
    assert!(f.get(2, 0).unwrap().is_continuation());
    assert_eq!(glyph_at(&f, 3, 0), " ");
    assert_eq!(glyph_at(&f, 4, 0), "人");
    f.debug_validate().unwrap();
}

#[test]
fn damage_cap_collapses_to_union() {
    let mut f = frame(80, 24);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(
        Surface::new(Size::new(80, 24), Cell::EMPTY),
        Point::ZERO,
        0,
    )];
    comp.flatten(&mut f, &mut layers); // consume initial damage
                                       // Scatter far more distinct writes than max_rects.
    for i in 0..30 {
        layers[0]
            .surface_mut()
            .draw_text((i * 2) % 78, (i * 5) % 24, "x", Style::new());
    }
    let damage = comp.flatten(&mut f, &mut layers).to_vec();
    assert!(
        damage.len() <= comp.max_rects,
        "count capped: {}",
        damage.len()
    );
}

#[test]
fn pooled_glyph_and_link_adopted_into_frame() {
    let mut f = frame(4, 1);
    let mut comp = Compositor::new();
    let mut s = Surface::new(Size::new(4, 1), Cell::EMPTY);
    let link = s.register_link("https://example.com");
    let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
    s.draw_text(0, 0, family, Style::new().link(link));
    let mut layers = vec![Layer::new(s, Point::ZERO, 0)];
    comp.flatten(&mut f, &mut layers);
    let cell = f.get(0, 0).unwrap();
    assert_eq!(f.glyph_str(cell), family);
    assert_eq!(f.link_uri(cell.link), Some("https://example.com"));
}

// -- cycle 3: blend modes, grades, shaders -----------------------------------

/// Flatten two stacked color fields and read one composed cell.
fn compose_two(bottom: Rgba, top: Rgba, configure: impl FnOnce(&mut Layer)) -> Cell {
    let mut f = frame(2, 1);
    let mut comp = Compositor::new();
    let mut under = Surface::new(Size::new(2, 1), Cell::EMPTY);
    under.fill_rect(under.bounds(), Cell::EMPTY.with_bg(bottom));
    let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
    over.fill_rect(over.bounds(), Cell::EMPTY.with_bg(top));
    let mut layers = vec![
        Layer::new(under, Point::ZERO, 0),
        Layer::new(over, Point::ZERO, 1),
    ];
    configure(&mut layers[1]);
    comp.flatten(&mut f, &mut layers);
    *f.get(0, 0).unwrap()
}

#[test]
fn additive_adds_and_saturates() {
    let out = compose_two(Rgba::rgb(200, 10, 0), Rgba::rgb(100, 10, 30), |l| {
        l.set_blend(Blend::Additive)
    });
    assert_eq!(
        (out.bg.r, out.bg.g, out.bg.b),
        (255, 20, 30),
        "channel add saturates"
    );

    // Black adds nothing — the additive identity.
    let out = compose_two(Rgba::rgb(42, 43, 44), Rgba::rgb(0, 0, 0), |l| {
        l.set_blend(Blend::Additive)
    });
    assert_eq!((out.bg.r, out.bg.g, out.bg.b), (42, 43, 44));
}

#[test]
fn additive_light_wash_brightens_glyph_ink() {
    let mut f = frame(2, 1);
    let mut comp = Compositor::new();
    let mut under = Surface::new(Size::new(2, 1), Cell::EMPTY);
    under.fill_rect(under.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(10, 10, 10)));
    under.draw_text(0, 0, "x", Style::new().fg(Rgba::rgb(100, 100, 100)));
    let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
    over.fill_rect(over.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(50, 0, 0)));
    let mut layers = vec![
        Layer::new(under, Point::ZERO, 0),
        Layer::new(over, Point::ZERO, 1),
    ];
    layers[1].set_blend(Blend::Additive);
    comp.flatten(&mut f, &mut layers);
    let cell = f.get(0, 0).unwrap();
    assert_eq!(f.glyph_str(cell), "x", "glyph survives a light wash");
    assert_eq!(cell.fg.r, 150, "ink brightened by the wash");
    assert_eq!(cell.bg.r, 60, "ground brightened too");
}

#[test]
fn additive_respects_opacity_via_premultiply() {
    let full = compose_two(Rgba::rgb(0, 0, 0), Rgba::rgb(100, 100, 100), |l| {
        l.set_blend(Blend::Additive)
    });
    let half = compose_two(Rgba::rgb(0, 0, 0), Rgba::rgb(100, 100, 100), |l| {
        l.set_blend(Blend::Additive);
        l.set_opacity(0.5);
    });
    assert_eq!(full.bg.r, 100);
    assert!(
        (half.bg.r as i32 - 50).abs() <= 1,
        "half opacity adds half: {:?}",
        half.bg
    );
}

#[test]
fn color_transforms_grade_the_contribution() {
    use crate::render::layer::ColorTransform;
    let out = compose_two(Rgba::rgb(0, 0, 0), Rgba::rgb(200, 100, 50), |l| {
        l.set_color_transform(ColorTransform::Dim(0.5))
    });
    assert_eq!(
        (out.bg.r, out.bg.g, out.bg.b),
        (100, 50, 25),
        "dim halves channels"
    );

    let out = compose_two(Rgba::rgb(0, 0, 0), Rgba::rgb(200, 100, 50), |l| {
        l.set_color_transform(ColorTransform::Tint(Rgba::rgb(0, 0, 255), 1.0))
    });
    assert_eq!(
        (out.bg.r, out.bg.g, out.bg.b),
        (0, 0, 255),
        "full tint is flat color"
    );

    let out = compose_two(Rgba::rgb(0, 0, 0), Rgba::rgb(200, 40, 40), |l| {
        l.set_color_transform(ColorTransform::Grayscale(1.0))
    });
    assert_eq!(out.bg.r, out.bg.g, "fully grayscale is gray");
    assert_eq!(out.bg.g, out.bg.b);
}

/// Test shader: paints fg from a closure-ish parameterization without
/// captures (CellShader is object-safe over plain structs).
struct FgFromXT;
impl crate::render::layer::CellShader for FgFromXT {
    fn shade(&self, x: i32, _y: i32, t: f32, cell: Cell) -> Cell {
        let mut c = cell;
        c.fg = Rgba::rgb((x * 10) as u8 + t as u8, 0, 0);
        c
    }
}

struct RedFg;
impl crate::render::layer::CellShader for RedFg {
    fn shade(&self, _x: i32, _y: i32, _t: f32, cell: Cell) -> Cell {
        let mut c = cell;
        c.fg = Rgba::rgb(255, 0, 0);
        c
    }
}

#[test]
fn shader_applies_only_within_layer_bounds() {
    let mut f = frame(8, 1);
    let mut comp = Compositor::new();
    let mut layers = vec![
        Layer::new(surface_with_text(8, 1, "aaaaaaaa"), Point::ZERO, 0),
        Layer::new(surface_with_text(3, 1, "bbb"), Point::new(2, 0), 1),
    ];
    layers[1].set_shader(Some(Box::new(RedFg)));
    comp.flatten(&mut f, &mut layers);
    assert_eq!(
        f.get(2, 0).unwrap().fg,
        Rgba::rgb(255, 0, 0),
        "inside: shaded"
    );
    assert_ne!(
        f.get(0, 0).unwrap().fg,
        Rgba::rgb(255, 0, 0),
        "left of layer: untouched"
    );
    assert_ne!(
        f.get(6, 0).unwrap().fg,
        Rgba::rgb(255, 0, 0),
        "right of layer: untouched"
    );
}

#[test]
fn shader_sees_frame_positions_and_layer_clock() {
    let mut f = frame(4, 1);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(
        surface_with_text(2, 1, "xy"),
        Point::new(1, 0),
        0,
    )];
    layers[0].set_shader(Some(Box::new(FgFromXT)));
    comp.flatten(&mut f, &mut layers);
    // shader_t starts at 0; positions are FRAME coordinates.
    assert_eq!(f.get(1, 0).unwrap().fg.r, 10);
    assert_eq!(f.get(2, 0).unwrap().fg.r, 20);
    // Animate: advancing the shader clock damages the layer by itself.
    layers[0].set_shader_t(2.0);
    comp.flatten(&mut f, &mut layers);
    assert_eq!(
        f.get(1, 0).unwrap().fg.r,
        12,
        "clock advanced through set_shader_t"
    );
}

struct OddColumnTint;
impl crate::render::layer::CellShader for OddColumnTint {
    fn shade(&self, x: i32, _y: i32, _t: f32, cell: Cell) -> Cell {
        let mut c = cell;
        if x % 2 == 1 {
            c.fg = Rgba::rgb(9, 9, 9);
        }
        c
    }
}

#[test]
fn shader_output_keeps_wide_pairs_consistent() {
    let mut f = frame(6, 1);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(surface_with_text(6, 1, "世界"), Point::ZERO, 0)];
    // A shader that tints only ODD columns would split-style a pair; the
    // repair pass must re-mirror from the leader.
    layers[0].set_shader(Some(Box::new(OddColumnTint)));
    comp.flatten(&mut f, &mut layers);
    f.debug_validate().unwrap();
    let leader = f.get(0, 0).unwrap();
    let cont = f.get(1, 0).unwrap();
    assert_eq!(cont.fg, leader.fg, "pair re-mirrored to the leader's style");
}

// -- cycle 7: shader cost is damage-bounded (RT6-3 structural answer) ----

#[test]
fn shader_runs_only_for_damaged_cells_and_never_when_static() {
    use std::cell::Cell as StdCell;
    use std::rc::Rc;

    // Instrumented shader: counts shade() calls. (Interior mutability is
    // fine HERE — determinism of output still holds; the counter is test
    // scaffolding, not shader state.)
    struct Counting(Rc<StdCell<u64>>);
    impl crate::render::layer::CellShader for Counting {
        fn shade(&self, _x: i32, _y: i32, _t: f32, cell: Cell) -> Cell {
            self.0.set(self.0.get() + 1);
            cell
        }
    }

    let calls = Rc::new(StdCell::new(0u64));
    let mut f = frame(200, 60);
    let mut comp = Compositor::new();
    let mut layers = vec![Layer::new(
        surface_with_text(200, 60, "content"),
        Point::ZERO,
        0,
    )];
    layers[0].set_shader(Some(Box::new(Counting(calls.clone()))));
    comp.flatten(&mut f, &mut layers); // first paint: full frame shades
    let first_paint = calls.get();
    assert!(
        first_paint >= 200 * 60,
        "installation re-shades the layer once"
    );

    // STATIC shader (t not advanced), no damage: flatten composes
    // NOTHING — zero shade calls. This is RT6-3's demand (a): effect
    // passes do NOT run per-cell per-frame while static.
    calls.set(0);
    for _ in 0..5 {
        comp.flatten(&mut f, &mut layers);
    }
    assert_eq!(calls.get(), 0, "static shader on an idle layer costs zero");

    // Small damage: shading is bounded by the (±1-expanded) damage rect,
    // not the layer size.
    layers[0].surface_mut().draw_text(50, 30, "x", Style::new());
    comp.flatten(&mut f, &mut layers);
    let small = calls.get();
    assert!(
        small > 0 && small < 64,
        "one-cell damage shades a handful of cells, not the layer: {small}"
    );

    // Advancing the clock IS the animation: full re-shade, billed as one.
    calls.set(0);
    layers[0].set_shader_t(1.0);
    comp.flatten(&mut f, &mut layers);
    assert!(calls.get() >= 200 * 60, "clock advance re-shades the layer");
}

// -- cycle 6: damage visualizer -----------------------------------------

#[test]
fn debug_damage_outlines_repaint_regions_only() {
    let mut f = frame(20, 6);
    let mut comp = Compositor::new();
    comp.set_debug_damage(true);
    assert!(comp.debug_damage());
    let mut layers = vec![Layer::new(
        Surface::new(Size::new(20, 6), Cell::EMPTY),
        Point::ZERO,
        0,
    )];
    comp.flatten(&mut f, &mut layers); // initial full-frame damage: outlined
    let frame1_interior = f.get(4, 2).unwrap().bg;
    assert!(
        frame1_interior.is_transparent(),
        "interior of frame 1 untinted"
    );

    // Small write: only its (expanded) rect gets outlined this frame.
    layers[0].surface_mut().draw_text(8, 3, "x", Style::new());
    let damage = comp.flatten(&mut f, &mut layers).to_vec();
    assert_eq!(damage.len(), 1);
    let r = damage[0];
    // Perimeter tinted (bg moved toward pink)…
    let corner = f.get(r.x, r.y).unwrap().bg;
    assert!(
        corner.r > 100 && corner.b > 80,
        "outline visible: {corner:?}"
    );
    // …cells outside BOTH the new damage and frame 1's outline untouched
    // ((4,2) was interior before and outside the new rect now).
    assert!(
        !r.contains(Point::new(4, 2)),
        "test premise: (4,2) outside the rect"
    );
    assert!(
        f.get(4, 2).unwrap().bg.is_transparent(),
        "no outline outside damage"
    );
    // Frame 1's border outline is STALE by design (documented): it stays
    // until damage covers it, exactly like any pixels.
    assert!(
        !f.get(0, 0).unwrap().bg.is_transparent(),
        "stale outline persists"
    );
    f.debug_validate().unwrap();

    // Toggle off: newly damaged region composes clean.
    comp.set_debug_damage(false);
    layers[0].surface_mut().draw_text(8, 3, "y", Style::new());
    comp.flatten(&mut f, &mut layers);
    let inside = f.get(8, 3).unwrap();
    assert!(
        inside.bg.is_transparent(),
        "content composed clean: {inside:?}"
    );
}

#[test]
fn debug_damage_keeps_wide_pairs_consistent() {
    let mut f = frame(10, 3);
    let mut comp = Compositor::new();
    comp.set_debug_damage(true);
    let mut layers = vec![Layer::new(
        surface_with_text(10, 3, "世界人あい"),
        Point::ZERO,
        0,
    )];
    comp.flatten(&mut f, &mut layers);
    f.debug_validate().unwrap(); // outline crossed pairs; invariant holds
}

// -- cycle 5: theme ground -----------------------------------------------

#[test]
fn ground_feeds_additive_over_terminal_default() {
    // Additive light over a default-bg (alpha 0) cell: legacy adds onto
    // black; with a declared ground it adds onto the theme bg.
    let light = Rgba::rgb(40, 10, 10);
    let compose = |ground: Option<Rgba>| -> Cell {
        let mut f = frame(2, 1);
        let mut comp = Compositor::new();
        comp.set_ground(ground);
        let under = Surface::new(Size::new(2, 1), Cell::EMPTY); // default bg
        let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
        over.fill_rect(over.bounds(), Cell::EMPTY.with_bg(light));
        let mut layers = vec![
            Layer::new(under, Point::ZERO, 0),
            Layer::new(over, Point::ZERO, 1),
        ];
        layers[1].set_blend(Blend::Additive);
        comp.flatten(&mut f, &mut layers);
        *f.get(0, 0).unwrap()
    };
    let legacy = compose(None);
    assert_eq!(
        (legacy.bg.r, legacy.bg.g, legacy.bg.b),
        (40, 10, 10),
        "onto black"
    );
    let grounded = compose(Some(Rgba::rgb(20, 22, 40)));
    assert_eq!(
        (grounded.bg.r, grounded.bg.g, grounded.bg.b),
        (60, 32, 50),
        "onto the theme ground"
    );
    assert!(grounded.bg.is_opaque(), "grounded result is concrete");
}

#[test]
fn ground_feeds_translucent_veils_over_terminal_default() {
    let veil = Rgba::new(200, 0, 0, 128);
    let compose = |ground: Option<Rgba>| -> Cell {
        let mut f = frame(2, 1);
        let mut comp = Compositor::new();
        comp.set_ground(ground);
        let under = Surface::new(Size::new(2, 1), Cell::EMPTY);
        let mut over = Surface::new(Size::new(2, 1), Cell::EMPTY);
        over.fill_rect(over.bounds(), Cell::EMPTY.with_bg(veil));
        let mut layers = vec![
            Layer::new(under, Point::ZERO, 0),
            Layer::new(over, Point::ZERO, 1),
        ];
        comp.flatten(&mut f, &mut layers);
        *f.get(0, 0).unwrap()
    };
    let legacy = compose(None);
    assert!(!legacy.bg.is_opaque(), "legacy: veil stays translucent");
    let g = Rgba::rgb(10, 10, 60);
    let grounded = compose(Some(g));
    assert!(grounded.bg.is_opaque(), "grounded veil is a concrete blend");
    assert!(
        grounded.bg.r > 60 && grounded.bg.b > 20 && grounded.bg.b < 60,
        "half red over dark blue ground: {:?}",
        grounded.bg
    );
}

#[test]
fn ground_never_leaks_into_untouched_cells() {
    let mut f = frame(4, 1);
    let mut comp = Compositor::new();
    comp.set_ground(Some(Rgba::rgb(99, 99, 99)));
    let mut under = Surface::new(Size::new(4, 1), Cell::EMPTY);
    under.draw_text(0, 0, "ab", Style::new()); // default fg/bg text
    let mut layers = vec![Layer::new(under, Point::ZERO, 0)];
    comp.flatten(&mut f, &mut layers);
    let c = f.get(0, 0).unwrap();
    assert!(
        c.bg.is_transparent(),
        "no blend happened: bg stays terminal-default"
    );
    assert!(f.get(3, 0).unwrap().bg.is_transparent());
}

#[test]
fn defaults_are_byte_identical_to_ungraded_compositor() {
    // The identity path (no blend change, no tint, no desat, no shader)
    // must produce the exact same frame as cycle 2 — pinned by comparing
    // a graded-then-reset layer against a never-graded one.
    let build = |grade: bool| -> Vec<u8> {
        let mut f = frame(12, 3);
        let mut comp = Compositor::new();
        let mut under = Surface::new(Size::new(12, 3), Cell::EMPTY);
        under.fill_rect(under.bounds(), Cell::EMPTY.with_bg(Rgba::rgb(20, 20, 40)));
        under.draw_text(
            0,
            0,
            "hello 世界",
            Style::new().fg(Rgba::rgb(200, 200, 200)),
        );
        let mut over = Surface::new(Size::new(6, 1), Cell::EMPTY);
        over.fill_rect(
            over.bounds(),
            Cell::EMPTY.with_bg(Rgba::new(255, 0, 0, 100)),
        );
        let mut layers = vec![
            Layer::new(under, Point::ZERO, 0),
            Layer::new(over, Point::new(2, 1), 1),
        ];
        if grade {
            use crate::render::layer::ColorTransform;
            struct Id;
            impl crate::render::layer::CellShader for Id {
                fn shade(&self, _x: i32, _y: i32, _t: f32, c: Cell) -> Cell {
                    c
                }
            }
            // Exercise the setters, then return to identity.
            layers[1].set_blend(Blend::Additive);
            layers[1].set_color_transform(ColorTransform::Tint(Rgba::rgb(1, 2, 3), 0.7));
            layers[1].set_shader(Some(Box::new(Id)));
            layers[1].set_shader_t(3.5);
            layers[1].set_blend(Blend::Normal);
            layers[1].set_color_transform(ColorTransform::None);
            layers[1].set_shader(None);
        }
        comp.flatten(&mut f, &mut layers);
        // Serialize the frame through the presenter for byte comparison.
        let mut diff = super::super::diff::FrameDiff::new();
        let prev = Surface::new(Size::new(12, 3), Cell::EMPTY);
        let runs = diff.compute_full(&prev, &f).to_vec();
        let mut p = super::super::present::Presenter::new();
        let mut out = Vec::new();
        p.emit(
            &runs,
            &f,
            &super::super::present::PresentCaps::FULL,
            &mut out,
        );
        out
    };
    assert_eq!(
        build(false),
        build(true),
        "identity grade must be invisible"
    );
}
