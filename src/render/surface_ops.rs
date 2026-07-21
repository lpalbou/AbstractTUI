//! Region operations on [`Surface`]: scroll and resize — split from
//! `surface.rs` (file-size budget). Same invariants, same damage rules;
//! pairs straddling moved edges are severed/blanked exactly as the
//! in-file write paths do.

use crate::base::{Rect, Size};

use super::cell::Cell;
use super::surface::{sanitize_fill, Surface};

impl Surface {
    /// Scrolls `region` (clipped) up by `n` rows; vacated rows fill with
    /// `fill`. Pairs straddling the region's vertical edges are severed
    /// first — their halves would otherwise land in different rows.
    pub fn scroll_up(&mut self, region: Rect, n: i32, fill: Cell) {
        self.scroll(region, n, fill, true);
    }

    /// Scrolls `region` down by `n` rows; vacated rows fill with `fill`.
    pub fn scroll_down(&mut self, region: Rect, n: i32, fill: Cell) {
        self.scroll(region, n, fill, false);
    }

    fn scroll(&mut self, region: Rect, n: i32, fill: Cell, up: bool) {
        let r = region.intersect(self.bounds());
        if r.is_empty() || n <= 0 {
            return;
        }
        let fill = sanitize_fill(fill);
        if n >= r.h {
            self.fill_rect(r, fill);
            return;
        }
        self.sever_pairs_at_edges(r);
        let row_span = r.w as usize;
        if up {
            for y in r.y..r.bottom() - n {
                let src = self.idx(r.x, y + n);
                let dst = self.idx(r.x, y);
                self.cells.copy_within(src..src + row_span, dst);
            }
            self.fill_rect(Rect::new(r.x, r.bottom() - n, r.w, n), fill);
        } else {
            for y in (r.y + n..r.bottom()).rev() {
                let src = self.idx(r.x, y - n);
                let dst = self.idx(r.x, y);
                self.cells.copy_within(src..src + row_span, dst);
            }
            self.fill_rect(Rect::new(r.x, r.y, r.w, n), fill);
        }
        // ±1 column: sever_pairs_at_edges may blank cells just outside.
        self.add_damage(Rect::new(r.x - 1, r.y, r.w + 2, r.h));
    }

    /// Resizes in place, preserving the overlapping top-left region. New
    /// cells take `fill`. A pair cut by a narrower width blanks its leader.
    /// Pool entries referenced only by dropped cells are retained (bounded
    /// by unique long clusters; documented gap in the design notes).
    pub fn resize(&mut self, new_size: Size, fill: Cell) {
        let new_size = Size::new(new_size.w.max(0), new_size.h.max(0));
        if new_size == self.size {
            return;
        }
        let fill = sanitize_fill(fill);
        let fill = if fill.glyph.width() >= 2 {
            fill.blanked()
        } else {
            fill
        };
        let mut cells = vec![fill; (new_size.w * new_size.h).max(0) as usize];
        let copy_w = self.size.w.min(new_size.w) as usize;
        for y in 0..self.size.h.min(new_size.h) {
            let src = self.idx(0, y);
            let dst = (y * new_size.w) as usize;
            cells[dst..dst + copy_w].copy_from_slice(&self.cells[src..src + copy_w]);
            // A leader whose continuation fell off the new right edge.
            if copy_w > 0 && copy_w as i32 == new_size.w && cells[dst + copy_w - 1].is_wide_leader()
            {
                cells[dst + copy_w - 1] = cells[dst + copy_w - 1].blanked();
            }
        }
        self.cells = cells;
        self.size = new_size;
        self.damage_all();
    }
}
