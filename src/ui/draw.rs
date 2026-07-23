//! Paint walk: pre-order draw of the instance tree onto a
//! `StyledCanvas`, damage-scoped repaints, and `clip_overflow` nesting.
//! Split from `tree.rs` (file budget); same `impl UiTree` surface.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::reactive::enter_draw_phase;

use super::canvas::{ClippedCanvas, StyledCanvas};
use super::tree::{InstPayload, UiTree, ViewId};
use super::view::DrawFn;

impl UiTree {
    /// Paint the whole tree, parents under children (pre-order). Used by
    /// headless rendering and full repaints; the frame loop prefers
    /// [`UiTree::draw_damaged`].
    pub fn draw(&mut self, canvas: &mut dyn StyledCanvas) {
        self.layout();
        // RT1-2: draw closures are pure over captured data — the guard
        // makes a tracked signal read in any of them a debug panic
        // (release: counted, see reactive::diagnostics).
        let _phase = enter_draw_phase();
        let root = self.core.borrow().root;
        if let Some(root) = root {
            let vp = Rect::from_size(self.core.borrow().viewport);
            self.draw_node(root, canvas, vp);
        }
    }

    /// Paint only the parts of the tree intersecting `damage` (screen
    /// coords). Widgets whose rect intersects a damaged region repaint in
    /// full, clipped to the region — over-approximation is fine (the
    /// frame diff re-checks equality; identical repainted cells emit no
    /// bytes), missing a region is the bug this API exists to prevent.
    pub fn draw_damaged(&mut self, canvas: &mut dyn StyledCanvas, damage: &[Rect]) {
        self.layout();
        let _phase = enter_draw_phase();
        let root = self.core.borrow().root;
        let Some(root) = root else { return };
        for &rect in damage {
            if rect.is_empty() {
                continue;
            }
            let mut clipped = ClippedCanvas::new(canvas, rect);
            self.draw_node(root, &mut clipped, rect);
        }
    }

    /// Pre-order paint; children paint over parents, later siblings over
    /// earlier ones. A node whose layout style sets `clip_overflow` wraps
    /// its children's painting in a clip to its CONTENT box (padding
    /// excluded — a scroll container's scrollbar gutter lives in padding
    /// and stays unclipped).
    ///
    /// Recursive by necessity: nested clips are nested `ClippedCanvas`
    /// borrows, which cannot be flattened onto an explicit stack. Depth
    /// equals VIEW NESTING depth (tens, not thousands) — unlike the
    /// reactive graph walks, which stay iterative.
    fn draw_node(&self, id: ViewId, canvas: &mut dyn StyledCanvas, clip: Rect) {
        enum Paint {
            Draw(Rc<RefCell<DrawFn>>, Rect),
            Text(String, Rect, Rgba),
            None,
        }
        let (paint, children, child_clip) = {
            let core = self.core.borrow();
            let Some(inst) = core.insts.get(id.0) else {
                return;
            };
            let rect = core.layout.rect(inst.layout);
            // Skip subtrees fully outside the clip. Absolute children can
            // escape their parent's rect; they re-enter via their own
            // geometry damage when they move (documented conservative
            // skip; a clipping ancestor bounds them anyway). Exception:
            // a `probe_when_culled` node keeps its OWN draw (measurement
            // readback must not starve when the content scrolls fully
            // out — first-app/0281); its children still cull below.
            if !rect.intersects(clip) && !rect.is_empty() && !inst.probe_when_culled {
                return;
            }
            // A zero-AREA node paints nothing (the fusion class,
            // gateway-console field incident 2026-07-24): empty rects
            // never intersect anything, so they fell through the cull
            // above and their draw closures ran with degenerate rects
            // — a hand-rolled closure that clips on one axis only (a
            // title bar truncating horizontally, then painting "its"
            // row) smeared that row onto whichever sibling owned the
            // y after flex crushed the bar to zero. Collapse must be
            // CLEAN ABSENCE; the zero-collapse notice (solver-side,
            // layout/solve.rs) names it. Same exemption as the cull:
            // a `probe_when_culled` measurement probe must READ the
            // collapse (a scroll extent going to zero is a reading
            // the offset repair depends on — first-app/0281).
            // Children still walk below: their rects are truthful and
            // an empty parent's children can be non-empty (absolute
            // children size independently; flow children with an
            // explicit main-axis min hold their extent).
            let paint = if rect.is_empty() && !inst.probe_when_culled {
                Paint::None
            } else {
                match &inst.payload {
                    InstPayload::Element { draw: Some(d), .. } => Paint::Draw(d.clone(), rect),
                    InstPayload::Text { content } => {
                        Paint::Text(content.clone(), rect, core.text_fg)
                    }
                    _ => Paint::None,
                }
            };
            let child_clip = match core.layout.style(inst.layout) {
                Some(style) if style.clips_children() => {
                    let content = Rect::new(
                        rect.x + style.padding.left,
                        rect.y + style.padding.top,
                        (rect.w - style.padding.horizontal()).max(0),
                        (rect.h - style.padding.vertical()).max(0),
                    );
                    Some(clip.intersect(content))
                }
                _ => None,
            };
            (paint, inst.children.clone(), child_clip)
        };
        match paint {
            // User draw code runs with the core released. Reactive reads
            // here are the RT1-2 violation the guard reports; deliberate
            // peeks use get_untracked.
            Paint::Draw(draw, rect) => (draw.borrow_mut())(canvas, rect),
            Paint::Text(content, rect, fg) => {
                if !rect.is_empty() {
                    // Wrap-aware paint mirroring the measure callback:
                    // logical lines + width wrapping, rows clipped by the
                    // solved rect (a leaf squeezed below its measured
                    // height truncates instead of smearing one mega-row).
                    for (i, line) in crate::text::wrap(&content, rect.w).iter().enumerate() {
                        let y = rect.y + i as i32;
                        if y >= rect.bottom() {
                            break;
                        }
                        canvas.print(Point::new(rect.x, y), line, fg, Rgba::TRANSPARENT);
                    }
                }
            }
            Paint::None => {}
        }
        match child_clip {
            Some(cc) => {
                if cc.is_empty() {
                    return; // everything below is scrolled/clipped away
                }
                let mut wrapped = ClippedCanvas::new(canvas, cc);
                for child in children {
                    self.draw_node(child, &mut wrapped, cc);
                }
            }
            None => {
                for child in children {
                    self.draw_node(child, canvas, clip);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::base::{Rect, Size};
    use crate::layout::Style as LayoutStyle;
    use crate::reactive::create_root;
    use crate::ui::{BufferCanvas, Element, UiTree};

    /// The fusion fix at its narrowest: a zero-area node's own draw
    /// closure never runs (a crushed bar must not smear a sibling's
    /// row), while a `probe_when_culled` node's closure STILL runs on
    /// an empty rect — a measurement probe must read the collapse
    /// (scroll extents going to zero, first-app/0281).
    #[test]
    fn empty_rect_skips_draw_but_probes_still_read() {
        let calls: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let probe_rect: Rc<Cell<Option<Rect>>> = Rc::new(Cell::new(None));
        let mut tree = UiTree::new(Size::new(10, 4));
        let (root, ()) = create_root(|cx| {
            let c = calls.clone();
            let p = probe_rect.clone();
            let plain = Element::new()
                .style(LayoutStyle::default().w(0).h(0))
                .draw(move |_, _| c.set(c.get() + 1));
            let probe = Element::new()
                .style(LayoutStyle::default().w(0).h(0))
                .draw(move |_, rect| p.set(Some(rect)))
                .probe_when_culled();
            let view = Element::new()
                .child(plain.build())
                .child(probe.build())
                .build();
            tree.mount(cx, view);
        });
        tree.layout();
        let mut canvas = BufferCanvas::new(Size::new(10, 4));
        tree.draw(&mut canvas);
        assert_eq!(calls.get(), 0, "zero-area draw closures must not run");
        assert_eq!(
            probe_rect.get(),
            Some(Rect::new(0, 0, 0, 0)),
            "a probe reads its empty rect (the collapse IS the reading)"
        );
        root.dispose();
    }
}
