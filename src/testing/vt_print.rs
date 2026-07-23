//! The print plane of [`VtScreen`]: glyph printing with wide/cluster
//! handling, deferred-wrap semantics, and line feed / scroll-up.
//! `#[path]` child of vt.rs (file-size split) — same impl, different
//! file; `pub(super)` = exactly the old private-in-vt audience.
//!
//! OWNER: REDTEAM.

use unicode_width::UnicodeWidthChar;

use crate::base::Point;

use super::VtScreen;

impl VtScreen {
    pub(super) fn print_char(&mut self, c: char) {
        // ASCII printable fast path: width is definitionally 1; skipping
        // the unicode-width table walk here keeps the referee cheap on
        // the (dominant) plain-text portion of property-test frames.
        let width = if (' '..='\u{7e}').contains(&c) {
            1
        } else {
            match c.width() {
                Some(w) => w,
                None => {
                    // C1 controls and friends decode to width None.
                    self.note_unknown(&format!("unprintable U+{:04X}", c as u32));
                    return;
                }
            }
        };
        // Regional-indicator fuse has EXACTLY one-char lifetime: capture
        // and clear it here so any intervening scalar (ZWJ, combining
        // mark, or a normal glyph via the early returns below) cannot
        // leave it armed for a later indicator.
        let was_regional = std::mem::replace(&mut self.pending_regional, false);
        if c == '\u{200d}' {
            // ZWJ: joins the previous cluster with the NEXT printable
            // (render.md §2.5: a joined sequence is ONE cell, width ≤ 2).
            if let Some(p) = self.last_write {
                if self.grid.append_combining(p.x, p.y, c) {
                    self.pending_zwj = true;
                }
            }
            return;
        }
        if width == 0 {
            // Combining mark: attach to the last written glyph. VS16
            // (emoji presentation) can widen a narrow base — unicode-width
            // str metrics and the render contract agree; mirror it.
            if let Some(p) = self.last_write {
                self.grid.append_combining(p.x, p.y, c);
                if c == '\u{fe0f}' {
                    self.grow_cluster_at(p);
                }
            }
            return;
        }
        if self.pending_zwj {
            // The printable after a ZWJ joins the anchored cluster
            // instead of occupying its own cell.
            self.pending_zwj = false;
            if let Some(p) = self.last_write {
                if self.grid.append_combining(p.x, p.y, c) {
                    self.grow_cluster_at(p);
                    return;
                }
            }
            // No anchor to join (ZWJ at line start): print normally.
        }
        if ('\u{1f3fb}'..='\u{1f3ff}').contains(&c) {
            // Emoji skin-tone modifier: modern terminals (and the render
            // cluster convention) fuse it with the preceding emoji when
            // adjacent. Adjacency = the cursor still sits right after the
            // last-written cluster.
            if let Some(p) = self.last_write {
                let adjacent =
                    self.cursor.y == p.y && (self.cursor.x == p.x + 1 || self.cursor.x == p.x + 2);
                if adjacent && self.grid.append_combining(p.x, p.y, c) {
                    self.grow_cluster_at(p);
                    return;
                }
            }
            // Standalone modifier: falls through, prints as its own glyph.
        }
        if ('\u{1F1E6}'..='\u{1F1FF}').contains(&c) {
            // Regional indicator: two consecutive scalars form ONE flag
            // grapheme (width 2), the same segmentation the render layer
            // applies. Fuse with an immediately-preceding lone indicator;
            // otherwise start a new pending flag as a normal glyph below.
            if was_regional {
                if let Some(p) = self.last_write {
                    let adjacent = self.cursor.y == p.y
                        && (self.cursor.x == p.x + 1 || self.cursor.x == p.x + 2);
                    if adjacent && self.grid.append_combining(p.x, p.y, c) {
                        self.grow_cluster_at(p);
                        return; // pending_regional already cleared above
                    }
                }
            }
            // First indicator of a (possible) pair: print it, then arm the
            // fuse for the next one. (A lone trailing indicator stays a
            // width-1 glyph, which is what a real terminal shows too.)
            self.print_glyph(c, width);
            self.pending_regional = true;
            return;
        }
        self.print_glyph(c, width);
    }

    /// Write a base glyph of the given display width at the cursor, with
    /// the deferred-autowrap and wide-at-last-column policy. Split out of
    /// `print_char` so cluster-starting scalars (regional indicators)
    /// reuse the exact same placement path.
    pub(super) fn print_glyph(&mut self, c: char, width: usize) {
        let w = self.grid.w;
        if width == 1 {
            if self.wrap_pending {
                self.wrap_now();
            }
            self.grid
                .put_narrow(self.cursor.x, self.cursor.y, c, self.paint);
            self.last_write = Some(self.cursor);
            if self.cursor.x + 1 >= w {
                if self.modes.autowrap() {
                    self.wrap_pending = true; // deferred wrap (xterm)
                } // else: cursor sticks at the last column
            } else {
                self.cursor.x += 1;
            }
        } else {
            // width == 2
            if self.wrap_pending {
                self.wrap_now();
            }
            if self.cursor.x + 2 > w {
                // Wide glyph at the last column: with autowrap the glyph
                // moves to the next row and the orphan cell becomes a
                // styled blank; without autowrap the glyph is dropped.
                if self.modes.autowrap() {
                    self.grid.erase_row_range(
                        self.cursor.y,
                        self.cursor.x,
                        w,
                        self.paint.erase_paint(),
                    );
                    self.wrap_now();
                } else {
                    return;
                }
            }
            let p = self.cursor;
            self.grid.put_wide(p.x, p.y, c, self.paint);
            self.last_write = Some(p);
            if p.x + 2 >= w {
                if self.modes.autowrap() {
                    self.cursor.x = w - 1;
                    self.wrap_pending = true;
                } else {
                    self.cursor.x = w - 1;
                }
            } else {
                self.cursor.x = p.x + 2;
            }
        }
    }

    /// A zero-width append grew the cluster at `p` from narrow to wide:
    /// materialize the continuation cell and, when the cursor sits right
    /// after the glyph, advance it over the new continuation.
    pub(super) fn grow_cluster_at(&mut self, p: Point) {
        if self.grid.widen_to_wide(p.x, p.y) && self.cursor.y == p.y && self.cursor.x == p.x + 1 {
            if p.x + 2 >= self.grid.w {
                self.cursor.x = self.grid.w - 1;
                if self.modes.autowrap() {
                    self.wrap_pending = true;
                }
            } else {
                self.cursor.x = p.x + 2;
            }
        }
    }

    pub(super) fn wrap_now(&mut self) {
        self.wrap_pending = false;
        self.cursor.x = 0;
        self.line_feed();
    }

    pub(super) fn line_feed(&mut self) {
        let (top, bottom) = self.scroll_span();
        if self.cursor.y == bottom {
            // At the region's bottom margin: the REGION scrolls (rows
            // outside the margins never move — DECSTBM's whole point).
            self.grid
                .scroll_up_region(top, bottom, 1, self.paint.erase_paint());
            self.last_write = None; // rows moved; anchor is stale
        } else if self.cursor.y + 1 >= self.grid.h {
            // Below the region, at the screen's last row: stick.
        } else {
            self.cursor.y += 1;
        }
        self.wrap_pending = false;
    }
}
