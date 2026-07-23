//! Boot brandmark tests (split file, `#[path]`-included as
//! `brandmark::tests` — the file-size discipline).
//!
//! OWNER: GFX3D.

use super::*;
use crate::theme::default_theme;

/// R4-1 drift pin: the in-three reference params must equal the
/// boot::identity constants FIELD BY FIELD. Tests may look upward
/// (they are not part of the layer graph); src code must not —
/// this test is what makes the reference copy safe to exist.
#[test]
fn identity_drift_pin() {
    use crate::boot::identity as id;
    let p = BrandmarkParams::reference();
    assert_eq!(p.align_start_ms, id::PHASE_ALIGN_START_MS);
    assert_eq!(p.reveal_start_ms, id::PHASE_REVEAL_START_MS);
    assert_eq!(p.hold_start_ms, id::PHASE_HOLD_START_MS);
    assert_eq!(p.plane_stagger_ms, id::PLANE_STAGGER_MS);
    assert_eq!(p.plane_arrival_ms, id::PLANE_ARRIVAL_MS);
    assert_eq!(p.burst_at_ms, id::BURST_AT_MS);
    assert_eq!(p.burst_particles, id::BURST_PARTICLES);
    assert_eq!(p.burst_lifetime_ms, id::BURST_LIFETIME_MS);
    assert_eq!(p.afterglow_decay_per_100ms, id::AFTERGLOW_DECAY_PER_100MS);
    assert_eq!(p.ease_arrival, id::EASE_ARRIVAL);
    assert_eq!(p.ease_settle, id::EASE_SETTLE);
    assert_eq!(p.ease_tracking, id::EASE_TRACKING);
    assert_eq!(p.ease_fade, id::EASE_FADE);
    assert_eq!(p.camera_yaw_deg, id::CAMERA_YAW_DEG);
    assert_eq!(p.camera_pitch_deg, id::CAMERA_PITCH_DEG);
    assert_eq!(p.camera_dolly, id::CAMERA_DOLLY);
    assert_eq!(p.ramp, id::BRAND_RAMP);
    assert_eq!(p.field, id::BRAND_FIELD);
    assert_eq!(p.wordmark, id::WORDMARK);
    assert_eq!(p.tagline, id::TAGLINE);
    assert_eq!(p.skip_hint, id::SKIP_HINT);
    assert_eq!(p.wordmark_tracking, id::WORDMARK_TRACKING);
    // And the local ramp interpolation matches identity's.
    for k in [0.0f32, 0.2, 0.5, 0.77, 1.0] {
        assert_eq!(ramp_color(&p.ramp, k), id::brand_ramp(k), "ramp at {k}");
    }
}

fn coverage(surface: &Surface) -> usize {
    let mut n = 0;
    for y in 0..surface.size().h {
        for x in 0..surface.size().w {
            if let Some(c) = surface.get(x, y) {
                if !c.bg.is_transparent() || !c.fg.is_transparent() {
                    n += 1;
                }
            }
        }
    }
    n
}

#[test]
fn honors_requested_size_and_resizes() {
    let theme = default_theme();
    let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let s = r.render(0.5, Size::new(100, 30), theme);
    assert_eq!(s.size(), Size::new(100, 30));
    let s = r.render(0.6, Size::new(80, 24), theme);
    assert_eq!(s.size(), Size::new(80, 24));
    let s = r.render(0.7, Size::new(0, 0), theme);
    assert_eq!(s.size(), Size::new(0, 0)); // degenerate: no panic
}

#[test]
fn storyboard_beats_appear() {
    let theme = default_theme();
    let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    // Quiet beat: before any plane has traveled, the frame is
    // ground + vignette only (no mark cells brighter than ground).
    let s = r.render(0.0, Size::new(100, 30), theme);
    assert!(coverage(s) > 0, "vignette paints the ground");

    // Mid-flight: the mark is visible.
    let mut r2 = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let s = r2.render(1.0, Size::new(100, 30), theme);
    let mark_cells = mark_cell_count(s, theme);
    assert!(
        mark_cells > 30,
        "mark visible at t=1.0 ({mark_cells} cells)"
    );

    // Reveal: the wordmark text row exists.
    let mut r3 = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let s = r3.render(1.9, Size::new(100, 30), theme);
    let row: String = row_text(s, s.size().h - 3);
    assert!(row.contains('A'), "wordmark visible: {row:?}");
    let hint_row = row_text(s, s.size().h - 1);
    assert!(hint_row.contains("skip"), "skip hint: {hint_row:?}");
}

#[test]
fn deterministic_frames_fresh_renderers() {
    // Same t + size + theme through FRESH renderers = same bytes
    // (the trail makes SEQUENTIAL frames history-dependent by
    // design; determinism is per fresh start, which is what the
    // player restarts give).
    let theme = default_theme();
    let mut a = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let mut b = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let (sa, sb) = (
        frame_dump(a.render(1.2, Size::new(60, 20), theme)),
        frame_dump(b.render(1.2, Size::new(60, 20), theme)),
    );
    assert_eq!(sa, sb);
}

#[test]
fn camera_sweep_changes_the_frame() {
    let theme = default_theme();
    let mut a = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let mut b = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let early = frame_dump(a.render(0.5, Size::new(60, 20), theme));
    let late = frame_dump(b.render(1.3, Size::new(60, 20), theme));
    assert_ne!(early, late);
}

/// Budget: one frame at the typical 100x30 must fit comfortably in
/// a 30 fps cadence next to diff+present.
/// `cargo test --release -- --ignored perf_brandmark`
#[test]
#[ignore = "perf budget; run explicitly in release"]
fn perf_brandmark_100x30() {
    let theme = default_theme();
    let mut r = BrandmarkRenderer::with_params(BrandmarkParams::reference());
    let m = crate::testing::bench::time_median("brandmark_100x30", 3, 5, 20, |i| {
        let t = (i % 60) as f32 / 30.0;
        let s = r.render(t, Size::new(100, 30), theme);
        crate::testing::bench::sink(s.size());
    });
    eprintln!("{}", m.report());
    m.assert_under(std::time::Duration::from_millis(8));
}

fn row_text(s: &Surface, y: i32) -> String {
    (0..s.size().w)
        .map(|x| {
            s.get(x, y)
                .and_then(|c| s.glyph_str(c).chars().next())
                .unwrap_or(' ')
        })
        .collect()
}

fn mark_cell_count(s: &Surface, theme: &Theme) -> usize {
    let bg = theme.tokens.bg;
    let mut n = 0;
    for y in 0..s.size().h {
        for x in 0..s.size().w {
            if let Some(c) = s.get(x, y) {
                // Mark cells carry brand color well away from the
                // ground (vignette stays near it).
                let d = (c.bg.r as i32 - bg.r as i32).abs()
                    + (c.bg.g as i32 - bg.g as i32).abs()
                    + (c.bg.b as i32 - bg.b as i32).abs();
                if d > 120 {
                    n += 1;
                }
            }
        }
    }
    n
}

fn frame_dump(s: &Surface) -> Vec<(String, Rgba, Rgba)> {
    let mut out = Vec::new();
    for y in 0..s.size().h {
        for x in 0..s.size().w {
            let c = s.get(x, y).unwrap();
            out.push((s.glyph_str(c).to_string(), c.fg, c.bg));
        }
    }
    out
}
