//! Rendering core: cells, surfaces, layered compositor, frame diff and
//! ANSI presentation.
//!
//! OWNER: RENDER agent. Design notes and research: docs/design/render.md.
//!
//! Pipeline per frame (the app runtime drives it; shown here in full as
//! a compiling example — `FrameDiff` and `Presenter` are reused across
//! frames so the steady state allocates nothing):
//!
//! ```
//! use abstracttui::base::{Point, Rect, Size};
//! use abstracttui::render::{
//!     Cell, Compositor, FrameDiff, Layer, PresentCaps, Presenter, Style, Surface,
//! };
//!
//! let size = Size::new(80, 24);
//! let mut layers = vec![Layer::new(Surface::new(size, Cell::EMPTY), Point::ZERO, 0)];
//! let (mut comp, mut diff, mut presenter) =
//!     (Compositor::new(), FrameDiff::new(), Presenter::new());
//! let mut frame = Surface::new(size, Cell::EMPTY); // compositor back buffer
//! let mut prev = Surface::new(size, Cell::EMPTY);  // what the terminal shows
//! let mut out: Vec<u8> = Vec::new();
//!
//! layers[0].surface_mut().draw_text(0, 0, "status: ready", Style::new());
//!
//! let damage = comp.flatten(&mut frame, &mut layers).to_vec();
//! let runs = diff.compute(&prev, &frame, &damage);
//! presenter.emit(runs, &frame, &PresentCaps::FULL, &mut out);
//! // `out` now holds deterministic ANSI bytes; write them to the tty in
//! // ONE flush, then remember the frame:
//! assert!(!out.is_empty());
//! prev.blit(&frame, Rect::from_size(size), Point::ZERO);
//! ```
//!
//! Invariants the whole pipeline leans on:
//! - Wide glyphs are leader + continuation pairs; every write path repairs
//!   clobbered halves (`surface.rs`), the compositor re-mirrors pairs after
//!   blending, the diff never emits a run starting mid-pair, the presenter
//!   never prints half a glyph.
//! - Damage is an over-approximation of change; equality is re-checked by
//!   the diff, so stale damage costs time, never correctness.
//! - `Rgba` alpha means transparency while compositing and "terminal
//!   default color" once a frame reaches the presenter.
//! - Steady-state frames allocate nothing: `FrameDiff` and `Presenter` own
//!   reusable scratch; surfaces only allocate on resize or new long
//!   grapheme clusters.

mod attrs;
pub mod bridge;
pub mod cell;
pub mod color;
pub mod compositor;
pub mod diff;
pub mod layer;
pub mod md;
pub mod paint;
pub mod present;
pub mod rich;
pub mod scroll;
mod sgr;
pub mod snapshot;
pub mod style;
pub mod surface;
mod surface_ops;
mod validate;

pub use cell::{Attrs, Cell, Glyph, GlyphPool, GLYPH_INLINE_CAP, GLYPH_POOL_CAP};
pub use compositor::Compositor;
pub use diff::{FrameDiff, Run};
pub use layer::{Blend, CellShader, ColorTransform, Layer};
pub use present::{ColorDepth, PresentCaps, Presenter, PresenterOpts};
pub use rich::{HAlign, RichLine, RichText, Span};
pub use scroll::{ScrolledRuns, Shift};
pub use snapshot::{snapshot, snapshot_styles};
pub use style::Style;
pub use surface::Surface;

#[cfg(test)]
mod pipeline_tests {
    //! End-to-end: layers -> flatten -> diff -> bytes, the seam REDTEAM's
    //! VT model will replay.

    use super::*;
    use crate::base::{Point, Rect, Rgba, Size};

    #[test]
    fn full_pipeline_small_damage_small_bytes() {
        let size = Size::new(80, 24);
        let mut frame = Surface::new(size, Cell::EMPTY);
        let mut prev = Surface::new(size, Cell::EMPTY);
        let mut comp = Compositor::new();
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut layers = vec![Layer::new(Surface::new(size, Cell::EMPTY), Point::ZERO, 0)];

        // Frame 1: full paint.
        layers[0]
            .surface_mut()
            .draw_text(0, 0, "status: ready", Style::new());
        let mut out = Vec::new();
        let damage = comp.flatten(&mut frame, &mut layers).to_vec();
        let runs = diff.compute(&prev, &frame, &damage).to_vec();
        presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
        assert!(!out.is_empty());
        prev.blit(&frame, Rect::from_size(size), Point::ZERO);

        // Frame 2: one cell blinks. Damage, runs and bytes all stay tiny.
        layers[0]
            .surface_mut()
            .draw_text(8, 0, "R", Style::new().fg(Rgba::rgb(255, 0, 0)));
        let damage = comp.flatten(&mut frame, &mut layers).to_vec();
        let runs = diff.compute(&prev, &frame, &damage).to_vec();
        assert_eq!(runs.len(), 1, "one damaged cell, one run: {runs:?}");
        assert!(
            runs[0].len <= 3,
            "run stays near the changed cell: {runs:?}"
        );
        out.clear();
        presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
        assert!(out.len() < 80, "small change, small bytes: {}", out.len());

        // Frame 3: nothing changed — nothing moves through the pipeline.
        prev.blit(&frame, Rect::from_size(size), Point::ZERO);
        let damage = comp.flatten(&mut frame, &mut layers).to_vec();
        assert!(damage.is_empty());
        let runs = diff.compute(&prev, &frame, &damage).to_vec();
        assert!(runs.is_empty());
        out.clear();
        presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
        assert!(out.is_empty(), "idle frames emit zero bytes");
    }

    #[test]
    fn wide_glyphs_survive_the_whole_pipeline() {
        let size = Size::new(10, 2);
        let mut frame = Surface::new(size, Cell::EMPTY);
        let prev = Surface::new(size, Cell::EMPTY);
        let mut comp = Compositor::new();
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut layers = vec![Layer::new(Surface::new(size, Cell::EMPTY), Point::ZERO, 0)];
        layers[0]
            .surface_mut()
            .draw_text(0, 0, "世界", Style::new());

        let damage = comp.flatten(&mut frame, &mut layers).to_vec();
        let runs = diff.compute(&prev, &frame, &damage).to_vec();
        let mut out = Vec::new();
        presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
        let text = String::from_utf8_lossy(&out);
        assert_eq!(text.matches('世').count(), 1);
        assert_eq!(text.matches('界').count(), 1);
    }
}
