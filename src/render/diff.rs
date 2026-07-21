//! Frame diff: previous vs next surface, bounded by damage rects.
//!
//! Divergence from ratatui/OpenTUI (see docs/design/render.md §1.4): both
//! scan the full frame every time. Our reactivity layer knows what changed,
//! so the compositor hands us damage rects and we only touch those rows.
//! Correctness is *independent* of damage precision — damage is an
//! over-approximation contract ("every changed cell is inside some rect"),
//! and cells inside rects are still compared, so stale damage can only cost
//! time, never correctness. The empty damage list therefore means "nothing
//! changed", not "diff everything" (use [`FrameDiff::compute_full`]).
//!
//! Wide-glyph rules (the part REDTEAM should try to break):
//! - Continuation cells are first-class values that can never compare equal
//!   to a narrow cell, so prev-wide → next-narrow re-emits both columns
//!   without ratatui's `invalidated` counter.
//! - A changed leader whose continuation compares equal (same style, other
//!   wide glyph) emits just the leader — printing a wide glyph repaints
//!   both columns at the terminal, so the run may legally end between the
//!   halves.
//! - A run never *starts* at a continuation: if a changed continuation is
//!   found with no open run, the run is widened to include its leader
//!   (defense against invariant-violating input; for well-formed surfaces
//!   an equal leader implies an equal continuation).

use crate::base::{Rect, Size};

use super::cell::Cell;
use super::surface::Surface;

/// A contiguous span of changed cells in one row of the next frame.
/// The presenter reads cell content straight from the next surface.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// Row (frame coordinates).
    pub y: i32,
    /// First changed column.
    pub x: i32,
    /// Changed cell count (never starts or splits mid wide-pair).
    pub len: i32,
}

impl Run {
    /// One past the last changed column (`x + len`).
    pub const fn end(&self) -> i32 {
        self.x + self.len
    }
}

/// Reusable diff engine. Owns its scratch buffers so steady-state frames
/// allocate nothing (vision budget: zero heap allocation in diff/present).
/// Fields are `pub(super)` for the scroll-detection sibling
/// (`render::scroll`), which extends this type with `compute_scrolled` —
/// one scratch set, two diff strategies.
#[derive(Default)]
pub struct FrameDiff {
    /// Damaged column intervals, flattened to (y, x0, x1) and sorted; the
    /// merge pass then walks them row by row.
    pub(super) spans: Vec<(i32, i32, i32)>,
    pub(super) runs: Vec<Run>,
    /// Row fingerprints for shift detection (scroll.rs); unused (and
    /// unallocated) until `compute_scrolled` runs.
    pub(super) fp_prev: Vec<u64>,
    pub(super) fp_next: Vec<u64>,
}

impl FrameDiff {
    /// An empty diff engine (scratch grows to steady size on first use).
    pub fn new() -> FrameDiff {
        FrameDiff::default()
    }

    /// Diffs the full frame (resize, first paint, damage lost).
    pub fn compute_full<'a>(&'a mut self, prev: &Surface, next: &Surface) -> &'a [Run] {
        let all = [Rect::from_size(next.size())];
        self.compute(prev, next, &all)
    }

    /// Computes changed-cell runs within `damage`. If the two surfaces
    /// disagree on size the whole next frame is emitted (the previous
    /// content is unusable as a baseline).
    pub fn compute<'a>(&'a mut self, prev: &Surface, next: &Surface, damage: &[Rect]) -> &'a [Run] {
        self.runs.clear();
        self.spans.clear();

        let size = next.size();
        if size.is_empty() {
            return &self.runs;
        }
        if prev.size() != size {
            self.emit_all(next);
            return &self.runs;
        }

        self.collect_spans(damage, size);
        // Sort so each row's intervals are adjacent and left-to-right;
        // in-place unstable sort of Copy tuples allocates nothing.
        self.spans.sort_unstable();

        // Merge overlapping/touching intervals per row on the fly, then
        // scan each merged interval once.
        let mut i = 0;
        while i < self.spans.len() {
            let (y, x0, mut x1) = self.spans[i];
            i += 1;
            while i < self.spans.len() {
                let (ny, nx0, nx1) = self.spans[i];
                if ny != y || nx0 > x1 {
                    break;
                }
                x1 = x1.max(nx1);
                i += 1;
            }
            self.scan_interval(prev, next, y, x0, x1);
        }
        &self.runs
    }

    /// Every cell of `next` as runs (one per row).
    fn emit_all(&mut self, next: &Surface) {
        let size = next.size();
        for y in 0..size.h {
            self.runs.push(Run {
                y,
                x: 0,
                len: size.w,
            });
        }
    }

    /// Clips damage to the frame and explodes it into row intervals,
    /// expanded one column each side so a leader/continuation pair sliced
    /// by a rect edge is always examined together.
    pub(super) fn collect_spans(&mut self, damage: &[Rect], size: Size) {
        let bounds = Rect::from_size(size);
        for rect in damage {
            let r = rect.intersect(bounds);
            if r.is_empty() {
                continue;
            }
            let x0 = (r.x - 1).max(0);
            let x1 = (r.right() + 1).min(size.w);
            for y in r.y..r.bottom() {
                self.spans.push((y, x0, x1));
            }
        }
    }

    fn scan_interval(&mut self, prev: &Surface, next: &Surface, y: i32, x0: i32, x1: i32) {
        let prev_row = prev.row(y);
        let next_row = next.row(y);
        let mut run_start: Option<i32> = None;

        for x in x0..x1 {
            let a = &prev_row[x as usize];
            let b = &next_row[x as usize];
            let equal = cells_equal(prev, next, a, b);
            if equal {
                if let Some(start) = run_start.take() {
                    self.push_run(y, start, x);
                }
            } else if run_start.is_none() {
                // Never start a run on a continuation: widen to the leader
                // so the terminal receives a printable glyph. `x0 == 0`
                // cannot hold here for well-formed surfaces (no leaderless
                // continuation in column 0), but clamp anyway.
                let start = if b.is_continuation() {
                    (x - 1).max(0)
                } else {
                    x
                };
                run_start = Some(start);
            }
        }
        if let Some(start) = run_start {
            self.push_run(y, start, x1);
        }
    }

    /// Appends a run, merging with the previous one when they touch or
    /// overlap (interval expansion and leader-widening can create overlap).
    pub(super) fn push_run(&mut self, y: i32, x0: i32, x1: i32) {
        if x1 <= x0 {
            return;
        }
        if let Some(last) = self.runs.last_mut() {
            if last.y == y && x0 <= last.end() {
                let end = last.end().max(x1);
                last.len = end - last.x;
                return;
            }
        }
        self.runs.push(Run {
            y,
            x: x0,
            len: x1 - x0,
        });
    }
}

/// Cross-surface cell equality. The fast path is a plain bitwise compare —
/// valid whenever the glyph is inline and no hyperlink is involved. Pooled
/// glyphs and links carry surface-local ids, so those (rare) cells resolve
/// through their owning surface.
pub(super) fn cells_equal(prev: &Surface, next: &Surface, a: &Cell, b: &Cell) -> bool {
    if a.attrs != b.attrs || a.fg != b.fg || a.bg != b.bg || a.ul != b.ul {
        return false;
    }
    if !a.glyph.content_eq(&b.glyph, prev.pool(), next.pool()) {
        return false;
    }
    if a.link == 0 && b.link == 0 {
        return true;
    }
    prev.link_uri(a.link) == next.link_uri(b.link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Rgba, Size};
    use crate::render::style::Style;

    fn surf(w: i32, h: i32) -> Surface {
        Surface::new(Size::new(w, h), Cell::EMPTY)
    }

    fn full(s: &Surface) -> Vec<Rect> {
        vec![s.bounds()]
    }

    #[test]
    fn identical_surfaces_no_runs() {
        let a = surf(10, 3);
        let b = surf(10, 3);
        let mut diff = FrameDiff::new();
        assert!(diff.compute(&a, &b, &full(&a)).is_empty());
    }

    #[test]
    fn single_cell_change_single_tiny_run() {
        let a = surf(20, 5);
        let mut b = surf(20, 5);
        b.draw_text(7, 2, "x", Style::new());
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &[Rect::new(7, 2, 1, 1)]).to_vec();
        assert_eq!(runs, vec![Run { y: 2, x: 7, len: 1 }]);
    }

    #[test]
    fn damage_bounds_the_scan() {
        let a = surf(20, 5);
        let mut b = surf(20, 5);
        b.draw_text(0, 0, "changed", Style::new());
        b.draw_text(0, 4, "also changed", Style::new());
        let mut diff = FrameDiff::new();
        // Damage only covers row 0: row 4's change is (correctly, per the
        // over-approximation contract) not reported.
        let runs = diff.compute(&a, &b, &[Rect::new(0, 0, 20, 1)]).to_vec();
        assert!(runs.iter().all(|r| r.y == 0));
        assert_eq!(runs.len(), 1);
    }

    #[test]
    fn empty_damage_means_no_changes() {
        let a = surf(10, 2);
        let mut b = surf(10, 2);
        b.draw_text(0, 0, "hidden", Style::new());
        let mut diff = FrameDiff::new();
        assert!(diff.compute(&a, &b, &[]).is_empty());
    }

    #[test]
    fn equal_interior_splits_runs() {
        let mut a = surf(10, 1);
        a.draw_text(0, 0, "aaaaaaaaaa", Style::new());
        let mut b = surf(10, 1);
        b.draw_text(0, 0, "bbbaaabbbb", Style::new());
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &full(&a)).to_vec();
        assert_eq!(
            runs,
            vec![Run { y: 0, x: 0, len: 3 }, Run { y: 0, x: 6, len: 4 }]
        );
    }

    #[test]
    fn wide_to_narrow_re_emits_both_columns() {
        let mut a = surf(6, 1);
        a.draw_text(0, 0, "世", Style::new());
        let mut b = surf(6, 1);
        b.draw_text(0, 0, "ab", Style::new());
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &[Rect::new(0, 0, 2, 1)]).to_vec();
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 2 }]);
    }

    #[test]
    fn narrow_to_wide_covers_continuation() {
        let mut a = surf(6, 1);
        a.draw_text(0, 0, "ab", Style::new());
        let mut b = surf(6, 1);
        b.draw_text(0, 0, "世", Style::new());
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &full(&a)).to_vec();
        // Both cells differ (leader vs 'a', continuation vs 'b'): one run.
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 2 }]);
    }

    #[test]
    fn wide_to_wide_same_style_emits_leader_only() {
        let mut a = surf(6, 1);
        a.draw_text(0, 0, "世", Style::new());
        let mut b = surf(6, 1);
        b.draw_text(0, 0, "界", Style::new());
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &full(&a)).to_vec();
        // Continuations mirror identical styles and compare equal; printing
        // the leader repaints both columns.
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 1 }]);
    }

    #[test]
    fn damage_slicing_a_pair_still_reaches_the_leader() {
        let mut a = surf(6, 1);
        a.draw_text(0, 0, "ab", Style::new());
        let mut b = surf(6, 1);
        b.draw_text(0, 0, "世", Style::new());
        let mut diff = FrameDiff::new();
        // Damage covers only the continuation column; the ±1 interval
        // expansion pulls the leader in.
        let runs = diff.compute(&a, &b, &[Rect::new(1, 0, 1, 1)]).to_vec();
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 2 }]);
    }

    #[test]
    fn style_only_change_is_detected() {
        let mut a = surf(4, 1);
        a.draw_text(0, 0, "hi", Style::new());
        let mut b = surf(4, 1);
        b.draw_text(0, 0, "hi", Style::new().fg(Rgba::rgb(255, 0, 0)));
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &full(&a)).to_vec();
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 2 }]);
    }

    #[test]
    fn pooled_glyphs_compare_by_content_across_pools() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let mut a = surf(6, 1);
        // Skew pool ids between the two surfaces: intern another long
        // cluster first, then overwrite every cell it touched.
        a.draw_text(0, 0, "👩\u{200D}🚀", Style::new());
        a.draw_text(0, 0, family, Style::new());
        let mut b = surf(6, 1);
        b.draw_text(0, 0, family, Style::new());
        let mut diff = FrameDiff::new();
        assert!(
            diff.compute(&a, &b, &full(&a)).is_empty(),
            "same cluster in different pools must compare equal"
        );
    }

    #[test]
    fn link_identity_is_by_uri_not_id() {
        let mut a = surf(4, 1);
        let la = a.register_link("https://a.example");
        a.draw_text(0, 0, "x", Style::new().link(la));
        let mut b = surf(4, 1);
        b.register_link("https://padding.example"); // shift ids
        let lb = b.register_link("https://a.example");
        b.draw_text(0, 0, "x", Style::new().link(lb));
        let mut diff = FrameDiff::new();
        assert!(diff.compute(&a, &b, &full(&a)).is_empty());

        let mut c = surf(4, 1);
        let lc = c.register_link("https://other.example");
        c.draw_text(0, 0, "x", Style::new().link(lc));
        assert_eq!(diff.compute(&a, &c, &full(&a)).len(), 1);
    }

    #[test]
    fn size_mismatch_emits_everything() {
        let a = surf(4, 2);
        let b = surf(6, 3);
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &[]).to_vec();
        assert_eq!(runs.len(), 3);
        assert!(runs.iter().all(|r| r.len == 6));
    }

    #[test]
    fn overlapping_damage_rects_do_not_duplicate_runs() {
        let a = surf(10, 1);
        let mut b = surf(10, 1);
        b.draw_text(2, 0, "xxx", Style::new());
        let damage = vec![Rect::new(1, 0, 4, 1), Rect::new(3, 0, 4, 1)];
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &damage).to_vec();
        assert_eq!(runs, vec![Run { y: 0, x: 2, len: 3 }]);
    }

    #[test]
    fn steady_state_reuses_scratch() {
        let a = surf(80, 24);
        let mut b = surf(80, 24);
        b.draw_text(0, 0, "warmup", Style::new());
        let mut diff = FrameDiff::new();
        diff.compute(&a, &b, &full(&a));
        let cap_runs = diff.runs.capacity();
        let cap_spans = diff.spans.capacity();
        for _ in 0..10 {
            diff.compute(&a, &b, &full(&a));
        }
        assert_eq!(diff.runs.capacity(), cap_runs);
        assert_eq!(diff.spans.capacity(), cap_spans);
    }

    /// Functional half of RT2-8 (the allocator-counted half lives in
    /// tests/alloc_budget.rs): a warmed no-change frame must produce zero
    /// runs, zero bytes, and no scratch growth — the idle-adjacent path
    /// costs comparisons only.
    #[test]
    fn no_change_frame_is_scratch_only_and_byteless() {
        use crate::render::present::{PresentCaps, Presenter};
        let mut frame = surf(80, 24);
        frame.draw_text(0, 0, "steady content", Style::new().fg(Rgba::rgb(9, 9, 9)));
        let mut diff = FrameDiff::new();
        let mut presenter = Presenter::new();
        let mut out = Vec::new();
        // Warm scratch + presenter state.
        let runs = diff.compute_full(&frame, &frame).to_vec();
        presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
        out.clear();
        let (cap_spans, cap_runs) = (diff.spans.capacity(), diff.runs.capacity());

        for _ in 0..5 {
            let runs = diff.compute_full(&frame, &frame);
            assert!(runs.is_empty(), "identical frames produce no runs");
            let runs = runs.to_vec();
            presenter.emit(&runs, &frame, &PresentCaps::FULL, &mut out);
            assert!(out.is_empty(), "identical frames emit zero bytes");
        }
        assert_eq!(diff.spans.capacity(), cap_spans, "span scratch reused");
        assert_eq!(diff.runs.capacity(), cap_runs, "run scratch reused");
    }

    #[test]
    fn moved_layer_style_change_via_point_damage() {
        // Regression guard for the ±1 expansion interacting with merging:
        // adjacent 1-wide damage rects merge into one clean run.
        let a = surf(10, 1);
        let mut b = surf(10, 1);
        b.draw_text(0, 0, "abcd", Style::new());
        let damage: Vec<Rect> = (0..4).map(|x| Rect::new(x, 0, 1, 1)).collect();
        let mut diff = FrameDiff::new();
        let runs = diff.compute(&a, &b, &damage).to_vec();
        assert_eq!(runs, vec![Run { y: 0, x: 0, len: 4 }]);
    }
}
