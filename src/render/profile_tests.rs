//! Phase-split profiling for the shader pipeline (RT6-3): where does a
//! 200x60 + Shimmer frame actually spend its time — flatten (compose +
//! shade), diff, or present? Run explicitly, release, and ALONE (perf
//! numbers from co-scheduled parallel tests measure contention, not
//! code — the RT6-3 lesson):
//!
//! ```text
//! cargo test --release --lib render::profile -- --ignored --nocapture --test-threads=1
//! ```

use std::time::Instant;

use crate::anim::shaders::Shimmer;
use crate::base::{Point, Rgba, Size};
use crate::render::cell::Cell;
use crate::render::diff::FrameDiff;
use crate::render::present::{PresentCaps, Presenter};
use crate::render::style::Style;
use crate::render::surface::Surface;
use crate::render::{Compositor, Layer};

fn median_us(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

/// One measured pass per phase, isolated by re-running the pipeline up
/// to the phase under measurement each iteration.
#[test]
#[ignore]
fn profile_shader_pipeline_phases_200x60() {
    let size = Size::new(200, 60);
    let caps = PresentCaps::FULL;
    let mut surface = Surface::new(size, Cell::EMPTY);
    for y in 0..size.h {
        surface.draw_text(
            0,
            y,
            "abcdefghij0123456789abcdefghij0123456789",
            Style::new()
                .fg(Rgba::rgb(200, 180, 40))
                .bg(Rgba::rgb(12, 14, 22)),
        );
    }
    let mut layers = vec![Layer::new(surface, Point::ZERO, 0)];
    layers[0].set_shader(Some(Box::new(Shimmer::default())));
    let mut comp = Compositor::new();
    let mut frame = Surface::new(size, Cell::EMPTY);
    let mut prev = Surface::new(size, Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out: Vec<u8> = Vec::with_capacity(1 << 20);

    // Warm the pipeline (first flatten pays pool adoption growth).
    comp.flatten(&mut frame, &mut layers);

    const RUNS: usize = 31;
    let (mut t_flatten, mut t_diff, mut t_present) = (
        Vec::with_capacity(RUNS),
        Vec::with_capacity(RUNS),
        Vec::with_capacity(RUNS),
    );
    for i in 0..RUNS {
        layers[0].set_shader_t((i + 1) as f32 * 0.033);

        let s = Instant::now();
        let damage: Vec<_> = comp.flatten(&mut frame, &mut layers).to_vec();
        t_flatten.push(s.elapsed().as_secs_f64() * 1e6);

        let s = Instant::now();
        let runs = diff.compute(&prev, &frame, &damage);
        t_diff.push(s.elapsed().as_secs_f64() * 1e6);

        let s = Instant::now();
        out.clear();
        presenter.emit(runs, &frame, &caps, &mut out);
        t_present.push(s.elapsed().as_secs_f64() * 1e6);

        prev.blit(&frame, frame.bounds(), Point::ZERO);
    }
    let f = median_us(&mut t_flatten);
    let d = median_us(&mut t_diff);
    let p = median_us(&mut t_present);
    eprintln!("phase medians over {RUNS} frames (200x60, Shimmer, full re-shade):");
    eprintln!("  flatten (compose+shade): {f:8.1} us");
    eprintln!("  diff:                    {d:8.1} us");
    eprintln!("  present:                 {p:8.1} us");
    eprintln!("  total:                   {:8.1} us", f + d + p);

    // Envelope sanity, not a budget: a debug run is legitimately slower;
    // this test exists for its printout.
    assert!(f + d + p > 0.0);
}
