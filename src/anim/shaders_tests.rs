//! Built-in shader tests: determinism goldens at fixed sample points
//! (REDTEAM extends these into full-frame goldens) plus the behavioral
//! envelopes each effect promises.

use super::*;
use crate::base::{Point, Size};
use crate::render::cell::Attrs;
use crate::render::Style;
use crate::render::Surface;

fn ink_cell(fg: Rgba) -> Cell {
    // Build through a surface so the glyph path is the real one.
    let mut s = Surface::new(Size::new(2, 1), Cell::EMPTY);
    s.draw_text(0, 0, "x", Style::new().fg(fg).bg(Rgba::rgb(10, 10, 10)));
    *s.get(0, 0).unwrap()
}

#[test]
fn wave_is_triangle_with_exact_landmarks() {
    assert_eq!(wave(0.0), 1.0);
    assert_eq!(wave(0.25), 0.0);
    assert_eq!(wave(0.5), -1.0);
    assert_eq!(wave(0.75), 0.0);
    assert_eq!(wave(1.0), 1.0, "period 1");
    assert_eq!(wave(-0.5), wave(0.5), "negative phases wrap");
}

#[test]
fn cell_hash_is_stable_uniform_and_seeded() {
    // Golden pins: exact bit-stable values are the cross-platform
    // contract REDTEAM replays (integer avalanche + /2^24, no libm).
    assert_eq!(cell_hash(0, 0, 0), 0.0, "all-zero input avalanches to 0");
    assert_eq!(cell_hash(3, 7, 42), 0.725_178_1);
    assert_eq!(cell_hash(80, 24, 0xDEAD), 0.470_138_55);
    let a = cell_hash(3, 7, 42);
    assert_eq!(a, cell_hash(3, 7, 42), "pure function");
    assert_ne!(cell_hash(3, 7, 42), cell_hash(3, 7, 43), "seed matters");
    assert_ne!(
        cell_hash(3, 7, 42),
        cell_hash(7, 3, 42),
        "x/y not symmetric"
    );
    // Rough uniformity: mean of a grid sits near 0.5.
    let mut sum = 0.0;
    for y in 0..32 {
        for x in 0..32 {
            sum += cell_hash(x, y, 1);
        }
    }
    let mean = sum / 1024.0;
    assert!((mean - 0.5).abs() < 0.05, "hash mean {mean}");
}

#[test]
fn shimmer_scales_ink_not_ground_golden_points() {
    let sh = Shimmer {
        speed: 1.0,
        amplitude: 0.5,
        wavelength: 8.0,
    };
    let cell = ink_cell(Rgba::rgb(100, 100, 100));
    // t=0, x+y=0: phase 0 -> wave 1 -> factor 1.5.
    let peak = sh.shade(0, 0, 0.0, cell);
    assert_eq!((peak.fg.r, peak.fg.g, peak.fg.b), (150, 150, 150));
    assert_eq!(peak.bg, cell.bg, "background never shimmers");
    // Half period later the same cell dims: factor 0.5.
    let dip = sh.shade(0, 0, 0.5, cell);
    assert_eq!((dip.fg.r, dip.fg.g, dip.fg.b), (50, 50, 50));
    // Quarter period: factor 1.0 exactly (triangle zero crossing).
    let flat = sh.shade(0, 0, 0.25, cell);
    assert_eq!(flat.fg, cell.fg);
    // Default-colored ink passes through.
    let default_ink = Cell::EMPTY;
    assert_eq!(sh.shade(0, 0, 0.0, default_ink).fg, Rgba::TRANSPARENT);
}

#[test]
fn scanline_fade_reveals_top_down_golden_points() {
    let sf = ScanlineFade {
        duration: 1.0,
        rows: 10,
    };
    let cell = ink_cell(Rgba::rgb(200, 0, 0));
    // t=0.5: line at row 5. Row 2 fully shown, row 8 fully hidden.
    assert_eq!(sf.shade(0, 2, 0.5, cell), cell);
    let hidden = sf.shade(0, 8, 0.5, cell);
    assert!(
        hidden.glyph.is_empty() && hidden.bg.is_transparent(),
        "below the line: nothing"
    );
    // Row exactly under the line blends: coverage 0.5 at row 5... use row
    // 4 (coverage 1.0) and row 5 (coverage 0.0) boundary; fractional case
    // at t=0.45 -> line 4.5 -> row 4 coverage 0.5: bg half alpha, glyph
    // present (coverage not < 0.5).
    let half = sf.shade(0, 4, 0.45, cell);
    assert_eq!(half.bg.a, 128, "half-covered row halves bg alpha: {half:?}");
    assert!(!half.glyph.is_empty(), "glyph pops at half coverage");
    // Just under half coverage: ground only.
    let low = sf.shade(0, 4, 0.42, cell);
    assert!(low.glyph.is_empty(), "below half coverage the glyph waits");
    assert!(low.bg.a > 0, "but the ground is arriving");
    // t past duration: everything shown.
    assert_eq!(sf.shade(0, 9, 2.0, cell), cell);
}

#[test]
fn hue_drift_pulses_toward_rotation_and_back() {
    let hd = HueDrift {
        speed: 1.0,
        strength: 1.0,
    };
    let cell = ink_cell(Rgba::rgb(200, 40, 0));
    // t=0: wave peak -> pulse 1 -> full rotation (g,b,r).
    let peak = hd.shade(0, 0, 0.0, cell);
    assert_eq!((peak.fg.r, peak.fg.g, peak.fg.b), (40, 0, 200));
    // Half period: pulse 0 -> identity.
    let back = hd.shade(0, 0, 0.5, cell);
    assert_eq!(back.fg, cell.fg);
    // Subtle strength stays near the original.
    let subtle = HueDrift {
        speed: 1.0,
        strength: 0.2,
    };
    let s = subtle.shade(0, 0, 0.0, cell);
    assert!(
        s.fg.r > 150,
        "20% drift keeps the color recognizable: {:?}",
        s.fg
    );
    assert_eq!(hd.shade(0, 0, 0.0, cell).bg, cell.bg, "ground untouched");
}

#[test]
fn dissolve_is_monotone_seeded_and_total_at_the_ends() {
    let d = Dissolve {
        duration: 1.0,
        seed: 7,
    };
    let cell = ink_cell(Rgba::rgb(1, 2, 3));
    // t=0: nothing; t=duration: everything.
    for x in 0..8 {
        assert!(d.shade(x, 0, 0.0, cell).glyph.is_empty(), "t=0 hides all");
        assert_eq!(d.shade(x, 0, 1.0, cell), cell, "t=duration shows all");
    }
    // Monotone per cell: once visible, stays visible as t grows.
    for x in 0..16 {
        for y in 0..16 {
            let mut seen = false;
            for step in 0..=10 {
                let t = step as f32 / 10.0;
                let visible = !d.shade(x, y, t, cell).glyph.is_empty();
                assert!(!seen || visible, "cell ({x},{y}) flickered at t={t}");
                seen = visible;
            }
        }
    }
    // Progresses gradually: at t=0.5 roughly half the cells are visible.
    let mut visible = 0;
    for x in 0..32 {
        for y in 0..32 {
            if !d.shade(x, y, 0.5, cell).glyph.is_empty() {
                visible += 1;
            }
        }
    }
    assert!(
        (300..=700).contains(&visible),
        "≈50% at midpoint, saw {visible}/1024"
    );
    // Different seeds give different masks.
    let d2 = Dissolve {
        duration: 1.0,
        seed: 8,
    };
    let same: usize = (0..32)
        .map(|x| {
            usize::from(
                d.shade(x, 0, 0.5, cell).glyph.is_empty()
                    == d2.shade(x, 0, 0.5, cell).glyph.is_empty(),
            )
        })
        .sum();
    assert!(same < 32, "seeds must change the pattern");
}

#[test]
fn shaders_preserve_attrs_and_are_pure() {
    let mut cell = ink_cell(Rgba::rgb(9, 9, 9));
    cell.attrs = Attrs::BOLD | Attrs::UNDERLINE;
    let shaders: [&dyn CellShader; 7] = [
        &Shimmer::default(),
        &ScanlineFade {
            duration: 2.0,
            rows: 24,
        },
        &HueDrift::default(),
        &Dissolve {
            duration: 1.0,
            seed: 1,
        },
        &Pulse::default(),
        &Sweep::default(),
        &Rainbow::default(),
    ];
    for sh in shaders {
        let a = sh.shade(5, 5, 0.37, cell);
        let b = sh.shade(5, 5, 0.37, cell);
        assert_eq!(a, b, "identical inputs, identical outputs");
        if !a.glyph.is_empty() {
            assert_eq!(a.attrs, cell.attrs, "attrs ride through visible cells");
        }
    }
    let _ = Point::ZERO;
}

#[test]
fn pulse_breathes_uniformly_and_rests_calm() {
    let p = Pulse {
        speed: 1.0,
        amplitude: 0.5,
    };
    let cell = ink_cell(Rgba::rgb(100, 100, 100));
    // Phase 0 (t=0): calm — factor exactly 1.
    let calm = p.shade(0, 0, 0.0, cell);
    assert_eq!(calm.fg, cell.fg, "rest state is the unshaded look");
    // Half period: peak (+50%).
    let peak = p.shade(0, 0, 0.5, cell);
    assert_eq!((peak.fg.r, peak.fg.g, peak.fg.b), (150, 150, 150));
    // Spatially uniform: any cell agrees.
    assert_eq!(p.shade(40, 12, 0.5, cell).fg, peak.fg);
    assert_eq!(peak.bg, cell.bg, "pulse lights ink, not ground");
}

#[test]
fn sweep_band_brightens_with_feather_golden_points() {
    let sw = Sweep {
        period: 1.0,
        band: 4.0,
        boost: 1.0,
        travel: 40.0,
    };
    let cell = ink_cell(Rgba::rgb(100, 100, 100));
    // t=0.5 -> center at x+y = 20. Exact center: full boost.
    let center = sw.shade(20, 0, 0.5, cell);
    assert_eq!(center.fg.r, 200, "{:?}", center.fg);
    // One column off: 50% feather.
    let off1 = sw.shade(19, 0, 0.5, cell);
    assert_eq!(off1.fg.r, 150);
    // Outside the band: untouched (and bg too).
    let out = sw.shade(10, 0, 0.5, cell);
    assert_eq!(out, cell);
    // Diagonal: (x, y) contributes as x+y.
    assert_eq!(sw.shade(10, 10, 0.5, cell).fg, center.fg);
}

#[test]
fn vignette_dims_edges_not_center_golden_points() {
    let v = Vignette {
        size: (40, 10),
        inner: 0.3,
        strength: 0.6,
    };
    let cell = {
        let mut c = ink_cell(Rgba::rgb(100, 100, 100));
        c.bg = Rgba::rgb(50, 50, 50);
        c
    };
    // Dead center: untouched.
    let center = v.shade(20, 5, 0.0, cell);
    assert_eq!(center, cell);
    // Far corner: full strength (40% brightness).
    let corner = v.shade(0, 0, 0.0, cell);
    assert!(
        corner.fg.r <= 42 && corner.fg.r >= 38,
        "corner dimmed: {:?}",
        corner.fg
    );
    assert!(corner.bg.r < 25, "ground dims too: {:?}", corner.bg);
    // Monotone outward along a row.
    let a = v.shade(14, 5, 0.0, cell).fg.r;
    let b = v.shade(6, 5, 0.0, cell).fg.r;
    assert!(b <= a, "farther = dimmer: {a} vs {b}");
    // Pure function.
    assert_eq!(
        v.shade(3, 1, 9.0, cell),
        v.shade(3, 1, 0.0, cell),
        "t is unused"
    );
}

#[test]
fn gradient_reveal_wipes_directionally() {
    let g = GradientReveal {
        duration: 1.0,
        dir: (1.0, 0.0),
        travel: 20.0,
        softness: 2.0,
    };
    let cell = ink_cell(Rgba::rgb(200, 0, 0));
    // Halfway: front at -2 + 0.5*24 = 10.
    let shown = g.shade(5, 0, 0.5, cell);
    assert_eq!(shown, cell, "behind the front: fully shown");
    let hidden = g.shade(15, 0, 0.5, cell);
    assert!(
        hidden.glyph.is_empty() && hidden.bg.is_transparent(),
        "ahead: nothing"
    );
    // In the soft band: ground arriving, glyph not yet (coverage < .5).
    let soft = g.shade(9, 0, 0.5, cell);
    assert!(
        soft.bg.a > 0 && soft.bg.a < 255,
        "feathered ground: {:?}",
        soft.bg
    );
    // t=1: everything shown; t=0: nothing (front at -soft).
    assert_eq!(g.shade(19, 0, 1.0, cell), cell);
    assert!(g.shade(0, 0, 0.0, cell).glyph.is_empty());
    // Vertical direction wipes rows.
    let v = GradientReveal {
        duration: 1.0,
        dir: (0.0, 1.0),
        travel: 10.0,
        softness: 1.0,
    };
    assert_eq!(v.shade(0, 2, 0.6, cell), cell);
    assert!(v.shade(0, 9, 0.3, cell).glyph.is_empty());
}

// -- changed_region hints (cycle 7, RT6-3) --------------------------------

/// The hint contract, checked exhaustively over a frame-sized grid: for
/// every cell OUTSIDE the hint rect, `shade` must be bit-identical at
/// both clocks (over a spread of cell inks — reveal shaders synthesize
/// output from the cell, so one probe cell is not enough).
fn assert_stable_outside_hint(shader: &dyn CellShader, t0: f32, t1: f32, bounds: Rect) {
    let Some(hint) = shader.changed_region(t0, t1, bounds) else {
        return; // None = whole layer: nothing promised, nothing to check
    };
    let probes = [
        ink_cell(Rgba::rgb(200, 180, 40)),
        ink_cell(Rgba::rgb(10, 200, 255)),
        Cell::EMPTY,
    ];
    for y in bounds.y..bounds.bottom() {
        for x in bounds.x..bounds.right() {
            if hint.contains(Point::new(x, y)) {
                continue;
            }
            for c in probes {
                assert_eq!(
                    shader.shade(x, y, t0, c),
                    shader.shade(x, y, t1, c),
                    "({x},{y}) outside hint {hint:?} must be stable {t0}->{t1}"
                );
            }
        }
    }
}

#[test]
fn changed_region_hints_are_honest_for_every_builtin() {
    let bounds = Rect::new(0, 0, 60, 24);
    // Clock pairs covering mid-flight, settled, rewind and wrap shapes.
    let pairs = [
        (0.0, 0.033),
        (0.4, 0.45),
        (0.9, 1.1),
        (1.5, 2.0),
        (0.7, 0.3),
        (2.0, 5.0),
        (0.98, 1.02),
    ];
    let shaders: [&dyn CellShader; 8] = [
        &Shimmer::default(),
        &ScanlineFade {
            duration: 1.0,
            rows: 24,
        },
        &HueDrift::default(),
        &Dissolve {
            duration: 1.0,
            seed: 3,
        },
        &Pulse::default(),
        &Sweep {
            period: 1.0,
            band: 6.0,
            boost: 0.5,
            travel: 84.0,
        },
        &Rainbow::default(),
        &GradientReveal {
            duration: 1.0,
            dir: (1.0, 0.0),
            travel: 60.0,
            softness: 2.0,
        },
    ];
    for sh in shaders {
        for (t0, t1) in pairs {
            assert_stable_outside_hint(sh, t0, t1, bounds);
        }
    }
    // Vertical + reversed-direction wipes exercise the other axis branch.
    let down = GradientReveal {
        duration: 1.0,
        dir: (0.0, 1.0),
        travel: 24.0,
        softness: 1.0,
    };
    let left = GradientReveal {
        duration: 1.0,
        dir: (-1.0, 0.0),
        travel: 60.0,
        softness: 2.0,
    };
    for (t0, t1) in pairs {
        assert_stable_outside_hint(&down, t0, t1, bounds);
        assert_stable_outside_hint(&left, t0, t1, bounds);
    }
}

#[test]
fn settled_and_t_independent_shaders_tick_for_free() {
    let bounds = Rect::new(0, 0, 40, 10);
    // Vignette ignores t entirely.
    let v = Vignette {
        size: (40, 10),
        inner: 0.3,
        strength: 0.6,
    };
    assert_eq!(v.changed_region(0.0, 99.0, bounds), Some(Rect::ZERO));
    // A completed reveal: both clocks clamp to progress 1.
    let sf = ScanlineFade {
        duration: 1.0,
        rows: 10,
    };
    assert_eq!(sf.changed_region(1.0, 7.0, bounds), Some(Rect::ZERO));
    let g = GradientReveal {
        duration: 1.0,
        dir: (1.0, 0.0),
        travel: 40.0,
        softness: 2.0,
    };
    assert_eq!(g.changed_region(2.0, 3.0, bounds), Some(Rect::ZERO));
    let d = Dissolve {
        duration: 1.0,
        seed: 1,
    };
    assert_eq!(d.changed_region(1.0, 4.0, bounds), Some(Rect::ZERO));
    // Mid-flight the global effects honestly refuse to promise.
    assert_eq!(Shimmer::default().changed_region(0.0, 0.033, bounds), None);
    assert_eq!(d.changed_region(0.2, 0.4, bounds), None);
}

#[test]
fn banded_hints_bound_the_moving_region_not_the_layer() {
    let bounds = Rect::new(0, 0, 80, 24);
    // ScanlineFade moving from row 10 to row 12 (t 1.0->1.2 of rows=24
    // over duration 2.4... use duration 2.4): hint is a thin row band.
    let sf = ScanlineFade {
        duration: 2.4,
        rows: 24,
    };
    let hint = sf.changed_region(1.0, 1.2, bounds).expect("banded hint");
    assert!(hint.h <= 5, "moving scanline bounds a thin band: {hint:?}");
    assert!(hint.h >= 2, "band covers the swept rows: {hint:?}");
    // Sweep band: hint is narrower than the layer when the step is small.
    let sw = Sweep {
        period: 8.0,
        band: 6.0,
        boost: 0.5,
        travel: 104.0,
    };
    let hint = sw
        .changed_region(0.0, 0.033 / 2.0, bounds)
        .expect("slab hint");
    assert!(
        hint.w < bounds.w,
        "small sweep step bounds below the layer: {hint:?}"
    );
    // Axis wipe: slab in x only.
    let g = GradientReveal {
        duration: 2.0,
        dir: (1.0, 0.0),
        travel: 80.0,
        softness: 2.0,
    };
    let hint = g.changed_region(1.0, 1.05, bounds).expect("x slab");
    assert!(hint.w < 12 && hint.h == bounds.h, "x-slab hint: {hint:?}");
}

#[test]
fn rainbow_cycles_hue_preserving_level_and_seeds_by_position() {
    let rb = Rainbow {
        speed: 0.0,
        wavelength: 6.0,
        strength: 1.0,
    };
    let cell = ink_cell(Rgba::rgb(120, 120, 120));
    // hue at x+y=0 -> segment 0 -> pure red at the ink's level.
    let red = rb.shade(0, 0, 0.0, cell);
    assert_eq!((red.fg.r, red.fg.g, red.fg.b), (120, 0, 0));
    // Two cells later (hue 1/3): green segment.
    let green = rb.shade(2, 0, 0.0, cell);
    assert_eq!(
        (green.fg.r, green.fg.g, green.fg.b),
        (0, 120, 0),
        "{:?}",
        green.fg
    );
    // Level preservation: mean channel stays the ink's level per segment
    // corner (bright ink cycles bright, dark cycles dark).
    let dark = ink_cell(Rgba::rgb(30, 30, 30));
    let d = rb.shade(0, 0, 0.0, dark);
    assert_eq!(d.fg.r, 30);
    // Ground untouched; default ink untouched.
    assert_eq!(red.bg, cell.bg);
    assert_eq!(rb.shade(0, 0, 0.0, Cell::EMPTY).fg, Rgba::TRANSPARENT);
}
