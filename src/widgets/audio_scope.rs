//! audio_scope — a rolling waveform strip from level history
//! (backlog media-av/0620, the Meter's sibling).
//!
//! `AudioScope` renders a `Signal<Vec<f32>>` WINDOW as a braille
//! waveform through the chart substrate ([`super::LineChart`], axes
//! off). The ring lives in the producer lane by design: pair it with
//! [`crate::reactive::bounded_source`] and `OverflowPolicy::DropOldest`
//! — the source's retained window IS the scope's rolling history
//! (capacity = window length), with honest drop accounting riding along
//! for free. Any `Signal<Vec<f32>>` works; the scope draws whatever the
//! window currently holds.
//!
//! Zero idle cost, structurally: the scope owns NO clock and NO frame
//! tasks — when the signal stops changing, nothing re-renders and the
//! last frame stays (decay is the [`super::Meter`]'s business, not the
//! scope's). Non-finite samples render as gaps, never panics (the
//! chart substrate's data contract).
//!
//! ```ignore
//! let (tx, window, _stats) = bounded_source::<f32>(cx, 240, OverflowPolicy::DropOldest);
//! // producer: tx.send(level) per ~30ms chunk
//! AudioScope::new(window).range(0.0, 1.0).view(cx)
//! ```
//!
//! OWNER: INPUTAV (wave 3).

use crate::layout::Style as LayoutStyle;
use crate::reactive::{Scope, Signal};
use crate::ui::{dyn_view, View};

/// Rolling waveform strip over a level-history window signal.
pub struct AudioScope {
    window: Signal<Vec<f32>>,
    slot: usize,
    range: Option<(f32, f32)>,
    layout: Option<LayoutStyle>,
}

impl AudioScope {
    /// Draw the current contents of `window` as the waveform. The
    /// window signal is the rolling ring (see the module docs for the
    /// `bounded_source` pairing).
    pub fn new(window: Signal<Vec<f32>>) -> AudioScope {
        AudioScope {
            window,
            slot: 0,
            range: None,
            layout: None,
        }
    }

    /// Chart-ramp slot for the trace ink (clamped like
    /// [`crate::theme::TokenSet::chart`]).
    pub fn slot(mut self, slot: usize) -> AudioScope {
        self.slot = slot;
        self
    }

    /// Fixed value range (default: the window's own min/max — set an
    /// explicit range for a stable baseline, e.g. `0.0..1.0` for level
    /// streams or `-1.0..1.0` for sample streams).
    pub fn range(mut self, lo: f32, hi: f32) -> AudioScope {
        self.range = Some((lo, hi));
        self
    }

    /// Layout override (default: grow into the given region).
    pub fn layout(mut self, layout: LayoutStyle) -> AudioScope {
        self.layout = Some(layout);
        self
    }

    /// Build the reactive view: a `dyn_view` tracking the window signal
    /// and re-typesetting the braille trace only when data arrives.
    pub fn view(self, cx: Scope) -> View {
        let window = self.window;
        let slot = self.slot;
        let range = self.range;
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));
        dyn_view(layout, move || {
            let data = window.get(); // tracked: new frames re-render
                                     // Tokens resolve TRACKED inside the dyn scope (theme
                                     // switches restyle the trace).
            let t = super::theme_tokens(cx);
            // The chart contract colors series i with chart-ramp slot i
            // (chart.rs: "slot i for series i"); empty leading series
            // draw nothing, so they select the ramp slot honestly.
            let mut series = vec![Vec::new(); slot.min(7)];
            series.push(data);
            let chart = super::LineChart::new(series)
                .axes(false)
                .layout(LayoutStyle::default().grow(1.0));
            let chart = match range {
                Some((lo, hi)) => chart.range(lo, hi),
                None => chart,
            };
            chart.element(&t).build()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Point, Size};
    use crate::reactive::create_root;
    use crate::ui::{BufferCanvas, UiTree};

    fn braille_cells(canvas: &BufferCanvas, size: Size) -> usize {
        let mut n = 0;
        for y in 0..size.h {
            for x in 0..size.w {
                if let Some((ch, _, _)) = canvas.cell(Point::new(x, y)) {
                    if ('\u{2800}'..='\u{28FF}').contains(&ch) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn scope_draws_the_window_and_tracks_appends() {
        let size = Size::new(24, 4);
        let mut tree = UiTree::new(size);
        let (root, window) = create_root(|cx| {
            let window = cx.signal(Vec::<f32>::new());
            let view = AudioScope::new(window).range(0.0, 1.0).view(cx);
            tree.mount(cx, view);
            window
        });
        tree.layout();
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        assert_eq!(
            braille_cells(&canvas, size),
            0,
            "empty window draws nothing"
        );

        // Data arrives: the dyn re-renders and the trace appears.
        window.set((0..48).map(|i| i as f32 / 48.0).collect());
        crate::reactive::flush_effects();
        tree.layout();
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        assert!(
            braille_cells(&canvas, size) > 8,
            "a 48-sample ramp must paint a braille trace"
        );
        root.dispose();
    }

    #[test]
    fn slot_selects_the_chart_ramp_ink() {
        let size = Size::new(16, 2);
        let mut tree = UiTree::new(size);
        let (root, _) = create_root(|cx| {
            let window = cx.signal(vec![0.5f32; 32]);
            let view = AudioScope::new(window).slot(3).range(0.0, 1.0).view(cx);
            tree.mount(cx, view);
        });
        tree.layout();
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        let t = crate::theme::TokenSet::default();
        let ink = (0..size.h)
            .flat_map(|y| (0..size.w).map(move |x| Point::new(x, y)))
            .find_map(|p| match canvas.cell(p) {
                Some((ch, fg, _)) if ('\u{2800}'..='\u{28FF}').contains(&ch) => Some(fg),
                _ => None,
            })
            .expect("a trace cell");
        assert_eq!(ink, t.chart(3), "slot picks the ramp ink");
        root.dispose();
    }

    #[test]
    fn non_finite_samples_are_gaps_never_panics() {
        let size = Size::new(16, 2);
        let mut tree = UiTree::new(size);
        let (root, _) = create_root(|cx| {
            let window = cx.signal(vec![0.2, f32::NAN, 0.8, f32::INFINITY, 0.5]);
            let view = AudioScope::new(window).view(cx);
            tree.mount(cx, view);
        });
        tree.layout();
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas); // must not panic
        root.dispose();
    }

    #[test]
    fn quiet_signal_means_no_rerender() {
        // Structural zero-idle: with no signal change, the dyn region
        // never re-typesets — pinned by the tree's damage staying empty.
        let size = Size::new(16, 2);
        let mut tree = UiTree::new(size);
        let (root, _) = create_root(|cx| {
            let window = cx.signal(vec![0.1, 0.5, 0.9]);
            let view = AudioScope::new(window).view(cx);
            tree.mount(cx, view);
        });
        tree.layout();
        let mut canvas = BufferCanvas::new(size);
        tree.draw(&mut canvas);
        let _ = tree.take_damage();
        crate::reactive::flush_effects();
        assert!(
            tree.take_damage().is_empty(),
            "no data, no damage — the scope has no clock of its own"
        );
        root.dispose();
    }
}
